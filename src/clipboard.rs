//! Área de transferência e colagem automática.
//!
//! No Wayland o caminho confiável é o `wl-copy`, que assume a posse do conteúdo
//! num processo próprio. O `arboard` (X11, via XWayland) fica como reserva.

use anyhow::{Context, Result, anyhow};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Guardamos o WAYLAND_DISPLAY original porque o modo X11 remove essa variável
/// do processo — mas o `wl-copy` ainda precisa dela.
static WAYLAND_DISPLAY: OnceLock<Option<String>> = OnceLock::new();
static ARBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

/// Deve ser chamada no início do `main`, antes de mexer nas variáveis de ambiente.
pub fn remember_environment() {
    let _ = WAYLAND_DISPLAY.set(std::env::var("WAYLAND_DISPLAY").ok());
}

pub fn copy(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    match copy_with_wl_copy(text) {
        Ok(()) => return Ok(()),
        Err(e) => log::debug!("wl-copy indisponível ({e:#}), usando arboard"),
    }

    let holder = ARBOARD.get_or_init(|| Mutex::new(None));
    let mut guard = holder.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().context("abrindo a área de transferência")?);
    }
    guard
        .as_mut()
        .expect("clipboard inicializado")
        .set_text(text.to_string())
        .context("copiando texto")?;
    Ok(())
}

fn copy_with_wl_copy(text: &str) -> Result<()> {
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
        .write_all(text.as_bytes())?;

    // O wl-copy se desdobra em segundo plano para servir o conteúdo; o processo
    // que chamamos termina logo em seguida.
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("wl-copy terminou com {status}"))
    }
}

pub fn paste_available() -> bool {
    which("ydotool")
}

/// Envia Ctrl+V para a janela em foco.
pub fn paste() -> Result<()> {
    if !paste_available() {
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

pub fn wl_copy_available() -> bool {
    which("wl-copy")
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(program);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}
