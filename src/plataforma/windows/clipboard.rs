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

use anyhow::{Result, anyhow};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VK_CONTROL, VK_LCONTROL, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
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

/// Envia Ctrl+V para a janela em foco.
pub fn colar() -> Result<()> {
    if foco_inalcancavel() {
        return Err(anyhow!(
            "a janela em foco tem privilégio maior que o do Ditador e o Windows \
             não deixa teclas sintéticas chegarem nela. O texto está na área de \
             transferência: cole com Ctrl+V"
        ));
    }

    enviar(&montar_sequencia(&segurando_agora()))
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
}

/// O que a pessoa está segurando de verdade no instante da colagem.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Segurando {
    /// Shift, Alt e Win, que estragariam o Ctrl+V virando outro atalho.
    atrapalham: Vec<u16>,
    /// Um Ctrl já pressionado — o único modificador que ajuda.
    ctrl: bool,
}

/// Os modificadores que atrapalham, por lado. Ctrl fica de fora de propósito: ele
/// é o que se quer.
const ATRAPALHAM: [u16; 6] = [VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];

fn segurando_agora() -> Segurando {
    // O bit alto do `GetAsyncKeyState` é "está pressionada agora". O bit 0, que
    // este código ignora, é "foi pressionada desde a última pergunta" — e
    // perguntar já o zera, o que faria esta função tirar a resposta de quem
    // perguntasse depois.
    fn pressionada(vk: u16) -> bool {
        (unsafe { GetAsyncKeyState(vk as i32) } as u16) & 0x8000 != 0
    }

    Segurando {
        atrapalham: ATRAPALHAM
            .into_iter()
            .filter(|&vk| pressionada(vk))
            .collect(),
        ctrl: pressionada(VK_LCONTROL) || pressionada(VK_RCONTROL),
    }
}

/// Da situação do teclado para a sequência de teclas a enviar.
///
/// A regra é deixar o teclado como estava: tudo que é solto aqui é reapertado no
/// fim, na ordem inversa, e o que já estava apertado e serve — o Ctrl — não é
/// tocado. Um `SendInput` que solta uma tecla que a pessoa ainda segura deixa o
/// sistema em desacordo com a mão dela até a próxima batida, e o sintoma disso é
/// um Shift que "parou de funcionar".
fn montar_sequencia(agora: &Segurando) -> Vec<Tecla> {
    let mut fila = Vec::with_capacity(agora.atrapalham.len() * 2 + 4);

    for &vk in &agora.atrapalham {
        fila.push(Tecla { vk, soltar: true });
    }
    if !agora.ctrl {
        fila.push(Tecla {
            vk: VK_CONTROL,
            soltar: false,
        });
    }
    fila.push(Tecla {
        vk: VK_V,
        soltar: false,
    });
    fila.push(Tecla {
        vk: VK_V,
        soltar: true,
    });
    if !agora.ctrl {
        fila.push(Tecla {
            vk: VK_CONTROL,
            soltar: true,
        });
    }
    for &vk in agora.atrapalham.iter().rev() {
        fila.push(Tecla { vk, soltar: false });
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
                    // ignoram o resto. Não custa nada e alcança mais gente.
                    wScan: unsafe { MapVirtualKeyW(tecla.vk as u32, MAPVK_VK_TO_VSC) } as u16,
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
        Tecla { vk, soltar: false }
    }
    fn soltar(vk: u16) -> Tecla {
        Tecla { vk, soltar: true }
    }

    #[test]
    fn com_o_teclado_em_repouso_a_sequencia_e_so_o_ctrl_v() {
        assert_eq!(
            montar_sequencia(&Segurando::default()),
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
        // sairia sem Ctrl nenhum.
        let agora = Segurando {
            atrapalham: vec![],
            ctrl: true,
        };
        assert_eq!(montar_sequencia(&agora), vec![apertar(VK_V), soltar(VK_V)]);
    }

    #[test]
    fn o_shift_segurado_sai_da_frente_e_volta_depois() {
        // Sem isto o Ctrl+V viraria Ctrl+Shift+V, que é "colar sem formatação"
        // em metade dos programas e nada na outra metade.
        let agora = Segurando {
            atrapalham: vec![VK_LSHIFT],
            ctrl: false,
        };
        assert_eq!(
            montar_sequencia(&agora),
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
        // A propriedade que interessa, com qualquer combinação segurada: toda
        // tecla solta pela colagem é reapertada, e nada sobra apertado além do
        // que já estava.
        let agora = Segurando {
            atrapalham: vec![VK_LSHIFT, VK_RMENU, VK_LWIN],
            ctrl: false,
        };
        let fila = montar_sequencia(&agora);

        for &vk in &agora.atrapalham {
            let soltas = fila.iter().filter(|t| t.vk == vk && t.soltar).count();
            let apertadas = fila.iter().filter(|t| t.vk == vk && !t.soltar).count();
            assert_eq!((soltas, apertadas), (1, 1), "tecla {vk:#x}");
        }

        // E a volta é na ordem inversa da ida: quem foi solto por último volta
        // primeiro, que é como um teclado de verdade se comporta.
        let volta: Vec<u16> = fila
            .iter()
            .skip_while(|t| t.vk != VK_CONTROL || !t.soltar)
            .skip(1)
            .map(|t| t.vk)
            .collect();
        assert_eq!(volta, vec![VK_LWIN, VK_RMENU, VK_LSHIFT]);
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
        colar().expect("o Windows recusou a colagem");
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
