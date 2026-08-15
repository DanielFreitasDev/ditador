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

/// O estado do programa como o mundo de fora o vê.
///
/// Existe porque agora há dois públicos para a mesma pergunta — o ícone da
/// barra e a extensão do GNOME, do outro lado do D-Bus — e uma resposta só
/// evita que eles discordem. É o mesmo raciocínio do `gravando()`: uma pergunta,
/// num lugar só.
///
/// A diferença para o `icones::Estado` é que ali "carregando o modelo" e
/// "transcrevendo" dividem o símbolo de trabalho, porque para quem olha a barra
/// os dois querem dizer "espere"; aqui são coisas distintas — a carga acontece
/// uma vez, no arranque, e a transcrição a cada frase.
///
/// Não há um estado "iniciando" à parte: neste programa o arranque *é* a carga
/// do modelo, que começa antes de tudo. Nem "indisponível" — esse é a ausência
/// do nome no barramento, e quem o descobre é quem pergunta, não quem responde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoPublico {
    Carregando,
    Pronto,
    Gravando,
    Transcrevendo,
    Erro,
}

impl EstadoPublico {
    pub fn de(model: ModelState, view: View, gravando: bool) -> Self {
        match (model, gravando, view) {
            (ModelState::Loading, _, _) => Self::Carregando,
            (ModelState::Failed, _, _) => Self::Erro,
            (_, true, _) => Self::Gravando,
            (_, _, View::Processing) => Self::Transcrevendo,
            _ => Self::Pronto,
        }
    }

    /// Como o estado viaja pelo D-Bus. São estes textos que a extensão do GNOME
    /// compara, então mudar um deles é mudar o protocolo.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Carregando => "carregando",
            Self::Pronto => "pronto",
            Self::Gravando => "gravando",
            Self::Transcrevendo => "transcrevendo",
            Self::Erro => "erro",
        }
    }
}

/// Quais integrações nativas de área de trabalho estão no ar agora.
///
/// Cada uma segura um nome próprio no barramento enquanto está carregada, e
/// quem escreve aqui é o `dbus.rs`, observando esses nomes — nunca a própria
/// integração mandando avisar. É a diferença que faz o ícone voltar sozinho
/// quando o Shell reinicia, quando o `plasmashell` cai ou quando o widget é
/// removido: quem detém um nome é a *conexão*, e o barramento a solta sozinho
/// quando ela morre, com ou sem despedida.
///
/// As duas perguntas abaixo são separadas porque as duas integrações não
/// entregam a mesma coisa, e tratá-las como uma só apagaria o aviso de gravação
/// de quem usa o Plasma. Veja `mostram_o_aviso`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Integracoes {
    /// A extensão do GNOME Shell.
    pub gnome: bool,
    /// O widget do Plasma (`kde-plasma/`).
    pub plasma: bool,
}

impl Integracoes {
    /// Alguém já mostra o Ditador na barra — então o nosso StatusNotifierItem
    /// sai de cena, porque dois ícones do mesmo programa lado a lado é o tipo de
    /// coisa que ninguém escolhe de propósito.
    ///
    /// Vale para as duas: tanto o indicador do Shell quanto o widget do Plasma
    /// ocupam esse lugar.
    pub fn mostram_o_icone(self) -> bool {
        self.gnome || self.plasma
    }

    /// Alguém já avisa **na tela** que se está gravando — e então a nossa
    /// sobreposição some, para não dar o mesmo recado duas vezes.
    ///
    /// Só o GNOME. Não é esquecimento nem trabalho pela metade: no Plasma 6.6
    /// não existe API pública que desenhe um aviso passivo por cima da cena.
    /// O `SceneEffect` do KWin — o único caminho declarativo — herda o
    /// `paintScreen` do `QuickSceneEffect`, que **não** encadeia
    /// `effects->paintScreen()`: enquanto ativo, ele substitui a cena inteira,
    /// e a caixinha de "Gravando" viria acompanhada de todas as janelas
    /// desaparecendo. O único efeito que faz o que se quer aqui
    /// (`OutputLocatorEffect`) é C++ compilado dentro do próprio KWin, contra
    /// uma ABI interna que muda a cada versão. E o `org.kde.osdService`, que
    /// faria o serviço, é interno do `plasmashell`: não tem XML publicado em
    /// `/usr/share/dbus-1/interfaces/` nem promessa de estabilidade nenhuma.
    ///
    /// Então no Plasma quem avisa continua sendo a janela do próprio Ditador,
    /// que aliás funciona bem lá — ela sobe pelo XWayland, e o KWin honra o
    /// "sempre por cima" que o GNOME/Wayland recusa. O widget cuida da barra; a
    /// sobreposição continua nossa.
    pub fn mostram_o_aviso(self) -> bool {
        self.gnome
    }
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
    /// Que integrações nativas estão no ar — a extensão do GNOME, o widget do
    /// Plasma, as duas ou nenhuma.
    ///
    /// Quando há alguma, o que ela já mostra sai de cena do nosso lado, para não
    /// dizer o mesmo recado duas vezes. *O quê* sai depende de qual é: veja
    /// `Integracoes::mostram_o_icone` (o StatusNotifierItem, em `tray.rs`) e
    /// `Integracoes::mostram_o_aviso` (a sobreposição de gravação, em
    /// `tela_visivel`).
    pub integracoes: Integracoes,
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
            integracoes: Integracoes::default(),
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

    /// O estado como o D-Bus e o ícone da barra o publicam.
    pub fn estado_publico(&self) -> EstadoPublico {
        EstadoPublico::de(self.model, self.view, self.gravando())
    }

    /// A tela que a janela deve desenhar agora.
    ///
    /// É a `view`, com uma exceção: quando a integração no ar já avisa na tela
    /// que se está gravando, o aviso de "gravando" e o de "transcrevendo" passam
    /// a ser dela — o OSD do Shell diz as duas coisas, no lugar em que o GNOME
    /// sempre as diz, e a nossa sobreposição por cima seria o mesmo recado duas
    /// vezes.
    ///
    /// Quem responde a essa pergunta é `Integracoes::mostram_o_aviso`, e hoje só
    /// o GNOME responde que sim — no Plasma não há API pública para desenhar
    /// isso, e o motivo está escrito lá.
    ///
    /// As outras telas continuam nossas, e de propósito: resultado,
    /// configurações e erro carregam texto para copiar e botões que resolvem o
    /// problema (baixar o modelo, recarregar). Um OSD não tem onde pôr isso, e
    /// trocar uma tela com saída por uma frase que some em quatro segundos
    /// deixaria a pessoa sem o que fazer.
    ///
    /// A `view` em si não muda — quem grava continua em `View::Recording` para
    /// todo o resto do programa. O que muda é só o que a janela desenha.
    pub fn tela_visivel(&self) -> View {
        match self.view {
            View::Recording | View::Processing if self.integracoes.mostram_o_aviso() => {
                View::Hidden
            }
            outra => outra,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn shared() -> Shared {
        let mut s = Shared::new(Config::default(), Vec::new());
        s.model = ModelState::Ready;
        s
    }

    #[test]
    fn o_estado_publicado_pergunta_ao_microfone_e_nao_a_tela() {
        // A mesma armadilha de sempre, agora na porta de saída do programa: a
        // janela do resultado anterior pode estar por cima de um ditado em
        // andamento, e nesse intervalo quem está de fora precisa continuar
        // ouvindo "gravando".
        let mut s = shared();
        s.recording_since = Some(Instant::now());
        s.view = View::Result;
        assert_eq!(s.estado_publico(), EstadoPublico::Gravando);

        // Sem gravação, quem manda é a tela.
        s.recording_since = None;
        s.view = View::Processing;
        assert_eq!(s.estado_publico(), EstadoPublico::Transcrevendo);
        s.view = View::Hidden;
        assert_eq!(s.estado_publico(), EstadoPublico::Pronto);

        // E o modelo ganha de tudo: sem ele não há ditado nenhum.
        s.recording_since = Some(Instant::now());
        s.view = View::Recording;
        s.model = ModelState::Loading;
        assert_eq!(s.estado_publico(), EstadoPublico::Carregando);
        s.model = ModelState::Failed;
        assert_eq!(s.estado_publico(), EstadoPublico::Erro);
    }

    #[test]
    fn o_icone_sai_de_cena_para_as_duas_integracoes() {
        // A pergunta do `tray.rs`: alguém já está mostrando o Ditador na barra?
        // As duas integrações ocupam esse lugar, cada uma do seu jeito — o
        // indicador do Shell e o widget do Plasma.
        let nenhuma = Integracoes::default();
        assert!(!nenhuma.mostram_o_icone());

        for integracoes in [
            Integracoes {
                gnome: true,
                plasma: false,
            },
            Integracoes {
                gnome: false,
                plasma: true,
            },
            Integracoes {
                gnome: true,
                plasma: true,
            },
        ] {
            assert!(
                integracoes.mostram_o_icone(),
                "{integracoes:?} devia recolher o ícone da bandeja"
            );
        }
    }

    #[test]
    fn so_o_gnome_assume_o_aviso_de_gravacao() {
        // A outra pergunta, que é outra coisa: alguém já avisa *na tela*?
        //
        // Só o GNOME. O widget do Plasma cuida da barra e nada mais, porque no
        // Plasma 6.6 não há API pública que desenhe um aviso passivo por cima da
        // cena — o motivo por extenso está em `Integracoes::mostram_o_aviso`.
        // Se um dia as duas perguntas voltarem a ser uma só, é aqui que quebra:
        // quem usa Plasma ficaria sem nenhum aviso de que o microfone está
        // aberto, que é a única coisa que este programa precisa dizer.
        assert!(
            Integracoes {
                gnome: true,
                plasma: false
            }
            .mostram_o_aviso()
        );
        assert!(
            !Integracoes {
                gnome: false,
                plasma: true
            }
            .mostram_o_aviso(),
            "o widget do Plasma levou um aviso que ele não desenha"
        );
        assert!(!Integracoes::default().mostram_o_aviso());
    }

    #[test]
    fn a_tela_de_gravacao_continua_nossa_no_plasma() {
        let mut s = shared();
        s.integracoes = Integracoes {
            gnome: false,
            plasma: true,
        };

        // O widget do Plasma recolhe o ícone, mas não a sobreposição: lá ela é
        // o único aviso de que o microfone está aberto.
        for view in [View::Recording, View::Processing] {
            s.view = view;
            assert_eq!(
                s.tela_visivel(),
                view,
                "a sobreposição sumiu no Plasma, onde nada a substitui"
            );
        }
    }

    #[test]
    fn a_extensao_do_gnome_assume_so_as_telas_que_nao_tem_botao() {
        let mut s = shared();

        // Sem extensão, nada muda: a janela desenha o que a `view` diz.
        for view in [View::Recording, View::Processing, View::Result, View::Error] {
            s.view = view;
            assert_eq!(s.tela_visivel(), view, "sem extensão a tela mudou");
        }

        s.integracoes.gnome = true;

        // O aviso de gravação e o de transcrição passam a ser do OSD do Shell.
        s.view = View::Recording;
        assert_eq!(s.tela_visivel(), View::Hidden);
        s.view = View::Processing;
        assert_eq!(s.tela_visivel(), View::Hidden);

        // O resto continua nosso: são as telas com texto para copiar e com os
        // botões que resolvem o problema. Um OSD não tem onde pô-los.
        for view in [View::Result, View::Settings, View::Error] {
            s.view = view;
            assert_eq!(
                s.tela_visivel(),
                view,
                "a extensão levou uma tela que tem ação dentro"
            );
        }

        // A `view` em si nunca foi tocada — quem grava continua gravando para
        // todo o resto do programa.
        s.view = View::Recording;
        s.recording_since = Some(Instant::now());
        assert_eq!(s.tela_visivel(), View::Hidden);
        assert_eq!(s.view, View::Recording);
        assert!(s.gravando());
    }
}
