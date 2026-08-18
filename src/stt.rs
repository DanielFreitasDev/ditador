//! Transcrição com whisper.cpp (crate whisper-rs).

use crate::config::WHISPER_SAMPLE_RATE;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Backend com que este binário foi compilado.
pub const BACKEND: &str = if cfg!(feature = "cuda") {
    "CUDA"
} else if cfg!(feature = "vulkan") {
    "Vulkan"
} else {
    "CPU"
};

pub const GPU_CAPABLE: bool = cfg!(any(feature = "cuda", feature = "vulkan"));

/// O whisper.cpp rejeita áudio muito curto; completamos com silêncio.
const MIN_SAMPLES: usize = (WHISPER_SAMPLE_RATE as usize * 11) / 10;

#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub translate: bool,
    pub threads: i32,
    pub initial_prompt: String,
    pub normalize: bool,
    /// Tirar o silêncio das pontas antes de transcrever (ver `src/vad.rs`).
    pub aparar_silencio: bool,
}

#[derive(Debug)]
pub enum SttCmd {
    Load {
        model_path: PathBuf,
        use_gpu: bool,
    },
    /// Quanto tempo parado até soltar o modelo da memória; `None` desliga.
    ///
    /// Chega junto com as configurações e vale a partir da próxima espera —
    /// não há por que interromper uma contagem só para recomeçá-la.
    Ociosidade(Option<Duration>),
    /// "Vou precisar do modelo já já."
    ///
    /// É o que o controlador manda no instante em que a gravação começa, e é o
    /// que torna o descarregamento por ociosidade quase invisível: o modelo
    /// volta para a memória **enquanto** a pessoa fala, e não depois de ela
    /// terminar. Com o modelo já carregado não faz nada além de adiar o próximo
    /// descarregamento.
    Aquecer,
    Transcribe {
        /// O número do ditado acompanha o áudio e volta no evento, para que uma
        /// falha de uma frase antiga não tome a tela de quem está falando agora.
        ditado: u64,
        /// Mono, na taxa em que o microfone entregou; a conversão para os
        /// 16 kHz do Whisper acontece aqui (ver `transcribe`).
        samples: Vec<f32>,
        sample_rate: u32,
        options: TranscribeOptions,
    },
}

#[derive(Debug)]
pub enum SttEvent {
    Loading,
    Ready,
    LoadFailed(String),
    /// O modelo saiu da memória por falta de uso.
    ///
    /// Não é falha nem espera: o programa continua pronto para ditar, e quem
    /// apertar a tecla nem fica sabendo. Existe para o rodapé poder dizer o que
    /// aconteceu com a memória de quem ligou a opção — e para o teste poder
    /// afirmar que aconteceu.
    Descarregado,
    Done {
        ditado: u64,
        text: String,
        elapsed_ms: u128,
    },
    Failed {
        ditado: u64,
        message: String,
    },
}

pub fn spawn(events: Sender<SttEvent>) -> Sender<SttCmd> {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::Builder::new()
        .name("whisper".into())
        .spawn(move || run(rx, events))
        .expect("spawn whisper thread");
    tx
}

/// Estrutura em dois laços: o externo carrega o modelo, o interno transcreve
/// reaproveitando o mesmo `WhisperState` (que empresta o contexto). O laço
/// interno termina por três motivos — trocaram o modelo, ninguém dita há tempo
/// demais, ou o programa está encerrando — e é o `match` do fim que decide o
/// que fazer com o contexto em cada um deles.
fn run(rx: Receiver<SttCmd>, events: Sender<SttEvent>) {
    // O que carregar na próxima volta, quando já se sabe.
    let mut proxima: Option<Carga> = None;
    // O último modelo que alguém pediu. É o que permite voltar da ociosidade:
    // sem isto, um modelo descarregado só voltaria com alguém mandando
    // carregá-lo de novo — e ninguém manda, porque do lado de fora nada mudou.
    let mut ultima: Option<Carga> = None;
    let mut prazo: Option<Duration> = None;
    // Um `Transcribe` que chegou com o modelo fora da memória. Ele espera a
    // recarga e é o primeiro a ser atendido depois dela, em vez de virar o
    // "modelo ainda não terminou de carregar" que a pessoa não pediu.
    let mut adiado: Option<SttCmd> = None;
    // Esta carga é a volta de um descarregamento, e não um modelo novo.
    //
    // A diferença é o que se anuncia. Uma carga nova põe o programa inteiro em
    // "carregando" — a bandeja muda de ícone, a extensão do GNOME muda de
    // rótulo e o atalho recusa gravar. Uma recarga não pode fazer nada disso:
    // do lado de fora nada mudou, o programa continua pronto, e quem apertou a
    // tecla está justamente esperando para falar. Ela é silenciosa no sucesso e
    // barulhenta na falha, que é a única coisa que a pessoa precisa saber.
    let mut recarga = false;

    loop {
        // ------------------------------------------------------ de quem é a vez
        let carga = match proxima.take() {
            Some(carga) => carga,
            None => {
                // Sem modelo na memória: só um punhado de comandos tira esta
                // thread daqui, e cada um deles diz o que carregar.
                loop {
                    match rx.recv() {
                        Ok(SttCmd::Load {
                            model_path,
                            use_gpu,
                        }) => {
                            recarga = false;
                            break Carga::nova(model_path, use_gpu);
                        }
                        Ok(SttCmd::Ociosidade(novo)) => prazo = novo,
                        Ok(SttCmd::Aquecer) => match ultima.clone() {
                            Some(carga) => {
                                recarga = true;
                                break carga;
                            }
                            // Aquecer antes do primeiro `Load` não tem o que
                            // aquecer. Acontece no arranque, e a carga de
                            // verdade vem logo atrás.
                            None => continue,
                        },
                        Ok(SttCmd::Transcribe {
                            ditado,
                            samples,
                            sample_rate,
                            options,
                        }) => match ultima.clone() {
                            Some(carga) => {
                                recarga = true;
                                adiado = Some(SttCmd::Transcribe {
                                    ditado,
                                    samples,
                                    sample_rate,
                                    options,
                                });
                                break carga;
                            }
                            None => {
                                let _ = events.send(SttEvent::Failed {
                                    ditado,
                                    message: "O modelo ainda não terminou de carregar.".to_string(),
                                });
                            }
                        },
                        Err(_) => return,
                    }
                }
            }
        };
        ultima = Some(carga.clone());
        let Carga {
            caminho: model_path,
            gpu: use_gpu,
        } = carga;

        if !recarga {
            let _ = events.send(SttEvent::Loading);
        }

        // Uma carga que não acontece precisa responder a quem estava esperando
        // por ela: sem isto, um ditado adiado por uma recarga que falhou some em
        // silêncio, e a janela fica em "Transcrevendo…" para sempre.
        macro_rules! desistir {
            ($mensagem:expr) => {{
                let mensagem: String = $mensagem;
                if let Some(SttCmd::Transcribe { ditado, .. }) = adiado.take() {
                    let _ = events.send(SttEvent::Failed {
                        ditado,
                        message: mensagem.clone(),
                    });
                }
                let _ = events.send(SttEvent::LoadFailed(mensagem));
                continue;
            }};
        }

        if !model_path.exists() {
            desistir!(format!(
                "O modelo de transcrição ainda não está aqui ({}).",
                crate::config::caminho_curto(&model_path)
            ));
        }

        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu && GPU_CAPABLE);

        // As mensagens daqui vão para a tela: em português, com o caminho
        // encurtado e uma saída à mão. O erro cru do whisper-rs, que vem em
        // inglês e fala de ponteiros, fica no log — que é onde alguém que sabe
        // o que ele significa vai procurar.
        let context = match WhisperContext::new_with_params(&model_path, params) {
            Ok(ctx) => ctx,
            Err(e) => {
                log::error!("whisper não carregou {}: {e}", model_path.display());
                desistir!(format!(
                    "Não consegui carregar o modelo ({}). O arquivo pode estar \
                     incompleto — apague-o e baixe de novo.",
                    crate::config::caminho_curto(&model_path)
                ));
            }
        };

        let mut state = match context.create_state() {
            Ok(state) => state,
            Err(e) => {
                log::error!("whisper não criou o estado: {e}");
                desistir!(
                    "Não consegui preparar a transcrição. Se a GPU estiver sem \
                     memória, desligue o uso dela em Configurações → Desempenho."
                        .to_string()
                );
            }
        };

        log::info!(
            "modelo {}: {} (backend {BACKEND}, gpu={})",
            if recarga { "de volta" } else { "carregado" },
            model_path.display(),
            use_gpu && GPU_CAPABLE
        );
        if !recarga {
            let _ = events.send(SttEvent::Ready);
        }

        // ---------------------------------------------------- laço de trabalho
        //
        // Vive enquanto este modelo estiver carregado. O `prazo` entra como
        // limite de espera, e não como cronômetro à parte: cada comando que
        // chega recomeça a contagem por construção, que é exatamente o que
        // "parado há tanto tempo" quer dizer.
        let saida = loop {
            let comando = match adiado.take() {
                Some(comando) => comando,
                None => match prazo {
                    Some(prazo) => match rx.recv_timeout(prazo) {
                        Ok(comando) => comando,
                        Err(RecvTimeoutError::Timeout) => break Saida::Ocioso,
                        Err(RecvTimeoutError::Disconnected) => break Saida::Fim,
                    },
                    None => match rx.recv() {
                        Ok(comando) => comando,
                        Err(_) => break Saida::Fim,
                    },
                },
            };

            match comando {
                SttCmd::Load {
                    model_path,
                    use_gpu,
                } => break Saida::Trocar(Carga::nova(model_path, use_gpu)),
                SttCmd::Ociosidade(novo) => prazo = novo,
                // Com o modelo já na memória não há o que fazer: o comando já
                // cumpriu o papel dele ao chegar, adiando o descarregamento.
                SttCmd::Aquecer => {}
                SttCmd::Transcribe {
                    ditado,
                    samples,
                    sample_rate,
                    options,
                } => {
                    let started = Instant::now();
                    let audio_secs = samples.len() as f64 / sample_rate.max(1) as f64;
                    match transcribe(&mut state, samples, sample_rate, &options) {
                        Ok(text) => {
                            let elapsed_ms = started.elapsed().as_millis();
                            log::info!(
                                "transcrição {ditado}: {audio_secs:.1} s de áudio em \
                                 {elapsed_ms} ms ({} caracteres)",
                                text.chars().count()
                            );
                            log::debug!("texto: {text}");
                            let _ = events.send(SttEvent::Done {
                                ditado,
                                text,
                                elapsed_ms,
                            });
                        }
                        Err(e) => {
                            log::warn!("whisper falhou no ditado {ditado}: {e}");
                            let _ = events.send(SttEvent::Failed {
                                ditado,
                                message: "Não consegui transcrever este áudio. \
                                          O journal tem o motivo."
                                    .to_string(),
                            });
                        }
                    }
                    // Os buffers deste ditado — o áudio na taxa do dispositivo,
                    // o reamostrado e o scratch do ggml — acabaram de ser
                    // liberados. Este é o momento de devolvê-los ao sistema, e
                    // esta é a thread certa: ela já esperou pelo modelo, e o
                    // milissegundo do `malloc_trim` não atrasa nada que alguém
                    // esteja olhando. Ver `src/memoria.rs`.
                    crate::memoria::devolver_ao_sistema();
                }
            }
        };

        match saida {
            Saida::Trocar(carga) => {
                proxima = Some(carga);
                recarga = false;
            }
            Saida::Ocioso => {
                // Soltar o contexto aqui é seguro, e não é caminho novo: é
                // exatamente o que a troca de modelo sempre fez — o programa
                // vivo, esta thread parada, ninguém desmontando contexto
                // gráfico do outro lado. O que **não** é seguro é fazer isto no
                // encerramento (veja o `Saida::Fim`), e a diferença entre os
                // dois é quem mais está mexendo na GPU naquele instante.
                drop(state);
                drop(context);
                // O modelo eram centenas de megabytes numa alocação só. Sem
                // isto a glibc os guarda para uma próxima que pode não vir, e o
                // que a pessoa vê no monitor do sistema é o programa sem o
                // modelo ocupando a memória do modelo. Ver `src/memoria.rs`.
                crate::memoria::devolver_ao_sistema();
                log::info!(
                    "modelo descarregado por ociosidade: {}",
                    model_path.display()
                );
                let _ = events.send(SttEvent::Descarregado);
            }
            Saida::Fim => {
                // O canal fechou: o programa está encerrando. Não liberamos
                // os buffers da GPU aqui — fazer isso enquanto a thread
                // principal desmonta o contexto gráfico derruba o driver da
                // NVIDIA (SIGSEGV dentro de ggml_backend_vk_buffer_free_buffer,
                // com as duas threads dentro de libnvidia-glcore). O processo
                // vai morrer em seguida e o sistema recupera a memória.
                //
                // Só vale para o encerramento: ao trocar de modelo, e ao soltá-lo
                // por ociosidade, o contexto é liberado normalmente, com o
                // restante do programa vivo e parado.
                std::mem::forget(state);
                std::mem::forget(context);
                return;
            }
        }
    }
}

/// O modelo que a thread da transcrição deve ter na memória.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Carga {
    caminho: PathBuf,
    gpu: bool,
}

impl Carga {
    fn nova(caminho: PathBuf, gpu: bool) -> Self {
        Self { caminho, gpu }
    }
}

/// Por que o laço de trabalho terminou.
enum Saida {
    /// Pediram outro modelo.
    Trocar(Carga),
    /// Ninguém dita há tempo demais; o modelo sai da memória.
    Ocioso,
    /// O programa está encerrando.
    Fim,
}

fn transcribe(
    state: &mut whisper_rs::WhisperState,
    samples: Vec<f32>,
    sample_rate: u32,
    options: &TranscribeOptions,
) -> Result<String, String> {
    // A conversão para 16 kHz acontece aqui, e não na thread do áudio: são umas
    // cem multiplicações por amostra de saída, e prender a thread que atende os
    // comandos com isso atrasaria a abertura do microfone do ditado seguinte.
    // Aqui já se ia esperar pelo modelo de qualquer jeito.
    let mut samples = crate::resample::resample(&samples, sample_rate, WHISPER_SAMPLE_RATE);

    // O silêncio das pontas sai **antes** da normalização, e a ordem é o ponto:
    // o `normalize` multiplica o sinal inteiro por até dez para levantar
    // microfone fraco, e uma gravação de puro silêncio sai dele com o mesmo
    // aspecto de uma gravação de fala. Depois dela, o critério absoluto do
    // `vad` (o pico em dBFS) deixaria de querer dizer o que quer.
    if options.aparar_silencio {
        let Some(recorte) = crate::vad::achar_a_fala(&samples, WHISPER_SAMPLE_RATE) else {
            // Ninguém falou. O texto vazio é a resposta que este programa já
            // tem para isso ("Não identifiquei fala no áudio"), e chegar a ela
            // sem passar pelo modelo é justamente o ganho: o Whisper, posto
            // diante de silêncio, não devolve nada — devolve a frase mais
            // provável do treino dele.
            log::debug!(
                "áudio sem fala ({:.1} s); nada foi mandado ao modelo",
                samples.len() as f64 / f64::from(WHISPER_SAMPLE_RATE)
            );
            return Ok(String::new());
        };
        if recorte.amostras() < samples.len() {
            log::debug!(
                "silêncio aparado: {:.1} s → {:.1} s",
                samples.len() as f64 / f64::from(WHISPER_SAMPLE_RATE),
                recorte.amostras() as f64 / f64::from(WHISPER_SAMPLE_RATE)
            );
            samples = samples[recorte.inicio..recorte.fim].to_vec();
        }
    }

    if options.normalize {
        crate::resample::normalize(&mut samples);
    }
    if samples.len() < MIN_SAMPLES {
        samples.resize(MIN_SAMPLES, 0.0);
    }

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(options.threads.clamp(1, 32));
    params.set_translate(options.translate);
    params.set_language(options.language.as_deref());
    // Cada ditado é independente: sem isso, uma frase contamina a seguinte.
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    // Deixado desligado de propósito, ao contrário do padrão do Whisper da
    // OpenAI. Ele mascara a lista `non_speech_tokens`, e o efeito num ditado em
    // português é assimétrico: entre os 80 tokens proibidos estão `@`, `:` e
    // `/`, que ficam matematicamente inalcançáveis. "me chame às 14:30" saía
    // "me chame às 14 30", e nenhum e-mail podia ser ditado. O que ele defendia
    // — os símbolos musicais alucinados no silêncio — está coberto por
    // `is_non_speech_marker`, que é filtro nosso e não custa vocabulário.
    params.set_suppress_nst(false);
    params.set_temperature(0.0);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    if !options.initial_prompt.is_empty() {
        params.set_initial_prompt(&options.initial_prompt);
    }

    state
        .full(params, &samples)
        .map_err(|e| format!("falha na transcrição: {e}"))?;

    let mut parts: Vec<String> = Vec::new();
    for segment in state.as_iter() {
        // Segmentos que o próprio modelo considera silêncio costumam ser
        // alucinação ("Legendas pela comunidade...", "Obrigado.").
        if segment.no_speech_probability() > 0.85 {
            continue;
        }
        let text = segment.to_str_lossy().unwrap_or_default();
        let text = text.trim();
        if text.is_empty() || is_non_speech_marker(text) {
            continue;
        }
        parts.push(text.to_string());
    }

    Ok(collapse_whitespace(&parts.join(" ")))
}

/// Marcadores como "[BLANK_AUDIO]", "(música)", "[Risos]", "♪♪♪".
///
/// Os símbolos musicais entram aqui porque o `set_suppress_nst` está desligado
/// (ver `transcribe`): eles são o que o modelo alucina no silêncio, e um
/// segmento que só tem símbolo e pontuação nunca é fala.
fn is_non_speech_marker(text: &str) -> bool {
    if (text.starts_with('[') && text.ends_with(']'))
        || (text.starts_with('(') && text.ends_with(')'))
    {
        return true;
    }
    // Nenhuma letra e nenhum dígito: sobrou só nota musical, asterisco ou
    // pontuação solta.
    !text.is_empty() && !text.chars().any(char::is_alphanumeric)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Medição de um backend de verdade, com o modelo de verdade.
///
/// Fica em `#[ignore]` porque carrega centenas de megabytes, exige a GPU e leva
/// dezenas de segundos: nada disso pertence ao `cargo test` que se roda a cada
/// alteração. Mas pertence ao repositório — a escolha de qual backend é o padrão
/// de cada sistema é uma decisão que se toma com número, e um número que ninguém
/// consegue reproduzir é um número em que ninguém precisa acreditar.
///
/// ```text
/// DITADOR_AUDIO_DE_TESTE=frase.wav \
///   cargo test --release --no-default-features --features cuda \
///   -- --ignored --nocapture mede_o_backend
/// ```
///
/// O WAV precisa ser PCM de 16 bits, mono. Para gerar um reproduzível no
/// Windows, sem microfone e sem depender de alguém falar a mesma frase duas
/// vezes, dá para usar a síntese de voz do próprio sistema
/// (`System.Speech.Synthesis`, voz `Microsoft Maria Desktop`, 16 kHz).
/// Ensaios de ponta a ponta, com o modelo de verdade.
///
/// A diferença para o `mod tests` acima é o que eles exercitam: lá, a máquina de
/// estados da thread, sem carregar nada; aqui, o **efeito** — o modelo saindo da
/// memória e voltando, o silêncio aparado mudando ou não o que o Whisper
/// entende. Nada disso cabe num agente de CI (não há modelo, não há 200 MB para
/// baixar a cada push), e por isso todos são `#[ignore]`.
///
/// Como rodar, na máquina de quem mexeu:
///
/// ```text
/// curl -sL -o /tmp/jfk.wav \
///   https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav
/// ditador --baixar-modelo small-q5_1
/// DITADOR_AUDIO_DE_TESTE=/tmp/jfk.wav \
///   cargo test --release --no-default-features --features cpu ensaio \
///   -- --ignored --nocapture
/// ```
///
/// O modelo usado é o leve (`modelo::PADRAO_CPU`), e não o padrão: são 190 MB
/// contra 574 MB e 3,5 s contra 18 s por passada, e o que se está conferindo
/// aqui não depende do tamanho da rede. `DITADOR_MODELO_DE_TESTE` troca por
/// outro, como no `mede_o_backend`.
#[cfg(test)]
mod ensaio {
    use super::medicao::ler_wav;
    use super::*;

    /// O modelo destes ensaios, e a explicação de como consegui-lo quando falta.
    fn modelo() -> PathBuf {
        let escolhido = std::env::var("DITADOR_MODELO_DE_TESTE")
            .unwrap_or_else(|_| crate::modelo::PADRAO_CPU.to_string());
        let caminho = match crate::modelo::achar(&escolhido) {
            Some(m) => crate::modelo::caminho(m.nome),
            None => PathBuf::from(escolhido),
        };
        assert!(
            caminho.exists(),
            "o modelo destes ensaios não está em {}; rode: \
             ditador --baixar-modelo {}",
            caminho.display(),
            crate::modelo::PADRAO_CPU
        );
        caminho
    }

    fn audio_de_fala() -> (Vec<f32>, u32) {
        let Some(caminho) = std::env::var_os("DITADOR_AUDIO_DE_TESTE") else {
            panic!("defina DITADOR_AUDIO_DE_TESTE com o caminho de um WAV mono de 16 bits");
        };
        ler_wav(std::path::Path::new(&caminho))
    }

    fn opcoes(aparar_silencio: bool) -> TranscribeOptions {
        TranscribeOptions {
            // Sem idioma fixo: o áudio de teste pode estar em qualquer língua, e
            // o que estes ensaios comparam é o texto consigo mesmo.
            language: Some("auto".to_string()),
            translate: false,
            threads: 8,
            initial_prompt: String::new(),
            normalize: false,
            aparar_silencio,
        }
    }

    /// Carrega o modelo e devolve os canais, já com o `Ready` consumido.
    ///
    /// O prazo de ociosidade é mandado **depois** da carga, e não antes: é assim
    /// que ele chega no programa de verdade — quem o escolhe é a tela de
    /// configurações, com o modelo já na memória —, e é o braço do laço de
    /// trabalho que fica exercitado. Mandando antes, quem atenderia seria o
    /// laço de espera, que é outro código.
    fn thread_pronta(prazo: Option<Duration>) -> (Sender<SttCmd>, Receiver<SttEvent>) {
        let (tx_eventos, eventos) = crossbeam_channel::unbounded();
        let comandos = spawn(tx_eventos);
        comandos
            .send(SttCmd::Load {
                model_path: modelo(),
                use_gpu: false,
            })
            .expect("mandando carregar");
        loop {
            match eventos.recv_timeout(Duration::from_secs(120)) {
                Ok(SttEvent::Loading) => {}
                Ok(SttEvent::Ready) => break,
                Ok(SttEvent::LoadFailed(e)) => panic!("o modelo não carregou: {e}"),
                outro => panic!("evento inesperado durante a carga: {outro:?}"),
            }
        }
        if let Some(prazo) = prazo {
            comandos
                .send(SttCmd::Ociosidade(Some(prazo)))
                .expect("mandando a ociosidade");
        }
        (comandos, eventos)
    }

    fn transcrever(
        comandos: &Sender<SttCmd>,
        eventos: &Receiver<SttEvent>,
        ditado: u64,
        amostras: Vec<f32>,
        taxa: u32,
        aparar: bool,
    ) -> String {
        comandos
            .send(SttCmd::Transcribe {
                ditado,
                samples: amostras,
                sample_rate: taxa,
                options: opcoes(aparar),
            })
            .expect("mandando transcrever");
        loop {
            match eventos.recv_timeout(Duration::from_secs(300)) {
                Ok(SttEvent::Done {
                    ditado: d, text, ..
                }) if d == ditado => return text,
                Ok(SttEvent::Failed { ditado: d, message }) if d == ditado => {
                    panic!("o ditado {d} falhou: {message}")
                }
                Ok(_) => {}
                Err(e) => panic!("nada respondeu ao ditado {ditado}: {e}"),
            }
        }
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; veja o //! do módulo"]
    fn o_modelo_sai_da_memoria_e_volta_sozinho_para_transcrever() {
        // O ciclo inteiro do descarregamento por ociosidade, que nenhum teste de
        // unidade alcança: carregar, ficar parado, ser descarregado, e voltar
        // por conta própria porque chegou trabalho. O que se afirma no fim é o
        // que importa para quem usa: **o texto sai**, e sai igual ao de antes.
        let (amostras, taxa) = audio_de_fala();
        let (comandos, eventos) = thread_pronta(Some(Duration::from_millis(400)));

        let antes = transcrever(&comandos, &eventos, 1, amostras.clone(), taxa, true);
        assert!(!antes.trim().is_empty(), "o modelo não transcreveu nada");

        // Parado. O prazo é de 400 ms; a espera é generosa porque a máquina pode
        // estar ocupada, e o que interessa é que o evento **chegue**.
        let descarregou = loop {
            match eventos.recv_timeout(Duration::from_secs(30)) {
                Ok(SttEvent::Descarregado) => break true,
                Ok(_) => {}
                Err(_) => break false,
            }
        };
        assert!(
            descarregou,
            "o modelo não saiu da memória depois do prazo de ociosidade"
        );

        // E agora o teste de verdade: um ditado com o modelo fora da memória.
        let depois = transcrever(&comandos, &eventos, 2, amostras, taxa, true);
        assert_eq!(
            antes.trim(),
            depois.trim(),
            "a transcrição depois da recarga saiu diferente da de antes"
        );
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; veja o //! do módulo"]
    fn a_ociosidade_escolhida_antes_da_carga_vale_do_mesmo_jeito() {
        // A ordem que o programa de verdade usa, e que o outro ensaio não
        // exercita: o `Controller::run` chama `apply_audio_settings` **antes** de
        // `load_model`, então o prazo chega ao laço de espera, com a thread
        // ainda sem modelo nenhum na memória — e precisa continuar valendo
        // depois que o modelo carregar.
        let (tx_eventos, eventos) = crossbeam_channel::unbounded();
        let comandos = spawn(tx_eventos);

        comandos
            .send(SttCmd::Ociosidade(Some(Duration::from_millis(400))))
            .expect("mandando a ociosidade antes da carga");
        comandos
            .send(SttCmd::Load {
                model_path: modelo(),
                use_gpu: false,
            })
            .expect("mandando carregar");

        let mut descarregou = false;
        // Um punhado de eventos de folga: o `Loading` e o `Ready` da carga vêm
        // antes do que se está esperando.
        for _ in 0..5 {
            match eventos.recv_timeout(Duration::from_secs(120)) {
                Ok(SttEvent::Descarregado) => {
                    descarregou = true;
                    break;
                }
                Ok(SttEvent::LoadFailed(e)) => panic!("o modelo não carregou: {e}"),
                Ok(_) => {}
                Err(e) => panic!("nada mais chegou: {e}"),
            }
        }
        assert!(
            descarregou,
            "o prazo escolhido antes da carga não valeu depois dela"
        );
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; veja o //! do módulo"]
    fn o_silencio_em_volta_da_fala_nao_muda_o_que_o_modelo_entende() {
        // O que o `src/vad.rs` promete: aparar as pontas não pode mudar o texto.
        // A fala vai ao modelo duas vezes — nua e cercada de dois segundos de
        // silêncio de cada lado, que é como toda gravação deste programa chega.
        // As pontas são **ruído de sala**, e não silêncio digital: é o que um
        // microfone de verdade entrega, e é o que muda o resultado. Medido com o
        // `small-q5_1` e o `jfk.wav`, as mesmas pontas sem o aparo mudaram a
        // pontuação da frase ("And so my fellow Americans" em vez de "And so, my
        // fellow Americans") — o modelo lê o ruído como parte da fala e decide a
        // prosódia por ele.
        let (fala, taxa) = audio_de_fala();
        let ruido = ruido_de_sala(2);
        let mut cercada = ruido.clone();
        cercada.extend_from_slice(&fala);
        cercada.extend_from_slice(&ruido);

        let (comandos, eventos) = thread_pronta(None);
        let nua = transcrever(&comandos, &eventos, 1, fala, taxa, true);
        let com_pontas = transcrever(&comandos, &eventos, 2, cercada, taxa, true);

        assert_eq!(
            nua.trim(),
            com_pontas.trim(),
            "o ruído em volta mudou o que o modelo entendeu, mesmo com o aparo ligado"
        );
    }

    /// Ruído de sala: o que um microfone entrega quando ninguém fala.
    ///
    /// Amplitude de 0,004 — uns -55 dBFS de RMS, que é o piso de um microfone
    /// USB comum numa sala fechada. **Não** é silêncio digital, e a diferença
    /// entre os dois é o ponto do teste abaixo.
    fn ruido_de_sala(segundos: usize) -> Vec<f32> {
        let mut semente = 0x2545_F491_4F6C_DD1Du64;
        (0..WHISPER_SAMPLE_RATE as usize * segundos)
            .map(|_| {
                semente ^= semente << 13;
                semente ^= semente >> 7;
                semente ^= semente << 17;
                ((semente >> 40) as f32 / 8_388_608.0 - 1.0) * 0.004
            })
            .collect()
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; veja o //! do módulo"]
    fn uma_gravacao_sem_fala_nao_vira_texto() {
        // A tecla apertada sem querer, ou o atalho que pegou e ninguém falou.
        //
        // São **dois** casos, e só o segundo justifica este módulo existir:
        //
        //  1. silêncio digital (o microfone mudo no mixer, o cabo fora). Este as
        //     defesas antigas do `transcribe` já cobriam — medido: o modelo
        //     devolve texto vazio mesmo sem o aparo;
        //  2. ruído de sala, que é o caso de verdade. Medido com o
        //     `small-q5_1`, quatro segundos de ruído a -55 dBFS **sem** o aparo
        //     saíram como `"ស\u{17d2}\u{17d2}\u{17d2}\u{17d2}"` — cinco
        //     caracteres de khmer que ninguém falou, que não são marcador nem
        //     têm `no_speech_probability` alta o bastante, e que portanto
        //     atravessavam as duas defesas e caíam na área de transferência de
        //     quem esbarrou na tecla.
        //
        // O que o modelo devolve sem o aparo não é afirmado aqui, e de
        // propósito: alucinação muda com o modelo, com a versão do whisper.cpp e
        // com o ruído. Exigir uma alucinação específica seria um teste que
        // reprova sem nada ter piorado. O contrato é só o de cima — com o aparo
        // ligado, nada disso vira texto.
        let (comandos, eventos) = thread_pronta(None);

        let silencio = vec![0.0f32; WHISPER_SAMPLE_RATE as usize * 3];
        let texto = transcrever(&comandos, &eventos, 1, silencio, WHISPER_SAMPLE_RATE, true);
        assert!(
            texto.trim().is_empty(),
            "três segundos de silêncio viraram texto: {texto:?}"
        );

        let texto = transcrever(
            &comandos,
            &eventos,
            2,
            ruido_de_sala(4),
            WHISPER_SAMPLE_RATE,
            true,
        );
        assert!(
            texto.trim().is_empty(),
            "quatro segundos de ruído de sala viraram texto: {texto:?}"
        );
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; veja o //! do módulo"]
    fn trocar_de_modelo_com_a_thread_de_pe_continua_funcionando() {
        // O caminho que já existia antes de tudo isto, e que o laço externo do
        // `run` foi reescrito em volta. Vale conferir que ele sobreviveu: é o
        // que a tela de configurações faz quando alguém escolhe outro modelo na
        // lista nova.
        let (amostras, taxa) = audio_de_fala();
        let (comandos, eventos) = thread_pronta(None);
        let primeiro = transcrever(&comandos, &eventos, 1, amostras.clone(), taxa, true);

        comandos
            .send(SttCmd::Load {
                model_path: modelo(),
                use_gpu: false,
            })
            .expect("trocando de modelo");
        loop {
            match eventos.recv_timeout(Duration::from_secs(120)) {
                Ok(SttEvent::Ready) => break,
                Ok(SttEvent::LoadFailed(e)) => panic!("a troca de modelo falhou: {e}"),
                Ok(_) => {}
                Err(e) => panic!("a troca de modelo não terminou: {e}"),
            }
        }

        let segundo = transcrever(&comandos, &eventos, 2, amostras, taxa, true);
        assert_eq!(primeiro.trim(), segundo.trim());
    }
}

#[cfg(test)]
mod medicao {
    use super::*;
    use crate::config::Config;

    /// Lê um WAV PCM de 16 bits mono para as amostras que o Whisper espera.
    ///
    /// Vinte linhas em vez de uma dependência: é código só de teste, o formato é
    /// fixo e conhecido, e acrescentar uma crate ao `Cargo.toml` de um projeto
    /// que orgulhosamente tem poucas — para ler um cabeçalho de 44 bytes — seria
    /// desproporcional.
    pub(super) fn ler_wav(caminho: &std::path::Path) -> (Vec<f32>, u32) {
        let bytes = std::fs::read(caminho).expect("lendo o WAV de teste");
        assert_eq!(&bytes[0..4], b"RIFF", "não é um arquivo RIFF");
        assert_eq!(&bytes[8..12], b"WAVE", "não é um WAV");

        // Percorre os blocos até achar o `fmt ` e o `data`. O cabeçalho de 44
        // bytes é o caso comum, mas a síntese de voz do Windows intercala um
        // bloco `fact` — e presumir 44 lê o áudio deslocado, o que aparece como
        // uma transcrição de puro ruído e manda a gente culpar o modelo.
        let (mut canais, mut taxa, mut dados) = (0u16, 0u32, None);
        let mut i = 12;
        while i + 8 <= bytes.len() {
            let id = &bytes[i..i + 4];
            let tamanho = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
            let corpo = i + 8;
            match id {
                b"fmt " => {
                    canais = u16::from_le_bytes(bytes[corpo + 2..corpo + 4].try_into().unwrap());
                    taxa = u32::from_le_bytes(bytes[corpo + 4..corpo + 8].try_into().unwrap());
                    let bits =
                        u16::from_le_bytes(bytes[corpo + 14..corpo + 16].try_into().unwrap());
                    assert_eq!(bits, 16, "o WAV precisa ser PCM de 16 bits");
                }
                b"data" => dados = Some(&bytes[corpo..(corpo + tamanho).min(bytes.len())]),
                _ => {}
            }
            // Os blocos são alinhados em dois bytes.
            i = corpo + tamanho + (tamanho & 1);
        }

        assert_eq!(canais, 1, "o WAV precisa ser mono");
        let dados = dados.expect("o WAV não tem bloco de dados");
        let amostras = dados
            .chunks_exact(2)
            .map(|par| i16::from_le_bytes([par[0], par[1]]) as f32 / i16::MAX as f32)
            .collect();
        (amostras, taxa)
    }

    #[test]
    #[ignore = "carrega o modelo de verdade; rode com --ignored e DITADOR_AUDIO_DE_TESTE"]
    fn mede_o_backend() {
        let Some(caminho) = std::env::var_os("DITADOR_AUDIO_DE_TESTE") else {
            panic!("defina DITADOR_AUDIO_DE_TESTE com o caminho de um WAV mono de 16 bits");
        };
        let (amostras, taxa) = ler_wav(std::path::Path::new(&caminho));
        let duracao = amostras.len() as f64 / taxa as f64;

        let mut config = Config::load();
        // `DITADOR_MODELO_DE_TESTE` mede outro modelo sem mexer na configuração
        // de quem roda o teste. É o que permite comparar dois modelos na mesma
        // máquina, na mesma tarde, com o mesmo áudio — que é a única comparação
        // que quer dizer alguma coisa. Aceita o nome do catálogo
        // (`small-q5_1`) ou um caminho de arquivo.
        if let Some(escolhido) = std::env::var_os("DITADOR_MODELO_DE_TESTE") {
            let escolhido = escolhido.to_string_lossy().to_string();
            config.model_path = match crate::modelo::achar(&escolhido) {
                Some(modelo) => crate::modelo::caminho(modelo.nome),
                None => PathBuf::from(escolhido),
            };
        }
        assert!(
            config.model_path.exists(),
            "o modelo não está em {}; rode: ditador --baixar-modelo",
            config.model_path.display()
        );

        let (eventos_tx, eventos) = crossbeam_channel::unbounded();
        let comandos = spawn(eventos_tx);

        let relogio = std::time::Instant::now();
        comandos
            .send(SttCmd::Load {
                model_path: config.model_path.clone(),
                use_gpu: config.use_gpu,
            })
            .expect("mandando carregar");

        // Sem `Option` no meio: todo caminho de saída deste laço ou entrega o
        // tempo ou entra em pânico, então a variável só existe depois de medida.
        // A versão anterior começava em `None` e o compilador avisava que aquele
        // valor nunca era lido — estava certo.
        let carga = loop {
            match eventos.recv().expect("esperando o modelo carregar") {
                SttEvent::Loading => {}
                SttEvent::Ready => break relogio.elapsed(),
                SttEvent::LoadFailed(e) => panic!("o modelo não carregou: {e}"),
                outro => panic!("evento inesperado durante a carga: {outro:?}"),
            }
        };

        // Três passadas com o mesmo áudio, e as três são relatadas.
        //
        // Não é zelo estatístico — é que a primeira passada mede outra coisa. O
        // backend Vulkan compila os *pipelines* de shader na primeira vez que
        // cada um é usado, e esse custo cai inteiro dentro da primeira
        // transcrição: medindo só ela, o Vulkan aparece **mais lento que a CPU**
        // numa RTX 3060, que é uma conclusão errada tirada de um número certo.
        //
        // As duas informações interessam, e são diferentes. A primeira passada é
        // o que a pessoa sente ao ditar logo depois de ligar o computador; as
        // seguintes são o regime, que é o resto do dia. Um backend que ganha no
        // regime e perde feio na largada pode ser a escolha errada para um
        // programa cujo uso típico é uma frase de dez segundos, de vez em
        // quando.
        let opcoes = TranscribeOptions {
            language: Some(config.language.clone()),
            translate: config.translate,
            threads: config.threads,
            initial_prompt: config.initial_prompt.clone(),
            normalize: config.normalize_audio,
            // A medição compara backends, e o que ela precisa medir é o mesmo
            // áudio nos três. Aparar o silêncio mudaria o tamanho da entrada de
            // acordo com o que o WAV de teste tiver nas pontas, e os números de
            // duas execuções deixariam de ser comparáveis.
            aparar_silencio: false,
        };

        println!("\n╭─ backend {BACKEND}");
        println!("│  GPU pedida ......... {}", config.use_gpu);
        println!("│  áudio .............. {duracao:.1} s");
        println!("│  carga do modelo .... {:.2} s", carga.as_secs_f64());

        let mut texto_final = String::new();
        for passada in 1..=3u64 {
            comandos
                .send(SttCmd::Transcribe {
                    ditado: passada,
                    samples: amostras.clone(),
                    sample_rate: taxa,
                    options: opcoes.clone(),
                })
                .expect("mandando transcrever");

            match eventos.recv().expect("esperando a transcrição") {
                SttEvent::Done {
                    text, elapsed_ms, ..
                } => {
                    let segundos = elapsed_ms as f64 / 1000.0;
                    println!(
                        "│  passada {passada} ........... {segundos:6.2} s   ({:.1}× o tempo real){}",
                        duracao / segundos,
                        // Só a GPU tem o que compilar na largada, e só na
                        // primeiríssima execução da máquina: depois disso o
                        // driver guarda os pipelines em cache e nem a passada 1
                        // paga o preço. Rotular a passada 1 da CPU assim era
                        // dizer algo falso num relatório que existe para ser
                        // colado noutro lugar.
                        if passada == 1 && GPU_CAPABLE {
                            "  ← a 1ª vez na máquina compila os shaders"
                        } else {
                            ""
                        }
                    );
                    texto_final = text;
                }
                SttEvent::Failed { message, .. } => panic!("a transcrição falhou: {message}"),
                outro => panic!("evento inesperado: {outro:?}"),
            }
        }
        println!("╰─ \"{}\"\n", texto_final.trim());

        assert!(
            !texto_final.trim().is_empty(),
            "o backend {BACKEND} devolveu texto vazio — a medida de tempo não vale \
             nada se ele não transcreveu nada"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um modelo que não existe, para exercitar a máquina de estados da thread
    /// sem carregar 574 MB — o que nenhum agente de CI tem como fazer.
    fn caminho_inexistente() -> PathBuf {
        std::env::temp_dir().join("ditador-modelo-que-nao-existe.bin")
    }

    #[test]
    fn um_ditado_que_chega_com_o_modelo_fora_da_memoria_e_respondido() {
        // O caminho que o descarregamento por ociosidade criou: a thread já
        // esteve carregada, soltou o modelo, e agora chega um `Transcribe`. Ela
        // tenta recarregar — e, se a recarga falhar, quem estava esperando
        // **precisa** receber uma resposta. Sem isto o ditado some em silêncio e
        // a janela fica em "Transcrevendo…" para sempre, que é exatamente o tipo
        // de programa zumbi que o `canal_caiu` do controlador existe para evitar.
        let (tx_eventos, eventos) = crossbeam_channel::unbounded();
        let comandos = spawn(tx_eventos);

        comandos
            .send(SttCmd::Load {
                model_path: caminho_inexistente(),
                use_gpu: false,
            })
            .expect("mandando carregar");
        assert!(
            matches!(
                eventos.recv_timeout(Duration::from_secs(5)),
                Ok(SttEvent::Loading)
            ),
            "a carga de um modelo novo se anuncia"
        );
        assert!(
            matches!(
                eventos.recv_timeout(Duration::from_secs(5)),
                Ok(SttEvent::LoadFailed(_))
            ),
            "o arquivo não existe, então a carga falha"
        );

        comandos
            .send(SttCmd::Transcribe {
                ditado: 7,
                samples: vec![0.0; 16_000],
                sample_rate: WHISPER_SAMPLE_RATE,
                options: TranscribeOptions {
                    language: None,
                    translate: false,
                    threads: 1,
                    initial_prompt: String::new(),
                    normalize: false,
                    aparar_silencio: false,
                },
            })
            .expect("mandando transcrever");

        // A resposta do ditado 7 tem de chegar. A ordem entre ela e o
        // `LoadFailed` da recarga não importa e não é afirmada aqui — afirmá-la
        // seria travar uma decisão que não é contrato de ninguém.
        let mut respondeu = false;
        for _ in 0..3 {
            match eventos.recv_timeout(Duration::from_secs(5)) {
                Ok(SttEvent::Failed { ditado, .. }) => {
                    assert_eq!(ditado, 7);
                    respondeu = true;
                    break;
                }
                Ok(_) => continue,
                Err(e) => panic!("nada respondeu ao ditado adiado: {e}"),
            }
        }
        assert!(respondeu, "o ditado adiado ficou sem resposta");
    }

    #[test]
    fn aquecer_antes_de_haver_modelo_nao_trava_a_thread() {
        // O `Aquecer` sai no começo de toda gravação, e no arranque ele pode
        // chegar antes do primeiro `Load`. Sem nada para aquecer ele não pode
        // nem responder falha (não há ditado a quem responder) nem prender a
        // thread: o `Load` que vem logo atrás tem de ser atendido normalmente.
        let (tx_eventos, eventos) = crossbeam_channel::unbounded();
        let comandos = spawn(tx_eventos);

        comandos.send(SttCmd::Aquecer).expect("aquecendo do nada");
        comandos
            .send(SttCmd::Ociosidade(Some(Duration::from_secs(600))))
            .expect("mandando a ociosidade");
        comandos
            .send(SttCmd::Load {
                model_path: caminho_inexistente(),
                use_gpu: false,
            })
            .expect("mandando carregar");

        assert!(
            matches!(
                eventos.recv_timeout(Duration::from_secs(5)),
                Ok(SttEvent::Loading)
            ),
            "o Load depois de um Aquecer sem modelo continua sendo atendido"
        );
    }

    #[test]
    fn junta_espacos() {
        assert_eq!(collapse_whitespace("  olá   mundo \n "), "olá mundo");
    }

    #[test]
    fn identifica_marcadores() {
        assert!(is_non_speech_marker("[BLANK_AUDIO]"));
        assert!(is_non_speech_marker("(música)"));
        assert!(!is_non_speech_marker("olá [pessoal]"));
    }

    #[test]
    fn os_simbolos_musicais_alucinados_no_silencio_sao_descartados() {
        // Esta é a defesa que substitui o `set_suppress_nst`, desligado porque
        // ele proibia o modelo de escrever `@`, `:` e `/` — e nenhum e-mail,
        // hora ou data podia ser ditado.
        assert!(is_non_speech_marker("♪♪♪"));
        assert!(is_non_speech_marker("♪"));
        assert!(is_non_speech_marker("***"));
        assert!(is_non_speech_marker("..."));

        // E o que é fala continua passando — inclusive o que o suppress_nst
        // tornava impossível.
        assert!(!is_non_speech_marker("me chame às 14:30"));
        assert!(!is_non_speech_marker("ana@exemplo.com"));
        assert!(!is_non_speech_marker("12/08/2026"));
        assert!(!is_non_speech_marker("50%"));
        assert!(!is_non_speech_marker("♪ cantando ♪"));
    }
}
