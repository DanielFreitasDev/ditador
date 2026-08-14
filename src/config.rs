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
///
/// Os padrões são os da extensão de GNOME `ryohsuke1231/liquid-glass` (o
/// `gschema.xml` dela), que é a referência visual deste vidro: vidro claro e
/// quase transparente, refração forte, borda acesa em volta inteira e sem
/// reflexo concentrado. Os nomes seguem os de lá para dar para comparar valor
/// a valor.
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
    /// Brilho e saturação aplicados a ele antes de entrar. A extensão não mexe
    /// em nenhum dos dois (`dock-brightness`/`-saturation` = 1,0).
    pub wallpaper_brightness: f32,
    pub wallpaper_saturation: f32,
    /// Raio, em pixels, do desfoque aplicado ao que o vidro refrata. Na
    /// extensão é `dock-blur-radius`: ela sempre refrata um fundo já borrado.
    pub blur_radius: f32,

    /// Tinta do corpo (`dock-tint-color`) e o quanto dela entra
    /// (`dock-tint-strength`). Branco a 12% é o que dá o vidro claro.
    pub tint: [u8; 3],
    pub tint_strength: f32,
    /// Raio dos cantos do painel (`dock-corner-radius`). É ele também que dita
    /// a largura da faixa em que a superfície sobe da borda — na extensão a
    /// altura é normalizada justamente pelo raio.
    pub corner_radius: f32,
    /// Expoente do perfil da superfície (`glass-profile-shape-n`). Quanto
    /// maior, mais depressa o vidro sobe junto da borda e mais cedo achata no
    /// meio — é o que dá a cara de almofada.
    pub profile_n: f32,
    /// Altura máxima do relevo (`glass-max-z`), em pixels. Manda na inclinação
    /// da normal, ou seja, em para onde a luz entorta.
    pub max_z: f32,
    /// Escala do deslocamento da refração (`glass-displacement-scale`), em
    /// pixels. É o quanto o fundo anda de lado ao ser visto pelo bisel.
    pub displacement: f32,
    /// Largura, em pixels, do amaciamento da silhueta (`glass-edge-smoothing`).
    pub edge_smoothing: f32,
    /// Índice de refração (`glass-ior`). 1,0 = nada entorta; vidro real ~1,5;
    /// o padrão da extensão, 2,4, é bem mais duro do que vidro de verdade.
    pub ior: f32,
    /// Separação das componentes de cor na refração (`glass-chroma-strength`),
    /// em pixels.
    pub chroma: f32,

    /// Ângulo da luz, em graus (`glass-light-angle-deg`).
    pub light_angle: f32,
    /// Reflexo concentrado (`glass-specular-intensity`) e o quanto ele fecha
    /// (`glass-shininess`). A extensão vem com o reflexo desligado.
    pub specular: f32,
    pub shininess: f32,
    /// A borda acesa: largura, intensidade, o quanto ela segue a direção da luz,
    /// o expoente de Fresnel e o ganho de cor — `glass-rim-*`.
    pub rim_width: f32,
    pub rim_intensity: f32,
    pub rim_directional_power: f32,
    pub rim_power: f32,
    pub rim_color_intensity: f32,
    /// Véu amplo da face (`glass-sheen-intensity`).
    pub sheen: f32,
    /// Escurecimento por dentro, junto da borda: intensidade e alcance em
    /// pixels (`glass-ao-intensity`, `glass-ao-radius`).
    pub ao: f32,
    pub ao_radius: f32,
    /// Sombra projetada: raio em pixels e intensidade (`shadow-radius`,
    /// `shadow-intensity`).
    pub shadow_radius: f32,
    pub shadow_intensity: f32,

    /// Escolher a cor do texto pelo brilho do que está atrás, como o
    /// `enable-adaptive-text-color` da extensão. Com o vidro claro do padrão é
    /// o que mantém o texto legível sobre um papel de parede claro.
    pub adaptive_text: bool,

    /// Animação de mola ao abrir.
    pub animation: bool,
    /// Duração dela, em milissegundos.
    pub animation_ms: u64,
    /// Quanto ela ultrapassa o alvo antes de assentar, de 0 (nada) a 1.
    pub animation_bounce: f32,
    /// Tamanho de onde o painel parte, em fração do tamanho final.
    pub animation_scale: f32,
}

/// As duas tintas do vidro, que são as mesmas duas que o macOS 26.1 passou a
/// oferecer no botão **Clear / Tinted** da Aparência depois das reclamações de
/// legibilidade.
///
/// A escura é o padrão daqui pelo mesmo motivo que é o da Apple numa janela: no
/// System Settings do macOS a área de conteúdo é praticamente opaca, e o vidro
/// translúcido de verdade fica só na moldura — barra lateral, barra de
/// ferramentas, dock, Central de Controle. A óptica (refração, borda acesa,
/// véu, sombra) é a mesma nas duas; o que muda é a densidade do corpo.
pub const TINTA_ESCURA: [u8; 3] = [20, 21, 28];
pub const FORCA_ESCURA: f32 = 0.68;
/// A clara é o padrão da extensão de GNOME (branco a 20%, o valor que ela usa
/// nos menus). Deixa o papel de parede aparecer; sobre um papel de parede
/// movimentado, custa legibilidade.
pub const TINTA_CLARA: [u8; 3] = [255, 255, 255];
pub const FORCA_CLARA: f32 = 0.20;

impl Appearance {
    pub const PADRAO: Self = Self {
        wallpaper: true,
        // O papel de parede entra quase todo, mas por baixo de uma tinta densa:
        // é ele que dá à superfície a cor do ambiente (o "tint window
        // background with wallpaper colour" do macOS), não o desenho.
        wallpaper_opacity: 0.92,
        // Borrado com força, de propósito. Numa janela cheia de texto a Apple
        // não deixa o papel de parede legível por trás — no macOS a área de
        // conteúdo é praticamente opaca, e no iOS 26 beta 2 a Central de
        // Controle ganhou justamente mais desfoque e mais escuro por causa da
        // ilegibilidade. Reduzir a imagem a esta largura é o passa-baixa que
        // faz isso: o que atravessa o vidro é a luz e a cor do papel de parede,
        // distorcidas pela beirada, não o desenho dele.
        wallpaper_detail: 300,
        wallpaper_brightness: 1.0,
        wallpaper_saturation: 1.0,
        blur_radius: 5.0,

        tint: TINTA_ESCURA,
        tint_strength: FORCA_ESCURA,
        corner_radius: 30.0,
        profile_n: 7.0,
        max_z: 25.0,
        displacement: 78.5,
        edge_smoothing: 2.0,
        ior: 2.40,
        chroma: 0.006,

        light_angle: 50.0,
        specular: 0.0,
        shininess: 42.0,
        rim_width: 5.0,
        rim_intensity: 0.6,
        rim_directional_power: 2.7,
        rim_power: 6.0,
        rim_color_intensity: 1.4,
        sheen: 0.32,
        ao: 0.25,
        ao_radius: 7.5,
        shadow_radius: 30.0,
        shadow_intensity: 0.55,

        adaptive_text: true,

        animation: true,
        animation_ms: 260,
        animation_bounce: 0.6,
        animation_scale: 0.94,
    };

    /// Apara os valores para faixas em que o desenho continua fazendo sentido.
    /// O arquivo é editável à mão, e um índice de refração de 40 ou um papel de
    /// parede de 1 pixel deixariam a janela ilegível. As faixas são as do
    /// `gschema.xml` da extensão onde ele declara alguma.
    pub fn sanear(&mut self) {
        self.wallpaper_opacity = self.wallpaper_opacity.clamp(0.0, 1.0);
        self.wallpaper_detail = self.wallpaper_detail.clamp(16, 3840);
        self.wallpaper_brightness = self.wallpaper_brightness.clamp(0.05, 2.0);
        self.wallpaper_saturation = self.wallpaper_saturation.clamp(0.0, 3.0);
        self.blur_radius = self.blur_radius.clamp(0.0, 40.0);

        self.tint_strength = self.tint_strength.clamp(0.0, 1.0);
        self.corner_radius = self.corner_radius.clamp(0.0, 80.0);
        self.profile_n = self.profile_n.clamp(1.01, 20.0);
        self.max_z = self.max_z.clamp(0.0, 200.0);
        self.displacement = self.displacement.clamp(0.0, 400.0);
        self.edge_smoothing = self.edge_smoothing.clamp(0.0, 10.0);
        self.ior = self.ior.clamp(1.0, 3.0);
        self.chroma = self.chroma.clamp(0.0, 20.0);

        self.light_angle = self.light_angle.rem_euclid(360.0);
        self.specular = self.specular.clamp(0.0, 5.0);
        self.shininess = self.shininess.clamp(1.0, 200.0);
        self.rim_width = self.rim_width.clamp(0.0, 40.0);
        self.rim_intensity = self.rim_intensity.clamp(0.0, 5.0);
        self.rim_directional_power = self.rim_directional_power.clamp(1.0, 20.0);
        self.rim_power = self.rim_power.clamp(0.001, 20.0);
        self.rim_color_intensity = self.rim_color_intensity.clamp(0.0, 5.0);
        self.sheen = self.sheen.clamp(0.0, 3.0);
        self.ao = self.ao.clamp(0.0, 1.0);
        self.ao_radius = self.ao_radius.clamp(0.0, 50.0);
        self.shadow_radius = self.shadow_radius.clamp(0.0, 100.0);
        self.shadow_intensity = self.shadow_intensity.clamp(0.0, 1.0);

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

/// Temas prontos. Mexem só no que define o *material* — densidade do corpo,
/// quanto do papel de parede entra, força da óptica. Preferências que não são
/// do tema (animação de abertura, atalho, tudo o mais) ficam como estão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tema {
    /// O padrão: corpo denso, como a área de conteúdo de uma janela do macOS.
    Denso,
    /// O padrão da extensão de GNOME: branco a 20%, o papel de parede aparece.
    Claro,
    /// Sem papel de parede nenhum e corpo quase opaco. É o de máximo contraste,
    /// e o mais barato de desenhar — serve para tela pequena, monitor fraco ou
    /// quem simplesmente não quer o desktop competindo com o texto.
    Fosco,
    /// O oposto: vidro fino, refração forte, beirada acesa. É bonito e é o que
    /// mostra a óptica; sobre um papel de parede movimentado, custa leitura.
    Cristal,
}

impl Tema {
    pub const TODOS: [Tema; 4] = [Tema::Denso, Tema::Claro, Tema::Fosco, Tema::Cristal];

    pub fn nome(self) -> &'static str {
        match self {
            Tema::Denso => "Denso",
            Tema::Claro => "Claro",
            Tema::Fosco => "Fosco",
            Tema::Cristal => "Cristal",
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            Tema::Denso => "O padrão. Corpo escuro, papel de parede só como cor de ambiente.",
            Tema::Claro => "Vidro claro, com o papel de parede aparecendo através dele.",
            Tema::Fosco => "Sem papel de parede e quase opaco: contraste máximo.",
            Tema::Cristal => "Vidro fino e refração forte. O mais bonito, o menos legível.",
        }
    }

    /// Escreve o tema por cima da aparência, deixando o resto intacto.
    pub fn aplicar(self, a: &mut Appearance) {
        let p = Appearance::PADRAO;
        match self {
            Tema::Denso => {
                a.tint = TINTA_ESCURA;
                a.tint_strength = FORCA_ESCURA;
                a.wallpaper = true;
                a.wallpaper_opacity = p.wallpaper_opacity;
                a.wallpaper_detail = p.wallpaper_detail;
                a.blur_radius = p.blur_radius;
                a.ior = p.ior;
                a.displacement = p.displacement;
                a.rim_intensity = p.rim_intensity;
                a.sheen = p.sheen;
            }
            Tema::Claro => {
                a.tint = TINTA_CLARA;
                a.tint_strength = FORCA_CLARA;
                a.wallpaper = true;
                a.wallpaper_opacity = 0.82;
                // Bem menos borrado: no vidro claro é o desenho do papel de
                // parede, entortado pela beirada, que faz a peça parecer vidro.
                a.wallpaper_detail = 1280;
                a.blur_radius = p.blur_radius;
                a.ior = p.ior;
                a.displacement = p.displacement;
                a.rim_intensity = p.rim_intensity;
                a.sheen = p.sheen;
            }
            Tema::Fosco => {
                a.tint = [18, 19, 26];
                a.tint_strength = 0.94;
                a.wallpaper = false;
                a.blur_radius = 0.0;
                // A refração continua existindo nos cartões e nos botões, que
                // refratam o painel; no painel em si, sem fundo, ela não teria
                // o que entortar.
                a.displacement = 40.0;
                a.ior = 1.9;
                a.rim_intensity = 0.7;
                a.sheen = 0.26;
            }
            Tema::Cristal => {
                a.tint = TINTA_CLARA;
                a.tint_strength = 0.08;
                a.wallpaper = true;
                a.wallpaper_opacity = 0.92;
                a.wallpaper_detail = 1600;
                a.blur_radius = 3.0;
                a.ior = 2.6;
                a.displacement = 120.0;
                a.rim_intensity = 0.9;
                a.sheen = 0.45;
            }
        }
        // Com corpo claro, texto claro some sobre papel de parede claro.
        a.adaptive_text = true;
        a.sanear();
    }

    /// Qual tema a aparência atual representa, se for algum deles — é o que
    /// deixa o seletor mostrar o que está valendo depois de reabrir a janela.
    pub fn atual(a: &Appearance) -> Option<Tema> {
        Tema::TODOS.into_iter().find(|t| {
            let mut candidato = *a;
            t.aplicar(&mut candidato);
            candidato == *a
        })
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
    fn o_padrao_e_o_tema_denso_e_cada_tema_se_reconhece() {
        // O seletor descobre o tema comparando, então um tema que não voltasse
        // igual a si mesmo apareceria para sempre como "Personalizado".
        assert_eq!(Tema::atual(&Appearance::PADRAO), Some(Tema::Denso));
        for tema in Tema::TODOS {
            let mut a = Appearance::PADRAO;
            tema.aplicar(&mut a);
            assert_eq!(Tema::atual(&a), Some(tema), "{}", tema.nome());
        }
    }

    #[test]
    fn mexer_num_controle_sai_do_tema() {
        let mut a = Appearance::PADRAO;
        a.tint_strength += 0.1;
        assert_eq!(Tema::atual(&a), None);
    }

    #[test]
    fn valores_absurdos_do_arquivo_sao_aparados() {
        let mut a = Appearance {
            ior: 40.0,
            wallpaper_detail: 1,
            wallpaper_opacity: -3.0,
            animation_scale: 0.0,
            ..Appearance::PADRAO
        };
        a.sanear();
        assert_eq!(a.ior, 3.0);
        assert_eq!(a.wallpaper_detail, 16);
        assert_eq!(a.wallpaper_opacity, 0.0);
        assert_eq!(a.animation_scale, 0.3);
        // O padrão passa incólume.
        let mut padrao = Appearance::PADRAO;
        padrao.sanear();
        assert_eq!(padrao, Appearance::PADRAO);
    }
}
