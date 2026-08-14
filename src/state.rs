//! Estado compartilhado entre o controlador e a interface.

use crate::config::Config;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Hidden,
    Recording,
    Processing,
    Result,
    Settings,
    Error,
}

impl View {
    /// Tamanho da janela para cada tela, em pontos lógicos. Inclui a folga em
    /// volta do painel de vidro, onde a sombra é desenhada.
    pub fn size(self) -> [f32; 2] {
        let pad = 2.0 * crate::glass::shadow_pad();
        let [w, h] = match self {
            View::Hidden | View::Recording | View::Processing => [440.0, 152.0],
            View::Result => [620.0, 372.0],
            View::Settings => [660.0, 660.0],
            View::Error => [500.0, 200.0],
        };
        [w + pad, h + pad]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Loading,
    Ready,
    Failed,
}

/// Ações que a interface pede ao controlador.
#[derive(Debug, Clone)]
pub enum UiAction {
    Hide,
    Copy,
    Paste,
    OpenSettings,
    CloseSettings,
    /// Aplica e grava o rascunho de configuração.
    ApplyDraft,
    StartHotkeyCapture,
    CancelHotkeyCapture,
    ReloadModel,
    /// Baixa o modelo sugerido (só faz sentido quando ele está faltando).
    DownloadModel,
    Quit,
}

pub struct Shared {
    pub view: View,
    /// Configuração em uso.
    pub config: Config,
    /// Cópia editável pela tela de configurações.
    pub draft: Config,
    pub model: ModelState,
    /// Texto transcrito (editável na tela de resultado).
    pub text: String,
    /// Mensagem de erro ou aviso.
    pub message: String,
    /// Linha de rodapé: tempo de processamento, backend etc.
    pub status: String,
    pub capturing_hotkey: bool,
    pub copied_at: Option<Instant>,
    pub recording_since: Option<Instant>,
    pub result_shown_at: Option<Instant>,
    pub devices: Vec<String>,
    /// Sinaliza para a interface que o rascunho mudou fora dela.
    pub draft_revision: u64,
    /// Download do modelo em curso, se houver.
    pub download: Option<crate::modelo::Andamento>,
    /// Pedido de encerramento; a interface fecha a janela ao ver isto.
    pub quitting: bool,
}

impl Shared {
    pub fn new(config: Config, devices: Vec<String>) -> Self {
        Self {
            view: View::Hidden,
            draft: config.clone(),
            config,
            model: ModelState::Loading,
            text: String::new(),
            message: String::new(),
            status: String::new(),
            capturing_hotkey: false,
            copied_at: None,
            recording_since: None,
            result_shown_at: None,
            devices,
            draft_revision: 0,
            download: None,
            quitting: false,
        }
    }
}

pub type SharedState = Arc<Mutex<Shared>>;

pub fn lock(shared: &SharedState) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sinal de "o estado mudou": repinta a interface e avisa quem mais estiver
/// observando — hoje, o ícone da barra superior.
#[derive(Clone, Default)]
pub struct Sinal {
    interface: Arc<Mutex<Option<egui::Context>>>,
    observadores: Arc<Mutex<Vec<crossbeam_channel::Sender<()>>>>,
}

impl Sinal {
    /// Liga a interface ao sinal. Só dá para fazer isso depois que o eframe
    /// cria o contexto, já lá dentro do laço de eventos.
    pub fn ligar_interface(&self, ctx: egui::Context) {
        *self.interface.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
    }

    /// Canal que recebe um aviso a cada mudança de estado.
    ///
    /// Capacidade 1 e envio sem bloqueio: avisos em rajada se fundem num só, e
    /// quem observa lê o estado atual depois de acordar — nunca uma fila de
    /// estados velhos.
    pub fn observar(&self) -> crossbeam_channel::Receiver<()> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.observadores
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }

    pub fn mudou(&self) {
        if let Some(ctx) = self
            .interface
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            ctx.request_repaint();
        }
        for observador in self
            .observadores
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            let _ = observador.try_send(());
        }
    }
}
