//! Captura do microfone com cpal, reduzida a mono.
//!
//! O áudio sai daqui na taxa do próprio dispositivo, e não nos 16 kHz que o
//! Whisper exige: reamostrar custa uma centena de multiplicações por amostra de
//! saída, e fazer isso aqui prenderia a thread que atende os comandos — que é a
//! mesma que precisa estar pronta para abrir o microfone de novo. Falar outra
//! frase enquanto a anterior é transcrita é o uso normal do programa, então a
//! conversão foi para o lado de quem transcreve (ver `stt.rs`), que já ia
//! esperar mesmo.
//!
//! ## Os dois modos de microfone
//!
//! **Sob demanda** é como este arquivo sempre funcionou: o `cpal` abre o
//! dispositivo quando a tecla é apertada e o fecha quando ela é solta. Custa
//! zero enquanto ninguém dita, e custa a *abertura* — de 40 ms a algumas
//! centenas, dependendo do aparelho e da máquina — bem no instante em que a
//! pessoa já começou a falar. É de lá que vem a primeira sílaba cortada que as
//! "Limitações conhecidas" do README sempre listaram.
//!
//! **Sempre aberto** (o padrão desde a 0.7) mantém o stream de pé o tempo todo.
//! Fora de uma gravação as amostras não são guardadas: elas entram num anel de
//! [`PRE_GRAVACAO_MS`] milissegundos que se sobrescreve sozinho e nunca toca o
//! disco. Apertar a tecla vira uma troca de bandeira — instantânea — e o anel é
//! despejado no começo da gravação, de modo que o áudio começa **antes** do
//! aperto. Não é só o fim do corte: é margem para quem começa a falar junto com
//! a tecla.
//!
//! O preço é o indicador de "microfone em uso" do sistema ficar aceso enquanto
//! o Ditador está no ar. É por isso que a chave existe na tela, com essa
//! ressalva escrita ao lado: a resposta certa aqui depende de quem usa e de onde,
//! e não é do programa decidir por todo mundo.

use crate::config::WHISPER_SAMPLE_RATE;
use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Quantas amostras de nível guardamos para desenhar a animação.
pub const LEVEL_HISTORY: usize = 36;

/// Quanto áudio anterior ao aperto da tecla entra na gravação, no modo sempre
/// aberto.
///
/// Trezentos milissegundos cobrem com folga a sílaba que se perdia e ainda dão
/// margem para quem começa a falar no mesmo instante em que aperta. Mais do que
/// isso passaria a capturar o que foi dito *antes* de a pessoa decidir ditar —
/// o barulho da sala, o fim da conversa ao lado — e o Whisper transcreveria
/// aquilo junto, o que é pior do que perder o começo.
pub const PRE_GRAVACAO_MS: u64 = 300;

/// De quanto em quanto tempo tentamos reabrir um microfone que sumiu, no modo
/// sempre aberto.
const ESPERA_PARA_REABRIR: Duration = Duration::from_secs(3);

pub type Levels = Arc<Mutex<VecDeque<f32>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettings {
    pub device: Option<String>,
    pub max_secs: u64,
    /// Manter o dispositivo aberto entre os ditados (ver o bloco `//!`).
    pub sempre_aberto: bool,
    /// Qual canal usar; `None` mistura todos.
    pub canal: Option<u16>,
}

impl AudioSettings {
    /// Mudanças que obrigam a reabrir o dispositivo.
    ///
    /// `max_secs` não entra: reabrir por causa dele fecharia o microfone de quem
    /// mexeu num deslizante que não tem nada a ver com o aparelho. Quem paga
    /// esse preço é o `Captura::ajustar_o_teto`, que troca o teto com o stream
    /// de pé — e ele existe porque esta linha já disse, por um tempo, que o teto
    /// "é lido a cada gravação". Não era: ele era lido uma vez, no `abrir`, e no
    /// modo sempre aberto isso significava um `abrir` por execução do programa.
    fn pede_reabertura(&self, outra: &AudioSettings) -> bool {
        self.device != outra.device || self.canal != outra.canal
    }
}

#[derive(Debug)]
pub enum AudioCmd {
    Configure(AudioSettings),
    /// `ditado` é o número que o controlador deu a esta gravação; ele volta nos
    /// eventos para que uma gravação antiga não seja confundida com a atual.
    Start {
        ditado: u64,
    },
    Stop,
    /// Descarta a gravação em curso sem entregar áudio nenhum.
    ///
    /// É um comando próprio, e não um `Stop` cujo resultado o controlador joga
    /// fora, por dois motivos. O primeiro é que o áudio descartado nem chega a
    /// atravessar o canal — são megabytes que não são copiados nem alocados do
    /// outro lado. O segundo é que o `Stop` produz um `Captured`, e um
    /// `Captured` que não deve virar nada obrigaria o controlador a lembrar,
    /// quando ele chegasse, que aquele ditado foi cancelado — mais um estado
    /// para manter em dia, que é exatamente o tipo de coisa que já deu errado
    /// neste programa.
    Cancel,
}

#[derive(Debug)]
pub enum AudioEvent {
    Started,
    Captured {
        ditado: u64,
        /// Mono, na taxa do dispositivo — quem reamostra é o `stt`.
        samples: Vec<f32>,
        sample_rate: u32,
        duration_ms: u64,
    },
    Failed {
        ditado: u64,
        message: String,
    },
}

pub struct AudioHandle {
    pub tx: Sender<AudioCmd>,
    pub levels: Levels,
}

impl AudioHandle {
    pub fn send(&self, cmd: AudioCmd) {
        let _ = self.tx.send(cmd);
    }
}

pub fn spawn(settings: AudioSettings, events: Sender<AudioEvent>) -> AudioHandle {
    let (tx, rx) = crossbeam_channel::unbounded();
    let levels: Levels = Arc::new(Mutex::new(VecDeque::with_capacity(LEVEL_HISTORY)));

    let thread_levels = levels.clone();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || run(settings, rx, events, thread_levels))
        .expect("spawn audio thread");

    AudioHandle { tx, levels }
}

/// Lista os microfones disponíveis, para o seletor das configurações.
///
/// Os nomes repetidos são desfeitos aqui. O ALSA anuncia o mesmo microfone em
/// vários PCMs — `hw:`, `plughw:`, `sysdefault:`, `dsnoop:` — e a primeira linha
/// da descrição é idêntica em todos: num aparelho comum saíam sete entradas
/// iguais na lista, todas marcadas como escolhidas ao mesmo tempo. Pior, como o
/// que se grava na configuração é o nome, a busca sempre recaía sobre a
/// primeira da enumeração, que costuma ser o `hw:` cru — acesso exclusivo, sem
/// conversão de taxa — e não havia como escolher a que funcionaria.
pub fn list_input_devices() -> Vec<String> {
    rotular(cpal::default_host().input_devices().ok())
        .into_iter()
        .map(|(rotulo, _)| rotulo)
        .collect()
}

/// Acha o dispositivo pelo rótulo que a configuração guardou.
///
/// Se o rótulo exato não estiver mais lá, vale o nome sem o sufixo de desempate:
/// o ALSA renumera os PCMs entre sessões (`hw:CARD=2` hoje, `hw:CARD=Generic_1`
/// amanhã), e é o mesmo microfone. Melhor gravar no aparelho certo por um
/// caminho diferente do que dizer que ele sumiu.
fn achar_dispositivo(host: &cpal::Host, procurado: &str) -> Option<cpal::Device> {
    let candidatos = rotular(host.input_devices().ok());
    if let Some((_, device)) = candidatos.iter().find(|(rotulo, _)| rotulo == procurado) {
        return Some(device.clone());
    }

    let base = procurado
        .split_once(" (")
        .map_or(procurado, |(nome, _)| nome);
    let achado = candidatos
        .iter()
        .find(|(rotulo, _)| rotulo.split_once(" (").map_or(rotulo.as_str(), |(n, _)| n) == base);
    if let Some((rotulo, device)) = achado {
        log::info!("o microfone \"{procurado}\" mudou de endereço; usando \"{rotulo}\"");
        return Some(device.clone());
    }
    None
}

/// Dá a cada microfone um rótulo único, na ordem em que o cpal os anuncia.
///
/// A lista e a busca passam as duas por aqui de propósito: se as duas regras
/// vivessem separadas, o dia em que uma mudasse a configuração de alguém
/// passaria a apontar para um dispositivo que a tela nunca mostrou.
///
/// O primeiro de cada nome fica com o nome limpo, sem sufixo, porque é ele que
/// já está gravado nas configurações de quem usa o programa hoje.
fn rotular(devices: Option<impl Iterator<Item = cpal::Device>>) -> Vec<(String, cpal::Device)> {
    let Some(devices) = devices else {
        log::warn!("não consegui listar os microfones do sistema");
        return Vec::new();
    };

    let mut saida: Vec<(String, cpal::Device)> = Vec::new();
    let mut vistos: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for device in devices {
        let Some(descricao) = device.description().ok() else {
            continue;
        };
        let base = descricao.name().to_string();
        let quantos = vistos.entry(base.clone()).or_insert(0);
        *quantos += 1;
        let rotulo = match (*quantos, descricao.driver()) {
            (1, _) => base,
            // No ALSA o `driver` é o identificador do PCM (`hw:0,0`,
            // `plughw:0,0`, `dsnoop:…`), que é exatamente o que distingue as
            // sete entradas idênticas de um mesmo microfone.
            (_, Some(pcm)) if !pcm.is_empty() => format!("{base} ({pcm})"),
            (n, _) => format!("{base} #{n}"),
        };
        saida.push((rotulo, device));
    }
    saida
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

/// De quanto em quanto tempo a thread acorda, enquanto grava, para conferir se
/// a gravação já bateu o teto de duração.
const RONDA: Duration = Duration::from_millis(200);

/// O que o callback de áudio e a thread de comandos dividem.
///
/// Tudo aqui é tocado de dentro do callback, que roda em tempo real: nada de
/// alocar, nada de bloquear por muito tempo. Os dois `Mutex` são segurados por
/// microssegundos, o tempo de empurrar algumas centenas de amostras.
struct Captura {
    /// A gravação está acontecendo agora?
    ///
    /// É a bandeira que faz o modo sempre aberto valer a pena: apertar a tecla
    /// vira uma escrita atômica, e não a abertura de um dispositivo.
    gravando: AtomicBool,
    /// O áudio da gravação em curso.
    buffer: Mutex<Vec<f32>>,
    /// O anel de pré-gravação: os últimos [`PRE_GRAVACAO_MS`] de áudio, sempre.
    ///
    /// Só é alimentado no modo sempre aberto e fora de uma gravação. É o que
    /// entra no começo do buffer quando a tecla é apertada.
    pre: Mutex<VecDeque<f32>>,
    /// Quantas amostras o anel guarda.
    pre_maximo: usize,
    /// Teto de amostras de uma gravação.
    ///
    /// Atômico porque ele muda **sem** o dispositivo ser reaberto. No modo
    /// sempre aberto — o padrão desde a 0.7 — o stream fica de pé entre os
    /// ditados, e o teto era lido uma vez só, na abertura: quem mexesse no
    /// "Gravação máxima" das configurações continuava sendo cortado no valor
    /// antigo até reiniciar o programa, sem nada dizendo por quê. O
    /// `pede_reabertura` não o inclui de propósito (reabrir por causa de um
    /// deslizante fecharia o microfone de quem só arrastou um controle), e a
    /// contrapartida disso é ele ser ajustável de fora.
    max_samples: AtomicUsize,
    /// O dispositivo sumiu.
    ///
    /// Marcada pelo callback de erro do cpal e lida pela ronda de `run`. É por
    /// bandeira, e não por evento mandado dali de dentro, porque quem precisa
    /// largar o stream é a thread de comandos: um `Failed` vindo do callback
    /// zeraria o `recording_since` do controlador com o stream ainda de pé, o
    /// `stop_recording` seguinte sairia cedo, o `Stop` nunca chegaria aqui — e
    /// todo ditado depois disso ficaria preso em "Transcrevendo…".
    perdido: AtomicBool,
}

impl Captura {
    fn nova(pre_maximo: usize, max_samples: usize) -> Self {
        Self {
            gravando: AtomicBool::new(false),
            // O buffer nasce já do tamanho do teto de duração. Crescer sob
            // demanda significava `realloc` — com cópia de tudo — dentro do
            // callback de áudio, que roda em tempo real e não pode esperar o
            // alocador. Ao teto padrão de 120 s isso é meio megabyte.
            buffer: Mutex::new(Vec::with_capacity(max_samples)),
            pre: Mutex::new(VecDeque::with_capacity(pre_maximo)),
            pre_maximo,
            max_samples: AtomicUsize::new(max_samples),
            perdido: AtomicBool::new(false),
        }
    }

    /// Uma amostra mono acabou de chegar do dispositivo.
    fn amostra(&self, valor: f32) {
        if self.gravando.load(Ordering::Relaxed) {
            let mut buf = lock(&self.buffer);
            // Teto batido: para de acumular e deixa a ronda de `run` encerrar a
            // gravação, o que acontece na volta seguinte dela.
            if buf.len() < self.teto() {
                buf.push(valor);
            }
            return;
        }
        if self.pre_maximo == 0 {
            return;
        }
        let mut pre = lock(&self.pre);
        while pre.len() >= self.pre_maximo {
            pre.pop_front();
        }
        pre.push_back(valor);
    }

    /// O teto de amostras que vale agora.
    fn teto(&self) -> usize {
        self.max_samples.load(Ordering::Relaxed)
    }

    /// Troca o teto de duração sem fechar o microfone.
    ///
    /// Chamada pela thread de comandos ao receber um `Configure`, e nunca de
    /// dentro do callback de áudio — a reserva do buffer aloca, e alocar em
    /// tempo real é justamente o que ela existe para evitar.
    fn ajustar_o_teto(&self, max_samples: usize) {
        if self.teto() == max_samples {
            return;
        }
        self.max_samples.store(max_samples, Ordering::Relaxed);
        lock(&self.buffer).reserve(max_samples);
    }

    /// Começa a guardar, levando junto o que já estava no anel.
    fn comecar(&self) {
        let mut buf = lock(&self.buffer);
        buf.clear();
        // O `terminar` leva a alocação embora junto com as amostras — é o que
        // evita copiar megabytes para entregá-las —, então do segundo ditado em
        // diante o buffer voltava a nascer vazio e crescia dentro do callback de
        // áudio, que é o que o `with_capacity` do `nova` existia para impedir.
        // Reservar aqui repõe a promessa a cada gravação, e nesta thread, que
        // pode esperar o alocador.
        buf.reserve(self.teto());
        // A ordem importa: o anel é despejado **antes** de a bandeira subir,
        // senão as amostras que chegarem no meio do despejo entrariam no buffer
        // à frente das que já estavam no anel — e o ditado começaria com um
        // pedacinho do futuro antes do passado.
        let mut pre = lock(&self.pre);
        buf.extend(pre.drain(..));
        drop(pre);
        drop(buf);
        self.gravando.store(true, Ordering::Relaxed);
    }

    /// Para de guardar e devolve o que foi capturado.
    fn terminar(&self) -> Vec<f32> {
        self.gravando.store(false, Ordering::Relaxed);
        let capturado = std::mem::take(&mut *lock(&self.buffer));
        // O anel recomeça vazio: o que ficou nele é o fim da frase que acabou
        // de ser entregue, e ele apareceria de novo no começo da próxima.
        lock(&self.pre).clear();
        capturado
    }

    fn quantas(&self) -> usize {
        lock(&self.buffer).len()
    }

    fn cheia(&self) -> bool {
        self.quantas() >= self.teto()
    }

    fn perdeu_o_dispositivo(&self) -> bool {
        self.perdido.load(Ordering::Relaxed)
    }
}

/// Quantas amostras cabem no teto de duração, nesta taxa de amostragem.
///
/// Mora fora do `abrir` porque a conta é feita em dois momentos — ao abrir o
/// dispositivo e a cada `Configure` que mude o teto com ele já aberto —, e duas
/// cópias dela é o começo de os dois discordarem.
fn teto_em_amostras(max_secs: u64, sample_rate: u32) -> usize {
    (max_secs.max(1) as usize) * sample_rate as usize
}

/// O microfone aberto: o stream do cpal mais o que ele alimenta.
struct Aberto {
    /// O stream precisa continuar vivo enquanto o microfone está aberto; ele
    /// nunca sai desta thread.
    _stream: cpal::Stream,
    captura: Arc<Captura>,
    sample_rate: u32,
}

impl Aberto {
    fn duracao_ms(&self) -> u64 {
        self.captura.quantas() as u64 * 1000 / self.sample_rate.max(1) as u64
    }
}

fn run(
    mut settings: AudioSettings,
    rx: Receiver<AudioCmd>,
    events: Sender<AudioEvent>,
    levels: Levels,
) {
    /// O microfone está aberto e guardando um ditado, ou só aberto?
    struct Estado {
        aberto: Option<Aberto>,
        /// O número do ditado em curso, quando há um.
        ditado: Option<u64>,
        /// Quando tentar reabrir o dispositivo, no modo sempre aberto.
        reabrir_em: Option<Instant>,
    }

    let mut estado = Estado {
        aberto: None,
        ditado: None,
        reabrir_em: None,
    };

    // No modo sempre aberto, o microfone sobe junto com o programa. Uma falha
    // aqui não é motivo para nada: a ronda tenta de novo, e o caminho sob
    // demanda continua valendo como reserva no primeiro ditado.
    if settings.sempre_aberto {
        estado.aberto = abrir(&settings, &levels)
            .map_err(|e| {
                log::info!(
                    "microfone ainda não pôde ser aberto ({e:#}); tentando de novo em seguida"
                );
                estado.reabrir_em = Some(Instant::now() + ESPERA_PARA_REABRIR);
            })
            .ok();
    }

    loop {
        // A thread só acorda sozinha quando tem o que conferir, e o prazo é o da
        // coisa que ela está esperando:
        //
        //  * gravando, é a `RONDA` — o teto de duração precisa ser conferido de
        //    tempos em tempos, senão o buffer cheio pararia de aceitar amostras e
        //    a gravação seguiria de olhos abertos até alguém soltar a tecla;
        //  * esperando para reabrir um dispositivo, é o tempo que falta para a
        //    próxima tentativa. Usar a `RONDA` aqui foi o primeiro erro deste
        //    laço: numa máquina cujo microfone padrão está indisponível — um fone
        //    Bluetooth desligado é o caso comum —, o modo sempre aberto ficava
        //    acordando cinco vezes por segundo, para sempre, num programa que
        //    passa o dia parado na bandeja;
        //  * sem nada disso, ela dorme no `recv` e não custa nada.
        let espera = match (estado.ditado.is_some(), estado.reabrir_em) {
            (true, _) => Some(RONDA),
            (false, Some(quando)) => Some(
                quando
                    .saturating_duration_since(Instant::now())
                    // Um piso para o caso de o prazo já ter passado: sem ele, um
                    // `Duration::ZERO` faria o `recv_timeout` voltar na hora e o
                    // laço girar sem parar até a reabertura dar certo.
                    .max(Duration::from_millis(50)),
            ),
            (false, None) => None,
        };
        let cmd = if let Some(espera) = espera {
            match rx.recv_timeout(espera) {
                Ok(cmd) => Some(cmd),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => return,
            }
        };

        match cmd {
            Some(AudioCmd::Configure(nova)) => {
                let reabrir = settings.pede_reabertura(&nova);
                let modo_mudou = settings.sempre_aberto != nova.sempre_aberto;
                settings = nova;
                // Nada disso interrompe um ditado em curso: quem está falando
                // agora é dono do microfone até soltar a tecla. A configuração
                // nova vale a partir do próximo.
                if estado.ditado.is_some() {
                    continue;
                }
                if reabrir || modo_mudou || !settings.sempre_aberto {
                    estado.aberto = None;
                    estado.reabrir_em = None;
                }
                // O teto de duração não pede reabertura — reabrir por causa de
                // um deslizante fecharia o microfone de quem só arrastou um
                // controle —, e por isso ele precisa ser aplicado à mão no
                // dispositivo que ficou aberto. Sem esta linha, no modo sempre
                // aberto (o padrão) o "Gravação máxima" das configurações não
                // valia até o programa ser reiniciado.
                if let Some(aberto) = &estado.aberto {
                    aberto
                        .captura
                        .ajustar_o_teto(teto_em_amostras(settings.max_secs, aberto.sample_rate));
                }
                if settings.sempre_aberto && estado.aberto.is_none() {
                    abrir_ou_agendar(
                        &settings,
                        &levels,
                        &mut estado.aberto,
                        &mut estado.reabrir_em,
                    );
                }
            }

            Some(AudioCmd::Start { ditado }) => {
                if estado.ditado.is_some() {
                    continue;
                }
                // Sob demanda, ou sempre aberto com o dispositivo caído: abre
                // agora. É também o caminho de reserva do modo sempre aberto —
                // um microfone que sumiu não pode significar um ditado que não
                // acontece.
                if estado.aberto.is_none() {
                    match abrir(&settings, &levels) {
                        Ok(novo) => estado.aberto = Some(novo),
                        Err(e) => {
                            // O erro cru primeiro, a ajuda depois: o texto do
                            // sistema é o que se procura numa busca, e a frase
                            // da plataforma é o que se faz a respeito. No Linux
                            // não há ajuda a acrescentar e a mensagem sai como
                            // sempre saiu.
                            let cru = format!("{e:#}");
                            let message = match crate::plataforma::microfone::explicar_falha(&cru) {
                                Some(ajuda) => format!("{cru}\n\n{ajuda}"),
                                None => cru,
                            };
                            let _ = events.send(AudioEvent::Failed { ditado, message });
                            continue;
                        }
                    }
                }
                let aberto = estado.aberto.as_ref().expect("acabou de ser aberto");
                clear(&levels);
                aberto.captura.comecar();
                estado.ditado = Some(ditado);
                estado.reabrir_em = None;
                let _ = events.send(AudioEvent::Started);
            }

            Some(AudioCmd::Stop) => {
                if let Some(ditado) = estado.ditado.take() {
                    entregar(&mut estado.aberto, ditado, &settings, &levels, &events);
                }
            }

            Some(AudioCmd::Cancel) => {
                // Descartar é o mesmo caminho de parar, menos o evento: o áudio
                // é jogado fora aqui mesmo e nada segue para a transcrição.
                // Passa pelo `terminar` de propósito, para o anel de
                // pré-gravação ser zerado como em qualquer outro fim de ditado —
                // senão a frase descartada reapareceria no começo da seguinte.
                if let Some(ditado) = estado.ditado.take() {
                    let descartado = estado
                        .aberto
                        .as_ref()
                        .map(|a| a.captura.terminar().len())
                        .unwrap_or(0);
                    log::info!("ditado {ditado} cancelado: {descartado} amostras descartadas");
                    clear(&levels);
                    fechar_se_sob_demanda(&settings, &mut estado.aberto);
                }
            }

            // A ronda: nada chegou pelo canal.
            None => {
                // Um dispositivo a reabrir, no modo sempre aberto.
                if let Some(quando) = estado.reabrir_em
                    && Instant::now() >= quando
                    && estado.ditado.is_none()
                {
                    estado.reabrir_em = None;
                    abrir_ou_agendar(
                        &settings,
                        &levels,
                        &mut estado.aberto,
                        &mut estado.reabrir_em,
                    );
                }

                let Some(aberto) = &estado.aberto else {
                    continue;
                };

                if aberto.captura.perdeu_o_dispositivo() {
                    let perdido = estado.aberto.take().expect("acabou de ser conferido");
                    // Fora de uma gravação isto não é falha nenhuma: o fone foi
                    // desconectado, e o certo é reabrir quando ele voltar sem
                    // dizer nada a ninguém.
                    let Some(ditado) = estado.ditado.take() else {
                        log::info!("o microfone sumiu; tentando reabrir em seguida");
                        estado.reabrir_em = settings
                            .sempre_aberto
                            .then(|| Instant::now() + ESPERA_PARA_REABRIR);
                        continue;
                    };

                    // O microfone sumiu no meio da frase. Antes disto o callback
                    // de erro só escrevia no log: o stream continuava de pé, o
                    // buffer congelava, e como ele nunca mais encheria o teto de
                    // duração também não servia de rede. A tela seguia dizendo
                    // "Ouvindo" com o cronômetro correndo, e o pedaço já gravado
                    // morria calado no filtro de duração mínima.
                    let tinha = perdido.duracao_ms();
                    log::warn!("o microfone sumiu no meio da gravação ({tinha} ms capturados)");
                    // O que já foi falado não se perde: se dá uma frase, vai
                    // para a transcrição; se não dá, aí sim é uma falha para
                    // contar na tela.
                    let mut caixa = Some(perdido);
                    if tinha >= AVULSO_MINIMO_MS {
                        entregar(&mut caixa, ditado, &settings, &levels, &events);
                    } else {
                        drop(caixa);
                        clear(&levels);
                        let _ = events.send(AudioEvent::Failed {
                            ditado,
                            message: "o microfone foi desconectado".to_string(),
                        });
                    }
                    estado.reabrir_em = settings
                        .sempre_aberto
                        .then(|| Instant::now() + ESPERA_PARA_REABRIR);
                } else if estado.ditado.is_some() && aberto.captura.cheia() {
                    let ditado = estado.ditado.take().expect("acabou de ser conferido");
                    log::info!(
                        "teto de {} s atingido; encerrando a gravação",
                        settings.max_secs
                    );
                    entregar(&mut estado.aberto, ditado, &settings, &levels, &events);
                }
            }
        }
    }
}

/// Abaixo disto, um ditado interrompido não vale a pena mandar para o Whisper —
/// é menos que uma palavra, e o modelo devolveria alucinação.
const AVULSO_MINIMO_MS: u64 = 400;

/// Tenta abrir; não conseguindo, marca para tentar de novo.
fn abrir_ou_agendar(
    settings: &AudioSettings,
    levels: &Levels,
    aberto: &mut Option<Aberto>,
    reabrir_em: &mut Option<Instant>,
) {
    match abrir(settings, levels) {
        Ok(novo) => {
            *aberto = Some(novo);
            *reabrir_em = None;
        }
        Err(e) => {
            log::debug!("microfone ainda indisponível ({e:#})");
            *reabrir_em = Some(Instant::now() + ESPERA_PARA_REABRIR);
        }
    }
}

/// No modo sob demanda o microfone fecha ao fim de cada ditado; no sempre
/// aberto ele fica.
fn fechar_se_sob_demanda(settings: &AudioSettings, aberto: &mut Option<Aberto>) {
    if !settings.sempre_aberto {
        *aberto = None;
    }
}

/// Fecha a gravação e manda o áudio, ainda na taxa do dispositivo, para quem
/// transcreve. A conversão para 16 kHz acontece lá (ver o bloco `//!`).
fn entregar(
    aberto: &mut Option<Aberto>,
    ditado: u64,
    settings: &AudioSettings,
    levels: &Levels,
    events: &Sender<AudioEvent>,
) {
    let Some(microfone) = aberto.as_ref() else {
        return;
    };
    let sample_rate = microfone.sample_rate;
    let samples = microfone.captura.terminar();
    fechar_se_sob_demanda(settings, aberto);
    clear(levels);

    // A duração sai da contagem de amostras, e não do relógio: ao bater o teto
    // a gravação termina antes de a tecla ser solta, e o relógio contaria um
    // tempo de áudio que não existe.
    let duration_ms = samples.len() as u64 * 1000 / sample_rate.max(1) as u64;

    let _ = events.send(AudioEvent::Captured {
        ditado,
        samples,
        sample_rate,
        duration_ms,
    });
}

fn abrir(settings: &AudioSettings, levels: &Levels) -> Result<Aberto> {
    let host = cpal::default_host();

    let device = match &settings.device {
        Some(name) => achar_dispositivo(&host, name)
            .ok_or_else(|| anyhow!("microfone \"{name}\" não encontrado"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("nenhum microfone padrão disponível"))?,
    };

    let (config, sample_format) = pick_config(&device)?;
    let sample_rate = config.sample_rate;
    let channels = config.channels as usize;
    let max_samples = teto_em_amostras(settings.max_secs, sample_rate);
    let pre_maximo = if settings.sempre_aberto {
        (PRE_GRAVACAO_MS as usize * sample_rate as usize) / 1000
    } else {
        0
    };
    let canal = canal_valido(settings.canal, channels);

    log::info!(
        "microfone aberto em {} — {} Hz, {} canal(is), {:?}, {}{}",
        device_name(&device).unwrap_or_else(|| "?".into()),
        sample_rate,
        channels,
        sample_format,
        match canal {
            Some(n) => format!("canal {n}"),
            None => "todos os canais misturados".to_string(),
        },
        if settings.sempre_aberto {
            format!(", sempre aberto (pré-gravação de {PRE_GRAVACAO_MS} ms)")
        } else {
            String::new()
        }
    );

    let captura = Arc::new(Captura::nova(pre_maximo, max_samples));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            build::<f32>(&device, &config, &captura, levels, channels, canal)
        }
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, &config, &captura, levels, channels, canal)
        }
        cpal::SampleFormat::I32 => {
            build::<i32>(&device, &config, &captura, levels, channels, canal)
        }
        cpal::SampleFormat::I8 => build::<i8>(&device, &config, &captura, levels, channels, canal),
        cpal::SampleFormat::U8 => build::<u8>(&device, &config, &captura, levels, channels, canal),
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, &config, &captura, levels, channels, canal)
        }
        other => return Err(anyhow!("formato de amostra não suportado: {other:?}")),
    }?;

    stream.play()?;

    Ok(Aberto {
        _stream: stream,
        captura,
        sample_rate,
    })
}

/// O canal escolhido, se ele existir neste dispositivo.
///
/// Um canal que não existe não é motivo para o ditado não acontecer: quem
/// escolheu a entrada 4 de uma interface e depois plugou o headset do notebook
/// precisa continuar conseguindo falar. A mistura de sempre é a reserva, com uma
/// linha no log dizendo o que houve.
fn canal_valido(escolhido: Option<u16>, canais: usize) -> Option<u16> {
    match escolhido {
        Some(n) if (n as usize) < canais => Some(n),
        Some(n) => {
            log::warn!(
                "o canal {n} não existe neste microfone ({canais} canal(is)); \
                 misturando todos"
            );
            None
        }
        None => None,
    }
}

/// Prefere 16 kHz nativo (evita reamostrar), no melhor formato de amostra
/// disponível; cai para a configuração padrão do dispositivo.
fn pick_config(device: &cpal::Device) -> Result<(cpal::StreamConfig, cpal::SampleFormat)> {
    if let Ok(ranges) = device.supported_input_configs() {
        let mut candidates: Vec<_> = ranges
            .filter(|r| {
                r.min_sample_rate() <= WHISPER_SAMPLE_RATE
                    && r.max_sample_rate() >= WHISPER_SAMPLE_RATE
            })
            .collect();
        // Qualidade da amostra primeiro; entre iguais, menos canais.
        candidates.sort_by_key(|r| (format_rank(r.sample_format()), r.channels()));
        if let Some(range) = candidates.into_iter().next() {
            let format = range.sample_format();
            let supported = range.with_sample_rate(WHISPER_SAMPLE_RATE);
            return Ok((supported.into(), format));
        }
    }

    let default = device.default_input_config()?;
    let format = default.sample_format();
    Ok((default.into(), format))
}

/// Menor é melhor. O ALSA costuma anunciar U8 junto com formatos bons, e pegar
/// o primeiro da lista jogaria a gravação para 8 bits sem necessidade.
fn format_rank(format: cpal::SampleFormat) -> u8 {
    match format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I32 => 1,
        cpal::SampleFormat::I16 => 2,
        cpal::SampleFormat::U16 => 3,
        cpal::SampleFormat::I8 => 4,
        cpal::SampleFormat::U8 => 5,
        _ => 6,
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    captura: &Arc<Captura>,
    levels: &Levels,
    channels: usize,
    canal: Option<u16>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let captura_do_callback = captura.clone();
    let captura_do_erro = captura.clone();
    let levels = levels.clone();
    let channels = channels.max(1);
    let canal = canal.map(usize::from);

    let stream = device.build_input_stream(
        *config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut peak = 0.0f32;
            for frame in data.chunks(channels) {
                // Um canal escolhido usa aquele canal; sem escolha, a mistura de
                // sempre. O `unwrap_or` cobre o quadro incompleto que o driver
                // entrega no fim do buffer, que é curto demais para ter o canal
                // pedido — misturar o que veio é melhor do que descartar.
                let mono = match canal {
                    Some(n) => frame
                        .get(n)
                        .map(|s| s.to_sample::<f32>())
                        .unwrap_or_else(|| media(frame)),
                    None => media(frame),
                };
                peak = peak.max(mono.abs());
                captura_do_callback.amostra(mono);
            }

            // As barras do medidor só andam durante a gravação. Fora dela o
            // microfone pode estar aberto (modo sempre aberto) e não há nada na
            // tela para desenhar — e o sinal `Nivel` do D-Bus, que sai daqui, é
            // fechado dos dois lados de propósito: nada de emitir com o ditado
            // parado (veja a armadilha no CLAUDE.md).
            if !captura_do_callback.gravando.load(Ordering::Relaxed) {
                return;
            }
            let mut lv = lock(&levels);
            if lv.len() >= LEVEL_HISTORY {
                lv.pop_front();
            }
            lv.push_back(peak);
        },
        move |err| {
            // `Xrun` é rotina: o cpal reprepara o dispositivo sozinho e a
            // gravação continua. Estes dois não — no ALSA o worker chama este
            // callback e faz `return`, então a thread do stream morre ali e
            // nenhuma amostra chega mais. Filtrar importa: marcar a bandeira
            // num Xrun encerraria a gravação a cada engasgo do sistema.
            if matches!(
                err.kind(),
                cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated
            ) {
                log::warn!("o microfone deixou de responder: {err}");
                captura_do_erro.perdido.store(true, Ordering::Relaxed);
            } else {
                log::warn!("erro no stream de entrada: {err}");
            }
        },
        None,
    )?;

    Ok(stream)
}

/// A média do quadro — a redução a mono de sempre.
fn media<T>(frame: &[T]) -> f32
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    if frame.is_empty() {
        return 0.0;
    }
    let soma: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
    soma / frame.len() as f32
}

fn clear(levels: &Levels) {
    lock(levels).clear();
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma captura com o anel do tamanho de `pre` amostras e teto de `max`.
    fn captura(pre: usize, max: usize) -> Captura {
        Captura::nova(pre, max)
    }

    #[test]
    fn a_pre_gravacao_entra_no_comeco_do_ditado() {
        // O que este modo existe para resolver: a primeira sílaba, que se
        // perdia enquanto o dispositivo abria. As amostras anteriores ao aperto
        // precisam aparecer **antes** das seguintes, e na ordem em que chegaram.
        let c = captura(4, 100);
        for v in [1.0, 2.0, 3.0] {
            c.amostra(v);
        }
        c.comecar();
        for v in [4.0, 5.0] {
            c.amostra(v);
        }
        assert_eq!(c.terminar(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn o_anel_da_pre_gravacao_guarda_so_o_mais_recente() {
        // Ele se sobrescreve sozinho: um microfone aberto o dia inteiro não pode
        // ir acumulando áudio na memória.
        let c = captura(3, 100);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            c.amostra(v);
        }
        c.comecar();
        assert_eq!(
            c.terminar(),
            vec![3.0, 4.0, 5.0],
            "o anel guardou mais do que a capacidade dele"
        );
    }

    #[test]
    fn o_anel_e_zerado_no_fim_de_cada_ditado() {
        // Sem isto o fim de uma frase reapareceria no começo da seguinte — e o
        // Whisper transcreveria as últimas palavras duas vezes.
        let c = captura(4, 100);
        c.comecar();
        for v in [1.0, 2.0, 3.0] {
            c.amostra(v);
        }
        assert_eq!(c.terminar(), vec![1.0, 2.0, 3.0]);

        c.comecar();
        c.amostra(9.0);
        assert_eq!(
            c.terminar(),
            vec![9.0],
            "sobrou áudio do ditado anterior no começo deste"
        );
    }

    #[test]
    fn sem_pre_gravacao_o_modo_sob_demanda_se_comporta_como_sempre() {
        // Anel de capacidade zero: o que chega antes do `comecar` é descartado,
        // que é literalmente o que acontecia quando o dispositivo nem estava
        // aberto.
        let c = captura(0, 100);
        for v in [1.0, 2.0] {
            c.amostra(v);
        }
        c.comecar();
        c.amostra(3.0);
        assert_eq!(c.terminar(), vec![3.0]);
    }

    #[test]
    fn o_teto_de_duracao_para_de_acumular_sem_derrubar_nada() {
        let c = captura(0, 3);
        c.comecar();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            c.amostra(v);
        }
        assert!(c.cheia(), "a ronda precisa saber que o teto foi batido");
        assert_eq!(c.terminar(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn cancelar_joga_o_audio_fora_e_limpa_o_anel() {
        let c = captura(4, 100);
        for v in [1.0, 2.0] {
            c.amostra(v);
        }
        c.comecar();
        c.amostra(3.0);
        // `terminar` é o mesmo caminho do cancelamento: o que ele devolve é
        // descartado pelo chamador, e o estado precisa ficar limpo.
        assert_eq!(c.terminar(), vec![1.0, 2.0, 3.0]);
        assert!(!c.gravando.load(Ordering::Relaxed));
        assert_eq!(c.quantas(), 0);

        c.comecar();
        assert_eq!(c.terminar(), Vec::<f32>::new(), "o cancelado voltou depois");
    }

    #[test]
    fn o_teto_novo_vale_sem_o_microfone_ser_reaberto() {
        // O defeito: no modo sempre aberto — o padrão desde a 0.7 — o
        // dispositivo fica de pé entre os ditados, e o teto de duração era lido
        // uma vez só, na abertura. Quem arrastasse o "Gravação máxima" das
        // configurações continuava sendo cortado no valor antigo até reiniciar o
        // programa, e nada na tela dizia isso. O `pede_reabertura` não inclui o
        // teto de propósito, então a única saída é ajustá-lo com o microfone
        // aberto.
        let c = captura(0, 3);
        c.comecar();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            c.amostra(v);
        }
        assert_eq!(c.terminar(), vec![1.0, 2.0, 3.0]);

        c.ajustar_o_teto(5);
        c.comecar();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            c.amostra(v);
        }
        assert!(
            c.cheia(),
            "a ronda não vai encerrar a gravação no teto novo"
        );
        assert_eq!(
            c.terminar(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            "o teto novo não valeu: o áudio foi cortado no antigo"
        );

        // E para baixo também, que é o caso de quem reduz o limite.
        c.ajustar_o_teto(2);
        c.comecar();
        for v in [1.0, 2.0, 3.0] {
            c.amostra(v);
        }
        assert_eq!(c.terminar(), vec![1.0, 2.0]);
    }

    #[test]
    fn o_teto_em_amostras_e_a_mesma_conta_nos_dois_lugares() {
        // Ela é feita ao abrir o dispositivo e a cada `Configure`; duas cópias
        // dela é o começo de os dois discordarem.
        assert_eq!(teto_em_amostras(120, 16_000), 1_920_000);
        assert_eq!(teto_em_amostras(1, 48_000), 48_000);
        // Zero segundos vem de um `config.json` editado à mão. Um teto de zero
        // amostras faria toda gravação nascer cheia e ser encerrada na primeira
        // ronda — um segundo é o piso.
        assert_eq!(teto_em_amostras(0, 16_000), 16_000);
    }

    #[test]
    fn o_buffer_de_cada_ditado_nasce_com_espaco_para_o_teto_inteiro() {
        // O `terminar` leva a alocação embora junto com as amostras, que é o
        // que evita copiar megabytes para entregá-las. Do segundo ditado em
        // diante o buffer voltava a nascer com capacidade zero e crescia dentro
        // do callback de áudio — um `realloc` com cópia de tudo em tempo real,
        // que é exatamente o que o `with_capacity` do `nova` existia para
        // impedir e que ninguém percebia porque o primeiro ditado ia bem.
        let c = captura(0, 1_000);
        c.comecar();
        c.amostra(1.0);
        let _ = c.terminar();

        c.comecar();
        assert!(
            lock(&c.buffer).capacity() >= 1_000,
            "o buffer do segundo ditado vai crescer dentro do callback de áudio"
        );
    }

    #[test]
    fn o_canal_escolhido_e_conferido_contra_o_dispositivo() {
        // Quem escolheu a entrada 4 de uma interface e depois plugou o headset
        // do notebook precisa continuar conseguindo falar.
        assert_eq!(canal_valido(Some(0), 2), Some(0));
        assert_eq!(canal_valido(Some(1), 2), Some(1));
        assert_eq!(
            canal_valido(Some(2), 2),
            None,
            "aceitou um canal que não existe"
        );
        assert_eq!(canal_valido(Some(0), 1), Some(0));
        assert_eq!(canal_valido(None, 8), None);
        // Dispositivo que se anuncia sem canal nenhum não pode virar um índice
        // fora da faixa lá dentro do callback.
        assert_eq!(canal_valido(Some(0), 0), None);
    }

    #[test]
    fn a_mistura_e_a_media_do_quadro() {
        assert_eq!(media::<f32>(&[]), 0.0);
        assert_eq!(media(&[1.0f32]), 1.0);
        assert_eq!(media(&[1.0f32, 0.0]), 0.5);
        assert_eq!(media(&[1.0f32, -1.0]), 0.0);
    }

    #[test]
    fn so_o_dispositivo_e_o_canal_pedem_reabertura() {
        let base = AudioSettings {
            device: None,
            max_secs: 120,
            sempre_aberto: true,
            canal: None,
        };
        // O teto de duração só dimensiona o buffer: reabrir por causa dele
        // fecharia o microfone de quem mexeu num deslizante sem relação com o
        // aparelho.
        assert!(!base.pede_reabertura(&AudioSettings {
            max_secs: 60,
            ..base.clone()
        }));
        assert!(base.pede_reabertura(&AudioSettings {
            device: Some("outro".into()),
            ..base.clone()
        }));
        assert!(base.pede_reabertura(&AudioSettings {
            canal: Some(1),
            ..base.clone()
        }));
    }

    #[test]
    fn o_melhor_formato_de_amostra_ganha_do_pior() {
        // Esta ordem existe porque o ALSA anuncia U8 junto com formatos bons, e
        // pegar o primeiro da lista jogaria a gravação para 8 bits sem
        // necessidade — algo que ninguém percebe até ouvir o resultado.
        let mut oferta = [
            cpal::SampleFormat::U8,
            cpal::SampleFormat::I16,
            cpal::SampleFormat::F32,
            cpal::SampleFormat::I8,
        ];
        oferta.sort_by_key(|f| format_rank(*f));
        assert_eq!(oferta[0], cpal::SampleFormat::F32);
        assert_eq!(oferta[oferta.len() - 1], cpal::SampleFormat::U8);
        // Um formato que não conhecemos vai para o fim da fila, nunca para o
        // começo.
        assert!(format_rank(cpal::SampleFormat::F64) >= format_rank(cpal::SampleFormat::U8));
    }
}
