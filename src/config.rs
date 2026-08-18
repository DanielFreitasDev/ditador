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
    // No modo portátil tudo mora ao lado do executável, e é por isso que a
    // pergunta é feita aqui e não em cada chamador: `config_dir` e `data_dir`
    // são as duas únicas portas para o disco deste programa.
    if let Some(pasta) = crate::portatil::pasta() {
        return pasta.join("config");
    }
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
    if let Some(pasta) = crate::portatil::pasta() {
        return pasta.join("dados");
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// Onde ficam as transcrições guardadas (ver `src/historico.rs`).
///
/// Debaixo de `data_dir`, e não de `config_dir`, porque é dado e não
/// preferência: cresce sozinho, tem política de limpeza e — quando o áudio é
/// guardado junto — chega a dezenas de megabytes. No Windows a distinção é a
/// mesma que vale para os modelos: isto **não** pode ir para o Roaming.
pub fn historico_dir() -> PathBuf {
    data_dir().join("historico")
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

    // ------------------------------------------------------ campos novos
    //
    // Daqui para baixo os nomes são em português, como manda a seção "Idioma"
    // do CLAUDE.md. Os de cima ficaram em inglês porque já estão gravados no
    // `config.json` de quem usa o programa, e renomeá-los apagaria a
    // preferência de todo mundo na primeira leitura.
    /// Manter o microfone aberto o tempo todo, descartando o que chega, para
    /// que apertar a tecla comece a gravar na hora.
    ///
    /// Sem isto o `cpal` abre o dispositivo no instante do aperto, o que leva de
    /// 40 ms a algumas centenas — e é onde a primeira sílaba se perde. É a
    /// limitação que o README sempre listou.
    pub microfone_sempre_aberto: bool,
    /// Qual canal do dispositivo usar; `None` mistura todos.
    ///
    /// Só importa em interface de áudio com várias entradas: misturando tudo, um
    /// canal vazio entra como chiado por cima da voz.
    pub canal_do_microfone: Option<u16>,
    /// Como a colagem automática entrega o texto.
    pub metodo_de_colagem: MetodoDeColagem,
    /// O que apertar depois de colar — para ditar direto num campo de chat.
    pub tecla_de_envio: TeclaDeEnvio,
    /// Acrescentar um espaço no fim do texto, para ditar duas frases seguidas
    /// sem elas grudarem.
    pub espaco_no_fim: bool,
    /// Atalho que descarta a gravação em curso sem transcrever. Vazio desliga.
    pub atalho_de_cancelar: Vec<String>,
    /// Avisos sonoros de início e fim.
    pub sons: Sons,
    /// Correção de termos próprios no texto transcrito.
    pub dicionario: Dicionario,
    /// Registro das transcrições.
    pub historico: Historico,
    /// Aparar o silêncio das pontas antes de mandar o áudio ao Whisper.
    ///
    /// Nasce ligado, e essa é a escolha certa apesar de mudar o comportamento de
    /// quem já usa: o que ele tira é justamente o que faz o modelo inventar
    /// frase (ver `src/vad.rs`), e o módulo é conservador por construção —
    /// não achando fala com segurança, ele devolve o áudio inteiro em vez de
    /// arriscar comer uma palavra.
    pub aparar_silencio: bool,
    /// Soltar o modelo da memória depois de um tempo sem ditar.
    pub descarregar_o_modelo: Ociosidade,
    /// Conferir uma vez por dia se saiu uma versão nova (ver `src/versao.rs`).
    ///
    /// É a única coisa neste programa que fala com a rede sem alguém ter pedido
    /// na hora, e por isso está escrito aqui também: o que sai é um `GET` em
    /// `api.github.com`, sem nada sobre a máquina, e desligar aqui não cria nem
    /// a thread que perguntaria.
    pub aviso_de_versao: bool,
}

/// Como o texto chega à janela em foco quando a colagem automática está ligada.
///
/// Existe porque "Ctrl+V" não é universal: em muito terminal ele não cola (ou
/// cola outra coisa), e há campo que recusa colagem mas aceita digitação.
///
/// **`Digitar` é também a resposta a "não quero que o Ditador mexa na minha área
/// de transferência"**, e é por isso que não existe aqui uma opção separada de
/// "restaurar o conteúdo anterior": restaurar depende de adivinhar quando o
/// programa de destino leu o que estava lá, e todo palpite erra em alguma
/// máquina — o texto antigo volta antes de o novo ser lido e a pessoa cola o que
/// tinha copiado meia hora atrás. Digitar não encosta na área de transferência,
/// então não há o que restaurar nem corrida para perder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetodoDeColagem {
    #[default]
    CtrlV,
    /// O que os terminais entendem desde sempre.
    ShiftInsert,
    /// O Ctrl+V dos terminais do GNOME e do KDE.
    CtrlShiftV,
    /// Digita o texto tecla a tecla. Não usa a área de transferência.
    Digitar,
}

impl MetodoDeColagem {
    pub const TODOS: [MetodoDeColagem; 4] = [
        MetodoDeColagem::CtrlV,
        MetodoDeColagem::ShiftInsert,
        MetodoDeColagem::CtrlShiftV,
        MetodoDeColagem::Digitar,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            MetodoDeColagem::CtrlV => "Ctrl+V",
            MetodoDeColagem::ShiftInsert => "Shift+Insert",
            MetodoDeColagem::CtrlShiftV => "Ctrl+Shift+V",
            MetodoDeColagem::Digitar => "Digitar",
        }
    }

    pub fn explicacao(self) -> &'static str {
        match self {
            MetodoDeColagem::CtrlV => {
                "O de sempre. Funciona na maioria dos programas, mas não em muito terminal."
            }
            MetodoDeColagem::ShiftInsert => {
                "O que os terminais entendem. Use se o Ctrl+V não cola onde você escreve."
            }
            MetodoDeColagem::CtrlShiftV => "O atalho de colar dos terminais do GNOME e do KDE.",
            MetodoDeColagem::Digitar => {
                "Digita o texto tecla a tecla, sem passar pela área de transferência — \
                 o que você tinha copiado continua lá. É mais lento em textos longos e \
                 depende do layout de teclado do sistema."
            }
        }
    }

    /// Digitar não passa pela área de transferência, e várias decisões do
    /// programa dependem de saber disso — a começar por não anunciar "copiado".
    pub fn usa_a_area_de_transferencia(self) -> bool {
        !matches!(self, MetodoDeColagem::Digitar)
    }
}

/// O que apertar depois de colar.
///
/// Com `Enter`, ditar num campo de chat vira falar e soltar: a mensagem já foi.
/// O `CtrlEnter` existe porque em vários programas — Slack, algumas caixas de
/// comentário — é ele que envia, e o Enter sozinho quebra a linha.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeclaDeEnvio {
    #[default]
    Nenhuma,
    Enter,
    CtrlEnter,
}

impl TeclaDeEnvio {
    pub const TODAS: [TeclaDeEnvio; 3] = [
        TeclaDeEnvio::Nenhuma,
        TeclaDeEnvio::Enter,
        TeclaDeEnvio::CtrlEnter,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            TeclaDeEnvio::Nenhuma => "Nada",
            TeclaDeEnvio::Enter => "Enter",
            TeclaDeEnvio::CtrlEnter => "Ctrl+Enter",
        }
    }
}

/// Avisos sonoros.
///
/// Ligados por padrão, e a razão é o modo que este programa já oferecia sem
/// eles: com a cópia automática ligada e a janela de resultado desligada — ou
/// com a extensão do GNOME no ar — não aparece nada na tela, e quem fala não
/// tem como saber se o atalho pegou antes de já ter falado a frase inteira.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sons {
    pub ativo: bool,
    /// 0 a 1. O padrão é discreto de propósito: isto toca a cada frase.
    pub volume: f32,
}

impl Sons {
    pub const PADRAO: Self = Self {
        ativo: true,
        volume: 0.35,
    };

    pub fn sanear(&mut self) {
        self.volume = self.volume.clamp(0.0, 1.0);
        if !self.volume.is_finite() {
            self.volume = Self::PADRAO.volume;
        }
    }
}

impl Default for Sons {
    fn default() -> Self {
        Self::PADRAO
    }
}

/// Correção de termos próprios depois da transcrição (ver `src/dicionario.rs`).
///
/// Nasce ligado e **vazio**, que é a combinação certa: sem termos cadastrados
/// ele não faz absolutamente nada, então ligá-lo por padrão não muda o texto de
/// ninguém — e quem cadastrar o primeiro termo não precisa descobrir que existe
/// um interruptor a mais para ligar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dicionario {
    pub ativo: bool,
    /// Como a pessoa escreve o termo. É esta grafia que vai para o texto.
    pub termos: Vec<String>,
    /// De 0 a 1: quanto o texto pode diferir do termo e ainda ser corrigido.
    /// Mais alto, mais exigente.
    pub sensibilidade: f32,
}

impl Dicionario {
    /// Calibrado à mão contra os casos do teste de `dicionario.rs`: pega
    /// "cuber netes" → "Kubernetes" e recusa "carreto" → "Correto".
    pub const SENSIBILIDADE_PADRAO: f32 = 0.72;

    pub fn sanear(&mut self) {
        if !self.sensibilidade.is_finite() {
            self.sensibilidade = Self::SENSIBILIDADE_PADRAO;
        }
        self.sensibilidade = self.sensibilidade.clamp(0.5, 1.0);
        // Termo vazio casaria com tudo; espaço em volta é erro de digitação de
        // quem preencheu a lista, e o arquivo é editável à mão.
        for termo in &mut self.termos {
            *termo = termo.trim().to_string();
        }
        self.termos.retain(|t| !t.is_empty());
    }
}

impl Default for Dicionario {
    fn default() -> Self {
        Self {
            ativo: true,
            termos: Vec::new(),
            sensibilidade: Self::SENSIBILIDADE_PADRAO,
        }
    }
}

/// Registro das transcrições (ver `src/historico.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Historico {
    pub ativo: bool,
    /// Quantas transcrições guardar. As mais velhas saem primeiro.
    pub limite: usize,
    /// Guardar também o áudio de cada uma, em WAV.
    ///
    /// Desligado por padrão: são ~2 MB por minuto de fala, e a pergunta que o
    /// histórico existe para responder ("o que eu falei mesmo?") é respondida
    /// pelo texto. Quem quiser conferir a gravação liga isto sabendo o preço.
    pub guardar_audio: bool,
}

impl Historico {
    pub const PADRAO: Self = Self {
        ativo: true,
        limite: 200,
        guardar_audio: false,
    };

    pub fn sanear(&mut self) {
        self.limite = self.limite.clamp(1, 10_000);
    }
}

impl Default for Historico {
    fn default() -> Self {
        Self::PADRAO
    }
}

/// Quando soltar o modelo da memória por falta de uso.
///
/// O modelo padrão ocupa 574 MB de RAM — e, com a GPU ligada, outro tanto de
/// memória de vídeo — do instante em que o programa sobe até o instante em que
/// ele morre. Num programa que fica de pé o dia inteiro sob o serviço de
/// usuário e trabalha alguns minutos por dia, isso é a maior parte do custo de
/// tê-lo instalado.
///
/// **Nasce desligado, e não é timidez.** Descarregar troca memória por espera:
/// o ditado seguinte precisa esperar o modelo voltar. Essa espera é quase toda
/// escondida — quem religa o modelo é o começo da gravação, e não o fim dela
/// (ver `Controller::start_recording`), de modo que ele carrega *enquanto* a
/// pessoa fala —, mas "quase" não é "toda": num ditado de uma palavra em máquina
/// com disco lento, dá para sentir. Trocar memória por latência é uma decisão de
/// quem usa a máquina, e o padrão fica com o comportamento que sempre houve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ociosidade {
    pub ativo: bool,
    /// Minutos parado até soltar o modelo.
    pub minutos: u64,
}

impl Ociosidade {
    /// Dez minutos: mais do que qualquer pausa entre duas frases de uma mesma
    /// tarefa, e menos do que o intervalo entre duas tarefas do dia.
    pub const PADRAO: Self = Self {
        ativo: false,
        minutos: 10,
    };

    /// As opções oferecidas na tela.
    pub const MINUTOS: [u64; 5] = [1, 5, 10, 30, 60];

    pub fn sanear(&mut self) {
        // Zero descarregaria o modelo no instante seguinte ao de terminar de
        // carregá-lo, e o programa passaria a vida recarregando. O teto de um
        // dia é o mesmo que dizer "nunca", e quem quer nunca desliga a chave.
        self.minutos = self.minutos.clamp(1, 24 * 60);
    }

    /// Quanto tempo parado, se o descarregamento estiver ligado.
    ///
    /// "Parado" é medido pela thread da transcrição, e o que reinicia a contagem
    /// é qualquer comando que chegue a ela. Um efeito aceito de propósito: uma
    /// gravação mais longa que o prazo — possível com 1 minuto escolhido aqui e
    /// uma gravação de dois — descarrega o modelo **durante** a fala, e a
    /// transcrição logo em seguida o traz de volta. O resultado sai certo; o que
    /// se perde é o tempo de uma recarga que não precisava ter acontecido.
    /// Consertar isso exigiria a thread do Whisper saber que o microfone está
    /// aberto, que é estado do controlador e não dela.
    pub fn prazo(self) -> Option<std::time::Duration> {
        self.ativo
            .then(|| std::time::Duration::from_secs(self.minutos.max(1) * 60))
    }
}

impl Default for Ociosidade {
    fn default() -> Self {
        Self::PADRAO
    }
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

            microfone_sempre_aberto: true,
            canal_do_microfone: None,
            metodo_de_colagem: MetodoDeColagem::CtrlV,
            tecla_de_envio: TeclaDeEnvio::Nenhuma,
            espaco_no_fim: false,
            // O Esc é o que todo mundo aperta para desistir de alguma coisa, e o
            // custo de um falso positivo é baixo: ele só é lido enquanto o
            // microfone está aberto, e o que se perde é uma frase que se pode
            // repetir. Quem achar arriscado esvazia o campo na tela.
            atalho_de_cancelar: vec!["KEY_ESC".to_string()],
            sons: Sons::PADRAO,
            dicionario: Dicionario::default(),
            historico: Historico::PADRAO,
            aparar_silencio: true,
            descarregar_o_modelo: Ociosidade::PADRAO,
            aviso_de_versao: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Self::ler_de(&config_path());
        // Só em memória, e só no modo portátil: o caminho absoluto do modelo
        // apodrece quando a pasta portátil muda de endereço (pendrive noutra
        // máquina, outra letra de unidade), e quem o reencontra é o catálogo.
        // O arquivo no disco não é reescrito por isso — a regra de que apenas a
        // ausência dele autoriza gravar por cima continua valendo.
        crate::modelo::reencontrar_no_portatil(&mut cfg);
        cfg
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
        self.sons.sanear();
        self.dicionario.sanear();
        self.historico.sanear();
        self.descarregar_o_modelo.sanear();
        self.threads = self.threads.clamp(1, 32);
        self.min_recording_ms = self.min_recording_ms.min(5_000);
        self.max_recording_secs = self.max_recording_secs.clamp(1, 3_600);
        self.result_timeout_secs = self.result_timeout_secs.min(3_600);
        if self.hotkey.is_empty() {
            self.hotkey = Config::default().hotkey;
        }
        // A ordem de exibição do atalho é decidida uma vez, aqui, e não a cada
        // quadro pela interface. O de cancelar segue a mesma regra — mas pode
        // ficar vazio, que é como se desliga o cancelamento por tecla.
        crate::keys::sort_combo(&mut self.hotkey);
        crate::keys::sort_combo(&mut self.atalho_de_cancelar);
        // Um atalho de cancelar igual ao de ditar cancelaria todo ditado no
        // instante em que ele começa: o mesmo aperto que abre o microfone
        // dispararia os dois. Quem editou o arquivo à mão fica sem o de
        // cancelar, e não sem o de ditar.
        if self.atalho_de_cancelar == self.hotkey {
            log::warn!(
                "o atalho de cancelar é igual ao de ditar ({}); ignorando o de cancelar",
                crate::keys::combo_label(&self.hotkey)
            );
            self.atalho_de_cancelar.clear();
        }
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
    fn a_config_da_0_6_ganha_todos_os_padroes_novos() {
        // O arquivo de quem já usa o programa não tem nenhum dos campos da 0.7.
        // Todos precisam nascer no padrão — e nos padrões que o README promete.
        let antiga = r#"{
            "hotkey": ["KEY_PAUSE"],
            "language": "pt",
            "auto_copy": true,
            "threads": 4,
            "appearance": { "theme": "escuro", "animation": true, "animation_ms": 150 }
        }"#;
        let cfg: Config = serde_json::from_str(antiga).expect("config da 0.6");
        let padrao = Config::default();

        assert!(cfg.microfone_sempre_aberto);
        assert_eq!(cfg.canal_do_microfone, None);
        assert_eq!(cfg.metodo_de_colagem, MetodoDeColagem::CtrlV);
        assert_eq!(cfg.tecla_de_envio, TeclaDeEnvio::Nenhuma);
        assert!(!cfg.espaco_no_fim);
        assert_eq!(cfg.atalho_de_cancelar, padrao.atalho_de_cancelar);
        assert_eq!(cfg.sons, Sons::PADRAO);
        assert_eq!(cfg.dicionario, Dicionario::default());
        assert_eq!(cfg.historico, Historico::PADRAO);
        // E os da 0.8, que chegaram depois deste teste existir.
        assert!(cfg.aparar_silencio, "o aparo do silêncio nasce ligado");
        assert_eq!(cfg.descarregar_o_modelo, Ociosidade::PADRAO);
        assert!(
            !cfg.descarregar_o_modelo.ativo,
            "descarregar o modelo nasce desligado: ele troca memória por espera, \
             e essa troca é decisão de quem usa a máquina"
        );
        assert!(cfg.aviso_de_versao, "o aviso de versão nova nasce ligado");
        // E o que estava no arquivo continua valendo.
        assert_eq!(cfg.language, "pt");
        assert_eq!(cfg.appearance.theme, Tema::Escuro);
    }

    #[test]
    fn o_prazo_da_ociosidade_so_existe_quando_ela_esta_ligada() {
        let mut o = Ociosidade::PADRAO;
        assert_eq!(o.prazo(), None, "desligada, não há prazo nenhum");

        o.ativo = true;
        assert_eq!(o.prazo(), Some(std::time::Duration::from_secs(600)));

        // Um arquivo editado à mão não pode pedir zero: o modelo sairia da
        // memória no instante seguinte ao de terminar de carregar, e o programa
        // passaria a vida recarregando.
        o.minutos = 0;
        o.sanear();
        assert_eq!(o.minutos, 1);
        assert_eq!(o.prazo(), Some(std::time::Duration::from_secs(60)));

        // Nem um prazo absurdo, que é o mesmo que desligar por outro caminho.
        o.minutos = 999_999;
        o.sanear();
        assert_eq!(o.minutos, 24 * 60);
    }

    #[test]
    fn as_opcoes_de_prazo_da_tela_sobrevivem_ao_saneamento() {
        // A lista suspensa oferece estes valores; um deles fora da faixa do
        // `sanear` viraria outro número assim que a configuração fosse relida,
        // e a tela mostraria uma escolha que a pessoa não fez.
        for minutos in Ociosidade::MINUTOS {
            let mut o = Ociosidade {
                ativo: true,
                minutos,
            };
            o.sanear();
            assert_eq!(
                o.minutos, minutos,
                "o prazo de {minutos} min não sobreviveu"
            );
        }
    }

    #[test]
    fn os_nomes_gravados_dos_campos_novos_sao_os_que_o_readme_promete() {
        // O arquivo é editável à mão e o README publica estes nomes. Renomear um
        // deles apagaria a preferência de quem já escolheu — e a regra do
        // CLAUDE.md para o contrato D-Bus vale aqui: acrescentar, nunca
        // renomear.
        let cfg = Config {
            metodo_de_colagem: MetodoDeColagem::CtrlShiftV,
            tecla_de_envio: TeclaDeEnvio::CtrlEnter,
            canal_do_microfone: Some(2),
            ..Config::default()
        };
        let raw = serde_json::to_string(&cfg).expect("gravar");
        for esperado in [
            r#""microfone_sempre_aberto":true"#,
            r#""canal_do_microfone":2"#,
            r#""metodo_de_colagem":"ctrl_shift_v""#,
            r#""tecla_de_envio":"ctrl_enter""#,
            r#""espaco_no_fim":false"#,
            r#""aparar_silencio":true"#,
            r#""aviso_de_versao":true"#,
            r#""descarregar_o_modelo":{"ativo":false,"minutos":10}"#,
            r#""atalho_de_cancelar":["KEY_ESC"]"#,
        ] {
            assert!(raw.contains(esperado), "não achei {esperado} em {raw}");
        }
        // Os quatro valores de método e os três de envio, um a um.
        for (metodo, nome) in [
            (MetodoDeColagem::CtrlV, "ctrl_v"),
            (MetodoDeColagem::ShiftInsert, "shift_insert"),
            (MetodoDeColagem::CtrlShiftV, "ctrl_shift_v"),
            (MetodoDeColagem::Digitar, "digitar"),
        ] {
            assert_eq!(
                serde_json::to_string(&metodo).expect("gravar"),
                format!("\"{nome}\"")
            );
        }
        for (tecla, nome) in [
            (TeclaDeEnvio::Nenhuma, "nenhuma"),
            (TeclaDeEnvio::Enter, "enter"),
            (TeclaDeEnvio::CtrlEnter, "ctrl_enter"),
        ] {
            assert_eq!(
                serde_json::to_string(&tecla).expect("gravar"),
                format!("\"{nome}\"")
            );
        }
        // E a volta: o que foi gravado é lido de novo igual.
        let volta: Config = serde_json::from_str(&raw).expect("ler");
        assert_eq!(volta, cfg);
    }

    #[test]
    fn um_atalho_de_cancelar_igual_ao_de_ditar_e_recusado() {
        // Os dois iguais cancelariam todo ditado no instante em que ele começa: o
        // mesmo aperto dispararia os dois. Quem fica sem atalho é o de cancelar,
        // nunca o de ditar.
        let mut cfg = Config {
            hotkey: vec!["KEY_PAUSE".to_string()],
            atalho_de_cancelar: vec!["KEY_PAUSE".to_string()],
            ..Config::default()
        };
        cfg.sanear();
        assert_eq!(cfg.hotkey, vec!["KEY_PAUSE"]);
        assert!(cfg.atalho_de_cancelar.is_empty());

        // Diferentes, os dois sobrevivem.
        let mut ok = Config {
            hotkey: vec!["KEY_PAUSE".to_string()],
            atalho_de_cancelar: vec!["KEY_ESC".to_string()],
            ..Config::default()
        };
        ok.sanear();
        assert_eq!(ok.atalho_de_cancelar, vec!["KEY_ESC"]);
    }

    #[test]
    fn os_valores_novos_editados_a_mao_sao_aparados() {
        // A tela limita todos; o arquivo, não.
        let bruto = r#"{
            "sons": { "ativo": true, "volume": 9.5 },
            "dicionario": { "ativo": true, "termos": ["  Kubernetes  ", "", "   "], "sensibilidade": 0.1 },
            "historico": { "ativo": true, "limite": 0, "guardar_audio": false }
        }"#;
        let mut cfg: Config = serde_json::from_str(bruto).expect("config");
        cfg.sanear();

        assert_eq!(cfg.sons.volume, 1.0, "o volume passou de 100%");
        // O piso da sensibilidade é 0,5: abaixo dela a correção começa a trocar
        // palavras que estavam certas.
        assert_eq!(cfg.dicionario.sensibilidade, 0.5);
        // Espaço em volta é erro de digitação de quem preencheu a lista, e termo
        // vazio casaria com tudo.
        assert_eq!(cfg.dicionario.termos, vec!["Kubernetes"]);
        assert_eq!(cfg.historico.limite, 1);
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
