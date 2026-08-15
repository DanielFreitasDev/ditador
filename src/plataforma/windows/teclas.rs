//! Nomes de tecla, do lado do Windows — e a ponte entre o `VK_*` do Win32 e o
//! código canônico que o resto do programa usa.
//!
//! ## Por que existe uma tabela escrita à mão aqui
//!
//! O Windows não tem nada equivalente ao `input-event-codes.h`: os `VK_*` são
//! constantes soltas em `winuser.h`, sem nome legível em tempo de execução e sem
//! ordem que sirva de tabela. E o código canônico do Ditador é o do evdev — que
//! é o que está gravado no `config.json` de quem já usa o programa no Linux, e o
//! que a extensão do GNOME e o widget do Plasma leem (veja `plataforma/mod.rs`).
//!
//! Então a ponte precisa ser explícita. Ela cobre o que um teclado de PC produz,
//! que é o que interessa a um atalho: as 104 teclas comuns mais os F13–F24 que
//! alguns teclados oferecem. Não cobre as teclas de multimídia nem as ~600
//! entradas exóticas do evdev — nenhuma delas chega por Raw Input num teclado
//! comum, e inventá-las aqui só criaria linhas que nunca seriam exercitadas.
//!
//! ## As três armadilhas
//!
//! **Modificadores.** O Raw Input entrega `VK_SHIFT`, `VK_CONTROL` e `VK_MENU`
//! genéricos — não diz se foi o da esquerda ou o da direita. Quem desfaz isso é
//! `crate::plataforma::teclado`, usando o scan code e o sinalizador E0 antes de
//! chegar aqui. Esta tabela só conhece os lados já resolvidos (`VK_LSHIFT` etc.),
//! e é de propósito: um `VK_SHIFT` sem lado não deve virar tecla nenhuma, porque
//! escolher um dos dois no chute produziria um atalho que dispara com a tecla
//! errada.
//!
//! **Teclado numérico.** `VK_HOME`, `VK_END`, as setas e companhia são a *mesma*
//! constante do bloco de navegação e do numérico com o Num Lock desligado. O que
//! os separa também é o E0, e também é resolvido antes.
//!
//! **Pause.** É a tecla mais peculiar do teclado de PC e é justamente o atalho
//! padrão do Ditador. O tratamento dela está em `teclado.rs`, onde o scan code
//! ainda existe; aqui ela é só mais uma linha.

/// Uma tecla, nas três representações que o programa precisa juntar.
struct Tecla {
    /// O código do `input-event-codes.h` do kernel Linux, que é o canônico aqui.
    codigo: u16,
    nome: &'static str,
    /// O virtual-key do Win32, já com o lado resolvido quando for o caso.
    vk: u16,
}

/// Atalho para não repetir `Tecla {` cem vezes.
macro_rules! teclas {
    ($($codigo:expr, $nome:literal, $vk:expr;)*) => {
        &[$(Tecla { codigo: $codigo, nome: $nome, vk: $vk },)*]
    };
}

/// Os números da coluna da esquerda saem do `input-event-codes.h`; os da direita,
/// do `winuser.h`. Os dois arquivos são estáveis há décadas — o que muda é quem
/// os lê.
#[rustfmt::skip]
const TABELA: &[Tecla] = teclas![
    // ------------------------------------------------- linha de função e Esc
    1,   "KEY_ESC",         0x1B; // VK_ESCAPE
    59,  "KEY_F1",          0x70; // VK_F1
    60,  "KEY_F2",          0x71;
    61,  "KEY_F3",          0x72;
    62,  "KEY_F4",          0x73;
    63,  "KEY_F5",          0x74;
    64,  "KEY_F6",          0x75;
    65,  "KEY_F7",          0x76;
    66,  "KEY_F8",          0x77;
    67,  "KEY_F9",          0x78;
    68,  "KEY_F10",         0x79;
    87,  "KEY_F11",         0x7A;
    88,  "KEY_F12",         0x7B;
    183, "KEY_F13",         0x7C;
    184, "KEY_F14",         0x7D;
    185, "KEY_F15",         0x7E;
    186, "KEY_F16",         0x7F;
    187, "KEY_F17",         0x80;
    188, "KEY_F18",         0x81;
    189, "KEY_F19",         0x82;
    190, "KEY_F20",         0x83;
    191, "KEY_F21",         0x84;
    192, "KEY_F22",         0x85;
    193, "KEY_F23",         0x86;
    194, "KEY_F24",         0x87;

    // ------------------------------------------------------- linha dos números
    41,  "KEY_GRAVE",       0xC0; // VK_OEM_3
    2,   "KEY_1",           0x31;
    3,   "KEY_2",           0x32;
    4,   "KEY_3",           0x33;
    5,   "KEY_4",           0x34;
    6,   "KEY_5",           0x35;
    7,   "KEY_6",           0x36;
    8,   "KEY_7",           0x37;
    9,   "KEY_8",           0x38;
    10,  "KEY_9",           0x39;
    11,  "KEY_0",           0x30;
    12,  "KEY_MINUS",       0xBD; // VK_OEM_MINUS
    13,  "KEY_EQUAL",       0xBB; // VK_OEM_PLUS
    14,  "KEY_BACKSPACE",   0x08; // VK_BACK

    // ------------------------------------------------------------------ letras
    30,  "KEY_A",           0x41;
    48,  "KEY_B",           0x42;
    46,  "KEY_C",           0x43;
    32,  "KEY_D",           0x44;
    18,  "KEY_E",           0x45;
    33,  "KEY_F",           0x46;
    34,  "KEY_G",           0x47;
    35,  "KEY_H",           0x48;
    23,  "KEY_I",           0x49;
    36,  "KEY_J",           0x4A;
    37,  "KEY_K",           0x4B;
    38,  "KEY_L",           0x4C;
    50,  "KEY_M",           0x4D;
    49,  "KEY_N",           0x4E;
    24,  "KEY_O",           0x4F;
    25,  "KEY_P",           0x50;
    16,  "KEY_Q",           0x51;
    19,  "KEY_R",           0x52;
    31,  "KEY_S",           0x53;
    20,  "KEY_T",           0x54;
    22,  "KEY_U",           0x55;
    47,  "KEY_V",           0x56;
    17,  "KEY_W",           0x57;
    45,  "KEY_X",           0x58;
    21,  "KEY_Y",           0x59;
    44,  "KEY_Z",           0x5A;

    // ------------------------------------------------------ pontuação e demais
    15,  "KEY_TAB",         0x09; // VK_TAB
    26,  "KEY_LEFTBRACE",   0xDB; // VK_OEM_4
    27,  "KEY_RIGHTBRACE",  0xDD; // VK_OEM_6
    43,  "KEY_BACKSLASH",   0xDC; // VK_OEM_5
    39,  "KEY_SEMICOLON",   0xBA; // VK_OEM_1
    40,  "KEY_APOSTROPHE",  0xDE; // VK_OEM_7
    28,  "KEY_ENTER",       0x0D; // VK_RETURN, sem E0
    51,  "KEY_COMMA",       0xBC; // VK_OEM_COMMA
    52,  "KEY_DOT",         0xBE; // VK_OEM_PERIOD
    53,  "KEY_SLASH",       0xBF; // VK_OEM_2
    86,  "KEY_102ND",       0xE2; // VK_OEM_102, a tecla a mais dos teclados ABNT/ISO
    57,  "KEY_SPACE",       0x20; // VK_SPACE
    58,  "KEY_CAPSLOCK",    0x14; // VK_CAPITAL

    // ------------------------------------------------------------ modificadores
    //
    // Só os lados já resolvidos entram. Veja o comentário do módulo.
    29,  "KEY_LEFTCTRL",    0xA2; // VK_LCONTROL
    97,  "KEY_RIGHTCTRL",   0xA3; // VK_RCONTROL
    42,  "KEY_LEFTSHIFT",   0xA0; // VK_LSHIFT
    54,  "KEY_RIGHTSHIFT",  0xA1; // VK_RSHIFT
    56,  "KEY_LEFTALT",     0xA4; // VK_LMENU
    100, "KEY_RIGHTALT",    0xA5; // VK_RMENU — o AltGr dos teclados ABNT
    125, "KEY_LEFTMETA",    0x5B; // VK_LWIN
    126, "KEY_RIGHTMETA",   0x5C; // VK_RWIN
    127, "KEY_COMPOSE",     0x5D; // VK_APPS, a tecla de menu de contexto

    // ------------------------------------------------- navegação e o bloco de 6
    110, "KEY_INSERT",      0x2D; // VK_INSERT
    111, "KEY_DELETE",      0x2E; // VK_DELETE
    102, "KEY_HOME",        0x24; // VK_HOME
    107, "KEY_END",         0x23; // VK_END
    104, "KEY_PAGEUP",      0x21; // VK_PRIOR
    109, "KEY_PAGEDOWN",    0x22; // VK_NEXT
    103, "KEY_UP",          0x26; // VK_UP
    108, "KEY_DOWN",        0x28; // VK_DOWN
    105, "KEY_LEFT",        0x25; // VK_LEFT
    106, "KEY_RIGHT",       0x27; // VK_RIGHT

    // -------------------------------------------------- o trio acima das setas
    99,  "KEY_SYSRQ",       0x2C; // VK_SNAPSHOT (Print Screen)
    70,  "KEY_SCROLLLOCK",  0x91; // VK_SCROLL
    119, "KEY_PAUSE",       0x13; // VK_PAUSE — o atalho padrão do Ditador

    // -------------------------------------------------------- teclado numérico
    69,  "KEY_NUMLOCK",     0x90; // VK_NUMLOCK
    98,  "KEY_KPSLASH",     0x6F; // VK_DIVIDE
    55,  "KEY_KPASTERISK",  0x6A; // VK_MULTIPLY
    74,  "KEY_KPMINUS",     0x6D; // VK_SUBTRACT
    78,  "KEY_KPPLUS",      0x6B; // VK_ADD
    96,  "KEY_KPENTER",     0x0E0D; // VK_RETURN com E0; veja `vk_estendido`
    83,  "KEY_KPDOT",       0x6E; // VK_DECIMAL
    82,  "KEY_KP0",         0x60; // VK_NUMPAD0
    79,  "KEY_KP1",         0x61;
    80,  "KEY_KP2",         0x62;
    81,  "KEY_KP3",         0x63;
    75,  "KEY_KP4",         0x64;
    76,  "KEY_KP5",         0x65;
    77,  "KEY_KP6",         0x66;
    71,  "KEY_KP7",         0x67;
    72,  "KEY_KP8",         0x68;
    73,  "KEY_KP9",         0x69;
];

/// O Enter do teclado numérico compartilha o `VK_RETURN` com o Enter comum e só
/// se distingue pelo sinalizador E0 do scan code. Como a tabela é um mapa de
/// `vk` para tecla, ele entra com o E0 embutido no número — `0x0E00 | vk` — e
/// quem consulta precisa dizer se veio estendido ou não.
///
/// É feio, e é feio de propósito: a alternativa era um campo `estendido: bool`
/// em todas as cem linhas para servir a uma delas.
const MARCA_E0: u16 = 0x0E00;

/// Junta o virtual-key com o sinalizador de tecla estendida, do jeito que a
/// tabela espera.
pub(super) fn vk_estendido(vk: u16, e0: bool) -> u16 {
    // Só o Enter do numérico precisa da distinção hoje. Marcar todas as teclas
    // estendidas quebraria as outras — `VK_HOME` estendido *é* o Home comum, e
    // é a versão sem E0 (o numérico com Num Lock desligado) que o Windows já
    // traduz para nós.
    if e0 && vk == 0x0D { MARCA_E0 | vk } else { vk }
}

/// "KEY_PAUSE" → código canônico.
pub fn parse(nome: &str) -> Option<u16> {
    TABELA
        .iter()
        .find(|tecla| tecla.nome == nome)
        .map(|tecla| tecla.codigo)
}

/// Código canônico → "KEY_PAUSE".
///
/// `None` para códigos que esta tabela não conhece — o que inclui teclas que
/// existem no Linux e não aqui. Um atalho gravado numa máquina Linux com uma
/// tecla de multimídia vai cair neste `None`, e quem trata disso é o
/// `crate::hotkey`, avisando na tela em vez de sumir com o atalho em silêncio.
pub fn name(codigo: u16) -> Option<String> {
    TABELA
        .iter()
        .find(|tecla| tecla.codigo == codigo)
        .map(|tecla| tecla.nome.to_string())
}

/// Virtual-key (já com o lado resolvido e o E0 aplicado) → código canônico.
///
/// É a porta por onde o Raw Input entra no programa.
pub(super) fn do_windows(vk: u16) -> Option<u16> {
    TABELA
        .iter()
        .find(|tecla| tecla.vk == vk)
        .map(|tecla| tecla.codigo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nenhum_codigo_nem_vk_aparece_duas_vezes() {
        // Uma linha repetida faria `do_windows` devolver a primeira e o resto da
        // tabela mentir em silêncio — o tipo de erro que só aparece quando
        // alguém escolhe justamente aquela tecla como atalho.
        let mut codigos = HashSet::new();
        let mut vks = HashSet::new();
        let mut nomes = HashSet::new();
        for tecla in TABELA {
            assert!(
                codigos.insert(tecla.codigo),
                "código repetido: {} ({})",
                tecla.codigo,
                tecla.nome
            );
            assert!(vks.insert(tecla.vk), "VK repetido: {:#x}", tecla.vk);
            assert!(nomes.insert(tecla.nome), "nome repetido: {}", tecla.nome);
        }
    }

    #[test]
    fn o_raw_input_chega_ao_atalho_padrao() {
        // O caminho inteiro da tecla Pause, que é o atalho padrão do Ditador:
        // VK_PAUSE → código canônico → nome que a configuração guarda.
        let codigo = do_windows(0x13).expect("VK_PAUSE não está na tabela");
        assert_eq!(codigo, 119);
        assert_eq!(name(codigo).as_deref(), Some("KEY_PAUSE"));
        assert_eq!(parse("KEY_PAUSE"), Some(119));
    }

    #[test]
    fn os_dois_enters_nao_se_confundem() {
        // O Enter comum e o do teclado numérico compartilham o VK_RETURN; o que
        // os separa é o E0. Sem isso, um atalho no Enter do numérico disparava
        // também no Enter comum.
        assert_eq!(do_windows(vk_estendido(0x0D, false)), Some(28)); // KEY_ENTER
        assert_eq!(do_windows(vk_estendido(0x0D, true)), Some(96)); // KEY_KPENTER

        // E nenhuma outra tecla é afetada pelo E0: o Home estendido continua
        // sendo o Home.
        assert_eq!(do_windows(vk_estendido(0x24, true)), Some(102));
        assert_eq!(do_windows(vk_estendido(0x24, false)), Some(102));
    }

    #[test]
    fn o_modificador_sem_lado_nao_vira_tecla() {
        // VK_SHIFT, VK_CONTROL e VK_MENU genéricos não estão na tabela de
        // propósito: quem os resolve é o `teclado.rs`, e escolher um lado no
        // chute aqui produziria um atalho que dispara com a tecla errada.
        for generico in [0x10u16, 0x11, 0x12] {
            assert_eq!(
                do_windows(generico),
                None,
                "o modificador {generico:#x} entrou na tabela sem lado"
            );
        }
        // Já os lados resolvidos precisam estar todos lá.
        assert_eq!(do_windows(0xA0), Some(42)); // VK_LSHIFT
        assert_eq!(do_windows(0xA1), Some(54)); // VK_RSHIFT
        assert_eq!(do_windows(0xA2), Some(29)); // VK_LCONTROL
        assert_eq!(do_windows(0xA3), Some(97)); // VK_RCONTROL
        assert_eq!(do_windows(0xA4), Some(56)); // VK_LMENU
        assert_eq!(do_windows(0xA5), Some(100)); // VK_RMENU
    }
}
