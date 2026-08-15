//! Escuta global de teclado por Raw Input.
//!
//! O equivalente Windows do evdev: recebe *apertar* e *soltar* de verdade,
//! mesmo com outra janela em foco, sem interferir no teclado de ninguém.
//!
//! ## Por que Raw Input, e não as outras três opções
//!
//! * **`RegisterHotKey`** entrega `WM_HOTKEY` quando a combinação é acionada —
//!   um evento só, sem o "soltou". Serve para atalho de ativação e não serve
//!   para segurar-para-falar, que é a semântica inteira deste programa. Pode vir
//!   a existir aqui como um atalho *opcional* de alternar, mas não substitui o
//!   Pause.
//! * **`WH_KEYBOARD_LL`** funciona, mas é um gancho global: cada tecla que
//!   qualquer pessoa digita em qualquer aplicativo passa pelo nosso processo
//!   antes de chegar ao destino. A própria documentação da Microsoft recomenda
//!   Raw Input para monitoramento, e avisa que um gancho lento é **removido em
//!   silêncio** pelo sistema — falha que aparece como "o atalho parou de
//!   funcionar depois de um tempo" e que é quase impossível de diagnosticar.
//!   Fica como reserva, se o Raw Input não der conta em hardware real.
//! * **Ganchos de journaling** (`WH_JOURNALRECORD`) estão fora de questão: são
//!   legado com caminho de descontinuação.
//!
//! ## `RIDEV_INPUTSINK` e nada além
//!
//! `RIDEV_INPUTSINK` é o que faz a entrada chegar mesmo sem foco — é
//! exatamente o que se quer. O que **não** se usa aqui é `RIDEV_NOLEGACY`: ele
//! suprimiria as mensagens normais de teclado, ou seja, o Ditador passaria a
//! *comer* as teclas em vez de observá-las. A leitura é passiva, como a do
//! evdev: quem está digitando não percebe que existimos.
//!
//! ## A janela invisível, que não pode ser *message-only*
//!
//! Raw Input precisa de um `HWND` para receber `WM_INPUT`. A escolha óbvia seria
//! uma janela *message-only* — filha de `HWND_MESSAGE`, que existe só como caixa
//! postal. **Ela não funciona aqui**, e foi a primeira versão deste arquivo: o
//! registro passa, o `RegisterRawInputDevices` devolve sucesso, a janela nasce com
//! um handle válido, e nenhum `WM_INPUT` chega jamais. Nada reclama.
//!
//! O que se usa é uma janela comum de zero por zero pixels, nunca mostrada, com
//! `WS_EX_TOOLWINDOW` para ficar fora do Alt+Tab e da barra de tarefas — que era
//! tudo o que se queria da message-only. Ela mora numa thread própria com laço de
//! mensagens, porque a interface é do eframe e não podemos pendurar nada no laço
//! dele.
//!
//! Se um dia o atalho parar de funcionar sem explicação, o `log::trace!` de
//! `tratar_entrada` responde em uma rodada se as teclas estão chegando.

use crate::hotkey::{Acao, HotkeyEvent, HotkeyListener, Origem};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MAPVK_VSC_TO_VK_EX, MapVirtualKeyW, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW, MSG,
    RegisterClassExW, SetWindowLongPtrW, TranslateMessage, WM_INPUT, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

/// `GWLP_USERDATA`, onde guardamos o ponteiro para o ouvinte.
const GWLP_USERDATA: i32 = -21;

/// Página e uso HID do teclado, na tabela de *usage* do USB HID.
/// 0x01 = Generic Desktop, 0x06 = Keyboard.
const HID_GENERIC_DESKTOP: u16 = 0x01;
const HID_KEYBOARD: u16 = 0x06;

/// Sinalizadores do `RAWKEYBOARD::Flags`.
const RI_KEY_BREAK: u16 = 0x01; // soltou (sem ele, apertou)
const RI_KEY_E0: u16 = 0x02;
const RI_KEY_E1: u16 = 0x04;

/// O virtual-key que o Windows usa como enchimento nas sequências de prefixo.
///
/// A tecla Pause é a razão de esta constante existir: o scan code dela é a
/// sequência `E1 1D 45`, e o Raw Input a entrega em **duas** mensagens — a
/// primeira só com o prefixo `E1` e `VKey = 0xFF`, que não é tecla nenhuma. Sem
/// descartá-la, o Ditador via um evento a mais a cada aperto do próprio atalho
/// padrão.
const VK_ENCHIMENTO: u16 = 0xFF;

/// Sobe a escuta do teclado numa thread própria.
pub fn vigiar(listener: Arc<HotkeyListener>) {
    std::thread::Builder::new()
        .name("hotkey-rawinput".into())
        .spawn(move || {
            if let Err(motivo) = laco_de_mensagens(&listener) {
                log::warn!("Raw Input indisponível: {motivo}");
                listener.avisar(HotkeyEvent::Unavailable(format!(
                    "Não consegui observar o teclado ({motivo}). O atalho global \
                     não vai funcionar; use o ícone da barra para ditar."
                )));
            }
        })
        .expect("spawn hotkey-rawinput");
}

/// Quantos teclados dá para ler agora.
///
/// No Windows a pergunta não é sobre permissão — Raw Input não exige nenhuma —,
/// então a resposta é sobre o registro ter dado certo. `GetRawInputDeviceList`
/// contaria os teclados físicos, mas isso responderia outra pergunta: o que
/// interessa é se *nós* estamos recebendo, e quem sabe disso é o registro.
pub fn teclados_legiveis() -> usize {
    usize::from(registrado())
}

/// A linha do `ditador --diagnostico` sobre a leitura do teclado.
///
/// Repare que ela responde honestamente `false` quando rodada de um terminal: o
/// `--diagnostico` é outro processo, que não registrou Raw Input nenhum. É
/// intencional — naquele processo o Ditador de fato não está observando o
/// teclado, e o detalhe abaixo manda a pessoa perguntar a quem sabe.
pub fn diagnostico() -> (Option<bool>, &'static str, String) {
    (
        // `None` — informativo, e não reprovado. Rodar `--diagnostico` num
        // terminal cria um processo que nunca registrou Raw Input, e marcá-lo
        // como falha faria o comando dizer "há o que resolver" numa máquina onde
        // está tudo certo. É o oposto do que este comando existe para fazer.
        registrado().then_some(true),
        "Leitura do teclado (Raw Input)",
        if registrado() {
            "registrado; o atalho funciona com qualquer janela em foco.".to_string()
        } else {
            "quem observa o teclado é a instância em execução, não este comando. \
             Para saber se ela está observando: ditador --status."
                .to_string()
        },
    )
}

/// Como o `--diagnostico` diz que não há instância rodando.
pub const COMO_SUBIR_O_SERVICO: &str =
    "nenhuma. Para subir: abra o Ditador pelo menu Iniciar, ou rode ditador.exe";

/// Se o registro do Raw Input já aconteceu com sucesso nesta execução.
///
/// Um booleano global e não um campo porque quem pergunta é o `--diagnostico`,
/// que roda em outro processo... e portanto sempre lê `false`. Está certo assim:
/// naquele processo o Ditador de fato não está observando o teclado. A pergunta
/// "a instância que está rodando consegue?" é respondida pelo `--status`, que
/// atravessa o named pipe.
static REGISTRADO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn registrado() -> bool {
    REGISTRADO.load(std::sync::atomic::Ordering::Relaxed)
}

fn laco_de_mensagens(listener: &Arc<HotkeyListener>) -> Result<(), String> {
    let hwnd = criar_janela_postal(listener)?;
    registrar_teclado(hwnd)?;

    REGISTRADO.store(true, std::sync::atomic::Ordering::Relaxed);
    log::info!(
        "observando o teclado por Raw Input (janela {hwnd:?}, thread {:?})",
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
    );
    listener.avisar(HotkeyEvent::Available);

    // `GetMessageW` devolve 0 no `WM_QUIT` e -1 em erro. O laço só termina no
    // encerramento do programa, e nesse caminho o processo sai por `_exit` — a
    // saída limpa daqui existe para o caso de erro, que não deve acontecer.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r == 0 {
            return Ok(());
        }
        if r == -1 {
            return Err(format!("GetMessage: {}", std::io::Error::last_os_error()));
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn criar_janela_postal(listener: &Arc<HotkeyListener>) -> Result<HWND, String> {
    let classe = utf16("DitadorRawInput");

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(janela_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: classe.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    // Registrar duas vezes a mesma classe devolve erro, mas isto roda uma vez
    // só por processo; se um dia rodar duas, o `CreateWindowExW` abaixo é quem
    // vai reclamar de verdade.
    unsafe { RegisterClassExW(&wc) };

    // Uma janela comum, de zero por zero pixels e nunca mostrada — e **não**
    // uma janela *message-only* (filha de `HWND_MESSAGE`).
    //
    // A message-only é a escolha óbvia para quem só quer uma caixa postal, e foi
    // a primeira tentativa aqui. Ela não funciona: o registro do Raw Input passa,
    // o `RegisterRawInputDevices` devolve sucesso, a janela é criada com um
    // handle válido — e o `WM_INPUT` simplesmente nunca chega. Nada no caminho
    // reclama, o que torna a falha especialmente cara de diagnosticar: o log diz
    // "observando o teclado por Raw Input" e o teclado não é observado.
    //
    // O motivo é que uma janela message-only não participa da entrega de entrada
    // do sistema; ela recebe mensagens endereçadas a ela, e o `WM_INPUT` de um
    // `RIDEV_INPUTSINK` não é endereçado assim. A janela precisa ser uma
    // top-level de verdade.
    //
    // Sem `WS_VISIBLE` ela nunca aparece, e o `WS_EX_TOOLWINDOW` a mantém fora do
    // Alt+Tab e da barra de tarefas — que é tudo o que se queria da
    // message-only, obtido de outro jeito.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            classe.as_ptr(),
            utf16("Ditador").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(format!(
            "CreateWindowEx: {}",
            std::io::Error::last_os_error()
        ));
    }

    // O ouvinte precisa chegar até o `janela_proc`, que é uma função `extern
    // "system"` e não pode capturar nada. `GWLP_USERDATA` é o lugar que o Win32
    // oferece para isso. O `Arc` é vazado de propósito: a janela vive tanto
    // quanto o processo, e um `Weak` aqui só acrescentaria uma verificação que
    // nunca falharia.
    let ponteiro = Arc::into_raw(listener.clone());
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, ponteiro as isize) };

    Ok(hwnd)
}

fn registrar_teclado(hwnd: HWND) -> Result<(), String> {
    let dispositivo = RAWINPUTDEVICE {
        usUsagePage: HID_GENERIC_DESKTOP,
        usUsage: HID_KEYBOARD,
        // Só INPUTSINK: receber sem foco. Sem NOLEGACY — não queremos tirar a
        // tecla de ninguém, só olhar.
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };

    let ok = unsafe {
        RegisterRawInputDevices(
            &dispositivo,
            1,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )
    };
    if ok == 0 {
        return Err(format!(
            "RegisterRawInputDevices: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// A janela postal. Só nos interessa o `WM_INPUT`.
///
/// É um limite de FFI: um `panic!` daqui atravessaria a ABI do Windows, o que é
/// comportamento indefinido. Por isso todo o trabalho acontece dentro de um
/// `catch_unwind`. Um evento de tecla perdido é infinitamente melhor do que um
/// processo derrubado no meio de um ditado.
unsafe extern "system" fn janela_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT {
        let _ = std::panic::catch_unwind(|| unsafe {
            let ponteiro = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const HotkeyListener;
            if !ponteiro.is_null() {
                tratar_entrada(&*ponteiro, lparam as HRAWINPUT);
            }
        });
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Lê o `RAWINPUT` e entrega a tecla à máquina de estados.
///
/// Faz o mínimo e volta: decodifica, traduz e manda por um canal. Nada de
/// áudio, nada de disco, nada de Whisper — esta função roda dentro do laço de
/// mensagens do Windows, e segurá-la atrasa o teclado do sistema inteiro.
unsafe fn tratar_entrada(listener: &HotkeyListener, handle: HRAWINPUT) {
    unsafe {
        let mut bytes = 0u32;
        let cabecalho = std::mem::size_of::<RAWINPUTHEADER>() as u32;

        // Primeira chamada para descobrir o tamanho, como manda a API.
        if GetRawInputData(
            handle,
            RID_INPUT,
            std::ptr::null_mut(),
            &mut bytes,
            cabecalho,
        ) != 0
        {
            return;
        }
        if bytes == 0 || bytes as usize > std::mem::size_of::<RAWINPUT>() * 4 {
            return;
        }

        let mut buffer = vec![0u8; bytes as usize];
        let lidos = GetRawInputData(
            handle,
            RID_INPUT,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
            cabecalho,
        );
        if lidos != bytes {
            return;
        }

        let bruto = &*buffer.as_ptr().cast::<RAWINPUT>();
        // 1 = RIM_TYPEKEYBOARD. Registramos só teclado, mas conferir custa nada
        // e evita ler lixo se um dia o registro mudar.
        if bruto.header.dwType != 1 {
            return;
        }

        let teclado = &bruto.data.keyboard;

        // O rastro das teclas cruas, para quando o atalho "não faz nada".
        //
        // É a única maneira de separar as três causas possíveis, que de fora
        // parecem idênticas: a mensagem não chega, a mensagem chega e a tradução
        // a descarta, ou a tradução acerta e o problema está adiante. Nenhuma
        // linha aqui significa a primeira — e a causa mais provável dela está
        // escrita em `criar_janela_postal`.
        //
        // Fica em `trace`, que o filtro padrão não liga (veja `FILTRO_PADRAO` em
        // `main.rs`) e que nem `RUST_LOG=ditador=debug` alcança — é preciso pedir
        // `ditador=trace` de propósito. Isto aqui é o teclado inteiro de quem
        // está usando o computador, e não é coisa para ficar ligada.
        log::trace!(
            "raw: vkey={:#04x} scan={:#04x} flags={:#06b}",
            teclado.VKey,
            teclado.MakeCode,
            teclado.Flags
        );

        let Some((codigo, acao)) = traduzir(teclado.VKey, teclado.MakeCode, teclado.Flags) else {
            return;
        };

        // `hDevice` é o teclado que originou a tecla. É ele que distingue o
        // teclado físico de input sintético (`SendInput`), que é o mesmo problema
        // que o teclado virtual do `ydotool` cria no Linux — e a razão de a
        // máquina de teclas guardar quem segura o quê.
        listener.evento(codigo, acao, Origem(bruto.header.hDevice as u64));
    }
}

/// Do `RAWKEYBOARD` cru para o código canônico e a ação.
///
/// Separada de `tratar_entrada` porque é a parte que dá para testar sem um
/// teclado e sem o Windows entregando mensagens: são três números entrando e uma
/// decisão saindo.
fn traduzir(vkey: u16, scan: u16, flags: u16) -> Option<(u16, Acao)> {
    // O enchimento das sequências de prefixo não é tecla. Veja `VK_ENCHIMENTO`.
    if vkey == VK_ENCHIMENTO {
        return None;
    }

    let e0 = flags & RI_KEY_E0 != 0;
    let soltou = flags & RI_KEY_BREAK != 0;

    let vkey = desambiguar(vkey, scan, e0, flags);
    let codigo = super::teclas::do_windows(super::teclas::vk_estendido(vkey, e0))?;

    // O Raw Input **não** marca repetição automática: ele entrega um "apertou"
    // a cada repique. Quem os distingue é a própria máquina de teclas, que só
    // reage à transição — um segundo `Apertou` com a tecla já pressionada não
    // muda nada lá. Por isso aqui nunca sai `Acao::Repetiu`: seria uma
    // classificação que não temos como fazer e que não muda o resultado.
    Some((codigo, if soltou { Acao::Soltou } else { Acao::Apertou }))
}

/// Descobre de que lado veio o modificador.
///
/// O Raw Input entrega `VK_SHIFT`, `VK_CONTROL` e `VK_MENU` genéricos, sem dizer
/// se foi o da esquerda ou o da direita. Um atalho em "Ctrl esquerdo" que
/// disparasse no direito seria um bug silencioso e irritante, então a distinção
/// é feita aqui:
///
/// * **Shift** não tem prefixo E0 nos dois lados — o que os separa é o scan
///   code (0x2A e 0x36), e quem sabe traduzir isso é o
///   `MapVirtualKey(MAPVK_VSC_TO_VK_EX)`;
/// * **Ctrl e Alt** da direita chegam com E0; os da esquerda, sem.
fn desambiguar(vkey: u16, scan: u16, e0: bool, flags: u16) -> u16 {
    const VK_LSHIFT: u16 = 0xA0;
    const VK_LCONTROL: u16 = 0xA2;
    const VK_RCONTROL: u16 = 0xA3;
    const VK_LMENU: u16 = 0xA4;
    const VK_RMENU: u16 = 0xA5;

    match vkey {
        v if v == VK_SHIFT => {
            let lado = unsafe { MapVirtualKeyW(scan as u32, MAPVK_VSC_TO_VK_EX) } as u16;
            if lado == 0 { VK_LSHIFT } else { lado }
        }
        v if v == VK_CONTROL => {
            // A tecla Pause com Ctrl (o "Break") chega como Ctrl com E1, não E0.
            // Tratá-la como Ctrl direito seria inventar uma tecla que ninguém
            // apertou.
            if flags & RI_KEY_E1 != 0 {
                vkey
            } else if e0 {
                VK_RCONTROL
            } else {
                VK_LCONTROL
            }
        }
        v if v == VK_MENU => {
            if e0 {
                VK_RMENU
            } else {
                VK_LMENU
            }
        }
        outro => outro,
    }
}

fn utf16(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const APERTOU: u16 = 0x00;
    const SOLTOU: u16 = RI_KEY_BREAK;

    #[test]
    fn o_atalho_padrao_atravessa_inteiro() {
        // Pause apertado e solto, que é o ciclo de vida de todo ditado.
        // Scan code 0x45, que é o que sobra da sequência E1 1D 45.
        assert_eq!(traduzir(0x13, 0x45, APERTOU), Some((119, Acao::Apertou)));
        assert_eq!(traduzir(0x13, 0x45, SOLTOU), Some((119, Acao::Soltou)));
    }

    #[test]
    fn o_enchimento_do_prefixo_da_tecla_pause_e_descartado() {
        // A primeira das duas mensagens que o Pause gera não é tecla nenhuma.
        // Sem descartá-la, o Ditador via um evento a mais a cada aperto do
        // próprio atalho padrão — e a máquina de teclas contaria uma origem
        // fantasma segurando uma tecla que ninguém apertou.
        assert_eq!(traduzir(VK_ENCHIMENTO, 0x1D, RI_KEY_E1), None);
        assert_eq!(
            traduzir(VK_ENCHIMENTO, 0x1D, RI_KEY_E1 | RI_KEY_BREAK),
            None
        );
    }

    #[test]
    fn ctrl_e_alt_da_direita_nao_viram_os_da_esquerda() {
        // 29 = KEY_LEFTCTRL, 97 = KEY_RIGHTCTRL, 56 = KEY_LEFTALT,
        // 100 = KEY_RIGHTALT (o AltGr dos teclados ABNT).
        assert_eq!(
            traduzir(VK_CONTROL, 0x1D, APERTOU),
            Some((29, Acao::Apertou))
        );
        assert_eq!(
            traduzir(VK_CONTROL, 0x1D, RI_KEY_E0),
            Some((97, Acao::Apertou))
        );
        assert_eq!(traduzir(VK_MENU, 0x38, APERTOU), Some((56, Acao::Apertou)));
        assert_eq!(
            traduzir(VK_MENU, 0x38, RI_KEY_E0),
            Some((100, Acao::Apertou))
        );
    }

    #[test]
    fn o_ctrl_do_break_nao_vira_ctrl_direito() {
        // Ctrl+Pause manda um Ctrl com prefixo E1. Lido como E0, viraria "Ctrl
        // direito apertado" sem que ninguém tivesse encostado nele — e um
        // atalho em Ctrl direito dispararia sozinho.
        assert_eq!(desambiguar(VK_CONTROL, 0x1D, false, RI_KEY_E1), VK_CONTROL);
    }

    #[test]
    fn a_tecla_que_nao_conhecemos_e_ignorada_sem_barulho() {
        // Teclas de multimídia e afins chegam por Raw Input e não estão na
        // tabela. Devolver None é o certo: elas não podem virar atalho, e
        // inventar um código para elas encheria a máquina de teclas de fantasmas.
        assert_eq!(traduzir(0xAD, 0x20, APERTOU), None); // VK_VOLUME_MUTE
    }
}
