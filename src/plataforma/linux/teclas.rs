//! Nomes de tecla, do lado do Linux: a tabela é a do próprio evdev.
//!
//! Não copiamos a tabela para cá porque ela tem quase setecentas entradas e o
//! evdev já a mantém em dia com o kernel. O preço é depender do `Debug` e do
//! `FromStr` da crate, que são dois lados diferentes dela e nada promete que
//! continuem combinando — e é justamente a configuração gravada de todo mundo
//! que depende disso. Por isso há um teste no `crate::keys` fazendo os nomes
//! irem e voltarem: se uma atualização mudar a grafia, ele avisa antes do
//! usuário.

use evdev::KeyCode;
use std::str::FromStr;

/// "KEY_PAUSE" → 119
pub fn parse(nome: &str) -> Option<u16> {
    KeyCode::from_str(nome).ok().map(|tecla| tecla.code())
}

/// 119 → "KEY_PAUSE"
///
/// `None` para códigos que o evdev não nomeia: o `Debug` dele devolve
/// "unknown key: 217" nesses casos, e gravar isso na configuração produziria um
/// atalho que nunca mais dispara, sem nada avisando.
pub fn name(codigo: u16) -> Option<String> {
    let nome = format!("{:?}", KeyCode::new(codigo));
    nome.starts_with("KEY_").then_some(nome)
}
