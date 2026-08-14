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
    /// Aparência do vidro.
    pub appearance: Appearance,
}

/// Todos os parâmetros do vidro líquido. O que a interface expõe é um
/// subconjunto; o resto vive aqui para quem quiser mexer no arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Deixar o papel de parede da área de trabalho entrar por baixo do vidro,
    /// borrado, para o painel ter o que refratar. Nenhum compositor do Linux
    /// entrega o que está mesmo atrás da janela; isto é o mais perto disso.
    pub wallpaper: bool,
    /// Quanto dele aparece por baixo da tinta, de 0 a 1. Não vai a 1 de
    /// propósito: o resto do alfa é o que deixa o que está atrás aparecer.
    pub wallpaper_opacity: f32,
    /// Largura, em pixels, para a qual o papel de parede é reduzido — é o
    /// controle do desfoque. Menor = mais borrado.
    pub wallpaper_detail: u32,
    /// Brilho e saturação aplicados a ele antes de entrar.
    pub wallpaper_brightness: f32,
    pub wallpaper_saturation: f32,
    /// Índice de refração do material. 1,0 = nada entorta; vidro real ~1,5.
    pub refraction: f32,
    /// Multiplicador da espessura aparente: quanto a refração desloca.
    pub thickness: f32,
    /// Multiplicador da separação das cores na refração.
    pub chromatic: f32,
    /// Multiplicadores da luz: borda especular, reflexo, véu da face, oclusão.
    pub edge: f32,
    pub specular: f32,
    pub sheen: f32,
    pub occlusion: f32,
    /// Intensidade da sombra projetada do painel, de 0 a 1.
    pub shadow: f32,
    /// Animação de mola ao abrir.
    pub animation: bool,
    /// Duração dela, em milissegundos.
    pub animation_ms: u64,
    /// Quanto ela ultrapassa o alvo antes de assentar, de 0 (nada) a 1.
    pub animation_bounce: f32,
    /// Tamanho de onde o painel parte, em fração do tamanho final.
    pub animation_scale: f32,
}

impl Appearance {
    pub const PADRAO: Self = Self {
        wallpaper: true,
        wallpaper_opacity: 0.55,
        wallpaper_detail: 260,
        wallpaper_brightness: 0.55,
        wallpaper_saturation: 1.18,
        refraction: 1.52,
        thickness: 1.0,
        chromatic: 1.0,
        edge: 1.0,
        specular: 1.0,
        sheen: 1.0,
        occlusion: 1.0,
        shadow: 0.62,
        animation: true,
        animation_ms: 260,
        animation_bounce: 0.6,
        animation_scale: 0.94,
    };

    /// Apara os valores para faixas em que o desenho continua fazendo sentido.
    /// O arquivo é editável à mão, e um índice de refração de 40 ou um papel de
    /// parede de 1 pixel deixariam a janela ilegível.
    pub fn sanear(&mut self) {
        self.wallpaper_opacity = self.wallpaper_opacity.clamp(0.0, 1.0);
        self.wallpaper_detail = self.wallpaper_detail.clamp(16, 3840);
        self.wallpaper_brightness = self.wallpaper_brightness.clamp(0.05, 2.0);
        self.wallpaper_saturation = self.wallpaper_saturation.clamp(0.0, 3.0);
        self.refraction = self.refraction.clamp(1.0, 2.5);
        self.thickness = self.thickness.clamp(0.0, 3.0);
        self.chromatic = self.chromatic.clamp(0.0, 4.0);
        self.edge = self.edge.clamp(0.0, 3.0);
        self.specular = self.specular.clamp(0.0, 3.0);
        self.sheen = self.sheen.clamp(0.0, 3.0);
        self.occlusion = self.occlusion.clamp(0.0, 2.0);
        self.shadow = self.shadow.clamp(0.0, 1.0);
        self.animation_ms = self.animation_ms.clamp(0, 2000);
        self.animation_bounce = self.animation_bounce.clamp(0.0, 1.0);
        self.animation_scale = self.animation_scale.clamp(0.3, 1.0);
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self::PADRAO
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
    fn valores_absurdos_do_arquivo_sao_aparados() {
        let mut a = Appearance {
            refraction: 40.0,
            wallpaper_detail: 1,
            wallpaper_opacity: -3.0,
            animation_scale: 0.0,
            ..Appearance::PADRAO
        };
        a.sanear();
        assert_eq!(a.refraction, 2.5);
        assert_eq!(a.wallpaper_detail, 16);
        assert_eq!(a.wallpaper_opacity, 0.0);
        assert_eq!(a.animation_scale, 0.3);
        // O padrão passa incólume.
        let mut padrao = Appearance::PADRAO;
        padrao.sanear();
        assert_eq!(padrao, Appearance::PADRAO);
    }
}
