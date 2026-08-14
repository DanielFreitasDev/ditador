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
    /// Abrir a janela com o texto ao terminar de transcrever. Desligue quando a
    /// cópia automática já resolve: o texto vai para a área de transferência e
    /// nada aparece na tela. Só vale quando o texto de fato chegou lá — senão a
    /// transcrição se perderia sem ninguém ver.
    pub show_result: bool,
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
    /// Subir junto com a sessão gráfica (serviço de usuário do systemd).
    /// Não é lido daqui: quem manda é o próprio systemd, e este campo só guarda
    /// o que o usuário escolheu na última vez (ver `autostart.rs`).
    pub start_with_session: bool,
    /// Tema e animação da janela.
    pub appearance: Appearance,
}

/// Aparência da janela.
///
/// É curta de propósito. O visual é sólido e tem só duas versões — clara e
/// escura —, então não há o que regular: o que existia aqui antes era a régua
/// de um efeito óptico que não existe mais.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Tema,
    /// Animação de entrada da janela.
    pub animation: bool,
    /// Duração dela, em milissegundos.
    pub animation_ms: u64,
}

impl Appearance {
    pub const PADRAO: Self = Self {
        theme: Tema::Sistema,
        animation: true,
        animation_ms: 150,
    };

    /// O arquivo é editável à mão; uma animação de meio minuto não ajudaria
    /// ninguém.
    pub fn sanear(&mut self) {
        self.animation_ms = self.animation_ms.clamp(0, 1000);
    }

    /// Se a janela deve ser desenhada escura agora.
    pub fn escuro(&self) -> bool {
        self.theme.escuro()
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self::PADRAO
    }
}

/// Os dois temas, mais a opção de seguir o sistema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tema {
    /// Segue *Configurações → Aparência* do GNOME.
    #[default]
    Sistema,
    Claro,
    Escuro,
}

impl Tema {
    pub const TODOS: [Tema; 3] = [Tema::Sistema, Tema::Claro, Tema::Escuro];

    /// Nome curto, do tamanho de uma aba do seletor. O que cada um faz está na
    /// nota logo abaixo dele, não no rótulo.
    pub fn nome(self) -> &'static str {
        match self {
            Tema::Sistema => "Sistema",
            Tema::Claro => "Claro",
            Tema::Escuro => "Escuro",
        }
    }

    /// Se este tema pede o desenho escuro agora.
    pub fn escuro(self) -> bool {
        match self {
            Tema::Sistema => crate::tema::sistema_escuro(),
            Tema::Claro => false,
            Tema::Escuro => true,
        }
    }
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
            show_result: true,
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
            start_with_session: false,
            appearance: Appearance::PADRAO,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    cfg.appearance.sanear();
                    cfg
                }
                Err(e) => {
                    log::warn!(
                        "config inválida em {}: {e}. Usando padrões.",
                        path.display()
                    );
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
        std::fs::create_dir_all(&dir).with_context(|| format!("criando {}", dir.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_antiga_ganha_a_aparencia_padrao() {
        // Arquivos gravados antes desta versão não têm a seção de aparência, e
        // precisam continuar abrindo.
        let antiga = r#"{"language":"en","threads":4}"#;
        let cfg: Config = serde_json::from_str(antiga).expect("config antiga");
        assert_eq!(cfg.language, "en");
        assert_eq!(cfg.appearance, Appearance::PADRAO);
    }

    #[test]
    fn a_aparencia_do_vidro_e_lida_sem_derrubar_a_config() {
        // Quem usava a versão do vidro tem um `appearance` cheio de campos que
        // não existem mais. O arquivo inteiro precisa continuar abrindo, e a
        // aparência voltar ao padrão em vez de virar lixo.
        let vidro = r#"{
            "language": "en",
            "appearance": {
                "ior": 2.4, "tint": [20, 21, 28], "tint_strength": 0.68,
                "wallpaper": true, "animation": false, "animation_ms": 260
            }
        }"#;
        let cfg: Config = serde_json::from_str(vidro).expect("config do vidro");
        assert_eq!(cfg.language, "en");
        assert_eq!(cfg.appearance.theme, Tema::Sistema);
        // O que sobreviveu à mudança continua valendo.
        assert!(!cfg.appearance.animation);
        assert_eq!(cfg.appearance.animation_ms, 260);
    }

    #[test]
    fn os_temas_sao_gravados_por_nome() {
        let cfg = Config {
            appearance: Appearance {
                theme: Tema::Escuro,
                ..Appearance::PADRAO
            },
            ..Config::default()
        };
        let raw = serde_json::to_string(&cfg).expect("gravar");
        assert!(raw.contains(r#""theme":"escuro""#), "{raw}");
        let volta: Config = serde_json::from_str(&raw).expect("ler");
        assert_eq!(volta.appearance.theme, Tema::Escuro);
    }

    #[test]
    fn valores_absurdos_do_arquivo_sao_aparados() {
        let mut a = Appearance {
            animation_ms: 90_000,
            ..Appearance::PADRAO
        };
        a.sanear();
        assert_eq!(a.animation_ms, 1000);
        // O padrão passa incólume.
        let mut padrao = Appearance::PADRAO;
        padrao.sanear();
        assert_eq!(padrao, Appearance::PADRAO);
    }
}
