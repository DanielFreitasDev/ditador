//! Área de transferência e colagem automática.
//!
//! Duas camadas: a plataforma tenta o caminho nativo dela, e o `arboard` é a
//! reserva. No Linux o nativo é o `wl-copy`, que assume a posse do conteúdo num
//! processo próprio — coisa que o `arboard` não faz bem no Wayland. No Windows
//! não há nativo melhor que o `arboard`, então ele é o caminho único, e é a
//! própria plataforma que diz isso devolvendo `Err`.

use anyhow::{Context, Result};
use std::sync::{Mutex, OnceLock};

use crate::plataforma::clipboard as nativo;

pub use nativo::COMO_HABILITAR_A_COLAGEM;

static ARBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

/// Deve ser chamada no início do `main`, antes de mexer nas variáveis de
/// ambiente.
///
/// **Não reordene.** No Linux o modo X11 remove o `WAYLAND_DISPLAY` do processo,
/// e sem o retrato tirado antes disso o `wl-copy` para de funcionar. É uma das
/// armadilhas registradas no `CLAUDE.md`, e continua valendo — o que mudou foi
/// só o endereço de quem tira o retrato.
pub fn remember_environment() {
    nativo::lembrar_o_ambiente();
}

pub fn copy(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    match nativo::copiar(text) {
        Ok(()) => return Ok(()),
        Err(e) => log::debug!("caminho nativo indisponível ({e:#}), usando arboard"),
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

/// Dá para colar sozinho na janela em foco?
///
/// No Linux depende do `ydotool` estar instalado; no Windows é sempre `false`, e
/// por decisão — o porquê está em `plataforma/windows/clipboard.rs`.
pub fn paste_available() -> bool {
    nativo::colagem_disponivel()
}

/// Envia Ctrl+V para a janela em foco.
pub fn paste() -> Result<()> {
    nativo::colar()
}

/// Aviso de que a cópia está indo por um caminho pior, se estiver.
pub fn aviso_da_copia() -> Option<&'static str> {
    nativo::aviso_da_copia()
}
