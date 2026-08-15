//! O lado Windows: Raw Input, named pipes, Shell_NotifyIcon, registro.
//!
//! Tudo aqui é Win32 oficial pelo `windows-sys`, sem biblioteca de terceiros
//! embrulhando o que o sistema já oferece. "Win32 antigo" não é o mesmo que
//! "Win32 obsoleto": o Raw Input e o `Shell_NotifyIcon` continuam sendo as APIs
//! atuais e recomendadas para o que fazem, e a Microsoft as documenta como tal.
//! O que se evita aqui é o *legado de verdade* — nada de UWP, nada de
//! journaling hooks, nada de driver de teclado.
//!
//! ## O que não está aqui
//!
//! A interface. O Windows não ganha uma cópia da janela do egui feita em Win32,
//! e nem por isso fica sem ícone na barra ou sem aviso de gravação: quem cuida
//! disso é o `Ditador.Windows`, o frontend em WinUI 3 que conversa com este
//! processo pelo named pipe. É o mesmo arranjo do GNOME e do Plasma — o Rust é a
//! fonte da verdade, e quem desenha é quem sabe desenhar do jeito daquele
//! desktop.
//!
//! A diferença para o Linux é de quem toca primeiro. No GNOME e no Plasma o
//! Ditador publica um StatusNotifierItem por conta própria e a integração o
//! recolhe quando aparece; no Windows não há protocolo equivalente a "outro
//! programa já mostra este ícone", então o dono do ícone é decidido de uma vez —
//! e é o frontend. Isso está detalhado em `tray.rs`.

pub mod autostart;
pub mod clipboard;
pub mod integracoes;
pub mod ipc;
pub mod teclado;
pub mod teclas;
pub mod tray;
