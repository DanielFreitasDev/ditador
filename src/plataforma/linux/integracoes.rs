//! Quem mais está mostrando o Ditador na área de trabalho — no Linux, o D-Bus.
//!
//! Uma fachada de duas linhas sobre o `dbus.rs`, e não uma camada de verdade. Ela
//! existe para que o `main.rs` possa chamar `plataforma::integracoes::start(…)`
//! sem saber que do lado de cá isso quer dizer "publique a interface D-Bus e
//! vigie os nomes da extensão do GNOME e do widget do Plasma", e que do lado do
//! Windows quer dizer outra coisa completamente diferente.

pub use super::dbus::{integracoes_no_ar, start};

/// O que o `--diagnostico` diz quando não há integração nenhuma no ar.
///
/// Não é um problema: a bandeja é a reserva de todo mundo e funciona em qualquer
/// área de trabalho que hospede StatusNotifierItem. A frase existe para
/// responder à pergunta que traz a pessoa até aqui — "por que o ícone do Ditador
/// sumiu da barra?" — e para apontar o instalador certo sem obrigá-la a
/// descobrir qual é o desktop dela.
pub fn sem_nenhuma() -> String {
    let qual = match std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default() {
        d if d.contains("KDE") => "./kde-plasma/instalar.sh",
        d if d.contains("GNOME") => "./gnome-extension/instalar.sh",
        _ => "veja o README",
    };
    format!(
        "nenhuma. O Ditador aparece pelo ícone da bandeja, que funciona em toda \
         área de trabalho que tenha um. Para instalar a do seu desktop: {qual}"
    )
}
