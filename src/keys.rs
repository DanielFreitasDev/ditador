//! Nomes de tecla: os que a configuração guarda e os que a interface mostra.
//!
//! O programa inteiro fala de teclas por um `u16` — o código canônico, que é o
//! do evdev nas duas plataformas (o porquê está em `plataforma/mod.rs`). Quem
//! traduz entre esse número e o nome `KEY_*` é `plataforma::teclas`, porque a
//! tabela é diferente de cada lado: no Linux ela vem do próprio evdev, no
//! Windows é nossa e cobre o que um teclado de PC produz.
//!
//! O resto deste arquivo — quem é modificador, o rótulo em português, a ordem da
//! combinação — não depende de sistema nenhum e nunca dependeu. Repare que tudo
//! aqui trabalha sobre o **nome**, não sobre o código: é o nome que está gravado
//! no `config.json` de quem já usa o Ditador, é o nome que a extensão do GNOME e
//! o widget do Plasma leem, e decidir pelo nome mantém uma configuração escrita
//! no Linux funcionando no Windows e vice-versa.

/// "KEY_PAUSE" → código canônico.
pub fn parse(nome: &str) -> Option<u16> {
    crate::plataforma::teclas::parse(nome)
}

/// Código canônico → "KEY_PAUSE". `None` quando a tecla não tem nome próprio.
pub fn name(codigo: u16) -> Option<String> {
    crate::plataforma::teclas::name(codigo)
}

/// Teclas que só fazem sentido como parte de uma combinação.
///
/// Decidido pelo nome, e não pelo código, para não precisar de uma tabela por
/// plataforma: são oito nomes, iguais dos dois lados.
pub fn is_modifier(nome: &str) -> bool {
    matches!(
        nome,
        "KEY_LEFTCTRL"
            | "KEY_RIGHTCTRL"
            | "KEY_LEFTSHIFT"
            | "KEY_RIGHTSHIFT"
            | "KEY_LEFTALT"
            | "KEY_RIGHTALT"
            | "KEY_LEFTMETA"
            | "KEY_RIGHTMETA"
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
    keys.sort_by_key(|k| (!is_modifier(k), parse(k).unwrap_or(u16::MAX)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_nomes_das_teclas_vao_e_voltam() {
        // No Linux `name` sai do Debug do evdev e `parse` volta pelo FromStr
        // dele: são dois lados da crate, e nada promete que continuem
        // combinando. No Windows os dois saem da nossa tabela, que pode ganhar
        // uma linha torta. É a configuração gravada de todo mundo que depende
        // disso — se a grafia mudar, o teste avisa antes do usuário.
        //
        // Estas são as teclas que a interface oferece e que a documentação cita;
        // o `KEY_PAUSE` é o atalho padrão e o mais importante da lista.
        for nome in [
            "KEY_PAUSE",
            "KEY_LEFTCTRL",
            "KEY_RIGHTCTRL",
            "KEY_LEFTSHIFT",
            "KEY_LEFTALT",
            "KEY_RIGHTALT",
            "KEY_LEFTMETA",
            "KEY_SPACE",
            "KEY_ESC",
            "KEY_ENTER",
            "KEY_TAB",
            "KEY_CAPSLOCK",
            "KEY_SCROLLLOCK",
            "KEY_INSERT",
            "KEY_HOME",
            "KEY_END",
            "KEY_F1",
            "KEY_F12",
            "KEY_F13",
            "KEY_A",
            "KEY_Z",
            "KEY_0",
            "KEY_9",
        ] {
            let codigo = parse(nome).unwrap_or_else(|| panic!("não reconheceu {nome}"));
            assert_eq!(
                name(codigo).as_deref(),
                Some(nome),
                "o código {codigo} não voltou a ser {nome}"
            );
        }
        assert_eq!(parse("KEY_QUE_NAO_EXISTE"), None);
    }

    #[test]
    fn o_codigo_canonico_e_o_mesmo_nas_duas_plataformas() {
        // Uma configuração escrita no Linux precisa valer no Windows e
        // vice-versa. Estes números são os do `input-event-codes.h` do kernel, e
        // estão escritos à mão aqui de propósito: no Linux eles vêm do evdev, no
        // Windows da nossa tabela, e este teste é o único lugar em que os dois
        // são conferidos contra a mesma fonte.
        for (nome, codigo) in [
            ("KEY_ESC", 1u16),
            ("KEY_A", 30),
            ("KEY_SPACE", 57),
            ("KEY_LEFTCTRL", 29),
            ("KEY_LEFTSHIFT", 42),
            ("KEY_LEFTALT", 56),
            ("KEY_LEFTMETA", 125),
            ("KEY_PAUSE", 119),
            ("KEY_F1", 59),
            ("KEY_F12", 88),
        ] {
            assert_eq!(parse(nome), Some(codigo), "{nome} mudou de número");
        }
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
