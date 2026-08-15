//! Ícone na barra superior do GNOME.
//!
//! O GNOME não tem bandeja nativa, mas a extensão AppIndicators (que vem
//! habilitada no Ubuntu) publica um `org.kde.StatusNotifierWatcher` no
//! barramento de sessão. Registramos um StatusNotifierItem nele: o ícone muda
//! conforme o estado e o menu dá acesso ao que o atalho faz.
//!
//! Subimos junto com a sessão gráfica, quase sempre antes de o Shell terminar
//! de carregar as extensões: no primeiro registro o vigia costuma nem existir
//! ainda. Por isso pedimos ao ksni que trate essa ausência como espera, e não
//! como erro — ele fica de olho no barramento e nos registra quando o vigia
//! chega. Se ele nunca chegar (extensão desligada, outra área de trabalho), o
//! programa segue sem ícone — nunca é motivo para não subir.
//!
//! No Plasma o mesmo item aparece na bandeja do sistema sem extensão nenhuma —
//! o `plasmashell` é hospedeiro de StatusNotifierItem nativamente. É o que faz
//! este arquivo continuar sendo a reserva de todo mundo: KDE sem o widget,
//! GNOME sem a extensão, e qualquer outra área de trabalho.
//!
//! ## Quando uma integração nativa está no ar
//!
//! Aí este ícone sai de cena, porque ela publica o dela — dois ícones do mesmo
//! programa, lado a lado na mesma barra, é o tipo de coisa que ninguém escolhe
//! de propósito. Vale para a extensão do GNOME Shell e para o widget do Plasma,
//! que dessa perspectiva são a mesma notícia; quem sabe distingui-las é o
//! `state::Integracoes`, e ele precisa saber porque a *outra* pergunta — quem
//! desenha o aviso de gravação — tem respostas diferentes para cada uma.
//!
//! Sair de cena aqui é **desregistrar o item**, não marcá-lo como `Passive`. O
//! protocolo diz que um item passivo *pode* ser escondido, e "pode" é decisão de
//! cada hospedeiro: a promessa que se quer aqui é que o ícone suma, e a única
//! que não depende de ninguém é não haver item nenhum. Quando a extensão sai, o
//! item é registrado de novo — o hospedeiro trata isso como um programa que
//! acabou de subir, que é exatamente o que ele parece ser.

use crate::controller::IpcCommand;
use crate::icones::{self, Estado};
use crate::keys;
use crate::state::{EstadoPublico, SharedState, Sinal, lock};
use crossbeam_channel::Sender;
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, OfflineReason, Status, ToolTip};

/// Retrato do estado que o ícone precisa. Guardamos uma cópia para que os
/// callbacks do ksni, que rodam na thread do D-Bus, nunca travem o mutex
/// principal.
#[derive(Clone, PartialEq)]
struct Retrato {
    /// O estado publicado, o mesmo que vai pelo D-Bus. A regra de qual estado é
    /// qual mora no `EstadoPublico::de`, e não aqui: eram duas cópias da mesma
    /// tabela, e duas cópias é uma a mais do que se consegue manter iguais.
    estado: EstadoPublico,
    /// O microfone está aberto. Vem do `recording_since`, nunca da `view`.
    ///
    /// Sem este campo a bandeja decidia pela tela — a única fonte que o
    /// CLAUDE.md proíbe consultar — e o controlador cria o estado divergente
    /// nos padrões: `on_transcription` põe `View::Result` com a gravação ainda
    /// correndo. Nesse intervalo o ícone voltava ao normal, a dica dizia
    /// "Pronto · segure Pause" com o microfone aberto, e o item do menu
    /// oferecia "Ditar agora" — mas o clique manda `Toggle`, que lê o
    /// `recording_since` certinho e **para** a gravação. O rótulo prometia o
    /// oposto do que o item fazia.
    gravando: bool,
    atalho: String,
    /// Alguma integração nativa já está mostrando o Ditador na barra — e então
    /// este ícone não deve existir. Vale para a extensão do GNOME e para o
    /// widget do Plasma; do ponto de vista daqui as duas são a mesma notícia.
    integracao_mostra_o_icone: bool,
}

impl Retrato {
    fn tirar(shared: &SharedState) -> Self {
        let estado = lock(shared);
        Self {
            estado: estado.estado_publico(),
            gravando: estado.gravando(),
            atalho: keys::combo_label(&estado.config.hotkey),
            integracao_mostra_o_icone: estado.integracoes.mostram_o_icone(),
        }
    }

    fn icone(&self) -> Estado {
        Estado::do_publico(self.estado)
    }

    /// O modelo carregou, então dá para ditar.
    fn pronto_para_ditar(&self) -> bool {
        !matches!(self.estado, EstadoPublico::Carregando | EstadoPublico::Erro)
    }

    fn resumo(&self) -> String {
        match self.estado {
            EstadoPublico::Carregando => "Carregando o modelo…".to_string(),
            EstadoPublico::Erro => "O modelo não carregou".to_string(),
            EstadoPublico::Gravando => "Ouvindo…".to_string(),
            EstadoPublico::Transcrevendo => "Transcrevendo…".to_string(),
            EstadoPublico::Pronto => format!("Pronto · segure {}", self.atalho),
        }
    }
}

/// Os ícones embutidos, no formato que o StatusNotifierItem pede.
///
/// O `icones::bandeja` devolve um mapa de bits neutro porque o Windows precisa
/// dos mesmos pixels para montar um `HICON`, e `ksni::Icon` é um tipo do
/// protocolo do Linux. A conversão é uma mudança de nome de campo — os bytes já
/// estão em ARGB32 na ordem de rede, que é justamente o que o protocolo espera.
fn pixmaps(estado: Estado) -> Vec<ksni::Icon> {
    icones::bandeja(estado)
        .into_iter()
        .map(|bitmap| ksni::Icon {
            width: bitmap.largura as i32,
            height: bitmap.altura as i32,
            data: bitmap.argb,
        })
        .collect()
}

pub struct Icone {
    retrato: Retrato,
    comandos: Sender<IpcCommand>,
}

impl Icone {
    fn enviar(&self, comando: IpcCommand) {
        let _ = self.comandos.send(comando);
    }
}

impl ksni::Tray for Icone {
    /// Clique abre o menu. Alternar a gravação no clique seria ambíguo: a
    /// extensão do GNOME também abre o menu, e o usuário veria as duas coisas.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").to_string()
    }

    fn title(&self) -> String {
        "Ditador".to_string()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        self.retrato.icone().nome().to_string()
    }

    /// Reserva para quando o tema não tiver os nossos ícones. O protocolo manda
    /// o hospedeiro preferir o nome e só cair no mapa de bits se não achar.
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        pixmaps(self.retrato.icone())
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Ditador".to_string(),
            description: self.retrato.resumo(),
            icon_name: self.retrato.icone().nome().to_string(),
            icon_pixmap: pixmaps(self.retrato.icone()),
        }
    }

    /// Sem vigia não há onde pendurar o ícone. Devolver `true` mantém o serviço
    /// vivo à espera dele: é o caso normal no login, quando chegamos antes das
    /// extensões do Shell, e também quando o usuário reinicia o Shell.
    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        log::info!("bandeja sem vigia ({reason:?}); esperando ele aparecer");
        true
    }

    fn watcher_online(&self) {
        log::info!("vigia da bandeja no ar; ícone publicado na barra superior");
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        // O `Toggle` que este item manda decide pelo `recording_since`; o
        // rótulo precisa decidir pela mesma coisa, senão promete o contrário.
        let gravando = self.retrato.gravando;
        let pronto = self.retrato.pronto_para_ditar();

        vec![
            StandardItem {
                label: self.retrato.resumo(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: if gravando {
                    "Parar e transcrever".to_string()
                } else {
                    "Ditar agora".to_string()
                },
                icon_name: if gravando {
                    "media-playback-stop-symbolic".to_string()
                } else {
                    Estado::Pronto.nome().to_string()
                },
                enabled: pronto,
                activate: Box::new(|this: &mut Self| this.enviar(IpcCommand::Toggle)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Configurações".to_string(),
                icon_name: "preferences-system-symbolic".to_string(),
                activate: Box::new(|this: &mut Self| this.enviar(IpcCommand::Settings)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Encerrar".to_string(),
                icon_name: "application-exit-symbolic".to_string(),
                activate: Box::new(|this: &mut Self| this.enviar(IpcCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Registra o item na barra e o mantém em dia — recolhendo-o enquanto a
/// extensão do GNOME estiver no ar e trazendo-o de volta quando ela sair.
///
/// Não devolve erro de propósito: ficar sem ícone é um degrau abaixo, não uma
/// falha de inicialização.
pub fn start(shared: SharedState, sinal: &Sinal, comandos: Sender<IpcCommand>) {
    let mudancas = sinal.observar();
    let retrato = Retrato::tirar(&shared);

    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            let mut atual = retrato;
            // Se a integração já estava de pé quando o Ditador subiu, o item nem
            // chega a ser registrado — nada pisca na barra.
            let mut publicado = (!atual.integracao_mostra_o_icone)
                .then(|| publicar(&atual, &comandos))
                .flatten();

            // Um aviso por mudança de estado; o retrato é lido depois, então
            // avisos acumulados se resolvem numa atualização só.
            while mudancas.recv().is_ok() {
                let novo = Retrato::tirar(&shared);
                if novo == atual {
                    continue;
                }
                atual = novo.clone();

                if novo.integracao_mostra_o_icone {
                    if let Some(handle) = publicado.take() {
                        // Esperar o fim é de graça aqui e evita a única janela
                        // de tempo em que os dois ícones existiriam juntos.
                        handle.shutdown().wait();
                        log::info!(
                            "ícone da barra recolhido; quem mostra o Ditador é a integração do desktop"
                        );
                    }
                    continue;
                }

                match publicado.take() {
                    None => publicado = publicar(&novo, &comandos),
                    Some(handle) => {
                        if handle.update(move |icone| icone.retrato = novo).is_some() {
                            publicado = Some(handle);
                        } else {
                            log::debug!("o ícone da barra encerrou sozinho");
                        }
                    }
                }
            }
        })
        .expect("spawn tray thread");
}

fn publicar(
    retrato: &Retrato,
    comandos: &Sender<IpcCommand>,
) -> Option<ksni::blocking::Handle<Icone>> {
    let icone = Icone {
        retrato: retrato.clone(),
        comandos: comandos.clone(),
    };
    // Vigia ausente vira espera, não erro: veja o comentário do módulo.
    match icone.assume_sni_available(true).spawn() {
        Ok(handle) => {
            log::info!("serviço do ícone da barra superior no ar");
            Some(handle)
        }
        Err(e) => {
            log::warn!("sem ícone na barra superior ({e})");
            None
        }
    }
}
