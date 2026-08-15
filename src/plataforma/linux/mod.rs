//! O lado Linux: evdev, D-Bus, StatusNotifierItem, systemd, Wayland.
//!
//! Nada aqui mudou de comportamento na portabilidade para o Windows — o que
//! mudou foi o endereço. Os arquivos vieram do topo de `src/` inteiros, e o
//! histórico do git os acompanha (`git log --follow`), porque a alternativa
//! seria reescrever de memória um código que já foi depurado contra o mundo
//! real: cada comentário aqui dentro é uma armadilha que alguém já pisou.

pub mod autostart;
pub mod clipboard;
pub mod dbus;
pub mod integracoes;
pub mod ipc;
pub mod microfone;
pub mod registro;
pub mod teclado;
pub mod teclas;
pub mod tray;
