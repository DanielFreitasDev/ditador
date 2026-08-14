//! Configuração persistida em ~/.config/ditador/config.json

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_NAME: &str = "ditador";
pub const DEFAULT_MODEL_FILE: &str = "ggml-large-v3-turbo-q5_0.bin";

/// Taxa de amostragem exigida pelo Whisper.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Teclas do atalho, em nomes evdev (ex.: ["KEY_PAUSE"] ou ["KEY_LEFTMETA", "KEY_SPACE"]).
    /// Enquanto todas estiverem pressionadas, grava.
    pub hotkey: Vec<String>,
    /// Caminho do modelo GGML do Whisper.
    pub model_path: PathBuf,
    /// Código do idioma ("pt", "en", ...) ou "auto" para detecção automática.
    pub language: String,
    /// Traduzir para inglês em vez de transcrever no idioma original.
    pub translate: bool,
    /// Copiar o resultado para a área de transferência automaticamente.
    pub auto_copy: bool,
    /// Após copiar, colar automaticamente na janela em foco (usa ydotool).
    pub auto_paste: bool,
    /// Usar aceleração por GPU (ignorado em builds só-CPU).
    pub use_gpu: bool,
    /// Threads de CPU usadas na inferência.
    pub threads: i32,
    /// Nome do dispositivo de entrada do cpal; None = padrão do sistema.
    pub input_device: Option<String>,
    /// Texto de contexto passado ao modelo (ajuda com jargão e pontuação).
    pub initial_prompt: String,
    /// Fecha a janela de resultado sozinha após N segundos (0 = nunca).
    pub result_timeout_secs: u64,
    /// Gravações mais curtas que isto são descartadas (evita toques acidentais).
    pub min_recording_ms: u64,
    /// Trava de segurança para o tamanho máximo de uma gravação.
    pub max_recording_secs: u64,
    /// Normalizar o volume do áudio antes de transcrever (ajuda microfones fracos).
    pub normalize_audio: bool,
    /// Forçar a janela pelo XWayland (X11), onde "sempre visível" e o
    /// posicionamento funcionam de verdade. Desligue para usar Wayland nativo.
    pub force_x11: bool,
    /// Manter o texto do resultado editável antes de copiar.
    pub editable_result: bool,
}

impl Default for Config {
    fn default() -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8) as i32;

        Self {
            hotkey: vec!["KEY_PAUSE".to_string()],
            model_path: models_dir().join(DEFAULT_MODEL_FILE),
            language: "pt".to_string(),
            translate: false,
            auto_copy: true,
            auto_paste: false,
            use_gpu: true,
            threads,
            input_device: None,
            initial_prompt: String::new(),
            result_timeout_secs: 0,
            min_recording_ms: 300,
            max_recording_secs: 120,
            normalize_audio: true,
            force_x11: true,
            editable_result: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(cfg) => cfg,
                Err(e) => {
                    log::warn!("config inválida em {}: {e}. Usando padrões.", path.display());
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                if let Err(e) = cfg.save() {
                    log::warn!("não consegui gravar a config inicial: {e}");
                }
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("criando {}", dir.display()))?;
        let raw = serde_json::to_string_pretty(self)?;
        let path = config_path();
        std::fs::write(&path, raw).with_context(|| format!("gravando {}", path.display()))?;
        Ok(())
    }

    /// Idioma no formato que o whisper.cpp espera (None = detecção automática).
    pub fn whisper_language(&self) -> Option<&str> {
        if self.language.is_empty() || self.language == "auto" {
            None
        } else {
            Some(self.language.as_str())
        }
    }
}
