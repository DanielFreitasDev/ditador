//! Transcrição com whisper.cpp (crate whisper-rs).

use crate::config::WHISPER_SAMPLE_RATE;
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::time::Instant;
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
}

#[derive(Debug)]
pub enum SttCmd {
    Load {
        model_path: PathBuf,
        use_gpu: bool,
    },
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
/// reaproveitando o mesmo `WhisperState` (que empresta o contexto). Quando chega
/// um novo `Load`, o laço interno termina e o contexto antigo é liberado.
fn run(rx: Receiver<SttCmd>, events: Sender<SttEvent>) {
    let mut pending: Option<(PathBuf, bool)> = None;

    loop {
        let (model_path, use_gpu) = match pending.take() {
            Some(load) => load,
            None => match rx.recv() {
                Ok(SttCmd::Load {
                    model_path,
                    use_gpu,
                }) => (model_path, use_gpu),
                Ok(SttCmd::Transcribe { ditado, .. }) => {
                    let _ = events.send(SttEvent::Failed {
                        ditado,
                        message: "O modelo ainda não terminou de carregar.".to_string(),
                    });
                    continue;
                }
                Err(_) => return,
            },
        };

        let _ = events.send(SttEvent::Loading);

        if !model_path.exists() {
            let _ = events.send(SttEvent::LoadFailed(format!(
                "O modelo de transcrição ainda não está aqui ({}).",
                crate::config::caminho_curto(&model_path)
            )));
            continue;
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
                let _ = events.send(SttEvent::LoadFailed(format!(
                    "Não consegui carregar o modelo ({}). O arquivo pode estar \
                     incompleto — apague-o e baixe de novo.",
                    crate::config::caminho_curto(&model_path)
                )));
                continue;
            }
        };

        let mut state = match context.create_state() {
            Ok(state) => state,
            Err(e) => {
                log::error!("whisper não criou o estado: {e}");
                let _ = events.send(SttEvent::LoadFailed(
                    "Não consegui preparar a transcrição. Se a GPU estiver sem \
                     memória, desligue o uso dela em Configurações → Desempenho."
                        .to_string(),
                ));
                continue;
            }
        };

        log::info!(
            "modelo carregado: {} (backend {BACKEND}, gpu={})",
            model_path.display(),
            use_gpu && GPU_CAPABLE
        );
        let _ = events.send(SttEvent::Ready);

        // Laço de trabalho: vive enquanto este modelo estiver carregado.
        loop {
            match rx.recv() {
                Ok(SttCmd::Load {
                    model_path,
                    use_gpu,
                }) => {
                    pending = Some((model_path, use_gpu));
                    break;
                }
                Ok(SttCmd::Transcribe {
                    ditado,
                    samples,
                    sample_rate,
                    options,
                }) => {
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
                }
                Err(_) => {
                    // O canal fechou: o programa está encerrando. Não liberamos
                    // os buffers da GPU aqui — fazer isso enquanto a thread
                    // principal desmonta o contexto gráfico derruba o driver da
                    // NVIDIA (SIGSEGV dentro de ggml_backend_vk_buffer_free_buffer,
                    // com as duas threads dentro de libnvidia-glcore). O processo
                    // vai morrer em seguida e o sistema recupera a memória.
                    //
                    // Só vale para o encerramento: ao trocar de modelo (o `break`
                    // acima) o contexto é liberado normalmente, com o restante do
                    // programa vivo e parado.
                    std::mem::forget(state);
                    std::mem::forget(context);
                    return;
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
