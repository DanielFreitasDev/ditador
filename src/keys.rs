//! Conversão entre nomes de tecla do evdev e rótulos amigáveis.

use evdev::KeyCode;
use std::str::FromStr;

/// "KEY_PAUSE" -> KeyCode
pub fn parse(name: &str) -> Option<KeyCode> {
    KeyCode::from_str(name).ok()
}

/// KeyCode -> "KEY_PAUSE"
pub fn name(code: KeyCode) -> String {
    format!("{code:?}")
}

/// Teclas que só fazem sentido como parte de uma combinação.
pub fn is_modifier(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::KEY_LEFTCTRL
            | KeyCode::KEY_RIGHTCTRL
            | KeyCode::KEY_LEFTSHIFT
            | KeyCode::KEY_RIGHTSHIFT
            | KeyCode::KEY_LEFTALT
            | KeyCode::KEY_RIGHTALT
            | KeyCode::KEY_LEFTMETA
            | KeyCode::KEY_RIGHTMETA
    )
}

/// Rótulo curto em português para exibir na interface.
pub fn label(name: &str) -> String {
    let friendly = match name {
        "KEY_PAUSE" => "Pause/Break",
        "KEY_SCROLLLOCK" => "Scroll Lock",
        "KEY_SYSRQ" => "Print Screen",
        "KEY_INSERT" => "Insert",
        "KEY_DELETE" => "Delete",
        "KEY_HOME" => "Home",
        "KEY_END" => "End",
        "KEY_PAGEUP" => "Page Up",
        "KEY_PAGEDOWN" => "Page Down",
        "KEY_CAPSLOCK" => "Caps Lock",
        "KEY_LEFTCTRL" => "Ctrl esquerdo",
        "KEY_RIGHTCTRL" => "Ctrl direito",
        "KEY_LEFTSHIFT" => "Shift esquerdo",
        "KEY_RIGHTSHIFT" => "Shift direito",
        "KEY_LEFTALT" => "Alt",
        "KEY_RIGHTALT" => "AltGr",
        "KEY_LEFTMETA" => "Super esquerdo",
        "KEY_RIGHTMETA" => "Super direito",
        "KEY_SPACE" => "Espaço",
        "KEY_ENTER" => "Enter",
        "KEY_ESC" => "Esc",
        "KEY_TAB" => "Tab",
        "KEY_BACKSPACE" => "Backspace",
        "KEY_MENU" => "Menu",
        "KEY_COMPOSE" => "Menu de contexto",
        other => {
            // KEY_F13 -> "F13", KEY_A -> "A", KEY_KP0 -> "KP0"
            return other.strip_prefix("KEY_").unwrap_or(other).to_string();
        }
    };
    friendly.to_string()
}

/// Rótulo da combinação inteira, ex.: "Super esquerdo + Espaço".
pub fn combo_label(keys: &[String]) -> String {
    if keys.is_empty() {
        return "(nenhum)".to_string();
    }
    keys.iter()
        .map(|k| label(k))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Ordena a combinação com os modificadores primeiro, para exibir de forma previsível.
pub fn sort_combo(keys: &mut [String]) {
    keys.sort_by_key(|k| {
        let code = parse(k);
        let is_mod = code.map(is_modifier).unwrap_or(false);
        (!is_mod, code.map(|c| c.code()).unwrap_or(u16::MAX))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_nomes_do_evdev_vao_e_voltam() {
        // `name` sai do Debug do evdev e `parse` volta pelo FromStr dele. São
        // dois lados da crate, e nada promete que continuem combinando: é a
        // configuração gravada de todo mundo que depende disso. Se uma
        // atualização mudar a grafia, o teste avisa antes do usuário.
        for code in [
            KeyCode::KEY_PAUSE,
            KeyCode::KEY_LEFTMETA,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_F13,
            KeyCode::KEY_A,
        ] {
            let nome = name(code);
            assert!(nome.starts_with("KEY_"), "grafia inesperada: {nome}");
            assert_eq!(parse(&nome), Some(code), "não voltou: {nome}");
        }
        assert_eq!(parse("KEY_QUE_NAO_EXISTE"), None);
    }

    #[test]
    fn a_combinacao_sai_com_os_modificadores_na_frente() {
        let mut combo = vec!["KEY_SPACE".to_string(), "KEY_LEFTMETA".to_string()];
        sort_combo(&mut combo);
        assert_eq!(combo, ["KEY_LEFTMETA", "KEY_SPACE"]);
        assert_eq!(combo_label(&combo), "Super esquerdo + Espaço");
    }

    #[test]
    fn a_tecla_sem_nome_proprio_perde_o_prefixo() {
        assert_eq!(label("KEY_PAUSE"), "Pause/Break");
        assert_eq!(label("KEY_F13"), "F13");
        assert_eq!(label("KEY_KP0"), "KP0");
        assert_eq!(combo_label(&[]), "(nenhum)");
    }
}
