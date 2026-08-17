//! Área de transferência e colagem automática, do lado do Windows.
//!
//! ## A cópia não precisa de nada nosso
//!
//! O `arboard`, que no Linux é a reserva para quando o `wl-copy` não está
//! disponível, no Windows é o caminho principal e único — ele fala
//! `OpenClipboard`/`SetClipboardData` direto, que é a API oficial e não tem
//! concorrente. Por isso `copiar` aqui devolve `Err` de propósito: não é falha,
//! é dizer "não há caminho nativo melhor, use a reserva", e o `crate::clipboard`
//! entende exatamente isso.
//!
//! Escrever um segundo caminho em Win32 cru daria a mesma coisa com mais linhas
//! e mais chances de vazar um handle de clipboard — que, quando vaza, trava a
//! área de transferência do Windows inteiro até o processo morrer.
//!
//! ## A colagem automática, por `SendInput`
//!
//! No Linux o Ditador cola com o `ydotool`, que sintetiza Ctrl+V num teclado
//! virtual. O equivalente aqui é o `SendInput`, que é a API documentada para
//! isso — a mesma que o teclado na tela do Windows usa — e é o que está abaixo.
//! A opção existe nos dois sistemas, desligada por padrão nos dois, e quem a
//! liga vê na tela o que ela custa.
//!
//! Ela **não** é isenta de arestas, e nenhuma delas é escondida de quem usa:
//!
//! * o Ctrl+V vai para **onde o foco estiver** no instante em que a transcrição
//!   termina, que não é necessariamente onde estava quando a pessoa começou a
//!   falar. Ditar uma frase longa e trocar de janela no meio manda o texto para
//!   a janela nova. É a mesma aresta do `ydotool` no Linux, e não tem conserto
//!   possível: quem cola é o teclado, e o teclado escreve onde o foco está;
//! * `SendInput` **não alcança janelas de integridade mais alta** (UIPI): num
//!   editor aberto como administrador o texto simplesmente não apareceria, sem
//!   erro nenhum. Aqui isso é conferido *antes* de tentar — veja
//!   `foco_inalcancavel` —, e o que a pessoa recebe é a janela de resultado com
//!   o texto e uma frase dizendo por que ele não foi colado, em vez de silêncio;
//! * há antivírus que tratam injeção de teclado como comportamento de
//!   *keylogger*, e o Ditador já lê o teclado globalmente por Raw Input. É por
//!   isso que a interface avisa (`SOBRE_A_COLAGEM`) antes de a pessoa ligar.
//!
//! ### As teclas que já estão seguradas
//!
//! Um Ctrl+V sintético solto no meio de um Shift segurado vira Ctrl+Shift+V, que
//! em metade dos programas é "colar sem formatação" e na outra metade não é
//! nada. Quem grava por *alternar* com um atalho de modificador ainda está com a
//! mão nele quando a transcrição termina, e esse caso é comum o bastante para
//! ser tratado: `montar_sequencia` solta o que atrapalha antes do Ctrl+V e o
//! devolve depois, na ordem inversa. Um Ctrl já segurado é o único que ajuda —
//! nesse caso a sequência aproveita o que está lá em vez de apertar por cima e
//! soltar depois, o que deixaria o sistema achando que a pessoa soltou uma tecla
//! que ela ainda segura.
//!
//! ### O que voltar pelo nosso próprio Raw Input
//!
//! O Ctrl+V sintético chega de volta à escuta de teclado deste mesmo processo,
//! como qualquer tecla — mas com `hDevice` nulo, porque não veio de teclado
//! nenhum. A máquina de teclas conta origens por dispositivo justamente por
//! isso (é a mesma defesa que existe no Linux por causa do teclado virtual do
//! `ydotool`), então o "soltar" sintético não derruba a tecla que a pessoa
//! esteja segurando de verdade. O que continua valendo nos dois sistemas: um
//! atalho configurado *em Ctrl+V* seria disparado pela própria colagem. Ninguém
//! configura esse atalho, e ignorar teclas de `hDevice` nulo custaria caro —
//! quem usa teclado na tela ou software de acessibilidade ficaria sem atalho
//! nenhum.

use crate::config::{MetodoDeColagem, TeclaDeEnvio};
use anyhow::{Result, anyhow};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VK_CONTROL,
    VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// No Windows não há variável de ambiente a preservar antes que o programa mexa
/// nelas — o equivalente Linux existe por causa do `WAYLAND_DISPLAY`, que o modo
/// X11 remove.
pub fn lembrar_o_ambiente() {}

/// Não há caminho nativo melhor que o `arboard`. Veja o comentário do módulo.
pub fn copiar(_texto: &str) -> Result<()> {
    Err(anyhow!("no Windows a cópia vai direto pelo arboard"))
}

/// O `SendInput` faz parte do `user32` e está sempre lá — diferente do `ydotool`
/// do Linux, que é um pacote à parte e pode não estar instalado. Aqui a pergunta
/// "dá para colar?" não tem como ser respondida com `false`.
pub fn colagem_disponivel() -> bool {
    true
}

/// A marca que vai em cada tecla sintética nossa, no `dwExtraInfo`.
///
/// O Raw Input não entrega este campo (quem o lê são os ganchos de teclado), e
/// portanto ela não serve para *nós* filtrarmos nada. Serve para quem estiver do
/// outro lado: programas de automação e de acessibilidade olham o `dwExtraInfo`
/// para não reprocessar o que eles mesmos ou terceiros injetaram, e um valor
/// próprio nos torna identificáveis em vez de anônimos. Custa zero.
const ASSINATURA: usize = 0x0D17_AD05;

/// Envia para a janela em foco o atalho de colar que a configuração escolheu.
pub fn colar(metodo: MetodoDeColagem, texto: &str) -> Result<()> {
    conferir_o_foco()?;

    match acorde_de_colagem(metodo) {
        Some(acorde) => enviar(&montar_sequencia(&segurando_agora(), acorde)),
        // `Digitar` não usa a área de transferência: o texto sai tecla a tecla.
        None => digitar(texto),
    }
}

/// Aperta a tecla que envia o texto, depois de ele ter sido colado.
pub fn enviar_tecla(tecla: TeclaDeEnvio) -> Result<()> {
    let Some(acorde) = acorde_de_envio(tecla) else {
        return Ok(());
    };
    conferir_o_foco()?;
    enviar(&montar_sequencia(&segurando_agora(), acorde))
}

fn conferir_o_foco() -> Result<()> {
    if foco_inalcancavel() {
        return Err(anyhow!(
            "a janela em foco tem privilégio maior que o do Ditador e o Windows \
             não deixa teclas sintéticas chegarem nela. O texto está na área de \
             transferência: cole com Ctrl+V"
        ));
    }
    Ok(())
}

/// Digita o texto, caractere a caractere, sem passar pela área de transferência.
///
/// `KEYEVENTF_UNICODE` manda o **caractere**, e não uma tecla: o `wVk` vai zero e
/// o `wScan` carrega a unidade UTF-16. É por isso que este caminho não depende do
/// layout de teclado — um "ç" sai "ç" num teclado americano — e é a diferença
/// entre digitar e sintetizar as teclas que produziriam aquelas letras, que seria
/// impossível de acertar para todo layout que existe.
///
/// Os modificadores segurados **precisam** sair da frente aqui, e por um motivo
/// mais grave do que na colagem: um Ctrl segurado transforma cada caractere
/// digitado num caractere de controle, e o que chega ao editor não é texto
/// estranho, é uma sequência de comandos.
fn digitar(texto: &str) -> Result<()> {
    if texto.is_empty() {
        return Ok(());
    }

    let segurando = segurando_agora();
    let mut fila: Vec<Tecla> = Vec::new();
    let atrapalham = segurando.no_caminho_de(&[]);
    for &vk in &atrapalham {
        fila.push(Tecla::solta(vk));
    }
    for unidade in texto.encode_utf16() {
        fila.push(Tecla::caractere(unidade, false));
        fila.push(Tecla::caractere(unidade, true));
    }
    for &vk in atrapalham.iter().rev() {
        fila.push(Tecla::aperta(vk));
    }

    // Em blocos, e não numa chamada só como a colagem: um texto de mil
    // caracteres vira quatro mil `INPUT`, e a fila de entrada do Windows tem
    // limite — passando dele, o `SendInput` aceita uma parte e descarta o resto,
    // que sairia como um texto cortado no meio sem erro nenhum. O bloco também
    // dá ao programa de destino a chance de processar o que já chegou.
    const BLOCO: usize = 200;
    for pedaco in fila.chunks(BLOCO) {
        enviar(pedaco)?;
    }
    Ok(())
}

/// O que dizer na tela quando a colagem automática não está disponível.
///
/// No Windows ela está sempre — `colagem_disponivel` devolve `true` e pronto —, e
/// esta constante existe porque o contrato da plataforma a exige. Se um dia este
/// texto aparecer na tela, é sinal de que alguém mexeu naquela função.
pub const COMO_HABILITAR_A_COLAGEM: &str = "A colagem automática usa o Ctrl+V do próprio Windows e não depende de nada \
     instalado.";

/// O que a colagem automática faz nesta plataforma, e o que ela custa.
///
/// Aparece nas configurações quando a chave é ligada e no `--diagnostico`. É o
/// aviso, e ele é dito antes — não depois de o texto ter ido para o lugar errado.
pub const SOBRE_A_COLAGEM: &str = "O Ctrl+V é sintético: ele vai para a janela que estiver em foco quando a \
     transcrição terminar, não alcança janelas abertas como administrador, e há \
     antivírus que olham torto para programas que digitam sozinhos.";

/// A cópia no Windows não tem caminho degradado — ou funciona, ou o erro aparece
/// na hora.
pub fn aviso_da_copia() -> Option<&'static str> {
    None
}

/// Uma tecla a apertar ou a soltar, antes de virar `INPUT`.
///
/// Existe para separar a decisão (que sequência mandar) do envio (o Win32), que
/// é a única parte que não dá para testar sem um teclado e um foco de verdade.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Tecla {
    vk: u16,
    soltar: bool,
    /// Uma unidade UTF-16 a digitar, em vez de uma tecla a apertar. Com ela o
    /// `vk` vai zero e o Windows entrega o caractere direto (ver `digitar`).
    caractere: Option<u16>,
}

impl Tecla {
    fn aperta(vk: u16) -> Self {
        Self {
            vk,
            soltar: false,
            caractere: None,
        }
    }
    fn solta(vk: u16) -> Self {
        Self {
            vk,
            soltar: true,
            caractere: None,
        }
    }
    fn caractere(unidade: u16, soltar: bool) -> Self {
        Self {
            vk: 0,
            soltar,
            caractere: Some(unidade),
        }
    }
}

/// Uma combinação a enviar: os modificadores e a tecla principal.
///
/// Os modificadores são os virtual-keys **genéricos** (`VK_CONTROL`, `VK_SHIFT`,
/// `VK_MENU`) e não os de lado. É de propósito: quando o Ditador precisa apertar
/// um modificador ele não tem por que escolher um lado, e quando a pessoa já está
/// segurando um, o lado é dela — o que importa é saber que aquele modificador
/// está coberto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Acorde {
    modificadores: &'static [u16],
    tecla: u16,
}

fn acorde_de_colagem(metodo: MetodoDeColagem) -> Option<Acorde> {
    Some(match metodo {
        MetodoDeColagem::CtrlV => Acorde {
            modificadores: &[VK_CONTROL],
            tecla: VK_V,
        },
        MetodoDeColagem::ShiftInsert => Acorde {
            modificadores: &[VK_SHIFT],
            tecla: VK_INSERT,
        },
        MetodoDeColagem::CtrlShiftV => Acorde {
            modificadores: &[VK_CONTROL, VK_SHIFT],
            tecla: VK_V,
        },
        MetodoDeColagem::Digitar => return None,
    })
}

fn acorde_de_envio(tecla: TeclaDeEnvio) -> Option<Acorde> {
    Some(match tecla {
        TeclaDeEnvio::Nenhuma => return None,
        TeclaDeEnvio::Enter => Acorde {
            modificadores: &[],
            tecla: VK_RETURN,
        },
        TeclaDeEnvio::CtrlEnter => Acorde {
            modificadores: &[VK_CONTROL],
            tecla: VK_RETURN,
        },
    })
}

/// O que a pessoa está segurando de verdade no instante da colagem.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Segurando {
    /// Os modificadores pressionados, cada um pelo virtual-key **do lado** —
    /// é ele que precisa ser solto e reapertado, porque soltar o genérico
    /// deixaria o outro lado preso.
    teclas: Vec<u16>,
}

/// Todos os modificadores que interessam, por lado.
const MODIFICADORES: [u16; 8] = [
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LMENU,
    VK_RMENU,
    VK_LWIN,
    VK_RWIN,
];

/// De um modificador de lado para o genérico correspondente.
///
/// O Win precisa de um cuidado que os outros não têm: não existe um "VK_WIN"
/// genérico, e ele nunca aparece num acorde nosso — então ele é sempre "não
/// coberto", que é o que faz o Ditador soltá-lo antes de colar. É o certo: a
/// tecla Win segurada junto com qualquer coisa abre um atalho do Windows.
fn generico(vk: u16) -> u16 {
    match vk {
        VK_LCONTROL | VK_RCONTROL => VK_CONTROL,
        VK_LSHIFT | VK_RSHIFT => VK_SHIFT,
        VK_LMENU | VK_RMENU => VK_MENU,
        outro => outro,
    }
}

impl Segurando {
    /// Os modificadores segurados que **não** fazem parte do acorde — os que
    /// precisam sair da frente e voltar depois.
    fn no_caminho_de(&self, acorde: &[u16]) -> Vec<u16> {
        self.teclas
            .iter()
            .copied()
            .filter(|&vk| !acorde.contains(&generico(vk)))
            .collect()
    }

    /// Este modificador do acorde já está sendo segurado pela pessoa?
    fn ja_tem(&self, modificador: u16) -> bool {
        self.teclas.iter().any(|&vk| generico(vk) == modificador)
    }
}

fn segurando_agora() -> Segurando {
    // O bit alto do `GetAsyncKeyState` é "está pressionada agora". O bit 0, que
    // este código ignora, é "foi pressionada desde a última pergunta" — e
    // perguntar já o zera, o que faria esta função tirar a resposta de quem
    // perguntasse depois.
    fn pressionada(vk: u16) -> bool {
        (unsafe { GetAsyncKeyState(vk as i32) } as u16) & 0x8000 != 0
    }

    Segurando {
        teclas: MODIFICADORES
            .into_iter()
            .filter(|&vk| pressionada(vk))
            .collect(),
    }
}

/// Da situação do teclado para a sequência de teclas a enviar.
///
/// A regra é deixar o teclado como estava: tudo que é solto aqui é reapertado no
/// fim, na ordem inversa, e o que já estava apertado e serve — um Ctrl quando o
/// acorde é Ctrl+V — não é tocado. Um `SendInput` que solta uma tecla que a
/// pessoa ainda segura deixa o sistema em desacordo com a mão dela até a próxima
/// batida, e o sintoma disso é um Shift que "parou de funcionar".
fn montar_sequencia(agora: &Segurando, acorde: Acorde) -> Vec<Tecla> {
    let atrapalham = agora.no_caminho_de(acorde.modificadores);
    let faltam: Vec<u16> = acorde
        .modificadores
        .iter()
        .copied()
        .filter(|&m| !agora.ja_tem(m))
        .collect();

    let mut fila = Vec::with_capacity(atrapalham.len() * 2 + faltam.len() * 2 + 2);

    for &vk in &atrapalham {
        fila.push(Tecla::solta(vk));
    }
    for &vk in &faltam {
        fila.push(Tecla::aperta(vk));
    }
    fila.push(Tecla::aperta(acorde.tecla));
    fila.push(Tecla::solta(acorde.tecla));
    for &vk in faltam.iter().rev() {
        fila.push(Tecla::solta(vk));
    }
    for &vk in atrapalham.iter().rev() {
        fila.push(Tecla::aperta(vk));
    }

    fila
}

/// As teclas cujo scan code carrega o prefixo `E0`.
///
/// Sem a bandeira, o `SendInput` deriva o scan code do virtual-key pelo
/// `MapVirtualKey`, que devolve o do lado esquerdo: soltar o Alt **direito** iria
/// soltar o esquerdo, e o direito ficaria preso.
fn estendida(vk: u16) -> bool {
    matches!(vk, VK_RCONTROL | VK_RMENU | VK_LWIN | VK_RWIN)
}

/// As bandeiras do `KEYBDINPUT` para uma tecla nossa.
fn bandeiras(tecla: &Tecla) -> u32 {
    let mut bandeiras = 0;
    if tecla.soltar {
        bandeiras |= KEYEVENTF_KEYUP;
    }
    if tecla.caractere.is_some() {
        // Com `UNICODE` o `wVk` precisa ser zero e o `wScan` carrega o
        // caractere; a bandeira de estendida não se aplica e o `MapVirtualKeyW`
        // não é consultado.
        return bandeiras | KEYEVENTF_UNICODE;
    }
    if estendida(tecla.vk) {
        bandeiras |= KEYEVENTF_EXTENDEDKEY;
    }
    bandeiras
}

/// Manda a sequência inteira numa chamada só.
///
/// Uma chamada e não várias porque o `SendInput` garante que os eventos de uma
/// mesma chamada não sejam intercalados com os de outro programa — e uma tecla de
/// outra pessoa entrando entre o nosso Ctrl e o nosso V é exatamente o tipo de
/// corrida que não se quer aqui.
fn enviar(fila: &[Tecla]) -> Result<()> {
    let eventos: Vec<INPUT> = fila
        .iter()
        .map(|tecla| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: tecla.vk,
                    // O scan code vai preenchido junto com o virtual-key porque há
                    // programas (jogos, terminais, emuladores) que leem o scan e
                    // ignoram o resto. Não custa nada e alcança mais gente. Num
                    // caractere Unicode ele é o próprio caractere, e é o `wVk`
                    // que vai zero.
                    wScan: match tecla.caractere {
                        Some(unidade) => unidade,
                        None => unsafe { MapVirtualKeyW(tecla.vk as u32, MAPVK_VK_TO_VSC) as u16 },
                    },
                    dwFlags: bandeiras(tecla),
                    time: 0,
                    dwExtraInfo: ASSINATURA,
                },
            },
        })
        .collect();

    let enviados = unsafe {
        SendInput(
            eventos.len() as u32,
            eventos.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };

    if enviados as usize == eventos.len() {
        return Ok(());
    }

    // Zero eventos aceitos é o que o UIPI faz quando bloqueia — e é o caminho que
    // sobra quando o `foco_inalcancavel` não viu o bloqueio vindo, o que acontece
    // se o foco mudar entre a pergunta e o envio.
    if enviados == 0 {
        return Err(anyhow!(
            "o Windows recusou as teclas ({}). Se a janela em foco roda como \
             administrador, é isso: o texto está na área de transferência, cole \
             com Ctrl+V",
            std::io::Error::last_os_error()
        ));
    }
    Err(anyhow!(
        "SendInput aceitou {enviados} de {} teclas: {}",
        eventos.len(),
        std::io::Error::last_os_error()
    ))
}

/// Se a janela em foco está fora do alcance de teclas sintéticas.
///
/// Não existe uma pergunta direta para isso no Win32. O que existe é a
/// consequência: um processo de nível de integridade mais alto recusa até o
/// `PROCESS_QUERY_LIMITED_INFORMATION`, que é o acesso mais fraco que há, e
/// devolve `ERROR_ACCESS_DENIED`. Quem recusa isso também recusa nossas teclas.
///
/// Erra para o lado de tentar: qualquer outra falha (janela sem processo, foco
/// em nada, um erro que não seja acesso negado) devolve `false` e o `SendInput`
/// acontece. Uma colagem que não sai por engano é pior do que uma tentativa que o
/// próprio Windows barra e que o `enviar` sabe explicar.
fn foco_inalcancavel() -> bool {
    /// `ERROR_ACCESS_DENIED`, o único código que interessa aqui.
    const ACESSO_NEGADO: i32 = 5;

    unsafe {
        let janela = GetForegroundWindow();
        if janela.is_null() {
            return false;
        }

        let mut processo_id = 0u32;
        GetWindowThreadProcessId(janela, &mut processo_id);
        if processo_id == 0 {
            return false;
        }

        let processo = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, processo_id);
        if processo.is_null() {
            return std::io::Error::last_os_error().raw_os_error() == Some(ACESSO_NEGADO);
        }
        CloseHandle(processo);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apertar(vk: u16) -> Tecla {
        Tecla::aperta(vk)
    }
    fn soltar(vk: u16) -> Tecla {
        Tecla::solta(vk)
    }
    fn segurando(teclas: &[u16]) -> Segurando {
        Segurando {
            teclas: teclas.to_vec(),
        }
    }
    fn ctrl_v() -> Acorde {
        acorde_de_colagem(MetodoDeColagem::CtrlV).expect("Ctrl+V é um acorde")
    }

    #[test]
    fn com_o_teclado_em_repouso_a_sequencia_e_so_o_ctrl_v() {
        assert_eq!(
            montar_sequencia(&Segurando::default(), ctrl_v()),
            vec![
                apertar(VK_CONTROL),
                apertar(VK_V),
                soltar(VK_V),
                soltar(VK_CONTROL)
            ]
        );
    }

    #[test]
    fn um_ctrl_ja_segurado_e_aproveitado_em_vez_de_reapertado() {
        // Apertar por cima e soltar depois deixaria o sistema achando que a
        // pessoa soltou o Ctrl que ela ainda segura — e o próximo clique dela
        // sairia sem Ctrl nenhum. Vale para os dois lados da tecla.
        for lado in [VK_LCONTROL, VK_RCONTROL] {
            assert_eq!(
                montar_sequencia(&segurando(&[lado]), ctrl_v()),
                vec![apertar(VK_V), soltar(VK_V)],
                "o Ctrl do lado {lado:#x} não foi aproveitado"
            );
        }
    }

    #[test]
    fn o_shift_segurado_sai_da_frente_e_volta_depois() {
        // Sem isto o Ctrl+V viraria Ctrl+Shift+V, que é "colar sem formatação"
        // em metade dos programas e nada na outra metade.
        assert_eq!(
            montar_sequencia(&segurando(&[VK_LSHIFT]), ctrl_v()),
            vec![
                soltar(VK_LSHIFT),
                apertar(VK_CONTROL),
                apertar(VK_V),
                soltar(VK_V),
                soltar(VK_CONTROL),
                apertar(VK_LSHIFT),
            ]
        );
    }

    #[test]
    fn o_teclado_termina_como_comecou() {
        // A propriedade que interessa, com qualquer combinação segurada e
        // qualquer acorde: toda tecla solta pela colagem é reapertada, tudo que
        // apertamos é solto, e nada sobra diferente do que estava.
        let combinacoes: [&[u16]; 5] = [
            &[],
            &[VK_LSHIFT],
            &[VK_LSHIFT, VK_RMENU, VK_LWIN],
            &[VK_LCONTROL, VK_RSHIFT],
            &[VK_RWIN, VK_LMENU, VK_RCONTROL, VK_LSHIFT],
        ];
        let acordes = [
            acorde_de_colagem(MetodoDeColagem::CtrlV),
            acorde_de_colagem(MetodoDeColagem::ShiftInsert),
            acorde_de_colagem(MetodoDeColagem::CtrlShiftV),
            acorde_de_envio(TeclaDeEnvio::Enter),
            acorde_de_envio(TeclaDeEnvio::CtrlEnter),
        ];

        for teclas in combinacoes {
            for acorde in acordes.into_iter().flatten() {
                let agora = segurando(teclas);
                let fila = montar_sequencia(&agora, acorde);

                // Cada modificador segurado termina apertado — ou porque nunca
                // foi tocado, ou porque foi solto e devolvido.
                for &vk in teclas {
                    let soltas = fila.iter().filter(|t| t.vk == vk && t.soltar).count();
                    let apertadas = fila.iter().filter(|t| t.vk == vk && !t.soltar).count();
                    assert_eq!(
                        soltas, apertadas,
                        "a tecla {vk:#x} não voltou como estava com {acorde:?}"
                    );
                }
                // E cada modificador que **nós** apertamos é solto.
                for &m in acorde.modificadores {
                    if !agora.ja_tem(m) {
                        let soltas = fila.iter().filter(|t| t.vk == m && t.soltar).count();
                        let apertadas = fila.iter().filter(|t| t.vk == m && !t.soltar).count();
                        assert_eq!(
                            (apertadas, soltas),
                            (1, 1),
                            "o modificador {m:#x} de {acorde:?} ficou preso"
                        );
                    }
                }
                // A tecla principal é apertada e solta, exatamente uma vez.
                let principal: Vec<bool> = fila
                    .iter()
                    .filter(|t| t.vk == acorde.tecla && t.caractere.is_none())
                    .map(|t| t.soltar)
                    .collect();
                assert_eq!(principal, vec![false, true], "{acorde:?}");
            }
        }
    }

    #[test]
    fn a_volta_e_na_ordem_inversa_da_ida() {
        // É como um teclado de verdade se comporta: quem foi solto por último
        // volta primeiro.
        let fila = montar_sequencia(&segurando(&[VK_LSHIFT, VK_RMENU, VK_LWIN]), ctrl_v());
        let volta: Vec<u16> = fila
            .iter()
            .skip_while(|t| t.vk != VK_CONTROL || !t.soltar)
            .skip(1)
            .map(|t| t.vk)
            .collect();
        assert_eq!(volta, vec![VK_LWIN, VK_RMENU, VK_LSHIFT]);
    }

    #[test]
    fn o_shift_segurado_e_aproveitado_pelo_shift_insert() {
        // O mesmo Shift que atrapalha o Ctrl+V é o que o Shift+Insert quer.
        // Antes de os métodos existirem, "atrapalha" era uma lista fixa — e com
        // ela este acorde soltaria justamente o modificador de que precisa.
        let acorde = acorde_de_colagem(MetodoDeColagem::ShiftInsert).expect("acorde");
        assert_eq!(
            montar_sequencia(&segurando(&[VK_RSHIFT]), acorde),
            vec![apertar(VK_INSERT), soltar(VK_INSERT)]
        );
    }

    #[test]
    fn o_ctrl_shift_v_aproveita_os_dois_modificadores() {
        let acorde = acorde_de_colagem(MetodoDeColagem::CtrlShiftV).expect("acorde");
        assert_eq!(
            montar_sequencia(&segurando(&[VK_LCONTROL, VK_LSHIFT]), acorde),
            vec![apertar(VK_V), soltar(VK_V)]
        );
        // E aperta só o que falta.
        assert_eq!(
            montar_sequencia(&segurando(&[VK_LCONTROL]), acorde),
            vec![
                apertar(VK_SHIFT),
                apertar(VK_V),
                soltar(VK_V),
                soltar(VK_SHIFT)
            ]
        );
    }

    #[test]
    fn a_tecla_win_nunca_e_aproveitada() {
        // Não existe acorde nosso com Win, e um Win segurado junto com qualquer
        // coisa abre um atalho do Windows. Ele sempre sai da frente.
        for win in [VK_LWIN, VK_RWIN] {
            let fila = montar_sequencia(&segurando(&[win]), ctrl_v());
            assert_eq!(fila[0], soltar(win), "o {win:#x} não saiu da frente");
        }
    }

    #[test]
    fn o_enter_sozinho_nao_aperta_modificador_nenhum() {
        let acorde = acorde_de_envio(TeclaDeEnvio::Enter).expect("acorde");
        assert_eq!(
            montar_sequencia(&Segurando::default(), acorde),
            vec![apertar(VK_RETURN), soltar(VK_RETURN)]
        );
        // Mas um Ctrl segurado transformaria o Enter em Ctrl+Enter, que envia
        // noutros programas — então ele sai da frente.
        let fila = montar_sequencia(&segurando(&[VK_LCONTROL]), acorde);
        assert_eq!(fila.first(), Some(&soltar(VK_LCONTROL)));
        assert_eq!(fila.last(), Some(&apertar(VK_LCONTROL)));
    }

    #[test]
    fn digitar_manda_o_caractere_e_nao_a_tecla() {
        // O que faz este caminho não depender de layout: com `UNICODE` o `wVk`
        // vai zero e o `wScan` carrega o caractere. Um "ç" sai "ç" num teclado
        // americano.
        let tecla = Tecla::caractere('ç' as u16, false);
        assert_eq!(tecla.vk, 0);
        assert_eq!(tecla.caractere, Some(0xE7));
        assert_eq!(bandeiras(&tecla) & KEYEVENTF_UNICODE, KEYEVENTF_UNICODE);
        assert_eq!(bandeiras(&tecla) & KEYEVENTF_KEYUP, 0);
        assert_eq!(
            bandeiras(&Tecla::caractere('ç' as u16, true)) & KEYEVENTF_KEYUP,
            KEYEVENTF_KEYUP
        );
        // E a bandeira de estendida nunca entra num caractere: ela não se aplica
        // e o `wScan` já está ocupado.
        assert_eq!(bandeiras(&tecla) & KEYEVENTF_EXTENDEDKEY, 0);
    }

    #[test]
    fn o_generico_de_cada_lado_e_o_que_se_espera() {
        assert_eq!(generico(VK_LCONTROL), VK_CONTROL);
        assert_eq!(generico(VK_RCONTROL), VK_CONTROL);
        assert_eq!(generico(VK_LSHIFT), VK_SHIFT);
        assert_eq!(generico(VK_RSHIFT), VK_SHIFT);
        assert_eq!(generico(VK_LMENU), VK_MENU);
        assert_eq!(generico(VK_RMENU), VK_MENU);
        // O Win não tem genérico, e é isso que o mantém sempre "não coberto".
        assert_eq!(generico(VK_LWIN), VK_LWIN);
        assert_eq!(generico(VK_RWIN), VK_RWIN);
    }

    /// O `SendInput` de verdade, com o Windows de verdade do outro lado.
    ///
    /// Fica `ignore` porque ele **digita**: as teclas saem para a janela que
    /// estiver em foco na hora, como qualquer Ctrl+V. Não é o tipo de coisa para
    /// acontecer no meio de um `cargo test` de rotina, e é o mesmo motivo pelo
    /// qual não há aqui um teste que confira o texto colado — isso exigiria
    /// abrir uma janela, dar foco a ela e ler o que chegou, que é uma máquina de
    /// testes de interface e não um teste de unidade.
    ///
    /// O que ele responde é a única pergunta que os outros não podem responder:
    /// o que montamos serve para o Windows? Uma bandeira errada, um `cbSize`
    /// errado ou um `wVk` fora da faixa fazem o `SendInput` recusar tudo e
    /// devolver zero. Passa pelo `colar` inteiro de propósito — é ele que a
    /// aplicação chama, e com ele vêm o `segurando_agora` e o `foco_inalcancavel`.
    ///
    ///     cargo test --no-default-features --features cpu -- --ignored o_windows_aceita
    #[test]
    #[ignore = "digita Ctrl+V na janela em foco"]
    fn o_windows_aceita_as_teclas_que_montamos() {
        colar(MetodoDeColagem::CtrlV, "").expect("o Windows recusou a colagem");
    }

    /// O mesmo, para o caminho que digita. Ele usa uma bandeira diferente
    /// (`KEYEVENTF_UNICODE`) e um `wVk` zerado, que é justamente a combinação
    /// que o `SendInput` recusa quando está errada.
    ///
    ///     cargo test --no-default-features --features cpu -- --ignored o_windows_aceita_o_texto
    #[test]
    #[ignore = "digita texto na janela em foco"]
    fn o_windows_aceita_o_texto_que_digitamos() {
        digitar("ação").expect("o Windows recusou a digitação");
    }

    #[test]
    fn as_teclas_da_direita_levam_a_bandeira_de_estendida() {
        // Sem ela, soltar o Alt direito solta o esquerdo — e o direito fica
        // preso, que é um teclado quebrado até a pessoa apertar e soltar de novo.
        assert!(estendida(VK_RMENU));
        assert!(estendida(VK_RCONTROL));
        assert!(estendida(VK_LWIN));
        assert!(estendida(VK_RWIN));

        assert!(!estendida(VK_LSHIFT));
        assert!(!estendida(VK_RSHIFT));
        assert!(!estendida(VK_LMENU));
        assert!(!estendida(VK_LCONTROL));
        assert!(!estendida(VK_V));
    }
}
