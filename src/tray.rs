//! Ícone na barra superior do GNOME.
//!
//! O GNOME não tem bandeja nativa, mas a extensão AppIndicators (que vem
//! habilitada no Ubuntu) publica um `org.kde.StatusNotifierWatcher` no
//! barramento de sessão. Registramos um StatusNotifierItem nele: o ícone muda
//! conforme o estado e o menu dá acesso ao que o atalho faz.
//!
//! Se o vigia não existir (extensão desligada, outra área de trabalho), o
//! registro falha e o programa segue sem ícone — nunca é motivo para não subir.

use crate::controller::IpcCommand;
use crate::icones::{self, Estado};
use crate::keys;
use crate::state::{ModelState, SharedState, Sinal, View, lock};
use crossbeam_channel::Sender;
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip};

/// Retrato do estado que o ícone precisa. Guardamos uma cópia para que os
/// callbacks do ksni, que rodam na thread do D-Bus, nunca travem o mutex
/// principal.
#[derive(Clone, PartialEq)]
struct Retrato {
    view: View,
    model: ModelState,
    atalho: String,
}

impl Retrato {
    fn tirar(shared: &SharedState) -> Self {
        let estado = lock(shared);
        Self {
            view: estado.view,
            model: estado.model,
            atalho: keys::combo_label(&estado.config.hotkey),
        }
    }

    fn estado(&self) -> Estado {
        Estado::de(self.model, self.view)
    }

    fn resumo(&self) -> String {
        match (self.model, self.view) {
            (ModelState::Loading, _) => "Carregando o modelo…".to_string(),
            (ModelState::Failed, _) => "O modelo não carregou".to_string(),
            (_, View::Recording) => "Ouvindo…".to_string(),
            (_, View::Processing) => "Transcrevendo…".to_string(),
            _ => format!("Pronto · segure {}", self.atalho),
        }
    }
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
        self.retrato.estado().nome().to_string()
    }

    /// Reserva para quando o tema não tiver os nossos ícones. O protocolo manda
    /// o hospedeiro preferir o nome e só cair no mapa de bits se não achar.
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        icones::bandeja(self.retrato.estado())
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Ditador".to_string(),
            description: self.retrato.resumo(),
            icon_name: self.retrato.estado().nome().to_string(),
            icon_pixmap: icones::bandeja(self.retrato.estado()),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let gravando = self.retrato.view == View::Recording;
        let pronto = self.retrato.model == ModelState::Ready;

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

/// Publica o ícone e o mantém em dia. Não devolve erro de propósito: ficar sem
/// ícone é um degrau abaixo, não uma falha de inicialização.
pub fn start(shared: SharedState, sinal: &Sinal, comandos: Sender<IpcCommand>) {
    let mudancas = sinal.observar();
    let retrato = Retrato::tirar(&shared);

    let handle = match (Icone {
        retrato: retrato.clone(),
        comandos,
    })
    .spawn()
    {
        Ok(handle) => handle,
        Err(e) => {
            log::warn!(
                "sem ícone na barra superior ({e}). No GNOME, isso costuma \
                 significar que a extensão AppIndicators está desligada."
            );
            return;
        }
    };

    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            let mut atual = retrato;
            // Um aviso por mudança de estado; o retrato é lido depois, então
            // avisos acumulados se resolvem numa atualização só.
            while mudancas.recv().is_ok() {
                let novo = Retrato::tirar(&shared);
                if novo == atual {
                    continue;
                }
                atual = novo.clone();
                if handle.update(move |icone| icone.retrato = novo).is_none() {
                    log::debug!("ícone da barra encerrado");
                    return;
                }
            }
        })
        .expect("spawn tray thread");

    log::info!("ícone publicado na barra superior");
}
