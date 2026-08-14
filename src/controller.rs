//! Orquestra atalho → gravação → transcrição → resultado.

use crate::audio::{AudioCmd, AudioEvent, AudioHandle, AudioSettings};
use crate::clipboard;
use crate::config::Config;
use crate::hotkey::{HotkeyEvent, HotkeyListener};
use crate::state::{ModelState, SharedState, Sinal, UiAction, View, lock};
use crate::stt::{SttCmd, SttEvent, TranscribeOptions};
use crossbeam_channel::{Receiver, select};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Comandos vindos do socket (ícone do app, atalho do GNOME, terminal).
#[derive(Debug, Clone, Copy)]
pub enum IpcCommand {
    /// Alterna gravar/parar — útil quando não se pode segurar a tecla.
    Toggle,
    Settings,
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
}

impl Controller {
    pub fn run(self, channels: Channels) {
        self.apply_audio_settings();
        self.load_model();

        loop {
            select! {
                recv(channels.hotkey) -> msg => match msg {
                    Ok(event) => self.on_hotkey(event),
                    Err(_) => return,
                },
                recv(channels.audio) -> msg => match msg {
                    Ok(event) => self.on_audio(event),
                    Err(_) => return,
                },
                recv(channels.stt) -> msg => match msg {
                    Ok(event) => self.on_stt(event),
                    Err(_) => return,
                },
                recv(channels.ui) -> msg => match msg {
                    Ok(action) => self.on_ui(action),
                    Err(_) => return,
                },
                recv(channels.ipc) -> msg => match msg {
                    Ok(command) => self.on_ipc(command),
                    Err(_) => return,
                },
            }
        }
    }

    // ---------------------------------------------------------------- eventos

    fn on_hotkey(&self, event: HotkeyEvent) {
        match event {
            HotkeyEvent::Down => self.start_recording(),
            HotkeyEvent::Up => self.stop_recording(),

            HotkeyEvent::Captured(keys) => {
                let mut state = lock(&self.shared);
                state.capturing_hotkey = false;
                if !keys.is_empty() {
                    state.draft.hotkey = keys;
                    state.draft_revision += 1;
                }
                drop(state);
                self.sinal.mudou();
            }

            HotkeyEvent::Unavailable(message) => {
                let mut state = lock(&self.shared);
                state.message = message;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }
        }
    }

    fn on_audio(&self, event: AudioEvent) {
        match event {
            AudioEvent::Started => {}

            AudioEvent::Failed { ditado, message } => {
                let mut state = lock(&self.shared);
                // Um ditado que já foi substituído por outro não tem mais tela
                // para reclamar.
                if ditado != state.ditado_atual {
                    return;
                }
                state.recording_since = None;
                state.message = format!("Não consegui acessar o microfone: {message}");
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            AudioEvent::Captured {
                ditado,
                samples,
                duration_ms,
            } => {
                let (atual, too_short, options) = {
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
                        state.view = View::Processing;
                        state.status = format!("{:.1} s de áudio", duration_ms as f64 / 1000.0);
                    }
                    (
                        atual,
                        duration_ms < state.config.min_recording_ms,
                        TranscribeOptions {
                            language: state.config.whisper_language().map(str::to_string),
                            translate: state.config.translate,
                            threads: state.config.threads,
                            initial_prompt: state.config.initial_prompt.clone(),
                        },
                    )
                };

                if too_short {
                    log::debug!("gravação de {duration_ms} ms descartada (curta demais)");
                    if atual {
                        self.set_view(View::Hidden);
                    }
                    return;
                }

                self.sinal.mudou();
                let _ = self.stt.send(SttCmd::Transcribe(samples, options));
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
                if state.view == View::Error && state.message.starts_with("Carregando") {
                    state.view = View::Hidden;
                    state.message.clear();
                }
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::LoadFailed(message) => {
                let mut state = lock(&self.shared);
                state.model = ModelState::Failed;
                state.message = message;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Failed(message) => {
                let mut state = lock(&self.shared);
                state.message = message;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Done { text, elapsed_ms } => self.on_transcription(text, elapsed_ms),
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
    /// anterior não pode aparecer por cima de um ditado em andamento. A exceção
    /// é ela ser o único jeito de o texto não se perder — `tela_do_resultado`
    /// devolvendo `Result` —, aí ela aparece assim mesmo, e o microfone segue
    /// aberto por baixo.
    fn resultado_pode_aparecer(tela: View, gravando: bool) -> bool {
        !gravando || tela == View::Result
    }

    fn on_transcription(&self, text: String, elapsed_ms: u128) {
        let (auto_copy, auto_paste, show_result) = {
            let state = lock(&self.shared);
            (
                state.config.auto_copy,
                state.config.auto_paste,
                state.config.show_result,
            )
        };

        let mut copy_error = None;
        if !text.is_empty()
            && (auto_copy || auto_paste)
            && let Err(e) = clipboard::copy(&text)
        {
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

            let gravando = state.recording_since.is_some();
            if text.is_empty() {
                state.text.clear();
                state.message = "Não identifiquei fala no áudio.".to_string();
                if !gravando {
                    state.view = View::Error;
                }
            } else {
                state.text = text.clone();
                state.message = copy_error.clone().unwrap_or_default();
                let a_salvo = (auto_copy || auto_paste) && copy_error.is_none();
                state.copied_at = a_salvo.then(Instant::now);
                let tela = Self::tela_do_resultado(a_salvo, auto_paste, show_result);
                if Self::resultado_pode_aparecer(tela, gravando) {
                    state.view = tela;
                }
                state.result_shown_at = Some(Instant::now());
            }
        }
        self.sinal.mudou();

        if auto_paste && copy_error.is_none() && !text.is_empty() {
            let shared = self.shared.clone();
            let sinal = self.sinal.clone();
            std::thread::spawn(move || {
                // Espera a nossa janela sumir para o foco voltar ao aplicativo anterior.
                std::thread::sleep(Duration::from_millis(250));
                if let Err(e) = clipboard::paste() {
                    let mut state = lock(&shared);
                    state.message = format!("Copiei, mas não consegui colar: {e:#}");
                    state.view = View::Result;
                    drop(state);
                    sinal.mudou();
                }
            });
        }
    }

    fn on_ui(&self, action: UiAction) {
        match action {
            UiAction::Hide => self.set_view(View::Hidden),

            UiAction::Copy => self.copy_current(false),
            UiAction::Paste => self.copy_current(true),

            UiAction::OpenSettings => {
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.devices = crate::audio::list_input_devices();
                state.view = View::Settings;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CloseSettings => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.capturing_hotkey = false;
                state.view = View::Hidden;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::ApplyDraft => self.apply_draft(),

            UiAction::StartHotkeyCapture => {
                self.hotkey.begin_capture();
                let mut state = lock(&self.shared);
                state.capturing_hotkey = true;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CancelHotkeyCapture => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.capturing_hotkey = false;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::ReloadModel => self.load_model(),

            UiAction::DownloadModel => self.download_model(),

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
            IpcCommand::Settings => self.on_ui(UiAction::OpenSettings),
            IpcCommand::Quit => self.on_ui(UiAction::Quit),
        }
    }

    // ------------------------------------------------------------- gravação

    fn start_recording(&self) {
        let ditado = {
            let mut state = lock(&self.shared);

            // Não atrapalha quem está mexendo nas configurações.
            if state.view == View::Settings || state.capturing_hotkey {
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
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Failed => {
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Ready => {}
            }

            state.text.clear();
            state.message.clear();
            state.copied_at = None;
            state.recording_since = Some(Instant::now());
            state.view = View::Recording;
            state.ditado_atual += 1;
            state.ditado_atual
        };
        self.audio.send(AudioCmd::Start { ditado });
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
            state.view = View::Processing;
        }
        self.audio.send(AudioCmd::Stop);
        self.sinal.mudou();
    }

    // ---------------------------------------------------------------- apoio

    fn copy_current(&self, then_paste: bool) {
        let text = lock(&self.shared).text.clone();
        if text.is_empty() {
            return;
        }

        match clipboard::copy(&text) {
            Ok(()) => {
                let mut state = lock(&self.shared);
                state.copied_at = Some(Instant::now());
                state.message.clear();
                if then_paste {
                    state.view = View::Hidden;
                }
            }
            Err(e) => {
                let mut state = lock(&self.shared);
                state.message = format!("Não consegui copiar: {e:#}");
            }
        }
        self.sinal.mudou();

        if then_paste {
            let shared = self.shared.clone();
            let sinal = self.sinal.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                if let Err(e) = clipboard::paste() {
                    let mut state = lock(&shared);
                    state.message = format!("Copiei, mas não consegui colar: {e:#}");
                    state.view = View::Result;
                    drop(state);
                    sinal.mudou();
                }
            });
        }
    }

    fn apply_draft(&self) {
        let (draft, previous) = {
            let state = lock(&self.shared);
            (state.draft.clone(), state.config.clone())
        };

        if let Err(e) = draft.save() {
            let mut state = lock(&self.shared);
            state.message = format!("Não consegui gravar a configuração: {e:#}");
            drop(state);
            self.sinal.mudou();
            return;
        }

        if draft.hotkey != previous.hotkey {
            self.hotkey.set_target(&draft.hotkey);
        }

        let reload_model =
            draft.model_path != previous.model_path || draft.use_gpu != previous.use_gpu;

        {
            let mut state = lock(&self.shared);
            state.config = draft;
            state.capturing_hotkey = false;
            state.view = View::Hidden;
            state.message.clear();
        }
        self.hotkey.cancel_capture();
        self.apply_audio_settings();

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
            normalize: config.normalize_audio,
        }));
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
    fn download_model(&self) {
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

        let (andamento, pronto) = crate::modelo::baixar(crate::modelo::PADRAO, self.sinal.clone());
        lock(&self.shared).download = Some(andamento);
        self.sinal.mudou();

        let shared = self.shared.clone();
        let sinal = self.sinal.clone();
        let stt = self.stt.clone();
        let _ = std::thread::Builder::new()
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
    }

    fn set_view(&self, view: View) {
        lock(&self.shared).view = view;
        self.sinal.mudou();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
