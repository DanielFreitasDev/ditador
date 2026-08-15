//! Área de transferência e colagem automática, do lado do Linux.
//!
//! No Wayland o caminho confiável é o `wl-copy`, que assume a posse do conteúdo
//! num processo próprio. O `arboard` (X11, via XWayland) fica como reserva, e é
//! ele que o `crate::clipboard` usa quando esta função aqui desiste.

use anyhow::{Context, Result, anyhow};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Guardamos o WAYLAND_DISPLAY original porque o modo X11 remove essa variável
/// do processo — mas o `wl-copy` ainda precisa dela.
static WAYLAND_DISPLAY: OnceLock<Option<String>> = OnceLock::new();

/// Deve ser chamada no início do `main`, antes de mexer nas variáveis de
/// ambiente. Veja a armadilha registrada no `CLAUDE.md`.
pub fn lembrar_o_ambiente() {
    let _ = WAYLAND_DISPLAY.set(std::env::var("WAYLAND_DISPLAY").ok());
}

/// A cópia pelo caminho nativo desta plataforma. `Err` manda o chamador tentar
/// o `arboard`.
pub fn copiar(texto: &str) -> Result<()> {
    let Some(Some(display)) = WAYLAND_DISPLAY.get() else {
        return Err(anyhow!("sessão não é Wayland"));
    };

    let mut child = Command::new("wl-copy")
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("executando wl-copy")?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow!("sem stdin no wl-copy"))?
        .write_all(texto.as_bytes())?;

    // O wl-copy se desdobra em segundo plano para servir o conteúdo; o processo
    // que chamamos termina logo em seguida.
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("wl-copy terminou com {status}"))
    }
}

pub fn colagem_disponivel() -> bool {
    crate::programas::existe("ydotool")
}

/// Envia Ctrl+V para a janela em foco.
pub fn colar() -> Result<()> {
    if !colagem_disponivel() {
        return Err(anyhow!(
            "ydotool não encontrado (instale com: sudo apt install ydotool)"
        ));
    }
    // Códigos evdev: 29 = KEY_LEFTCTRL, 47 = KEY_V.
    let status = Command::new("ydotool")
        .args(["key", "29:1", "47:1", "47:0", "29:0"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .context("executando ydotool")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "ydotool falhou ({status}). O serviço ydotoold está ativo? \
             Tente: systemctl --user status ydotool"
        ))
    }
}

/// O que dizer na tela quando a colagem automática não está disponível.
pub const COMO_HABILITAR_A_COLAGEM: &str =
    "Colagem automática requer o ydotool: sudo apt install ydotool";

/// Aviso de que a cópia está indo por um caminho pior, se estiver.
pub fn aviso_da_copia() -> Option<&'static str> {
    (!crate::programas::existe("wl-copy"))
        .then_some("wl-copy não encontrado; usando a área de transferência do X11.")
}
