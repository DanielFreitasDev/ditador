//! Área de transferência e colagem automática, do lado do Linux.
//!
//! No Wayland o caminho confiável é o `wl-copy`, que assume a posse do conteúdo
//! num processo próprio. O `arboard` (X11, via XWayland) fica como reserva, e é
//! ele que o `crate::clipboard` usa quando esta função aqui desiste.

use crate::config::{MetodoDeColagem, TeclaDeEnvio};
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

/// Os códigos evdev das teclas que este módulo sintetiza.
///
/// São os mesmos números que o resto do programa usa para o atalho — a
/// numeração canônica que o `plataforma/mod.rs` explica. Escritos por nome
/// porque `29:1 47:1 47:0 29:0` é ilegível e já esteve errado.
mod tecla {
    pub const CTRL: u16 = 29;
    pub const SHIFT: u16 = 42;
    pub const V: u16 = 47;
    pub const ENTER: u16 = 28;
    pub const INSERT: u16 = 110;
}

/// A combinação que cada método de colagem produz, em códigos evdev.
///
/// `None` é o `Digitar`, que não é uma combinação de teclas e vai por outro
/// caminho.
fn combinacao(metodo: MetodoDeColagem) -> Option<(&'static [u16], u16)> {
    Some(match metodo {
        MetodoDeColagem::CtrlV => (&[tecla::CTRL], tecla::V),
        MetodoDeColagem::ShiftInsert => (&[tecla::SHIFT], tecla::INSERT),
        MetodoDeColagem::CtrlShiftV => (&[tecla::CTRL, tecla::SHIFT], tecla::V),
        MetodoDeColagem::Digitar => return None,
    })
}

fn combinacao_de_envio(envio: TeclaDeEnvio) -> Option<(&'static [u16], u16)> {
    Some(match envio {
        TeclaDeEnvio::Nenhuma => return None,
        TeclaDeEnvio::Enter => (&[], tecla::ENTER),
        TeclaDeEnvio::CtrlEnter => (&[tecla::CTRL], tecla::ENTER),
    })
}

/// Os argumentos do `ydotool key` para uma combinação.
///
/// Modificadores apertados na ordem, a tecla principal, e tudo solto na ordem
/// inversa — que é como um teclado de verdade se comporta e é o que os
/// programas de destino esperam ver.
///
/// Separada da execução porque é a única parte testável: rodar o `ydotool` de
/// verdade exige o serviço dele de pé e digita na janela de quem estiver
/// mexendo na máquina.
fn argumentos_da_combinacao(modificadores: &[u16], principal: u16) -> Vec<String> {
    let mut args = Vec::with_capacity(modificadores.len() * 2 + 2);
    for m in modificadores {
        args.push(format!("{m}:1"));
    }
    args.push(format!("{principal}:1"));
    args.push(format!("{principal}:0"));
    for m in modificadores.iter().rev() {
        args.push(format!("{m}:0"));
    }
    args
}

/// Envia para a janela em foco o atalho de colar que a configuração escolheu.
pub fn colar(metodo: MetodoDeColagem, texto: &str) -> Result<()> {
    conferir_o_ydotool()?;
    match combinacao(metodo) {
        Some((modificadores, principal)) => teclar(modificadores, principal),
        None => digitar(texto),
    }
}

/// Aperta a tecla que envia o texto, depois de ele ter sido colado.
pub fn enviar_tecla(envio: TeclaDeEnvio) -> Result<()> {
    let Some((modificadores, principal)) = combinacao_de_envio(envio) else {
        return Ok(());
    };
    conferir_o_ydotool()?;
    teclar(modificadores, principal)
}

fn conferir_o_ydotool() -> Result<()> {
    if colagem_disponivel() {
        return Ok(());
    }
    Err(anyhow!(
        "ydotool não encontrado (instale com: sudo apt install ydotool)"
    ))
}

fn teclar(modificadores: &[u16], principal: u16) -> Result<()> {
    rodar_ydotool(
        std::iter::once("key".to_string())
            .chain(argumentos_da_combinacao(modificadores, principal)),
    )
}

/// Digita o texto, sem passar pela área de transferência.
///
/// O `--` é obrigatório: sem ele, um texto transcrito que comece com hífen — o
/// que acontece com uma fala que começa por travessão — seria lido pelo
/// `ydotool` como opção de linha de comando, e a colagem falharia com uma
/// mensagem sobre um argumento desconhecido.
fn digitar(texto: &str) -> Result<()> {
    if texto.is_empty() {
        return Ok(());
    }
    rodar_ydotool(["type", "--", texto].into_iter().map(String::from))
}

fn rodar_ydotool(args: impl Iterator<Item = String>) -> Result<()> {
    let status = Command::new("ydotool")
        .args(args)
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

/// O que a colagem automática faz nesta plataforma, e o que ela custa.
///
/// Aparece nas configurações quando a chave é ligada e no `--diagnostico`. O
/// Windows tem a versão dele, com outras ressalvas — as duas plataformas colam,
/// mas por caminhos que falham de maneiras diferentes.
pub const SOBRE_A_COLAGEM: &str = "O Ctrl+V vai pelo ydotool, que precisa do serviço ativo — confira com \
     systemctl --user status ydotool — e chega na janela que estiver em foco \
     quando a transcrição terminar.";

/// Aviso de que a cópia está indo por um caminho pior, se estiver.
pub fn aviso_da_copia() -> Option<&'static str> {
    // A receita de instalação vai junto de propósito: quem lê esta linha no
    // `--diagnostico` está justamente atrás do que fazer a respeito, e a frase
    // sem ela já foi só uma constatação por um tempo.
    (!crate::programas::existe("wl-copy")).then_some(
        "wl-copy não encontrado; usando a área de transferência do X11. \
         Para instalar: sudo apt install wl-clipboard",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combinacao_aperta_na_ordem_e_solta_na_inversa() {
        // É como um teclado de verdade se comporta, e é o que os programas de
        // destino esperam ver. Soltar na mesma ordem deixaria o Shift solto
        // antes do Ctrl numa combinação de dois, e há programa que lê isso como
        // outro atalho.
        assert_eq!(
            argumentos_da_combinacao(&[tecla::CTRL], tecla::V),
            vec!["29:1", "47:1", "47:0", "29:0"]
        );
        assert_eq!(
            argumentos_da_combinacao(&[tecla::CTRL, tecla::SHIFT], tecla::V),
            vec!["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
        );
        // Sem modificador nenhum — o Enter do envio automático.
        assert_eq!(
            argumentos_da_combinacao(&[], tecla::ENTER),
            vec!["28:1", "28:0"]
        );
    }

    #[test]
    fn cada_metodo_produz_a_combinacao_que_o_nome_promete() {
        // O `Ctrl+V` da configuração precisa ser mesmo o Ctrl+V: estes números
        // já estiveram escritos à mão no meio de uma chamada de comando, e
        // trocar um deles produziria uma colagem que "não faz nada" sem erro.
        let esperado: &[(MetodoDeColagem, &[&str])] = &[
            (MetodoDeColagem::CtrlV, &["29:1", "47:1", "47:0", "29:0"]),
            (
                MetodoDeColagem::ShiftInsert,
                &["42:1", "110:1", "110:0", "42:0"],
            ),
            (
                MetodoDeColagem::CtrlShiftV,
                &["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
            ),
        ];
        for (metodo, args) in esperado {
            let (modificadores, principal) =
                combinacao(*metodo).expect("este método é uma combinação");
            assert_eq!(
                argumentos_da_combinacao(modificadores, principal),
                *args,
                "{metodo:?}"
            );
        }
        // Digitar não é combinação nenhuma.
        assert!(combinacao(MetodoDeColagem::Digitar).is_none());
    }

    #[test]
    fn os_codigos_evdev_sao_os_mesmos_que_o_resto_do_programa_usa() {
        // A numeração é a canônica do projeto, e há uma tabela dela em
        // `plataforma/linux/teclas.rs`. Divergindo, o atalho e a colagem
        // passariam a falar de teclas diferentes.
        assert_eq!(crate::keys::parse("KEY_LEFTCTRL"), Some(tecla::CTRL));
        assert_eq!(crate::keys::parse("KEY_LEFTSHIFT"), Some(tecla::SHIFT));
        assert_eq!(crate::keys::parse("KEY_V"), Some(tecla::V));
        assert_eq!(crate::keys::parse("KEY_ENTER"), Some(tecla::ENTER));
        assert_eq!(crate::keys::parse("KEY_INSERT"), Some(tecla::INSERT));
    }

    #[test]
    fn a_tecla_de_envio_nenhuma_nao_produz_combinacao() {
        assert!(combinacao_de_envio(TeclaDeEnvio::Nenhuma).is_none());
        let (mods, principal) = combinacao_de_envio(TeclaDeEnvio::CtrlEnter).expect("acorde");
        assert_eq!(
            argumentos_da_combinacao(mods, principal),
            vec!["29:1", "28:1", "28:0", "29:0"]
        );
    }
}
