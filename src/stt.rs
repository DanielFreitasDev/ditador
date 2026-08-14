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
}

#[derive(Debug)]
pub enum SttCmd {
    Load { model_path: PathBuf, use_gpu: bool },
    Transcribe(Vec<f32>, TranscribeOptions),
}

#[derive(Debug)]
pub enum SttEvent {
    Loading,
    Ready,
    LoadFailed(String),
    Done { text: String, elapsed_ms: u128 },
    Failed(String),
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
                Ok(SttCmd::Transcribe(..)) => {
                    let _ = events.send(SttEvent::Failed(
                        "o modelo ainda não foi carregado".to_string(),
                    ));
                    continue;
                }
                Err(_) => return,
            },
        };

        let _ = events.send(SttEvent::Loading);

        if !model_path.exists() {
            let _ = events.send(SttEvent::LoadFailed(format!(
                "modelo não encontrado em {}. Rode ./baixar-modelo.sh",
                model_path.display()
            )));
            continue;
        }

        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu && GPU_CAPABLE);

        let context = match WhisperContext::new_with_params(&model_path, params) {
            Ok(ctx) => ctx,
            Err(e) => {
                let _ = events.send(SttEvent::LoadFailed(format!(
                    "falha ao carregar {}: {e}",
                    model_path.display()
                )));
                continue;
            }
        };

        let mut state = match context.create_state() {
            Ok(state) => state,
            Err(e) => {
                let _ = events.send(SttEvent::LoadFailed(format!(
                    "falha ao criar o estado do Whisper: {e}"
                )));
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
                Ok(SttCmd::Transcribe(samples, options)) => {
                    let started = Instant::now();
                    let audio_secs = samples.len() as f64 / WHISPER_SAMPLE_RATE as f64;
                    match transcribe(&mut state, samples, &options) {
                        Ok(text) => {
                            let elapsed_ms = started.elapsed().as_millis();
                            log::info!(
                                "transcrição: {audio_secs:.1} s de áudio em {elapsed_ms} ms \
                                 ({} caracteres)",
                                text.chars().count()
                            );
                            log::debug!("texto: {text}");
                            let _ = events.send(SttEvent::Done { text, elapsed_ms });
                        }
                        Err(e) => {
                            let _ = events.send(SttEvent::Failed(e));
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
    mut samples: Vec<f32>,
    options: &TranscribeOptions,
) -> Result<String, String> {
    if samples.len() < MIN_SAMPLES {
        samples.resize(MIN_SAMPLES, 0.0);
    }

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(options.threads.max(1));
    params.set_translate(options.translate);
    params.set_language(options.language.as_deref());
    // Cada ditado é independente: sem isso, uma frase contamina a seguinte.
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
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

/// Marcadores como "[BLANK_AUDIO]", "(música)", "[Risos]".
fn is_non_speech_marker(text: &str) -> bool {
    (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('(') && text.ends_with(')'))
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
}
