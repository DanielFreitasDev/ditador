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
        let pad = 2.0 * crate::glass::SHADOW_PAD;
        let [w, h] = match self {
            View::Hidden | View::Recording | View::Processing => [400.0, 138.0],
            View::Result => [560.0, 330.0],
            View::Settings => [600.0, 570.0],
            View::Error => [460.0, 186.0],
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
            quitting: false,
        }
    }
}

pub type SharedState = Arc<Mutex<Shared>>;

pub fn lock(shared: &SharedState) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Canal de repaint: o controlador acorda a interface quando o estado muda.
/// Só fica disponível depois que o eframe cria o contexto.
#[derive(Clone, Default)]
pub struct Repainter(Arc<Mutex<Option<egui::Context>>>);

impl Repainter {
    pub fn set(&self, ctx: egui::Context) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
    }

    pub fn wake(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ctx.request_repaint();
        }
    }
}
