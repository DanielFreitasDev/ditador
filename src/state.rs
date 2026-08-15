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
    /// volta da superfície, onde a sombra é desenhada.
    pub fn size(self) -> [f32; 2] {
        let pad = 2.0 * crate::tema::FOLGA_SOMBRA;
        let [w, h] = match self {
            View::Hidden | View::Recording | View::Processing => [440.0, 178.0],
            View::Result => [620.0, 372.0],
            View::Settings => [660.0, 660.0],
            // A mais alta das mensagens é a do modelo faltando: título, duas
            // linhas de texto, os botões e a nota de rodapé — mais o aviso do
            // atalho, que aparece embaixo de tudo isso e é justamente o caso da
            // primeira execução, quando o usuário ainda não está no grupo
            // `input`. As duas linhas dele são a folga a mais aqui.
            View::Error => [520.0, 268.0],
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
    /// Para o download em curso e apaga o arquivo pela metade.
    CancelDownload,
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
    /// Aviso de que o atalho global não está funcionando. Mora fora do
    /// `message` porque os dois nascem no mesmo instante do arranque — teclado
    /// ilegível e modelo faltando — e, dividindo um campo só, o segundo apagava
    /// o primeiro antes de alguém ler.
    pub aviso_atalho: Option<String>,
    pub capturing_hotkey: bool,
    pub copied_at: Option<Instant>,
    /// Quando a gravação em curso começou. É ele, e não a tela, que diz se o
    /// microfone está aberto — a janela de um resultado pode aparecer por cima
    /// de um ditado em andamento.
    pub recording_since: Option<Instant>,
    /// Número do ditado em curso. Cresce a cada gravação, e volta nos eventos
    /// do áudio e da transcrição, para distinguir o que é do ditado de agora e
    /// o que é de um anterior que ainda estava a caminho.
    pub ditado_atual: u64,
    /// A tela de erro está no ar só esperando o modelo carregar, e não por uma
    /// falha. É o que decide se ela some sozinha quando o modelo fica pronto.
    ///
    /// Existe porque essa decisão já foi tomada comparando o texto exibido ao
    /// usuário (`message.starts_with("Carregando")`), e o caminho do download
    /// escrevia outra frase: a tela ficava presa com o emblema de erro ao lado
    /// de uma mensagem de sucesso. Texto de interface não é lugar de guardar
    /// estado — mudar uma vírgula na frase quebrava o fluxo em silêncio.
    pub erro_e_so_espera: bool,
    pub result_shown_at: Option<Instant>,
    pub devices: Vec<String>,
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
            aviso_atalho: None,
            capturing_hotkey: false,
            copied_at: None,
            recording_since: None,
            ditado_atual: 0,
            erro_e_so_espera: false,
            result_shown_at: None,
            devices,
            download: None,
            quitting: false,
        }
    }

    /// O microfone está aberto?
    ///
    /// Uma pergunta só, num lugar só. A resposta sai do `recording_since` e
    /// nunca da tela — a armadilha que este projeto já pisou duas vezes é
    /// justamente alguém consultar `view == View::Recording`, que é falso
    /// enquanto a janela do ditado anterior está por cima de quem fala agora.
    pub fn gravando(&self) -> bool {
        self.recording_since.is_some()
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
