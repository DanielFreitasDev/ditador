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

            AudioEvent::Failed(message) => {
                let mut state = lock(&self.shared);
                state.recording_since = None;
                state.message = format!("Não consegui acessar o microfone: {message}");
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            AudioEvent::Captured {
                samples,
                duration_ms,
            } => {
                let (too_short, options) = {
                    let state = lock(&self.shared);
                    (
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
                    self.set_view(View::Hidden);
                    return;
                }

                {
                    let mut state = lock(&self.shared);
                    state.view = View::Processing;
                    state.status = format!("{:.1} s de áudio", duration_ms as f64 / 1000.0);
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

    fn on_transcription(&self, text: String, elapsed_ms: u128) {
        let (auto_copy, auto_paste) = {
            let state = lock(&self.shared);
            (state.config.auto_copy, state.config.auto_paste)
        };

        let mut copy_error = None;
        if !text.is_empty() && (auto_copy || auto_paste) {
            if let Err(e) = clipboard::copy(&text) {
                copy_error = Some(format!("{e:#}"));
            }
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

            if text.is_empty() {
                state.text.clear();
                state.message = "Não identifiquei fala no áudio.".to_string();
                state.view = View::Error;
            } else {
                state.text = text.clone();
                state.message = copy_error.clone().unwrap_or_default();
                state.copied_at = if copy_error.is_none() && (auto_copy || auto_paste) {
                    Some(Instant::now())
                } else {
                    None
                };
                // Com colagem automática a janela não aparece: o texto vai
                // direto para onde o usuário estava digitando.
                state.view = if auto_paste && copy_error.is_none() {
                    View::Hidden
                } else {
                    View::Result
                };
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
                let recording = lock(&self.shared).view == View::Recording;
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
        {
            let mut state = lock(&self.shared);

            // Não atrapalha quem está mexendo nas configurações.
            if state.view == View::Settings || state.capturing_hotkey {
                return;
            }
            if state.view == View::Recording {
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
        }
        self.audio.send(AudioCmd::Start);
        self.sinal.mudou();
    }

    fn stop_recording(&self) {
        {
            let mut state = lock(&self.shared);
            if state.view != View::Recording {
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

    fn set_view(&self, view: View) {
        lock(&self.shared).view = view;
        self.sinal.mudou();
    }
}
