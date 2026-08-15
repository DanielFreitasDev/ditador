//! O ícone do Ditador na barra do sistema.
//!
//! Uma linha de fachada, porque a resposta é radicalmente diferente de cada lado
//! e a pergunta é a mesma: *mantenha um ícone em dia com o estado do programa*.
//!
//! * **Linux** — o Ditador publica um StatusNotifierItem por conta própria e o
//!   recolhe quando a extensão do GNOME ou o widget do Plasma aparecem. Ele é a
//!   reserva de todo mundo: funciona em qualquer área de trabalho que hospede o
//!   protocolo, sem instalar nada.
//! * **Windows** — o dono do ícone é o frontend `Ditador.Windows`, e este
//!   processo não desenha ícone nenhum. O porquê está em
//!   `plataforma/windows/tray.rs`.

pub fn start(
    shared: crate::state::SharedState,
    sinal: &crate::state::Sinal,
    comandos: crossbeam_channel::Sender<crate::controller::IpcCommand>,
) {
    crate::plataforma::tray::start(shared, sinal, comandos)
}
