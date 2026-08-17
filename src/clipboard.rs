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

pub use nativo::{COMO_HABILITAR_A_COLAGEM, SOBRE_A_COLAGEM};

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
/// No Linux depende do `ydotool` estar instalado; no Windows o `SendInput` é do
/// próprio sistema e a resposta é sempre sim. O que muda de um para o outro — e
/// o que cada um custa — está em `SOBRE_A_COLAGEM`, que a interface mostra na
/// hora de ligar a chave.
pub fn paste_available() -> bool {
    nativo::colagem_disponivel()
}

/// Entrega o texto à janela em foco, pelo método escolhido.
///
/// O `texto` só é usado pelo método `Digitar`, que não passa pela área de
/// transferência — os outros três apenas sintetizam o atalho de colar, e o que
/// eles colam é o que o `copy` acabou de pôr lá. Ele vai como argumento nos
/// quatro casos para que o chamador não precise saber qual é qual: a pergunta
/// que ele faz é "entregue este texto", e a resposta de como fazer isso é da
/// configuração.
pub fn paste(metodo: crate::config::MetodoDeColagem, texto: &str) -> Result<()> {
    nativo::colar(metodo, texto)
}

/// Aperta a tecla que envia o texto, logo depois de ele ser colado.
///
/// Sem nada configurado é um sucesso que não faz nada, e é por isso que o
/// chamador pode chamá-la sempre.
pub fn submit(tecla: crate::config::TeclaDeEnvio) -> Result<()> {
    nativo::enviar_tecla(tecla)
}

/// Aviso de que a cópia está indo por um caminho pior, se estiver.
pub fn aviso_da_copia() -> Option<&'static str> {
    nativo::aviso_da_copia()
}
