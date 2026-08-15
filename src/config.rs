//! Configuração persistida em ~/.config/ditador/config.json

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_NAME: &str = "ditador";
pub const DEFAULT_MODEL_FILE: &str = "ggml-large-v3-turbo-q5_0.bin";

/// Taxa de amostragem exigida pelo Whisper.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Os idiomas oferecidos na tela, e o nome de cada um.
///
/// Mora aqui, e não na interface, porque deixou de ter um público só: a
/// extensão do GNOME também mostra o idioma em uso, e uma segunda tabela do
/// lado do JavaScript envelheceria torta na primeira vez que alguém
/// acrescentasse uma língua aqui.
pub const IDIOMAS: &[(&str, &str)] = &[
    ("pt", "Português"),
    ("en", "Inglês"),
    ("es", "Espanhol"),
    ("fr", "Francês"),
    ("de", "Alemão"),
    ("it", "Italiano"),
    ("auto", "Detectar automaticamente"),
];

/// O nome do idioma, ou o próprio código quando ele não está na lista — o
/// arquivo é editável à mão e aceita qualquer código que o Whisper entenda.
pub fn nome_do_idioma(codigo: &str) -> &str {
    IDIOMAS
        .iter()
        .find(|(c, _)| *c == codigo)
        .map_or(codigo, |(_, nome)| *nome)
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Onde ficam os dados grandes: os modelos do Whisper.
///
/// **`data_local_dir`, e não `data_dir`.** No Linux os dois são a mesma pasta
/// (`~/.local/share`), então a troca não muda nada lá. No Windows eles são
/// coisas bem diferentes: `data_dir` é o **Roaming**, que o Windows sincroniza
/// entre as máquinas de um mesmo perfil de domínio, e `data_local_dir` é o
/// **Local**, que fica onde está.
///
/// O modelo padrão tem 574 MB. Deixá-lo no Roaming significaria meio giga
/// atravessando a rede a cada login numa rede corporativa, e o perfil do usuário
/// estourando a cota — um daqueles problemas que não aparecem na máquina de
/// quem programou e arruínam o dia de quem instalou.
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// O caminho encurtado para caber numa frase.
///
/// Escrito por extenso, o caminho do modelo sozinho ocupa três linhas da janela
/// de erro. A abreviação segue a convenção de cada sistema — `~/` no Unix,
/// `%LOCALAPPDATA%` e companhia no Windows —, porque um `~/AppData\Roaming\…`
/// não é como ninguém escreve um caminho no Windows e faz o leitor duvidar de
/// que o programa saiba onde está.
pub fn caminho_curto(caminho: &std::path::Path) -> String {
    encurtar(caminho)
}

#[cfg(target_os = "windows")]
fn encurtar(caminho: &std::path::Path) -> String {
    // Da pasta mais específica para a mais geral: `LocalAppData` e o `Roaming`
    // estão *dentro* do perfil do usuário, então conferi-los antes evita que a
    // resposta seja o prefixo mais curto e menos informativo
    // (`%USERPROFILE%\AppData\Local\…` em vez de `%LOCALAPPDATA%\…`).
    for (variavel, pasta) in [
        ("%LOCALAPPDATA%", dirs::data_local_dir()),
        ("%APPDATA%", dirs::config_dir()),
        ("%USERPROFILE%", dirs::home_dir()),
    ] {
        if let Some(pasta) = pasta
            && let Ok(resto) = caminho.strip_prefix(&pasta)
        {
            return format!("{variavel}\\{}", resto.display());
        }
    }
    caminho.display().to_string()
}

#[cfg(not(target_os = "windows"))]
fn encurtar(caminho: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|casa| caminho.strip_prefix(casa).ok()) {
        Some(resto) => format!("~/{}", resto.display()),
        None => caminho.display().to_string(),
    }
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
    /// Após copiar, colar automaticamente na janela em foco (ydotool no Linux,
    /// SendInput no Windows).
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
        Self::ler_de(&config_path())
    }

    pub fn save(&self) -> Result<()> {
        self.salvar_em(&config_path())
    }

    /// Lê a configuração de um caminho qualquer.
    ///
    /// `load` é uma casca fina em volta desta função para que o caminho de
    /// arquivo — quarentena do inválido, sobrevivência do anterior — possa ser
    /// testado numa pasta temporária em vez de na configuração de quem roda os
    /// testes.
    pub fn ler_de(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    cfg.sanear();
                    cfg
                }
                Err(e) => {
                    log::warn!(
                        "config inválida em {}: {e}. Usando padrões.",
                        path.display()
                    );
                    // Guarda o arquivo de lado antes que a primeira gravação o
                    // substitua pelos padrões: o arquivo é editável à mão, e
                    // quem errou uma vírgula precisa poder ver o que escreveu.
                    let guardado = path.with_extension("json.invalida");
                    match std::fs::rename(path, &guardado) {
                        Ok(()) => log::warn!("a anterior ficou em {}", guardado.display()),
                        Err(e) => log::warn!("não consegui guardar a config inválida: {e}"),
                    }
                    Config::default()
                }
            },
            // Só a ausência do arquivo autoriza gravar os padrões por cima.
            //
            // Qualquer erro de leitura levava a este caminho antes, e o `save`
            // logo abaixo destruía o original sem nem guardar cópia — o oposto
            // do cuidado que o ramo do JSON inválido tem. Um arquivo ilegível
            // por um instante (permissão trocada, setor ruim, bytes que não são
            // UTF-8) custava o atalho, o idioma e o caminho do modelo de quem
            // usa o programa, sem uma linha no log dizendo o que aconteceu.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Config::default();
                if let Err(e) = cfg.salvar_em(path) {
                    log::warn!("não consegui gravar a config inicial: {e}");
                }
                cfg
            }
            Err(e) => {
                log::error!(
                    "não consegui ler {}: {e}. Sigo com os padrões desta vez, \
                     sem gravar nada por cima do seu arquivo.",
                    path.display()
                );
                Config::default()
            }
        }
    }

    /// Grava a configuração num caminho qualquer, de forma atômica.
    pub fn salvar_em(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
        }
        let raw = serde_json::to_string_pretty(self)?;

        // Grava ao lado e só então troca pelo definitivo. A troca é atômica
        // dentro da mesma pasta: uma queda no meio da escrita deixa o arquivo
        // anterior inteiro, em vez de um JSON pela metade que o `load` seguinte
        // recusaria — e aí as configurações de quem usa sumiriam sem aviso.
        //
        // O nome do temporário carrega o processo e um contador porque duas
        // threads chamam esta função: quem clica em Salvar e quem termina o
        // download do modelo. Com um nome fixo, a que abrisse depois truncava o
        // arquivo que a outra estava escrevendo, e a gravação perdida não
        // aparecia em lugar nenhum — a tela dizia que tinha salvo.
        static CONTADOR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = CONTADOR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let parcial = path.with_extension(format!("json.{}.{n}.parcial", std::process::id()));

        std::fs::write(&parcial, raw).with_context(|| format!("gravando {}", parcial.display()))?;
        if let Err(e) = std::fs::rename(&parcial, path) {
            let _ = std::fs::remove_file(&parcial);
            return Err(e).with_context(|| format!("gravando {}", path.display()));
        }
        Ok(())
    }

    /// Apara os valores que só um arquivo editado à mão produz.
    ///
    /// A tela já limita todos eles; o arquivo, não. `min_recording_ms` alto
    /// demais descartava calado todo ditado, e `threads` fora de faixa ia
    /// direto para o whisper.cpp.
    fn sanear(&mut self) {
        self.appearance.sanear();
        self.threads = self.threads.clamp(1, 32);
        self.min_recording_ms = self.min_recording_ms.min(5_000);
        self.max_recording_secs = self.max_recording_secs.clamp(1, 3_600);
        self.result_timeout_secs = self.result_timeout_secs.min(3_600);
        if self.hotkey.is_empty() {
            self.hotkey = Config::default().hotkey;
        }
        // A ordem de exibição do atalho é decidida uma vez, aqui, e não a cada
        // quadro pela interface.
        crate::keys::sort_combo(&mut self.hotkey);
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
mod testes_de_caminho {
    use super::*;

    /// Os 574 MB do modelo não podem cair numa pasta que o Windows sincroniza.
    ///
    /// Este teste existe porque o erro já aconteceu: a primeira versão desta
    /// portabilidade usava `dirs::data_dir()`, que no Linux é `~/.local/share` e
    /// no Windows é o **Roaming** — e o `--diagnostico` da primeira execução no
    /// Windows apontou alegremente para
    /// `AppData\Roaming\ditador\models\`. Numa máquina de domínio isso seria
    /// meio giga atravessando a rede a cada login.
    #[test]
    #[cfg(target_os = "windows")]
    fn o_modelo_nao_vai_para_o_roaming() {
        let modelos = models_dir();
        let local = dirs::data_local_dir().expect("o Windows sempre tem LocalAppData");
        assert!(
            modelos.starts_with(&local),
            "os modelos saíram de LocalAppData: {}",
            modelos.display()
        );
        assert!(
            !modelos.to_string_lossy().contains("Roaming"),
            "os modelos foram parar no Roaming: {}",
            modelos.display()
        );
    }

    /// A configuração, ao contrário, *pode* acompanhar o usuário: são poucos
    /// quilobytes de preferências, e sincronizá-las entre as máquinas de um mesmo
    /// perfil é o comportamento desejável.
    #[test]
    #[cfg(target_os = "windows")]
    fn a_configuracao_pode_acompanhar_o_usuario() {
        let config = config_path();
        assert!(
            config.to_string_lossy().contains("Roaming"),
            "a configuração saiu do Roaming: {}",
            config.display()
        );
    }

    #[test]
    fn o_caminho_curto_usa_a_convencao_do_sistema() {
        let curto = caminho_curto(&models_dir());
        #[cfg(target_os = "windows")]
        assert!(
            curto.starts_with("%LOCALAPPDATA%\\"),
            "no Windows o caminho abreviado deve usar a variável de ambiente: {curto}"
        );
        #[cfg(not(target_os = "windows"))]
        assert!(
            curto.starts_with("~/"),
            "no Unix o caminho abreviado deve usar o til: {curto}"
        );
        // Um caminho que não está debaixo de nenhuma pasta conhecida sai inteiro.
        let alheio = std::path::Path::new(if cfg!(windows) {
            r"D:\outra\coisa"
        } else {
            "/opt/outra/coisa"
        });
        assert_eq!(caminho_curto(alheio), alheio.display().to_string());
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

    #[test]
    fn o_arquivo_editado_a_mao_tem_os_numeros_aparados_na_leitura() {
        // A tela limita todos estes; o arquivo, não. Um `min_recording_ms`
        // absurdo descartava calado todo ditado, e `threads` fora de faixa ia
        // direto para o whisper.cpp.
        let bruto = r#"{
            "threads": 9000,
            "min_recording_ms": 999999,
            "max_recording_secs": 0,
            "hotkey": []
        }"#;
        let cfg: Config = {
            let mut c: Config = serde_json::from_str(bruto).expect("config");
            c.sanear();
            c
        };
        assert_eq!(cfg.threads, 32);
        assert_eq!(cfg.min_recording_ms, 5_000);
        assert_eq!(cfg.max_recording_secs, 1);
        // Sem atalho nenhum o programa ficaria mudo, sem nada explicando.
        assert_eq!(cfg.hotkey, Config::default().hotkey);
    }

    /// Uma pasta só deste teste, para não encostar na config de quem o roda.
    fn pasta_de_teste(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ditador-teste-{}-{nome}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criando a pasta do teste");
        dir
    }

    #[test]
    fn a_config_vai_e_volta_do_disco() {
        let dir = pasta_de_teste("ida-e-volta");
        let arquivo = dir.join("config.json");

        // Arquivo ausente: nascem os padrões, e eles ficam gravados.
        let inicial = Config::ler_de(&arquivo);
        assert_eq!(inicial, Config::default());
        assert!(arquivo.is_file(), "a config inicial não foi gravada");

        let mudada = Config {
            language: "en".to_string(),
            threads: 3,
            ..Config::default()
        };
        mudada.salvar_em(&arquivo).expect("gravando");
        assert_eq!(Config::ler_de(&arquivo), mudada);

        // Nenhum temporário sobrou na pasta.
        let sobras: Vec<_> = std::fs::read_dir(&dir)
            .expect("lendo a pasta")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("parcial"))
            .collect();
        assert!(sobras.is_empty(), "temporários esquecidos: {sobras:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_config_invalida_vai_para_a_quarentena_em_vez_de_sumir() {
        let dir = pasta_de_teste("quarentena");
        let arquivo = dir.join("config.json");
        std::fs::write(&arquivo, "{ isto não é json").expect("gravando");

        let cfg = Config::ler_de(&arquivo);
        assert_eq!(cfg, Config::default());
        // O arquivo é editável à mão: quem errou uma vírgula precisa poder ver
        // o que escreveu.
        let guardado = dir.join("config.json.invalida");
        assert!(guardado.is_file(), "a config inválida foi destruída");
        assert_eq!(
            std::fs::read_to_string(&guardado).expect("lendo"),
            "{ isto não é json"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_config_ilegivel_nao_e_substituida_pelos_padroes() {
        // Só a ausência do arquivo autoriza gravar por cima. Qualquer outro
        // erro de leitura destruía o original sem nem guardar cópia.
        let dir = pasta_de_teste("ilegivel");
        // Uma pasta no lugar do arquivo: a leitura falha com algo que não é
        // NotFound, sem depender de mexer em permissões.
        let arquivo = dir.join("config.json");
        std::fs::create_dir(&arquivo).expect("criando a pasta no lugar do arquivo");

        let cfg = Config::ler_de(&arquivo);
        assert_eq!(cfg, Config::default());
        assert!(
            arquivo.is_dir(),
            "o que estava no lugar da config foi destruído"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
