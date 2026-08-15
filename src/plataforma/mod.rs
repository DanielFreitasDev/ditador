//! O que muda de sistema operacional para sistema operacional, num lugar só.
//!
//! Tudo o que está fora desta pasta é domínio: a máquina de estados do ditado, o
//! Whisper, a interface, a configuração. Nada disso sabe em que sistema está
//! rodando, e é assim que se pretende manter — o Ditador tem hoje três frentes
//! de área de trabalho (GNOME, Plasma, Windows) e uma quarta cópia das regras de
//! negócio seria a maneira mais rápida de elas passarem a discordar entre si.
//!
//! ## O contrato
//!
//! Cada plataforma precisa oferecer estes nove módulos, com estes nomes. Não é
//! um `trait` porque nada aqui é escolhido em tempo de execução: a plataforma é
//! decidida na compilação, e um `trait` só acrescentaria despacho dinâmico e
//! objetos vazios para representar uma escolha que já foi feita. O compilador
//! cobra o contrato do mesmo jeito — falta um módulo, falta um símbolo, e o
//! `cargo build` da plataforma reclama por nome.
//!
//! | Módulo | Linux | Windows |
//! |---|---|---|
//! | `teclado` | evdev (`/dev/input/event*`) | Raw Input (`WM_INPUT`) |
//! | `teclas` | tabela do evdev | tabela `VK_*` → código canônico |
//! | `ipc` | socket Unix em `$XDG_RUNTIME_DIR` | named pipe com DACL do usuário |
//! | `autostart` | serviço do systemd ou `.desktop` do XDG | `HKCU\…\Run` |
//! | `tray` | StatusNotifierItem (ksni) | quem mostra o ícone é o frontend |
//! | `integracoes` | nomes no barramento D-Bus | presença do frontend no pipe |
//! | `clipboard` | `wl-copy` / `ydotool` | `arboard` / `SendInput` |
//! | `registro` | o journal do systemd | arquivo em `LocalAppData` |
//! | `microfone` | nada a explicar | a recusa por privacidade do Windows |
//!
//! Os nomes vieram dos módulos que já existiam no topo de `src/`, e ficaram como
//! estavam de propósito: mover um arquivo é necessário para o Windows compilar,
//! renomeá-lo não é, e um diff que faz as duas coisas ao mesmo tempo é bem mais
//! difícil de revisar do que dois. Arquivo novo daqui para frente nasce em
//! português, como manda o `CLAUDE.md`.
//!
//! ## Por que não emular
//!
//! Nenhuma linha aqui tenta fazer o Windows parecer Linux. Não há D-Bus
//! instalado à força, nem WSL, nem XWayland, nem camada de compatibilidade de
//! evdev. Um socket Unix e um named pipe são coisas diferentes com o mesmo
//! propósito, e é o propósito que atravessa a fronteira — não a API. Quem
//! insiste no contrário acaba com dois sistemas mal servidos em vez de dois bem
//! servidos.
//!
//! ## A numeração canônica das teclas
//!
//! Um detalhe que vale explicar porque não é óbvio: o código de tecla que
//! circula pelo programa inteiro é o **do evdev**, inclusive no Windows.
//!
//! Não é preguiça nem dívida. O arquivo de configuração de todo mundo que já usa
//! o Ditador guarda o atalho como `["KEY_PAUSE"]`, e o `CLAUDE.md` é explícito
//! sobre não renomear campo gravado. Escolher uma terceira numeração "neutra"
//! obrigaria a traduzir dos dois lados e ainda deixaria os arquivos antigos para
//! trás; escolher a do Windows quebraria o Linux. A do evdev já está no disco,
//! já é o formato que a extensão do GNOME e o widget do Plasma leem, e é
//! perfeitamente capaz de nomear a tecla Pause de um teclado de PC — que é
//! hardware, não sistema operacional.
//!
//! Então no Windows a tradução acontece **na borda**, em `teclas::do_windows`:
//! o Raw Input entrega um `VK_*` e um scan code, e o que entra no programa já é
//! o código canônico. Uma configuração escrita no Linux funciona no Windows e
//! vice-versa, o que é a coisa certa para quem usa as duas.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

// Fora dessas duas não há o que oferecer, e descobrir isso no meio de um erro de
// símbolo faltando custa muito mais caro do que ler esta frase.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!(
    "o Ditador tem suporte a Linux e Windows. Para acrescentar um sistema, \
     crie src/plataforma/<nome>/ com os sete módulos descritos no mod.rs \
     e registre-o aqui."
);
