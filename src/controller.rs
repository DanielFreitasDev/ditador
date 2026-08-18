//! Orquestra atalho → gravação → transcrição → resultado.

use crate::audio::{AudioCmd, AudioEvent, AudioHandle, AudioSettings};
use crate::clipboard;
use crate::config::{Config, MetodoDeColagem, TeclaDeEnvio};
use crate::hotkey::{HotkeyEvent, HotkeyListener};
use crate::sons::{self, Som};
use crate::state::{ModelState, QualAtalho, SharedState, Sinal, UiAction, View, lock};
use crate::stt::{SttCmd, SttEvent, TranscribeOptions};
use crossbeam_channel::{Receiver, select};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Comandos vindos de fora do programa: o socket (ícone do app, atalho do
/// GNOME, terminal) e o barramento D-Bus (a extensão do GNOME Shell).
#[derive(Debug, Clone, Copy)]
pub enum IpcCommand {
    /// Alterna gravar/parar — útil quando não se pode segurar a tecla.
    Toggle,
    /// Começa a gravar, se já não estiver gravando.
    ///
    /// O `Toggle` sozinho não basta para quem chama de fora conhecendo o estado:
    /// entre ler "pronto" e mandar alternar cabe um ditado inteiro pelo atalho, e
    /// aí o comando pararia a gravação em vez de começá-la. Pedir o resultado
    /// desejado em vez da troca tira essa corrida do caminho.
    Start,
    /// Para de gravar e manda transcrever, se estiver gravando.
    Stop,
    /// Descarta a gravação em curso sem transcrever.
    Cancel,
    Settings,
    /// Abre a lista das transcrições guardadas.
    Historico,
    Quit,
}

pub struct Channels {
    pub hotkey: Receiver<HotkeyEvent>,
    pub audio: Receiver<AudioEvent>,
    pub stt: Receiver<SttEvent>,
    pub ui: Receiver<UiAction>,
    pub ipc: Receiver<IpcCommand>,
}

pub struct Controller {
    pub shared: SharedState,
    pub sinal: Sinal,
    pub audio: AudioHandle,
    pub stt: crossbeam_channel::Sender<SttCmd>,
    pub hotkey: Arc<HotkeyListener>,
    /// O que o histórico precisa saber sobre o ditado que está sendo
    /// transcrito, e que só o evento do áudio conhece.
    ///
    /// Guarda um ditado só — falar de novo enquanto a frase anterior é
    /// transcrita sobrescreve o anterior, e aí aquela entrada fica sem áudio. O
    /// texto, que é o que importa, nunca se perde por isso.
    para_o_historico: std::sync::Mutex<Option<AudioGuardado>>,
}

/// O que espera a transcrição terminar para ser guardado com ela.
///
/// A duração vem sempre; as amostras, só quando o histórico foi configurado para
/// guardar o áudio. A separação é o ponto: a cópia das amostras é de megabytes e
/// só existe quando alguém pediu, mas a duração é um `u64` — e ela é a informação
/// que a lista de transcrições mostra ao lado de cada frase.
///
/// Isto já esteve junto, e o defeito só apareceu num ditado de verdade: com o
/// áudio desligado (que é o padrão) **nada** era guardado, então a duração ficava
/// em zero e a lista nunca a mostrava.
struct AudioGuardado {
    ditado: u64,
    duracao_ms: u64,
    amostras: Option<Vec<f32>>,
    taxa: u32,
}

impl Controller {
    /// Monta o controlador. O campo do áudio guardado nasce vazio e não faz
    /// parte da configuração de ninguém — por isso ele não vai na struct
    /// literal de quem constrói.
    pub fn novo(
        shared: SharedState,
        sinal: Sinal,
        audio: AudioHandle,
        stt: crossbeam_channel::Sender<SttCmd>,
        hotkey: Arc<HotkeyListener>,
    ) -> Self {
        Self {
            shared,
            sinal,
            audio,
            stt,
            hotkey,
            para_o_historico: std::sync::Mutex::new(None),
        }
    }

    pub fn run(self, channels: Channels) {
        self.apply_audio_settings();
        self.load_model();

        loop {
            select! {
                recv(channels.hotkey) -> msg => match msg {
                    Ok(event) => self.on_hotkey(event),
                    Err(_) => return self.canal_caiu("atalho"),
                },
                recv(channels.audio) -> msg => match msg {
                    Ok(event) => self.on_audio(event),
                    Err(_) => return self.canal_caiu("áudio"),
                },
                recv(channels.stt) -> msg => match msg {
                    Ok(event) => self.on_stt(event),
                    Err(_) => return self.canal_caiu("transcrição"),
                },
                recv(channels.ui) -> msg => match msg {
                    Ok(action) => self.on_ui(action),
                    Err(_) => return,
                },
                recv(channels.ipc) -> msg => match msg {
                    Ok(command) => self.on_ipc(command),
                    Err(_) => return self.canal_caiu("controle"),
                },
            }
        }
    }

    /// Uma thread de trabalho morreu e levou o canal dela junto.
    ///
    /// Antes isto era um `return` mudo, e o preço era um programa zumbi: o
    /// controlador saía de fininho, a janela ficava em "Transcrevendo…" para
    /// sempre, o atalho e todos os botões paravam de ter efeito, `--encerrar`
    /// respondia "encerrando" sem que nada acontecesse — e o systemd não
    /// reiniciava, porque o processo continuava vivo. Só um `kill` resolvia.
    ///
    /// Encerrar de verdade é melhor: a interface fecha ao ver o `quitting`, o
    /// processo termina, e quem estiver sob o serviço de usuário sobe de novo
    /// inteiro em vez de ficar meio morto.
    fn canal_caiu(&self, qual: &str) {
        log::error!("o canal de {qual} fechou — a thread dele morreu; encerrando");
        let mut state = lock(&self.shared);
        state.quitting = true;
        drop(state);
        self.sinal.mudou();
    }

    // ---------------------------------------------------------------- eventos

    fn on_hotkey(&self, event: HotkeyEvent) {
        match event {
            HotkeyEvent::Down => self.start_recording(),
            HotkeyEvent::Up => self.stop_recording(),
            HotkeyEvent::Cancelar => self.cancelar_gravacao(),

            HotkeyEvent::Captured(keys) => {
                let mut state = lock(&self.shared);
                // Para onde a combinação vai depende de qual botão abriu a
                // captura. Um booleano não sabia disso, e o palpite mais óbvio
                // — o atalho de ditar, que é o primeiro da tela — trocaria o
                // errado quando a pessoa estivesse escolhendo o de cancelar.
                let qual = state.capturando.take();
                if !keys.is_empty() {
                    match qual {
                        Some(QualAtalho::Ditar) => state.draft.hotkey = keys,
                        Some(QualAtalho::Cancelar) => state.draft.atalho_de_cancelar = keys,
                        // A captura foi cancelada por outro caminho enquanto a
                        // combinação estava a caminho. Descartar é o certo:
                        // gravar num campo que ninguém escolheu é pior do que
                        // não gravar.
                        None => {}
                    }
                }
                drop(state);
                self.sinal.mudou();
            }

            // O aviso do teclado tem campo próprio e não toma a tela.
            //
            // Tomando `message` e `view` ele disputava o único campo de texto
            // com o aviso do modelo faltando — e os dois nascem no mesmo
            // instante do arranque de uma instalação nova, então o perdedor
            // sumia. Pior: o vencedor era apagado na primeira ação seguinte, e
            // a pessoa ficava achando que o atalho tinha quebrado sozinho.
            HotkeyEvent::Unavailable(message) => {
                log::warn!("atalho global indisponível: {message}");
                let mut state = lock(&self.shared);
                state.aviso_atalho = Some(message);
                drop(state);
                self.sinal.mudou();
            }

            HotkeyEvent::Available => {
                let mut state = lock(&self.shared);
                if state.aviso_atalho.take().is_none() {
                    return;
                }
                drop(state);
                log::info!("atalho global de volta ao ar");
                self.sinal.mudou();
            }
        }
    }

    fn on_audio(&self, event: AudioEvent) {
        match event {
            AudioEvent::Started => {}

            AudioEvent::Failed { ditado, message } => {
                log::warn!("gravação {ditado} falhou: {message}");
                let mut state = lock(&self.shared);
                // Um ditado que já foi substituído por outro não tem mais tela
                // para reclamar.
                if ditado != state.ditado_atual {
                    return;
                }
                // O microfone fechou, e isso vale mesmo quando a tela pertence a
                // outra coisa: é o `recording_since` que responde por "estamos
                // ouvindo", e deixá-lo de pé prenderia o programa numa gravação
                // que já não existe.
                state.recording_since = None;
                // A tela de configurações é intocável, pelo mesmo motivo que já
                // vale para a falha da transcrição e para o fim da gravação:
                // reabri-la refaz o rascunho, então uma tela de erro por cima
                // dela apaga o que a pessoa estava digitando. O relato não se
                // perde — ele está no journal, na linha acima, e o aviso sonoro
                // abaixo continua tocando.
                if state.view != View::Settings {
                    state.message = format!("Não consegui acessar o microfone: {message}");
                    state.erro_e_so_espera = false;
                    state.view = View::Error;
                }
                let aviso = state.config.sons;
                drop(state);
                // O aviso sonoro importa mais aqui do que em qualquer outro
                // lugar: quem dita com a janela desligada não veria esta tela
                // aparecer, e ficaria falando para um microfone que não abriu.
                if aviso.ativo {
                    sons::tocar(Som::Falha, aviso.volume);
                }
                self.sinal.mudou();
            }

            AudioEvent::Captured {
                ditado,
                samples,
                sample_rate,
                duration_ms,
            } => {
                let (atual, too_short, options, historico) = {
                    let mut state = lock(&self.shared);
                    // Só o ditado mais novo manda na tela e no cronômetro. O
                    // áudio de um anterior ainda é transcrito — ele existe e é
                    // do usuário —, mas em silêncio, por baixo do que estiver
                    // acontecendo agora.
                    let atual = ditado == state.ditado_atual;
                    if atual {
                        // Também fecha a gravação encerrada pelo teto de
                        // duração, que termina sem passar pelo atalho.
                        state.recording_since = None;
                        state.status = format!("{:.1} s de áudio", duration_ms as f64 / 1000.0);
                        // A tela de configurações é intocável: refazer o
                        // rascunho ao reabri-la apagaria o que a pessoa estava
                        // escrevendo neste instante.
                        if state.view != View::Settings {
                            state.view = View::Processing;
                        }
                    }
                    (
                        atual,
                        duration_ms < state.config.min_recording_ms,
                        TranscribeOptions {
                            language: state.config.whisper_language().map(str::to_string),
                            translate: state.config.translate,
                            threads: state.config.threads,
                            initial_prompt: state.config.initial_prompt.clone(),
                            normalize: state.config.normalize_audio,
                            aparar_silencio: state.config.aparar_silencio,
                        },
                        state.config.historico,
                    )
                };

                if too_short {
                    log::debug!("gravação de {duration_ms} ms descartada (curta demais)");
                    if atual {
                        self.voltar_ao_repouso();
                    }
                    return;
                }

                // O que o histórico precisa e só este evento conhece: a duração
                // sempre, e as amostras só quando a chave do áudio está ligada —
                // porque aquela cópia é de megabytes.
                //
                // **Depois** do descarte por duração mínima, e não antes. O
                // guardado é um só, e quem chega depois toma o lugar de quem
                // estava lá: um toque na tecla curto demais para valer nunca
                // chega a ser transcrito, então ele levava embora a duração e o
                // áudio da frase que ainda estava no Whisper para não usá-los
                // para nada — e aquela entrada aparecia na lista sem duração e
                // sem gravação. De quebra, a cópia de megabytes deixa de ser
                // feita para um áudio que vai direto para o lixo.
                if historico.ativo {
                    *self
                        .para_o_historico
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(AudioGuardado {
                        ditado,
                        duracao_ms: duration_ms,
                        amostras: historico.guardar_audio.then(|| samples.clone()),
                        taxa: sample_rate,
                    });
                }

                self.sinal.mudou();
                let _ = self.stt.send(SttCmd::Transcribe {
                    ditado,
                    samples,
                    sample_rate,
                    options,
                });
            }
        }
    }

    fn on_stt(&self, event: SttEvent) {
        match event {
            SttEvent::Loading => {
                let mut state = lock(&self.shared);
                state.model = ModelState::Loading;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Ready => {
                let mut state = lock(&self.shared);
                state.model = ModelState::Ready;
                // A tela de erro que era só espera se recolhe sozinha. Quem
                // decide é o campo, e não mais o texto exibido: comparar o
                // prefixo da mensagem deixava a janela presa depois do
                // download, porque aquele caminho escrevia outra frase.
                if state.view == View::Error && state.erro_e_so_espera {
                    state.view = View::Hidden;
                    state.message.clear();
                }
                state.erro_e_so_espera = false;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Descarregado => {
                // Não mexe no `model`: para quem usa o programa nada mudou —
                // o atalho continua valendo e a bandeja continua dizendo
                // "pronto". Pôr `ModelState::Loading` aqui faria o
                // `start_recording` recusar gravar, que é o contrário do que
                // este recurso promete.
                let mut state = lock(&self.shared);
                state.status = "Modelo descarregado para liberar memória".to_string();
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::LoadFailed(message) => {
                log::error!("o modelo não carregou: {message}");
                let mut state = lock(&self.shared);
                state.model = ModelState::Failed;
                state.message = message;
                state.erro_e_so_espera = false;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Failed { ditado, message } => {
                log::warn!("transcrição {ditado} falhou: {message}");
                let mut state = lock(&self.shared);
                // Mesmo critério do áudio, que faltava aqui: um ditado já
                // substituído não tem mais tela para reclamar, e quem está
                // falando agora manda na janela. Sem isto, uma transcrição que
                // falhasse enquanto a pessoa dita de novo abria a janela de
                // erro sempre-no-topo por cima de quem ainda estava falando.
                // O relato não se perde — ele está no journal, na linha acima.
                if ditado != state.ditado_atual || state.gravando() || state.view == View::Settings
                {
                    return;
                }
                state.message = message;
                state.erro_e_so_espera = false;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Done {
                ditado,
                text,
                elapsed_ms,
            } => self.on_transcription(ditado, text, elapsed_ms),
        }
    }

    /// Mostrar ou não a janela com o texto, ao terminar de transcrever.
    ///
    /// `a_salvo` é o que manda: só dá para pular a janela se o texto tiver
    /// chegado mesmo à área de transferência, senão a transcrição sumiria sem
    /// ninguém ter visto. Com colagem automática ela nunca aparece — o texto já
    /// foi parar onde o usuário estava digitando; fora isso, quem decide é o
    /// `show_result`, para quem já tem a cópia automática e não quer nada na
    /// frente.
    fn tela_do_resultado(a_salvo: bool, auto_paste: bool, show_result: bool) -> View {
        if a_salvo && (auto_paste || !show_result) {
            View::Hidden
        } else {
            View::Result
        }
    }

    /// Se o resultado pode tomar a janela agora.
    ///
    /// Falar de novo enquanto a frase anterior é transcrita é o uso normal do
    /// programa, e quem está gravando é quem manda na tela: a janela do texto
    /// anterior não pode aparecer por cima de um ditado em andamento. Mexer nas
    /// configurações conta igual — reabrir aquela tela refaz o rascunho, então
    /// uma janela de resultado por cima dela apaga o que a pessoa digitou.
    ///
    /// A exceção, nos dois casos, é a janela ser o único jeito de o texto não se
    /// perder — `tela_do_resultado` devolvendo `Result` —, aí ela aparece assim
    /// mesmo, e o microfone segue aberto por baixo.
    fn resultado_pode_aparecer(tela: View, ocupada: bool) -> bool {
        !ocupada || tela == View::Result
    }

    fn on_transcription(&self, ditado: u64, text: String, elapsed_ms: u128) {
        let config = lock(&self.shared).config.clone();
        let (auto_copy, auto_paste, show_result) =
            (config.auto_copy, config.auto_paste, config.show_result);

        // O texto é acertado **uma vez**, aqui, antes de qualquer coisa
        // consumi-lo. Assim a janela, a área de transferência, a colagem e o
        // histórico veem todos exatamente o mesmo texto — o que se lê é o que
        // se cola é o que fica guardado. Deixar o espaço do fim só para a
        // colagem, por exemplo, faria o botão "Copiar" da janela entregar uma
        // coisa e a colagem automática outra.
        let text = crate::dicionario::corrigir(&text, &config.dicionario);
        let text = if config.espaco_no_fim && !text.is_empty() {
            format!("{text} ")
        } else {
            text
        };

        // Guardar vem antes de entregar, e de propósito: se alguma coisa der
        // errado da entrega para frente — a área de transferência recusando, a
        // colagem caindo na janela errada — o texto já está a salvo. É a razão
        // de este módulo existir.
        if !text.is_empty() {
            let guardado = self.do_ditado(ditado);
            crate::historico::registrar(
                &config.historico,
                &text,
                guardado.as_ref().map_or(0, |g| g.duracao_ms),
                guardado
                    .as_ref()
                    .and_then(|g| g.amostras.as_ref().map(|a| (a.as_slice(), g.taxa))),
            );
        }

        // Com `Digitar`, colar não passa pela área de transferência — então
        // copiar só acontece se a cópia automática tiver sido pedida por si
        // mesma. É assim que "não quero que o Ditador mexa no que eu copiei"
        // funciona sem uma chave a mais para explicar.
        let vai_para_a_area =
            auto_copy || (auto_paste && config.metodo_de_colagem.usa_a_area_de_transferencia());

        let mut copy_error = None;
        if !text.is_empty()
            && vai_para_a_area
            && let Err(e) = clipboard::copy(&text)
        {
            log::warn!("não consegui copiar o ditado {ditado}: {e:#}");
            copy_error = Some(format!("{e:#}"));
        }

        {
            let mut state = lock(&self.shared);
            state.status = format!(
                "{:.1} s · {} · {}",
                elapsed_ms as f64 / 1000.0,
                crate::stt::BACKEND,
                if state.config.use_gpu && crate::stt::GPU_CAPABLE {
                    "GPU"
                } else {
                    "CPU"
                }
            );

            // "Ocupada" é quem grava agora e também quem está no meio das
            // configurações: nos dois casos a janela pertence a outra coisa.
            let ocupada = state.gravando() || state.view == View::Settings;
            if text.is_empty() {
                state.text.clear();
                state.message = "Não identifiquei fala no áudio.".to_string();
                state.erro_e_so_espera = false;
                if !ocupada {
                    state.view = View::Error;
                }
            } else {
                state.text = text.clone();
                state.message = copy_error.clone().unwrap_or_default();
                let a_salvo = (auto_copy || auto_paste) && copy_error.is_none();
                // "Copiado" só quando houve cópia: com `Digitar` e a cópia
                // automática desligada nada foi para a área de transferência, e
                // o aviso verde estaria mentindo.
                state.copied_at = (vai_para_a_area && copy_error.is_none()).then(Instant::now);
                let tela = Self::tela_do_resultado(a_salvo, auto_paste, show_result);
                if Self::resultado_pode_aparecer(tela, ocupada) {
                    state.view = tela;
                }
                state.result_shown_at = Some(Instant::now());
            }
        }
        self.sinal.mudou();

        // O aviso sonoro de fim toca aqui, e não quando a tecla é solta: o que
        // se quer saber é que **o texto está pronto**, não que a gravação
        // acabou — quem soltou a tecla já sabe que soltou. Nos modos sem janela
        // este é o único sinal de que dá para colar.
        if config.sons.ativo {
            let aviso = if text.is_empty() {
                Som::Falha
            } else {
                Som::Fim
            };
            sons::tocar(aviso, config.sons.volume);
        }

        if auto_paste && copy_error.is_none() && !text.is_empty() {
            self.colar_depois(&text, config.metodo_de_colagem, config.tecla_de_envio);
        }
    }

    /// O que ficou guardado para este ditado, se for mesmo o dele.
    ///
    /// Falar de novo enquanto a frase anterior é transcrita sobrescreve o
    /// guardado, e aí a entrada anterior fica sem duração e sem áudio. É a
    /// decisão certa: o contrário seria uma fila de buffers de megabytes
    /// crescendo sem teto para resolver um caso raro de um recurso opcional.
    fn do_ditado(&self, ditado: u64) -> Option<AudioGuardado> {
        let mut guardado = self
            .para_o_historico
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match guardado.as_ref() {
            Some(a) if a.ditado == ditado => guardado.take(),
            _ => None,
        }
    }

    /// Espera antes de mandar o Ctrl+V.
    ///
    /// A nossa janela precisa sumir primeiro, senão o foco ainda é dela e a
    /// colagem cai no nada — ou, pior, dentro do próprio campo de resultado.
    const ESPERA_ANTES_DE_COLAR: Duration = Duration::from_millis(250);

    /// Espera entre colar e apertar a tecla de envio.
    ///
    /// O programa de destino precisa ter processado o texto antes de receber o
    /// Enter, senão a mensagem é enviada vazia — ou pior, pela metade. Cinquenta
    /// milissegundos são suficientes para um campo de chat comum e curtos o
    /// bastante para ninguém perceber.
    const ESPERA_ANTES_DE_ENVIAR: Duration = Duration::from_millis(50);

    /// Entrega o texto à janela em foco, numa thread à parte.
    ///
    /// Só é chamada depois de uma cópia bem-sucedida — ou, no método `Digitar`,
    /// sem cópia nenhuma, que é o ponto dele. Esse é o contrato dos três métodos
    /// que usam a área de transferência: colar sem ter copiado joga na janela do
    /// usuário o conteúdo anterior dela, que é dele e não tem nada a ver com o
    /// que ele acabou de falar.
    fn colar_depois(&self, texto: &str, metodo: MetodoDeColagem, envio: TeclaDeEnvio) {
        let shared = self.shared.clone();
        let sinal = self.sinal.clone();
        let texto = texto.to_string();
        let _ = std::thread::Builder::new()
            .name("colar".into())
            .spawn(move || {
                std::thread::sleep(Self::ESPERA_ANTES_DE_COLAR);
                if let Err(e) = clipboard::paste(metodo, &texto) {
                    Self::contar_a_falha_na_colagem(&shared, &sinal, metodo, &format!("{e:#}"));
                    return;
                }

                if envio == TeclaDeEnvio::Nenhuma {
                    return;
                }
                std::thread::sleep(Self::ESPERA_ANTES_DE_ENVIAR);
                if let Err(e) = clipboard::submit(envio) {
                    // Uma falha aqui não estraga nada: o texto **já está** na
                    // janela de destino, e o que faltou foi apertar Enter, que a
                    // pessoa faz com a mão. Por isso vira log e não janela — que
                    // roubaria o foco justamente do campo onde o texto acabou de
                    // chegar.
                    log::warn!("colei, mas não consegui apertar a tecla de envio: {e:#}");
                }
            });
    }

    /// Conta na tela que a entrega do texto não deu certo.
    ///
    /// A tela só é tomada quando ela está livre, e "livre" é o mesmo de sempre:
    /// ninguém gravando e ninguém nas configurações. A colagem acontece um
    /// quarto de segundo **depois** de o texto ficar pronto, e nesse intervalo
    /// cabe um ditado novo inteiro — tomando a janela assim mesmo, o que
    /// aparecia por cima de quem está falando (sempre-no-topo, como todas as
    /// nossas) era um campo de texto **vazio**, porque o `start_recording` já
    /// tinha limpado o `text`. Nas configurações o preço é outro e igualmente
    /// ruim: o rascunho digitado se perde ao reabrir.
    ///
    /// Ocupada a janela, o relato não se perde — ele está no journal, na linha
    /// que esta função escreve antes de tocar em qualquer coisa. É o mesmo
    /// desfecho que o `SttEvent::Failed` já escolhia.
    fn contar_a_falha_na_colagem(
        shared: &SharedState,
        sinal: &Sinal,
        metodo: MetodoDeColagem,
        erro: &str,
    ) {
        log::warn!("a colagem falhou: {erro}");
        {
            let mut state = lock(shared);
            if state.gravando() || state.view == View::Settings {
                return;
            }
            // A frase muda com o método: dizer "copiei, mas não consegui colar"
            // depois de uma digitação seria mentira — ali nada foi copiado, e
            // mandar a pessoa apertar Ctrl+V colaria o que ela tinha copiado
            // antes.
            state.message = if metodo.usa_a_area_de_transferencia() {
                format!("Copiei, mas não consegui colar: {erro}")
            } else {
                format!("Não consegui digitar o texto: {erro}")
            };
            state.erro_e_so_espera = false;
            state.view = View::Result;
        }
        sinal.mudou();
    }

    fn on_ui(&self, action: UiAction) {
        match action {
            UiAction::Hide => self.voltar_ao_repouso(),

            UiAction::Copy => self.copy_current(false),
            UiAction::Paste => self.copy_current(true),

            // Abrir as configurações fecha o microfone primeiro.
            //
            // Deixar a gravação correndo por baixo terminava mal dos dois
            // lados: o `Captured` que chega no meio troca a tela e o rascunho
            // digitado se perde ao reabrir; e, se a tela segurasse a troca, o
            // microfone ficaria aberto sem nada na tela dizendo isso, até o
            // teto de duração estourar. O que a pessoa já falou não se perde —
            // `stop_recording` entrega o áudio e ele segue para a transcrição.
            UiAction::OpenSettings => {
                if lock(&self.shared).gravando() {
                    self.stop_recording();
                }
                // A enumeração dos dispositivos abre o ALSA e é a parte cara
                // daqui; fica fora do mutex para não prender a interface e o
                // controlador enquanto isso acontece.
                let devices = crate::audio::list_input_devices();
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.devices = devices;
                state.view = View::Settings;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CloseSettings => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.capturando = None;
                let repouso = tela_de_repouso(&state);
                state.view = repouso;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::ApplyDraft => self.apply_draft(),

            UiAction::StartHotkeyCapture(qual) => {
                self.hotkey.begin_capture();
                let mut state = lock(&self.shared);
                state.capturando = Some(qual);
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CancelHotkeyCapture => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.capturando = None;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::DefinirAtalho(qual, teclas) => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.capturando = None;
                match qual {
                    QualAtalho::Ditar => state.draft.hotkey = teclas,
                    QualAtalho::Cancelar => state.draft.atalho_de_cancelar = teclas,
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::AbrirHistorico => self.abrir_historico(),

            UiAction::FecharHistorico => self.voltar_ao_repouso(),

            UiAction::CopiarOEnderecoDaVersao => {
                // A cópia acontece aqui, e não na interface, pelo mesmo motivo
                // que o `CopiarDoHistorico`: no Linux ela chama o `wl-copy`, que
                // é um processo, e um processo dentro do desenho da tela é um
                // quadro perdido — no meio de uma janela que a pessoa está
                // usando. Aqui é a thread do controlador, que não desenha nada.
                let endereco = lock(&self.shared)
                    .versao_nova
                    .as_ref()
                    .map(|n| n.endereco.clone());
                let Some(endereco) = endereco else { return };
                let mut state = lock(&self.shared);
                match clipboard::copy(&endereco) {
                    Ok(()) => {
                        state.copied_at = Some(Instant::now());
                        state.message.clear();
                    }
                    Err(e) => {
                        log::warn!("não consegui copiar o endereço da versão: {e:#}");
                        state.message = format!("Não consegui copiar: {e:#}");
                    }
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CopiarDoHistorico(indice) => {
                let texto = lock(&self.shared)
                    .historico
                    .get(indice)
                    .map(|e| e.texto.clone());
                let Some(texto) = texto else { return };
                let mut state = lock(&self.shared);
                match clipboard::copy(&texto) {
                    Ok(()) => {
                        state.copied_at = Some(Instant::now());
                        state.message.clear();
                        // O texto vai para o campo do resultado também: quem
                        // recuperou uma transcrição antiga em geral quer
                        // colá-la, e a tela de resultado é onde estão os botões
                        // que fazem isso.
                        state.text = texto;
                    }
                    Err(e) => {
                        log::warn!("não consegui copiar do histórico: {e:#}");
                        state.message = format!("Não consegui copiar: {e:#}");
                    }
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::LimparHistorico => {
                let mut state = lock(&self.shared);
                match crate::historico::limpar() {
                    Ok(()) => {
                        state.historico.clear();
                        state.historico_em_disco = 0;
                        state.message.clear();
                    }
                    Err(e) => {
                        log::warn!("não consegui apagar o histórico: {e:#}");
                        state.message = format!("Não consegui apagar o histórico: {e:#}");
                    }
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::Cancelar => self.cancelar_gravacao(),

            UiAction::ReloadModel => self.load_model(),

            UiAction::DownloadModel(nome) => self.download_model(&nome),

            UiAction::CancelDownload => {
                let state = lock(&self.shared);
                if let Some(andamento) = &state.download {
                    andamento
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .cancelado = true;
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::Quit => {
                let mut state = lock(&self.shared);
                state.quitting = true;
                drop(state);
                self.sinal.mudou();
            }
        }
    }

    fn on_ipc(&self, command: IpcCommand) {
        match command {
            IpcCommand::Toggle => {
                let recording = lock(&self.shared).recording_since.is_some();
                if recording {
                    self.stop_recording();
                } else {
                    self.start_recording();
                }
            }
            // As duas guardas de sempre valem aqui sem uma linha a mais:
            // `start_recording` desiste sozinho se o microfone já estiver aberto
            // e `stop_recording` se não estiver. A regra continua num lugar só.
            IpcCommand::Start => self.start_recording(),
            IpcCommand::Stop => self.stop_recording(),
            IpcCommand::Cancel => self.cancelar_gravacao(),
            IpcCommand::Settings => self.on_ui(UiAction::OpenSettings),
            IpcCommand::Historico => self.on_ui(UiAction::AbrirHistorico),
            IpcCommand::Quit => self.on_ui(UiAction::Quit),
        }
    }

    /// Carrega o histórico do disco e mostra a lista.
    ///
    /// A leitura acontece **fora** do mutex, como a enumeração dos microfones em
    /// `OpenSettings` e pelo mesmo motivo: ela toca o disco, e segurar o estado
    /// compartilhado durante uma chamada de sistema prende a interface junto.
    fn abrir_historico(&self) {
        // A captura de atalho não sobrevive à tela que a explica. "Ver as
        // transcrições" é um botão de dentro das configurações, e a bandeja abre
        // a mesma lista de qualquer tela: saindo com a captura de pé, o ouvinte
        // continua esperando uma combinação, o atalho de ditar deixa de ditar, e
        // o aperto seguinte vira um rascunho que ninguém vai salvar. É a mesma
        // limpeza que o `CloseSettings` faz.
        self.hotkey.cancel_capture();
        let entradas = crate::historico::ler_recentes(500);
        let em_disco = crate::historico::tamanho_em_disco();
        let mut state = lock(&self.shared);
        state.capturando = None;
        state.historico = entradas;
        state.historico_em_disco = em_disco;
        state.view = View::Historico;
        drop(state);
        self.sinal.mudou();
    }

    // ------------------------------------------------------------- gravação

    fn start_recording(&self) {
        let (ditado, sons_ligados, volume) = {
            let mut state = lock(&self.shared);

            // Não atrapalha quem está mexendo nas configurações — nem quem está
            // escolhendo um atalho, nem quem está lendo o histórico, que é uma
            // tela de leitura e não teria como voltar depois.
            if state.view == View::Settings
                || state.view == View::Historico
                || state.capturando.is_some()
            {
                return;
            }
            // Quem responde por "já estamos ouvindo" é o `recording_since`, e
            // não a tela: a janela do ditado anterior pode ter aparecido por
            // cima. Transcrever o anterior, por outro lado, não impede nada —
            // isso acontece numa thread só dele.
            if state.recording_since.is_some() {
                return;
            }

            match state.model {
                ModelState::Loading => {
                    state.message = "Carregando o modelo, só um instante…".to_string();
                    // Espera, não falha: a tela se recolhe sozinha assim que o
                    // modelo ficar pronto.
                    state.erro_e_so_espera = true;
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Failed => {
                    state.erro_e_so_espera = false;
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Ready => {}
            }

            state.text.clear();
            state.message.clear();
            state.erro_e_so_espera = false;
            state.copied_at = None;
            state.recording_since = Some(Instant::now());
            state.view = View::Recording;
            state.ditado_atual += 1;
            (
                state.ditado_atual,
                state.config.sons.ativo,
                state.config.sons.volume,
            )
        };
        // "Vou precisar do modelo" — mandado aqui, no começo da gravação, e não
        // no fim dela. Com o descarregamento por ociosidade ligado, é isto que
        // transforma a espera pela recarga em tempo que a pessoa passa falando:
        // o modelo volta para a memória enquanto a frase é dita, e não depois.
        // Com o modelo já carregado, só adia o próximo descarregamento — o que
        // é o certo, porque quem está gravando vai transcrever em seguida.
        let _ = self.stt.send(SttCmd::Aquecer);
        self.audio.send(AudioCmd::Start { ditado });
        // O aviso de início é o par do de fim (que toca quando o texto fica
        // pronto): juntos, eles cercam o intervalo em que falar adianta. É a
        // única confirmação de que o atalho pegou para quem dita com a janela
        // desligada ou com a extensão do GNOME no ar.
        if sons_ligados {
            sons::tocar(Som::Inicio, volume);
        }
        self.sinal.mudou();
    }

    fn stop_recording(&self) {
        {
            let mut state = lock(&self.shared);
            // De novo o `recording_since` no lugar da tela: olhando a tela, um
            // resultado que tivesse aparecido no meio do ditado faria este
            // `return` acontecer e o microfone ficaria aberto para sempre.
            if state.recording_since.is_none() {
                return;
            }
            state.recording_since = None;
            // As configurações continuam de pé: quem parou a gravação para
            // abri-las não quer a tela trocada por baixo do que está digitando.
            if state.view != View::Settings {
                state.view = View::Processing;
            }
        }
        self.audio.send(AudioCmd::Stop);
        self.sinal.mudou();
    }

    /// Descarta a gravação em curso sem transcrever nada.
    ///
    /// A saída que faltava: começou a gravar por engano, ou se enrolou no meio
    /// da frase, e a única alternativa era soltar a tecla e esperar o Whisper
    /// produzir um texto indesejado — que ainda ia para a área de transferência
    /// e, com a colagem automática ligada, para a janela em que a pessoa estava
    /// escrevendo.
    ///
    /// Sem gravação em curso não faz nada, e é isso que permite ligar o atalho
    /// numa tecla de uso comum: o Esc do dia a dia atravessa sem efeito nenhum.
    fn cancelar_gravacao(&self) {
        let sons_do_cancelamento = {
            let mut state = lock(&self.shared);
            if state.recording_since.is_none() {
                return;
            }
            state.recording_since = None;
            state.status.clear();
            // O número do ditado avança para que um `Captured` de um áudio que
            // já estivesse a caminho — o teto de duração estourando no mesmo
            // instante, por exemplo — não seja confundido com o ditado de agora.
            state.ditado_atual += 1;
            let repouso = tela_de_repouso(&state);
            state.view = repouso;
            state.config.sons
        };
        self.audio.send(AudioCmd::Cancel);
        log::info!("ditado cancelado a pedido");
        if sons_do_cancelamento.ativo {
            sons::tocar(Som::Cancelado, sons_do_cancelamento.volume);
        }
        self.sinal.mudou();
    }

    // ---------------------------------------------------------------- apoio

    fn copy_current(&self, then_paste: bool) {
        let text = lock(&self.shared).text.clone();
        if text.is_empty() {
            return;
        }

        // A colagem depende de a cópia ter dado certo, e essa guarda antes só
        // existia no caminho automático. Sem ela, uma cópia falha ainda mandava
        // o Ctrl+V — colando o conteúdo anterior da área de transferência — e a
        // mensagem "Copiei, mas não consegui colar" apagava a verdadeira, "Não
        // consegui copiar": o programa afirmava ter copiado justamente quando
        // não copiou.
        let (metodo, envio) = {
            let state = lock(&self.shared);
            (state.config.metodo_de_colagem, state.config.tecla_de_envio)
        };

        let copiou = match clipboard::copy(&text) {
            Ok(()) => {
                let mut state = lock(&self.shared);
                state.copied_at = Some(Instant::now());
                state.message.clear();
                if then_paste {
                    state.view = View::Hidden;
                }
                true
            }
            Err(e) => {
                log::warn!("cópia manual falhou: {e:#}");
                let mut state = lock(&self.shared);
                state.message = format!("Não consegui copiar: {e:#}");
                false
            }
        };
        self.sinal.mudou();

        if then_paste && copiou {
            self.colar_depois(&text, metodo, envio);
        }
    }

    fn apply_draft(&self) {
        let (draft, previous) = {
            let state = lock(&self.shared);
            (state.draft.clone(), state.config.clone())
        };

        if let Err(e) = draft.save() {
            // Sem este registro a falha não deixava rastro em lugar nenhum: a
            // tela de configurações não desenhava `message` (agora desenha) e o
            // journal não recebia uma linha. O Salvar simplesmente não fazia
            // nada, em silêncio.
            log::error!("não consegui gravar a configuração: {e:#}");
            let mut state = lock(&self.shared);
            state.message = format!("Não consegui gravar a configuração: {e:#}");
            drop(state);
            self.sinal.mudou();
            return;
        }

        if draft.hotkey != previous.hotkey {
            self.hotkey.set_target(&draft.hotkey);
        }
        if draft.atalho_de_cancelar != previous.atalho_de_cancelar {
            self.hotkey.set_cancelar(&draft.atalho_de_cancelar);
        }
        let reload_model =
            draft.model_path != previous.model_path || draft.use_gpu != previous.use_gpu;
        // Lido antes de o `draft` ser movido para dentro do estado, e por isso
        // fora do `if` que o usa lá embaixo.
        let ligou_o_aviso_de_versao = draft.aviso_de_versao && !previous.aviso_de_versao;

        {
            let mut state = lock(&self.shared);
            state.config = draft;
            state.capturando = None;
            let repouso = tela_de_repouso(&state);
            state.view = repouso;
            state.message.clear();
        }
        self.hotkey.cancel_capture();
        self.apply_audio_settings();

        // Ligar o aviso de versão com o programa aberto passa a valer agora, e
        // não no próximo arranque. A vigília não existe enquanto a opção está
        // desligada — é o que o `src/versao.rs` promete —, então religá-la é
        // criar a thread de novo; a trava de lá garante que não nasça uma
        // segunda quando ela ainda estiver viva.
        if ligou_o_aviso_de_versao {
            crate::versao::vigiar(self.shared.clone(), self.sinal.clone());
        }

        if reload_model {
            self.load_model();
        }
        self.sinal.mudou();
    }

    fn apply_audio_settings(&self) {
        let config: Config = lock(&self.shared).config.clone();
        self.audio.send(AudioCmd::Configure(AudioSettings {
            device: config.input_device.clone(),
            max_secs: config.max_recording_secs,
            sempre_aberto: config.microfone_sempre_aberto,
            canal: config.canal_do_microfone,
        }));
        // A ociosidade viaja junto porque é aplicada pelo mesmo caminho: quem
        // muda uma configuração muda todas de uma vez, e a thread do Whisper
        // precisa saber do novo prazo tanto quanto a do áudio precisa saber do
        // novo microfone.
        let _ = self
            .stt
            .send(SttCmd::Ociosidade(config.descarregar_o_modelo.prazo()));
    }

    fn load_model(&self) {
        let (model_path, use_gpu) = {
            let mut state = lock(&self.shared);
            state.model = ModelState::Loading;
            (state.config.model_path.clone(), state.config.use_gpu)
        };
        self.sinal.mudou();
        let _ = self.stt.send(SttCmd::Load {
            model_path,
            use_gpu,
        });
    }

    /// Baixa o modelo sugerido e, quando ele chegar, passa a usá-lo — inclusive
    /// gravando o caminho na configuração, porque quem clicou no botão não
    /// deveria precisar apontar o arquivo depois.
    fn download_model(&self, nome: &str) {
        {
            let mut state = lock(&self.shared);
            if state
                .download
                .as_ref()
                .is_some_and(|d| d.lock().unwrap_or_else(|e| e.into_inner()).andando())
            {
                return;
            }
            state.message = "Leva alguns minutos, dependendo da conexão. Pode fechar esta \
                             janela — o download continua."
                .to_string();
        }

        let (andamento, pronto) = crate::modelo::baixar(nome, self.sinal.clone());
        lock(&self.shared).download = Some(andamento);
        self.sinal.mudou();

        let shared = self.shared.clone();
        let sinal = self.sinal.clone();
        let stt = self.stt.clone();
        let vigia = std::thread::Builder::new()
            .name("modelo-pronto".into())
            .spawn(move || {
                let Ok(model_path) = pronto.recv() else {
                    return;
                };
                let use_gpu = {
                    let mut state = lock(&shared);
                    state.config.model_path = model_path.clone();
                    state.draft.model_path = model_path.clone();
                    state.model = ModelState::Loading;
                    state.message = "Modelo baixado; carregando…".to_string();
                    // É espera, não falha — a tela se recolhe sozinha quando o
                    // modelo terminar de carregar. Sem esta marca ela ficava no
                    // ar com o emblema vermelho de erro ao lado desta frase, e
                    // era assim que terminava a primeira execução de quem
                    // acabara de esperar 574 MB.
                    state.erro_e_so_espera = true;
                    if let Err(e) = state.config.save() {
                        log::warn!("modelo baixado, mas não consegui gravar a config: {e:#}");
                    }
                    state.config.use_gpu
                };
                sinal.mudou();
                let _ = stt.send(SttCmd::Load {
                    model_path,
                    use_gpu,
                });
            });
        if let Err(e) = vigia {
            // Sem esta thread o download continua, mas ninguém passa a usar o
            // arquivo quando ele chegar: a barra completa 100 % e nada mais
            // acontece.
            log::error!("não consegui acompanhar o download até o fim: {e}");
        }
    }

    /// Dispensa a tela atual e volta para onde o programa fica quando não tem
    /// nada a dizer.
    fn voltar_ao_repouso(&self) {
        let mut state = lock(&self.shared);
        let repouso = tela_de_repouso(&state);
        state.view = repouso;
        drop(state);
        self.sinal.mudou();
    }
}

/// Para onde a janela volta quando uma tela é dispensada.
///
/// Escondida, no caso normal. Mas se o microfone estiver aberto é a tela da
/// gravação que volta: fechar uma janela não pode deixar a pessoa falando sem
/// nada na tela dizendo que ela está sendo ouvida.
fn tela_de_repouso(state: &crate::state::Shared) -> View {
    if state.gravando() {
        View::Recording
    } else {
        View::Hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioCmd;
    use crate::state::Shared;
    use crossbeam_channel::Receiver;

    /// Um controlador de verdade, sem thread nenhuma.
    ///
    /// Os manipuladores (`on_hotkey`, `on_audio`, `on_stt`, `on_ui`) são
    /// chamados direto: o `run` só existe para escolher de qual canal ler, e o
    /// que quebrou duas vezes na história deste projeto foi a máquina de
    /// estados, não o `select!`.
    struct Bancada {
        controlador: Controller,
        audio: Receiver<AudioCmd>,
        stt: Receiver<SttCmd>,
        /// O que o ouvinte de teclas mandou. Guardado — e não descartado, como
        /// já esteve — porque a captura de atalho é estado dele, e a única
        /// forma de perguntar de fora se ela continua de pé é ver se uma tecla
        /// solta ainda vira `Captured`.
        hotkey: Receiver<HotkeyEvent>,
    }

    impl Bancada {
        fn nova() -> Self {
            // Obrigatório: com a cópia automática ligada — que é o padrão — o
            // desfecho de um ditado mexe na área de transferência da máquina de
            // quem roda `cargo test`, e sem sessão gráfica o arboard trava.
            //
            // O histórico e os sons entram pelo mesmo motivo: o primeiro
            // escreveria no `~/.local/share` de quem roda os testes, e os
            // segundos abririam o dispositivo de saída de áudio dele — os dois
            // são efeitos colaterais que um teste de unidade não pode ter.
            let config = Config {
                auto_copy: false,
                auto_paste: false,
                historico: crate::config::Historico {
                    ativo: false,
                    ..crate::config::Historico::PADRAO
                },
                sons: crate::config::Sons {
                    ativo: false,
                    ..crate::config::Sons::PADRAO
                },
                ..Config::default()
            };

            let shared: SharedState =
                Arc::new(std::sync::Mutex::new(Shared::new(config, Vec::new())));
            let (audio_tx, audio_rx) = crossbeam_channel::unbounded();
            let (stt_tx, stt_rx) = crossbeam_channel::unbounded();
            let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();

            let mut estado = lock(&shared);
            estado.model = ModelState::Ready;
            drop(estado);

            Self {
                controlador: Controller::novo(
                    shared.clone(),
                    Sinal::default(),
                    crate::audio::AudioHandle {
                        tx: audio_tx,
                        levels: Default::default(),
                    },
                    stt_tx,
                    HotkeyListener::novo(
                        &Config::default().hotkey,
                        &Config::default().atalho_de_cancelar,
                        hotkey_tx,
                    ),
                ),
                audio: audio_rx,
                stt: stt_rx,
                hotkey: hotkey_rx,
            }
        }

        fn estado(&self) -> std::sync::MutexGuard<'_, Shared> {
            lock(&self.controlador.shared)
        }

        /// Esvazia o que o `run` inicial teria mandado (Configure e Load).
        fn limpar(&self) {
            while self.audio.try_recv().is_ok() {}
            while self.stt.try_recv().is_ok() {}
        }
    }

    #[test]
    fn nenhum_teste_daqui_grava_a_configuracao_de_quem_roda_os_testes() {
        // Esta trava custou as configurações de uma máquina para existir.
        //
        // Um teste chamou `apply_draft` para conferir que a tela de Salvar
        // levava uma opção nova até a thread do Whisper. O `apply_draft` grava a
        // configuração em disco — e, num teste, o disco é o `~/.config/ditador`
        // de quem rodou `cargo test`. O que ficou gravado lá foi a configuração
        // da `Bancada`: cópia automática desligada, sons desligados, histórico
        // desligado. Escolhas de verdade, de uma pessoa de verdade, substituídas
        // pelos ajustes de um teste — e sem cópia de onde voltar, porque o
        // `salvar_em` grava de forma atômica justamente para não deixar arquivo
        // pela metade.
        //
        // O que se perdeu não apareceu em nenhuma reprovação: os 200 testes
        // continuaram verdes. Apareceu numa captura de tela, por acaso. Daí esta
        // conferência ser sobre o **texto do arquivo** — é o único jeito de
        // reprovar antes do estrago.
        //
        // Precisando testar o caminho do Salvar, use `apply_audio_settings`, que
        // é a parte que manda as configurações às threads sem tocar no disco.
        let arquivo = include_str!("controller.rs");
        let (_, testes) = arquivo
            .split_once("mod tests {")
            .expect("o módulo de testes deste arquivo mudou de forma");

        // Os nomes são montados em pedaços de propósito: escritos por extenso,
        // este próprio comentário e a linha do `assert` casariam com a busca, e a
        // trava reprovaria a si mesma.
        for proibido in [format!("apply_{}(", "draft"), format!("Apply{}", "Draft")] {
            assert!(
                !testes.contains(&proibido),
                "um teste deste arquivo chama `{proibido}`, que grava o \
                 config.json de quem estiver rodando os testes. Use \
                 `apply_audio_settings`, que aplica sem gravar."
            );
        }
    }

    #[test]
    fn a_gravacao_pede_o_modelo_de_volta_antes_de_abrir_o_microfone() {
        // O que torna o descarregamento por ociosidade suportável: o modelo
        // volta para a memória **enquanto** a pessoa fala. Mandando o `Aquecer`
        // só no fim da gravação, a espera pela recarga apareceria inteira depois
        // de a pessoa soltar a tecla — que é exatamente o momento em que ela
        // está esperando o texto.
        //
        // O `Aquecer` sai sempre, e não só com a opção ligada: com o modelo já
        // carregado ele não faz nada além de adiar o próximo descarregamento, e
        // um `if` aqui seria a configuração da thread do Whisper duplicada do
        // lado de cá.
        let b = Bancada::nova();
        b.limpar();

        b.controlador.start_recording();

        assert!(
            matches!(b.stt.try_recv(), Ok(SttCmd::Aquecer)),
            "a gravação começou sem pedir o modelo de volta"
        );
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Start { .. })),
            "o microfone não foi aberto"
        );
    }

    #[test]
    fn as_configuracoes_aplicadas_levam_a_ociosidade_ate_a_thread_do_whisper() {
        // A opção mora na configuração e quem a executa é a thread do Whisper.
        // Sem esta mensagem, ligar o descarregamento na tela não fazia nada até
        // o programa ser reiniciado — e desligá-lo, menos ainda.
        //
        // Por `apply_audio_settings` e **não** por `apply_draft`: o segundo
        // grava a configuração em disco, e num teste isso é o `config.json` de
        // quem rodou `cargo test`. A primeira versão deste teste fazia isso, e o
        // preço apareceu numa captura de tela: as configurações da máquina
        // tinham virado as da bancada — cópia automática desligada, sons
        // desligados, histórico desligado. A `Bancada` já tem o cuidado de não
        // encostar na área de transferência, no áudio e no histórico de quem
        // roda os testes; gravar por cima da configuração dele é o mesmo erro,
        // e é o mais caro dos três, porque apaga escolhas que ninguém tem como
        // recuperar depois.
        let b = Bancada::nova();
        b.limpar();

        {
            let mut estado = b.estado();
            estado.config.descarregar_o_modelo = crate::config::Ociosidade {
                ativo: true,
                minutos: 5,
            };
        }
        b.controlador.apply_audio_settings();

        let mut chegou = None;
        while let Ok(cmd) = b.stt.try_recv() {
            if let SttCmd::Ociosidade(prazo) = cmd {
                chegou = Some(prazo);
            }
        }
        assert_eq!(
            chegou,
            Some(Some(Duration::from_secs(300))),
            "o prazo escolhido na tela não chegou à thread da transcrição"
        );
    }

    #[test]
    fn o_modelo_descarregado_nao_tira_o_programa_do_ar() {
        // A armadilha desta funcionalidade: tratar "modelo fora da memória" como
        // "modelo carregando" faria o `start_recording` recusar gravar, e o
        // atalho pararia de funcionar sozinho depois de dez minutos de pausa —
        // sem nada na tela explicando por quê.
        let b = Bancada::nova();
        b.limpar();

        b.controlador.on_stt(SttEvent::Descarregado);

        assert_eq!(
            b.estado().model,
            ModelState::Ready,
            "o descarregamento mexeu no estado do modelo"
        );
        assert_eq!(
            b.estado().estado_publico(),
            crate::state::EstadoPublico::Pronto,
            "a bandeja e a extensão do GNOME passariam a dizer outra coisa"
        );

        // E gravar continua funcionando.
        b.controlador.start_recording();
        assert!(b.estado().gravando(), "o atalho parou de gravar");
    }

    #[test]
    fn o_stop_sai_mesmo_com_a_janela_do_resultado_por_cima() {
        // O bug do commit 770d74e: falar de novo enquanto a frase anterior é
        // transcrita é o uso normal, e nesse intervalo a janela do resultado
        // anterior pode aparecer por cima. Decidindo pela tela, o
        // `stop_recording` desistia e o microfone ficava aberto para sempre.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        assert!(b.estado().gravando(), "o microfone devia ter aberto");
        b.limpar();

        // O resultado do ditado anterior toma a tela.
        b.estado().view = View::Result;

        b.controlador.on_hotkey(HotkeyEvent::Up);
        assert!(
            !b.estado().gravando(),
            "o microfone ficou aberto: alguém voltou a decidir pela tela"
        );
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Stop)),
            "o comando de parar não chegou ao áudio"
        );
    }

    #[test]
    fn o_audio_de_um_ditado_atropelado_nao_mexe_na_tela() {
        // Do mesmo commit: o áudio de um ditado antigo ainda é transcrito — ele
        // existe e é do usuário —, mas em silêncio, sem tocar na tela nem no
        // cronômetro de quem está falando agora.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 1
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 2, em curso
        b.limpar();
        assert_eq!(b.estado().ditado_atual, 2);

        let quando = b.estado().recording_since;
        b.controlador.on_audio(AudioEvent::Captured {
            ditado: 1,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        let estado = b.estado();
        assert_eq!(
            estado.view,
            View::Recording,
            "a tela do ditado 2 foi trocada"
        );
        assert_eq!(
            estado.recording_since, quando,
            "o cronômetro foi reiniciado"
        );
        drop(estado);
        assert!(
            matches!(b.stt.try_recv(), Ok(SttCmd::Transcribe { ditado: 1, .. })),
            "o áudio do ditado 1 devia ter sido transcrito assim mesmo"
        );
    }

    #[test]
    fn a_falha_de_uma_transcricao_antiga_nao_toma_a_tela_de_quem_fala_agora() {
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 1
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 2, em curso
        b.limpar();

        b.controlador.on_stt(SttEvent::Failed {
            ditado: 1,
            message: "deu ruim".to_string(),
        });
        assert_eq!(
            b.estado().view,
            View::Recording,
            "a janela de erro apareceu por cima de um ditado em andamento"
        );

        // Sem ninguém falando, a mesma falha aparece.
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_stt(SttEvent::Failed {
            ditado: 2,
            message: "deu ruim".to_string(),
        });
        assert_eq!(b.estado().view, View::Error);
    }

    #[test]
    fn a_tela_de_espera_do_modelo_se_recolhe_quando_ele_fica_pronto() {
        // O caminho da primeira execução: baixar → carregar → pronto. A decisão
        // de fechar já foi tomada comparando o prefixo da mensagem exibida, e a
        // frase do download não casava — a tela ficava presa com o emblema de
        // erro ao lado de "Modelo baixado; carregando…".
        let b = Bancada::nova();
        {
            let mut estado = b.estado();
            estado.model = ModelState::Loading;
            estado.message = "Modelo baixado; carregando…".to_string();
            estado.erro_e_so_espera = true;
            estado.view = View::Error;
        }

        b.controlador.on_stt(SttEvent::Ready);

        let estado = b.estado();
        assert_eq!(estado.view, View::Hidden, "a tela de espera ficou presa");
        assert!(estado.message.is_empty());
        assert_eq!(estado.model, ModelState::Ready);
    }

    #[test]
    fn uma_falha_de_verdade_nao_e_apagada_quando_o_modelo_carrega() {
        // O outro lado da mesma moeda: a tela que está no ar por uma falha de
        // verdade continua lá.
        let b = Bancada::nova();
        {
            let mut estado = b.estado();
            estado.message = "Não consegui acessar o microfone".to_string();
            estado.erro_e_so_espera = false;
            estado.view = View::Error;
        }

        b.controlador.on_stt(SttEvent::Ready);
        assert_eq!(b.estado().view, View::Error);
    }

    #[test]
    fn abrir_as_configuracoes_fecha_o_microfone_antes() {
        // Deixar a gravação correndo por baixo das configurações terminava mal
        // dos dois lados: ou o `Captured` trocava a tela e o rascunho digitado
        // se perdia, ou o microfone ficava aberto sem nada dizendo isso.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        b.limpar();

        b.controlador.on_ui(UiAction::OpenSettings);

        assert!(!b.estado().gravando(), "o microfone continuou aberto");
        assert_eq!(b.estado().view, View::Settings);
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Stop)),
            "o áudio já falado precisa ser entregue, não descartado"
        );
    }

    #[test]
    fn o_fim_da_gravacao_nao_atropela_a_tela_de_configuracoes() {
        let b = Bancada::nova();
        b.controlador.on_ui(UiAction::OpenSettings);
        b.limpar();
        // Uma gravação que já estava a caminho termina agora. O número sai
        // antes da chamada: o guard vive até o fim da expressão, e passá-lo
        // dentro dos argumentos travaria o mutex contra o próprio `on_audio`.
        let ditado = {
            let mut estado = b.estado();
            estado.recording_since = Some(Instant::now());
            estado.ditado_atual
        };
        b.controlador.on_audio(AudioEvent::Captured {
            ditado,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        assert_eq!(
            b.estado().view,
            View::Settings,
            "as configurações foram trocadas por baixo de quem digitava"
        );
    }

    #[test]
    fn fechar_uma_janela_com_o_microfone_aberto_volta_para_a_tela_da_gravacao() {
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        b.limpar();
        b.estado().view = View::Result;

        b.controlador.on_ui(UiAction::Hide);

        assert_eq!(
            b.estado().view,
            View::Recording,
            "a pessoa ficou falando sem nada na tela dizendo que estava sendo ouvida"
        );
    }

    /// (a_salvo, auto_paste, show_result) → tela
    fn tela(a_salvo: bool, auto_paste: bool, show_result: bool) -> View {
        Controller::tela_do_resultado(a_salvo, auto_paste, show_result)
    }

    #[test]
    fn sem_a_janela_quando_a_copia_automatica_ja_resolveu() {
        // O caso novo: o texto está na área de transferência e o usuário não
        // quer nada na frente.
        assert_eq!(tela(true, false, false), View::Hidden);
        // Com a janela ligada, ela aparece, que é o padrão.
        assert_eq!(tela(true, false, true), View::Result);
    }

    #[test]
    fn a_colagem_automatica_esconde_a_janela_de_qualquer_jeito() {
        // O texto já foi para onde o usuário estava digitando; mostrar a janela
        // depois disso só atrapalharia.
        assert_eq!(tela(true, true, true), View::Hidden);
        assert_eq!(tela(true, true, false), View::Hidden);
    }

    #[test]
    fn um_ditado_em_andamento_fica_com_a_tela() {
        // Falar de novo enquanto a frase anterior é transcrita: a janela do
        // texto anterior não pode aparecer por cima de quem está falando…
        assert!(!Controller::resultado_pode_aparecer(View::Hidden, true));
        // …a menos que ela seja o único jeito de o texto não se perder.
        assert!(Controller::resultado_pode_aparecer(View::Result, true));
        // Sem ninguém gravando, as duas telas valem como sempre.
        assert!(Controller::resultado_pode_aparecer(View::Hidden, false));
        assert!(Controller::resultado_pode_aparecer(View::Result, false));
    }

    #[test]
    fn cancelar_descarta_a_gravacao_e_nao_transcreve_nada() {
        // A saída que faltava: o áudio é jogado fora no `audio.rs` e nada
        // chega ao Whisper.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        assert!(b.estado().gravando());
        b.limpar();

        b.controlador.on_hotkey(HotkeyEvent::Cancelar);

        assert!(!b.estado().gravando(), "o microfone continuou aberto");
        assert_eq!(b.estado().view, View::Hidden);
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Cancel)),
            "o comando de descartar não chegou ao áudio"
        );
        assert!(
            b.stt.try_recv().is_err(),
            "um ditado cancelado foi mandado para a transcrição"
        );
    }

    #[test]
    fn cancelar_sem_gravacao_nao_faz_nada() {
        // É o que permite pôr o atalho numa tecla de uso comum: o Esc do dia a
        // dia atravessa sem efeito nenhum.
        let b = Bancada::nova();
        b.limpar();
        let antes = b.estado().ditado_atual;

        b.controlador.on_hotkey(HotkeyEvent::Cancelar);

        assert!(b.audio.try_recv().is_err(), "mandou comando sem gravação");
        assert_eq!(b.estado().ditado_atual, antes, "gastou um número de ditado");
        assert_eq!(b.estado().view, View::Hidden);
    }

    #[test]
    fn o_audio_de_um_ditado_cancelado_nao_toma_a_tela_depois() {
        // O teto de duração pode estourar no mesmo instante do cancelamento, e
        // aí um `Captured` chega para um ditado que já não existe. O número do
        // ditado avança justamente para que ele seja reconhecido como antigo.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        let ditado = b.estado().ditado_atual;
        b.limpar();

        b.controlador.on_hotkey(HotkeyEvent::Cancelar);
        b.controlador.on_audio(AudioEvent::Captured {
            ditado,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        assert_eq!(
            b.estado().view,
            View::Hidden,
            "o áudio de um ditado cancelado abriu a tela de transcrição"
        );
    }

    #[test]
    fn a_captura_manda_a_combinacao_para_o_atalho_que_a_pediu() {
        // Um booleano não sabia para onde mandar, e o palpite mais óbvio — o
        // atalho de ditar, que é o primeiro da tela — trocaria o errado.
        let b = Bancada::nova();
        let original = b.estado().draft.hotkey.clone();

        b.controlador
            .on_ui(UiAction::StartHotkeyCapture(QualAtalho::Cancelar));
        b.controlador
            .on_hotkey(HotkeyEvent::Captured(vec!["KEY_F12".to_string()]));

        assert_eq!(b.estado().draft.atalho_de_cancelar, vec!["KEY_F12"]);
        assert_eq!(
            b.estado().draft.hotkey,
            original,
            "a combinação foi para o atalho errado"
        );
        assert_eq!(b.estado().capturando, None);

        // E o outro caminho continua valendo.
        b.controlador
            .on_ui(UiAction::StartHotkeyCapture(QualAtalho::Ditar));
        b.controlador
            .on_hotkey(HotkeyEvent::Captured(vec!["KEY_F13".to_string()]));
        assert_eq!(b.estado().draft.hotkey, vec!["KEY_F13"]);
        assert_eq!(b.estado().draft.atalho_de_cancelar, vec!["KEY_F12"]);
    }

    #[test]
    fn uma_combinacao_capturada_sem_ninguem_esperando_e_descartada() {
        // A captura pode ser cancelada por outro caminho enquanto a combinação
        // está a caminho pelo canal. Gravar num campo que ninguém escolheu é
        // pior do que não gravar.
        let b = Bancada::nova();
        let hotkey = b.estado().draft.hotkey.clone();
        let cancelar = b.estado().draft.atalho_de_cancelar.clone();

        b.controlador
            .on_hotkey(HotkeyEvent::Captured(vec!["KEY_F12".to_string()]));

        assert_eq!(b.estado().draft.hotkey, hotkey);
        assert_eq!(b.estado().draft.atalho_de_cancelar, cancelar);
    }

    #[test]
    fn o_dicionario_e_o_espaco_do_fim_valem_para_tudo_de_uma_vez() {
        // O texto é acertado uma vez só, antes de qualquer coisa consumi-lo:
        // a janela, a área de transferência e o histórico precisam ver
        // exatamente o mesmo texto.
        let b = Bancada::nova();
        {
            let mut estado = b.estado();
            estado.config.dicionario = crate::config::Dicionario {
                ativo: true,
                termos: vec!["Kubernetes".to_string()],
                sensibilidade: crate::config::Dicionario::SENSIBILIDADE_PADRAO,
            };
            estado.config.espaco_no_fim = true;
        }

        // O número sai antes da chamada, e não dentro dos argumentos dela: o
        // guard do mutex vive até o fim da expressão, e passá-lo ali travaria o
        // estado compartilhado contra o próprio `on_stt`. É a mesma armadilha
        // que o teste das configurações já registra logo acima.
        let ditado = b.estado().ditado_atual;
        b.controlador.on_stt(SttEvent::Done {
            ditado,
            text: "subimos o cuber netes hoje".to_string(),
            elapsed_ms: 100,
        });

        assert_eq!(
            b.estado().text,
            "subimos o Kubernetes hoje ",
            "o texto que a janela mostra não passou pelo dicionário"
        );
    }

    #[test]
    fn a_janela_aparece_quando_o_texto_nao_esta_a_salvo() {
        // Nada foi para a área de transferência (cópia desligada, ou a cópia
        // falhou): a janela é o único jeito de pegar o texto, então ela aparece
        // mesmo com todas as chaves pedindo o contrário.
        for auto_paste in [false, true] {
            for show_result in [false, true] {
                assert_eq!(
                    tela(false, auto_paste, show_result),
                    View::Result,
                    "auto_paste={auto_paste} show_result={show_result}"
                );
            }
        }
    }

    #[test]
    fn um_ditado_curto_demais_nao_rouba_o_historico_do_que_ainda_esta_sendo_transcrito() {
        // Falar de novo enquanto a frase anterior é transcrita é o uso normal
        // deste programa, e o guardado é um só: quem chega depois toma o lugar
        // de quem estava lá. Isso é a decisão certa entre dois ditados de
        // verdade — mas um toque na tecla, curto demais para valer, **nunca
        // chega a ser transcrito**: ele levava embora a duração e o áudio da
        // frase que ainda estava no Whisper para não usá-los para nada, e
        // aquela entrada aparecia na lista sem duração e sem gravação.
        let b = Bancada::nova();
        b.estado().config.historico.ativo = true;
        b.limpar();

        // Ditado 1: uma frase de verdade, já a caminho da transcrição.
        b.controlador.on_hotkey(HotkeyEvent::Down);
        let primeiro = b.estado().ditado_atual;
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_audio(AudioEvent::Captured {
            ditado: primeiro,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        // Ditado 2: um toque sem querer, abaixo da duração mínima.
        b.controlador.on_hotkey(HotkeyEvent::Down);
        let segundo = b.estado().ditado_atual;
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_audio(AudioEvent::Captured {
            ditado: segundo,
            samples: vec![0.0; 160],
            sample_rate: 16_000,
            duration_ms: 10,
        });

        assert_eq!(
            b.controlador.do_ditado(primeiro).map(|g| g.duracao_ms),
            Some(1_000),
            "o ditado descartado levou o histórico de quem ainda estava sendo transcrito"
        );
    }

    #[test]
    fn a_falha_do_microfone_nao_atropela_a_tela_de_configuracoes() {
        // Mesmo critério que o `SttEvent::Failed` já aplica, e que o comentário
        // de lá dizia — erradamente — que o áudio também aplicava: quem está
        // mexendo nas configurações é dono da janela. Trocando-a por uma tela de
        // erro, o rascunho digitado se perde ao reabrir, e o relato do microfone
        // não vale isso: ele está no journal, na linha que o controlador escreve
        // antes de tocar no estado.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        let ditado = b.estado().ditado_atual;
        b.controlador.on_ui(UiAction::OpenSettings);
        b.limpar();
        b.estado().draft.initial_prompt = "o que a pessoa estava digitando".to_string();

        b.controlador.on_audio(AudioEvent::Failed {
            ditado,
            message: "o microfone sumiu".to_string(),
        });

        assert_eq!(
            b.estado().view,
            View::Settings,
            "a tela de erro do microfone tomou as configurações"
        );
        assert_eq!(
            b.estado().draft.initial_prompt,
            "o que a pessoa estava digitando",
            "o rascunho se perdeu"
        );

        // Sem ninguém nas configurações, a mesma falha aparece como sempre.
        b.controlador.on_ui(UiAction::CloseSettings);
        b.controlador.on_hotkey(HotkeyEvent::Down);
        let ditado = b.estado().ditado_atual;
        b.controlador.on_audio(AudioEvent::Failed {
            ditado,
            message: "o microfone sumiu".to_string(),
        });
        assert_eq!(b.estado().view, View::Error);
    }

    #[test]
    fn a_falha_da_colagem_nao_toma_a_tela_de_quem_fala_agora() {
        // A colagem acontece um quarto de segundo depois de o texto ficar
        // pronto, e nesse intervalo cabe um ditado novo inteiro. A janela de
        // resultado é sempre-no-topo, e a essa altura o `start_recording` já
        // limpou o `text`: o que apareceria por cima de quem está falando é um
        // campo vazio com uma mensagem de erro.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        b.limpar();

        Controller::contar_a_falha_na_colagem(
            &b.controlador.shared,
            &b.controlador.sinal,
            MetodoDeColagem::CtrlV,
            "o ydotool não respondeu",
        );

        assert_eq!(
            b.estado().view,
            View::Recording,
            "a janela de erro da colagem apareceu por cima de um ditado em andamento"
        );

        // Parado, ela aparece — é a única forma de a pessoa saber que o texto
        // ficou só na área de transferência.
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.estado().recording_since = None;
        b.estado().view = View::Hidden;
        Controller::contar_a_falha_na_colagem(
            &b.controlador.shared,
            &b.controlador.sinal,
            MetodoDeColagem::CtrlV,
            "o ydotool não respondeu",
        );
        assert_eq!(b.estado().view, View::Result);
        assert!(
            b.estado()
                .message
                .starts_with("Copiei, mas não consegui colar")
        );
    }

    #[test]
    fn abrir_o_historico_encerra_a_captura_do_atalho() {
        // "Ver as transcrições" é um botão de dentro das configurações, e a
        // bandeja abre a mesma lista de qualquer tela. Saindo com a captura de
        // pé, o programa fica esperando uma combinação numa tela que não a
        // explica: o atalho de ditar deixa de ditar, e o aperto seguinte vira
        // um rascunho que ninguém vai salvar.
        let b = Bancada::nova();
        b.controlador.on_ui(UiAction::OpenSettings);
        b.controlador
            .on_ui(UiAction::StartHotkeyCapture(QualAtalho::Ditar));
        b.limpar();

        b.controlador.on_ui(UiAction::AbrirHistorico);

        assert_eq!(
            b.estado().capturando,
            None,
            "a tela saiu do ar e a captura ficou marcada no estado"
        );

        // E o ouvinte de teclas também precisa ter saído do modo de captura —
        // é ele, e não o estado, que decide o que fazer com a próxima tecla.
        let codigo = crate::keys::parse("KEY_F13").expect("KEY_F13 existe nas duas plataformas");
        let origem = crate::hotkey::Origem(1);
        b.controlador
            .hotkey
            .evento(codigo, crate::hotkey::Acao::Apertou, origem);
        b.controlador
            .hotkey
            .evento(codigo, crate::hotkey::Acao::Soltou, origem);
        assert!(
            !b.hotkey
                .try_iter()
                .any(|evento| matches!(evento, HotkeyEvent::Captured(_))),
            "o ouvinte continuou capturando depois de a tela de configurações sair"
        );
    }
}
