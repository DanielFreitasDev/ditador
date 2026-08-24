//! Área de transferência e colagem automática, do lado do Linux.
//!
//! No Wayland o caminho confiável é o `wl-copy`, que assume a posse do conteúdo
//! num processo próprio. O `arboard` (X11, via XWayland) fica como reserva, e é
//! ele que o `crate::clipboard` usa quando esta função aqui desiste.

use crate::config::{MetodoDeColagem, TeclaDeEnvio};
use anyhow::{Context, Result, anyhow};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
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

/// O `ydotoold` — o serviço por onde o `ydotool` tecla — está no ar?
///
/// Ter o `ydotool` instalado não basta e não é detalhe: sem o serviço ele tenta
/// abrir o `/dev/uinput` por conta própria, e esse arquivo é `root`-only em
/// qualquer instalação de fábrica. O que a pessoa vê então é a colagem morrendo
/// de SIGABRT com o texto já na área de transferência.
///
/// A resposta fica guardada junto com as dos programas do PATH, pelo mesmo
/// motivo delas: quem pergunta é o desenho da interface, que roda muitas vezes
/// por segundo. A memória é jogada fora a cada troca de tela, então quem subir o
/// serviço com o Ditador aberto não fica preso ao "não" de antes.
pub fn servico_no_ar() -> bool {
    // A chave não é "ydotoold" pelado porque ela divide a memória com os nomes
    // de programa do PATH: um `existe("ydotoold")` futuro receberia esta
    // resposta, que é sobre outra pergunta.
    crate::programas::lembrar("ydotoold:no-ar", conectar_no_servico)
}

/// A pergunta de verdade, sem a memória do `programas`.
///
/// É esta que a mensagem de erro usa: a resposta guardada pode ser de quando a
/// última tela foi aberta, e uma frase que diz "o serviço está no ar" para quem
/// acabou de vê-lo cair manda procurar o problema no lugar errado.
///
/// Conectar, e não perguntar se o arquivo existe: o `ydotoold` 0.1.8 cria o
/// socket **antes** de abrir o `/dev/uinput` e morre ali quando não tem
/// permissão, deixando o arquivo para trás. Um socket no disco não prova que há
/// alguém do outro lado; um `connect` que é aceito, sim.
fn conectar_no_servico() -> bool {
    sockets_possiveis().any(|caminho| UnixStream::connect(caminho).is_ok())
}

/// Os caminhos em que o socket do `ydotoold` pode estar.
///
/// São três porque o caminho mudou com as versões e não dá para perguntar ao
/// binário qual é a dele — `ydotool --version` responde "Unknown tool". O 0.1.8,
/// que é o que o Ubuntu 24.04 empacota, tem `/tmp/.ydotool_socket` **fixo no
/// código** e ignora a `YDOTOOL_SOCKET` (conferido com `strace`); do 1.0 em
/// diante a variável passou a valer e o padrão passou a ser o do
/// `$XDG_RUNTIME_DIR`. Tentar os três custa três `connect` que falham na hora.
fn sockets_possiveis() -> impl Iterator<Item = PathBuf> {
    const NOME: &str = ".ydotool_socket";
    [
        std::env::var_os("YDOTOOL_SOCKET").map(PathBuf::from),
        std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join(NOME)),
        Some(PathBuf::from("/tmp").join(NOME)),
    ]
    .into_iter()
    .flatten()
}

/// Uma tecla que este módulo sintetiza, nas duas línguas em que o `ydotool` já
/// pediu para ouvi-la.
///
/// O `codigo` é o do evdev — a numeração canônica que o `plataforma/mod.rs`
/// explica, a mesma que o atalho global usa. O `nome` é como o `ydotool` 0.1.x
/// chama a mesma tecla, e não dá para derivá-lo do outro: ali `ctrl` é o Ctrl
/// esquerdo (29), e o nome do evdev seria `leftctrl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Tecla {
    codigo: u16,
    nome: &'static str,
}

/// As teclas que este módulo sintetiza.
///
/// Escritas por nome porque `29:1 47:1 47:0 29:0` é ilegível e já esteve errado.
mod tecla {
    use super::Tecla;

    pub const CTRL: Tecla = Tecla {
        codigo: 29,
        nome: "ctrl",
    };
    pub const SHIFT: Tecla = Tecla {
        codigo: 42,
        nome: "shift",
    };
    pub const V: Tecla = Tecla {
        codigo: 47,
        nome: "v",
    };
    pub const ENTER: Tecla = Tecla {
        codigo: 28,
        nome: "enter",
    };
    pub const INSERT: Tecla = Tecla {
        codigo: 110,
        nome: "insert",
    };
}

/// Como esta versão do `ydotool` quer ouvir uma combinação de teclas.
///
/// **As duas sintaxes não se encontram**: o 1.0 reescreveu o `key` e a de antes
/// deixou de existir. Mandar a de hoje para o `ydotool` de ontem não dá erro
/// nenhum — dá texto errado, silenciosamente, e é o pior desfecho possível para
/// quem está olhando para a janela em que o texto deveria aparecer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dialeto {
    /// `ydotool key ctrl+v`: os nomes das teclas, unidos por `+`, num argumento
    /// só. É o 0.1.x — o que o Ubuntu 24.04 e o Debian 12 empacotam.
    Nomes,
    /// `ydotool key 29:1 47:1 47:0 29:0`: código do evdev e estado, um argumento
    /// por evento. Do 1.0 em diante, e lá é a única sintaxe aceita.
    Codigos,
}

/// Qual das duas sintaxes o `ydotool` desta máquina entende.
///
/// A pergunta é feita ao `ydotool help`, e não a um `--version` — ele não tem
/// um: `ydotool --version` responde `Unknown tool`. O que se procura na resposta
/// é a menção à `YDOTOOL_SOCKET`, porque a variável nasceu na **mesma** reescrita
/// do 1.0 que trocou a sintaxe do `key`; é o marco mais próximo do que se quer
/// saber. O `help` serve de sonda também por não abrir o backend: o
/// `ydotool key --help` do 1.0 falha quando o serviço está fora do ar, e o do
/// 0.1.8 aborta se ainda por cima não puder abrir o `/dev/uinput`.
///
/// Sem resposta nenhuma fica o dialeto de hoje — se o `ydotool` não responde ao
/// `help`, o problema não é a sintaxe.
fn dialeto() -> Dialeto {
    let moderno = crate::programas::lembrar("ydotool:dialeto", || {
        let Ok(saida) = crate::programas::sem_janela(&mut Command::new("ydotool"))
            .arg("help")
            .output()
        else {
            return true;
        };
        // O 0.1.8 imprime a ajuda no stdout e o 1.0 também, mas nenhum dos dois
        // promete isso — e a linha procurada é curta demais para se arriscar a
        // olhar só metade da saída.
        let ajuda = format!(
            "{}{}",
            String::from_utf8_lossy(&saida.stdout),
            String::from_utf8_lossy(&saida.stderr)
        );
        ajuda.contains("YDOTOOL_SOCKET")
    });
    if moderno {
        Dialeto::Codigos
    } else {
        Dialeto::Nomes
    }
}

/// A combinação que cada método de colagem produz.
///
/// `None` é o `Digitar`, que não é uma combinação de teclas e vai por outro
/// caminho.
fn combinacao(metodo: MetodoDeColagem) -> Option<(&'static [Tecla], Tecla)> {
    Some(match metodo {
        MetodoDeColagem::CtrlV => (&[tecla::CTRL], tecla::V),
        MetodoDeColagem::ShiftInsert => (&[tecla::SHIFT], tecla::INSERT),
        MetodoDeColagem::CtrlShiftV => (&[tecla::CTRL, tecla::SHIFT], tecla::V),
        MetodoDeColagem::Digitar => return None,
    })
}

fn combinacao_de_envio(envio: TeclaDeEnvio) -> Option<(&'static [Tecla], Tecla)> {
    Some(match envio {
        TeclaDeEnvio::Nenhuma => return None,
        TeclaDeEnvio::Enter => (&[], tecla::ENTER),
        TeclaDeEnvio::CtrlEnter => (&[tecla::CTRL], tecla::ENTER),
    })
}

/// Os argumentos do `ydotool key` para uma combinação, na sintaxe que esta
/// máquina entende.
///
/// Nos códigos: modificadores apertados na ordem, a tecla principal, e tudo
/// solto na ordem inversa — que é como um teclado de verdade se comporta e é o
/// que os programas de destino esperam ver. Nos nomes quem faz isso é o próprio
/// `ydotool`, e faz igual: medido no dispositivo virtual dele, `ctrl+shift+v`
/// emite `29:1 42:1 47:1 47:0 42:0 29:0`.
///
/// Separada da execução porque é a única parte testável: rodar o `ydotool` de
/// verdade exige o serviço dele de pé e digita na janela de quem estiver
/// mexendo na máquina.
fn argumentos_da_combinacao(
    dialeto: Dialeto,
    modificadores: &[Tecla],
    principal: Tecla,
) -> Vec<String> {
    match dialeto {
        Dialeto::Nomes => {
            let mut nomes: Vec<&str> = modificadores.iter().map(|t| t.nome).collect();
            nomes.push(principal.nome);
            vec![nomes.join("+")]
        }
        Dialeto::Codigos => {
            let mut args = Vec::with_capacity(modificadores.len() * 2 + 2);
            for m in modificadores {
                args.push(format!("{}:1", m.codigo));
            }
            args.push(format!("{}:1", principal.codigo));
            args.push(format!("{}:0", principal.codigo));
            for m in modificadores.iter().rev() {
                args.push(format!("{}:0", m.codigo));
            }
            args
        }
    }
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

fn teclar(modificadores: &[Tecla], principal: Tecla) -> Result<()> {
    rodar_ydotool(
        std::iter::once("key".to_string()).chain(argumentos_da_combinacao(
            dialeto(),
            modificadores,
            principal,
        )),
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
    // `output` e não `status`: o que o `ydotool` escreve no stderr antes de
    // abortar é a única coisa que distingue uma falha da outra. Com o `status`
    // o cano era criado e ninguém o lia — a queixa ia para o lixo, e sobrava um
    // número de sinal na tela de quem precisava saber o que instalar.
    let saida = Command::new("ydotool")
        .args(args)
        .stdout(Stdio::null())
        .output()
        .context("executando ydotool")?;

    if saida.status.success() {
        Ok(())
    } else {
        Err(anyhow!(diagnostico_da_falha(
            &saida.status,
            &String::from_utf8_lossy(&saida.stderr),
            conectar_no_servico(),
        )))
    }
}

/// A frase que explica uma falha do `ydotool`, com a causa na frente.
///
/// Separada da execução porque é a parte que dá para testar, e porque a frase
/// importa: ela aparece numa linha só, cortada no fim, ao lado de um texto que a
/// pessoa vai ter de colar com a mão. O código de saída sozinho não diz nada — o
/// 0.1.8 morre de SIGABRT tanto sem achar o serviço quanto sem conseguir abrir o
/// `/dev/uinput` —, então quem explica é o serviço estar no ar ou não, e a
/// queixa que o programa escreveu antes de abortar.
fn diagnostico_da_falha(status: &ExitStatus, stderr: &str, servico_no_ar: bool) -> String {
    let dito = match queixa_do_ydotool(stderr) {
        Some(queixa) => format!("{status}: {queixa}"),
        None => status.to_string(),
    };
    if servico_no_ar {
        format!("ydotool falhou ({dito})")
    } else {
        format!(
            "o serviço ydotoold não está no ar, e sem ele o ydotool não tecla ({dito}). \
             Instale com: sudo apt install ydotoold"
        )
    }
}

/// A última linha do stderr que diga alguma coisa a quem lê.
///
/// O `ydotool` é C++ e morre por exceção não capturada, o que imprime três
/// linhas: um aviso do próprio programa, o "terminate called after throwing an
/// instance of…" da libstdc++ e o `what():` com a mensagem de verdade. É essa
/// última que interessa, e é ela que vem sem o rótulo.
fn queixa_do_ydotool(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .find_map(|linha| match linha.split_once("what():") {
            Some((_, queixa)) => Some(queixa.trim().to_string()),
            None if linha.starts_with("terminate called") => None,
            None => Some(linha.to_string()),
        })
}

/// O que dizer na tela quando a colagem automática não está disponível.
///
/// Os dois pacotes, e não só o primeiro: no Debian e no Ubuntu o `ydotool` e o
/// serviço dele são pacotes separados, e instalar só o cliente produz uma
/// colagem que aborta no meio — que foi exatamente o que aconteceu com esta
/// frase escrita pela metade.
pub const COMO_HABILITAR_A_COLAGEM: &str =
    "Colagem automática requer o ydotool e o serviço dele: sudo apt install ydotool ydotoold";

/// O que falta para colar numa máquina que já tem o `ydotool` instalado.
///
/// Vale para a tela de configurações e para o `--diagnostico`: os dois precisam
/// dizer a mesma coisa, e nenhum dos dois pode dizer que está tudo pronto quando
/// o serviço não está no ar.
pub fn aviso_da_colagem() -> Option<&'static str> {
    // Só o fato, sem receita: quem lê isto no `--diagnostico` recebe a receita
    // logo abaixo, e quem lê nas configurações recebe o caminho para ela. Uma
    // frase que mandasse rodar o `--diagnostico` ficaria mandando o diagnóstico
    // rodar a si mesmo.
    (colagem_disponivel() && !servico_no_ar()).then_some(
        "O serviço ydotoold não está no ar; sem ele o ydotool tenta abrir o \
         /dev/uinput sozinho e a colagem falha.",
    )
}

/// A receita de deixar o `ydotoold` no ar, para o `--diagnostico` imprimir.
///
/// Vem escrita em texto cru, com a indentação do relatório dentro dela, porque
/// são comandos para copiar e colar: qualquer reformatação nossa estragaria a
/// regra do udev, que tem aspas dentro de aspas.
///
/// São quatro passos e não um porque o serviço precisa do `/dev/uinput`, que é
/// do `root`: ou ele roda como `root` — e aí o socket dele nasce fora do alcance
/// de quem vai colar — ou o dispositivo passa a ser do grupo `input`, que é o
/// mesmo grupo que o atalho global já exige nesta máquina. O segundo caminho é o
/// que esta receita ensina, por não deixar um serviço de sistema com poder de
/// digitar em qualquer janela de qualquer usuário.
///
/// A unidade do systemd é escrita à mão porque o pacote do Ubuntu não traz
/// nenhuma: o `ydotoold` de lá é um binário solto, sem serviço de sistema nem de
/// usuário, e sem opções de linha de comando — o caminho do socket é fixo.
pub const COMO_LIGAR_A_COLAGEM: &str = r#"
    sudo apt install ydotoold
    echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
      | sudo tee /etc/udev/rules.d/60-ditador-uinput.rules
    sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=uinput
    mkdir -p ~/.config/systemd/user && printf '[Unit]\nDescription=ydotoold\n[Service]\nExecStart=/usr/bin/ydotoold\nRestart=always\n[Install]\nWantedBy=default.target\n' \
      > ~/.config/systemd/user/ydotoold.service
    systemctl --user daemon-reload && systemctl --user enable --now ydotoold"#;

/// O que a colagem automática faz nesta plataforma, e o que ela custa.
///
/// Aparece nas configurações quando a chave é ligada e no `--diagnostico`. O
/// Windows tem a versão dele, com outras ressalvas — as duas plataformas colam,
/// mas por caminhos que falham de maneiras diferentes.
pub const SOBRE_A_COLAGEM: &str = "O Ctrl+V vai pelo ydotool, que precisa do serviço ydotoold no ar, e chega \
     na janela que estiver em foco quando a transcrição terminar.";

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
            argumentos_da_combinacao(Dialeto::Codigos, &[tecla::CTRL], tecla::V),
            vec!["29:1", "47:1", "47:0", "29:0"]
        );
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Codigos, &[tecla::CTRL, tecla::SHIFT], tecla::V),
            vec!["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
        );
        // Sem modificador nenhum — o Enter do envio automático.
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Codigos, &[], tecla::ENTER),
            vec!["28:1", "28:0"]
        );
    }

    #[test]
    fn no_ydotool_antigo_a_combinacao_vai_por_nome_num_argumento_so() {
        // O 0.1.x não entende código de tecla, e **não reclama** de receber um:
        // ele lê "29:1" como a tecla de nome "2" e digita um 2. O `ctrl+v` do
        // Ditador virava `2442` na janela de destino, com o `ydotool` saindo com
        // código zero e o programa achando que tinha colado.
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Nomes, &[tecla::CTRL], tecla::V),
            vec!["ctrl+v"]
        );
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Nomes, &[tecla::CTRL, tecla::SHIFT], tecla::V),
            vec!["ctrl+shift+v"]
        );
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Nomes, &[tecla::SHIFT], tecla::INSERT),
            vec!["shift+insert"]
        );
        // Sozinha, a tecla vai sem `+` nenhum — e é assim que o 0.1.8 a quer.
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Nomes, &[], tecla::ENTER),
            vec!["enter"]
        );
    }

    #[test]
    fn os_nomes_das_teclas_sao_os_que_o_ydotool_antigo_respondeu_na_maquina() {
        // Esta tabela não é derivável do código do evdev — no `ydotool` 0.1.8
        // `ctrl` é o Ctrl esquerdo, cujo nome no evdev é `leftctrl` —, então ela
        // é o registro do que foi medido: cada nome foi mandado ao `ydotool`
        // 0.1.8 e o dispositivo virtual dele foi lido para ver que código saiu.
        for (t, nome, codigo) in [
            (tecla::CTRL, "ctrl", 29),
            (tecla::SHIFT, "shift", 42),
            (tecla::V, "v", 47),
            (tecla::ENTER, "enter", 28),
            (tecla::INSERT, "insert", 110),
        ] {
            assert_eq!(t.nome, nome);
            assert_eq!(t.codigo, codigo);
        }
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
                argumentos_da_combinacao(Dialeto::Codigos, modificadores, principal),
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
        assert_eq!(crate::keys::parse("KEY_LEFTCTRL"), Some(tecla::CTRL.codigo));
        assert_eq!(
            crate::keys::parse("KEY_LEFTSHIFT"),
            Some(tecla::SHIFT.codigo)
        );
        assert_eq!(crate::keys::parse("KEY_V"), Some(tecla::V.codigo));
        assert_eq!(crate::keys::parse("KEY_ENTER"), Some(tecla::ENTER.codigo));
        assert_eq!(crate::keys::parse("KEY_INSERT"), Some(tecla::INSERT.codigo));
    }

    /// O status de saída de um processo morto por sinal, sem processo nenhum.
    fn morto_por_sinal(sinal: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt as _;
        ExitStatus::from_raw(sinal)
    }

    #[test]
    fn a_falha_diz_qual_pacote_falta_quando_o_servico_nao_esta_no_ar() {
        // O caso real, e o que motivou esta função: o Ubuntu empacota o
        // `ydotool` e o `ydotoold` separados, quem instala só o primeiro fica
        // com um cliente que aborta, e a mensagem antiga mandava conferir uma
        // unidade do systemd que não existe em distribuição nenhuma.
        let stderr = "ydotool: notice: ydotoold backend unavailable (may have latency+delay issues)\n\
             terminate called after throwing an instance of 'std::runtime_error'\n\
             \x20 what():  failed to open uinput device\n";
        let frase = diagnostico_da_falha(&morto_por_sinal(6), stderr, false);
        assert!(
            frase.starts_with("o serviço ydotoold não está no ar"),
            "{frase}"
        );
        assert!(frase.contains("failed to open uinput device"), "{frase}");
        assert!(frase.contains("apt install ydotoold"), "{frase}");
    }

    #[test]
    fn com_o_servico_no_ar_a_frase_e_a_queixa_do_proprio_ydotool() {
        // Aqui não há o que instalar, e dizer que falta um pacote mandaria
        // procurar o problema no lugar errado.
        let frase =
            diagnostico_da_falha(&morto_por_sinal(11), "ydotool: Unknown tool: xyz\n", true);
        assert!(frase.starts_with("ydotool falhou"), "{frase}");
        assert!(frase.contains("Unknown tool: xyz"), "{frase}");
        assert!(!frase.contains("apt install"), "{frase}");
    }

    #[test]
    fn a_queixa_pula_o_ruido_da_libstdcpp() {
        // O "terminate called…" é da biblioteca padrão do C++ e não diz nada a
        // quem lê; a linha seguinte é a mensagem de verdade, e ela vem com um
        // rótulo que também não interessa.
        assert_eq!(
            queixa_do_ydotool(
                "terminate called after throwing an instance of 'x'\n  what():  falhou feio\n"
            ),
            Some("falhou feio".to_string())
        );
        assert_eq!(
            queixa_do_ydotool("terminate called after throwing an instance of 'x'\n"),
            None
        );
        // Sem stderr nenhum não há queixa a inventar: sobra o código de saída.
        assert_eq!(queixa_do_ydotool("   \n\n"), None);
    }

    #[test]
    fn o_socket_do_servico_e_procurado_em_todos_os_caminhos_que_ja_valeram() {
        // A lista é a defesa contra a versão do `ydotool` da máquina: a 0.1.8
        // só olha o `/tmp`, e da 1.0 em diante o padrão mudou de casa. Perder um
        // deles faz o Ditador avisar que o serviço está fora do ar bem na frente
        // de quem o tem rodando.
        let caminhos: Vec<_> = sockets_possiveis().collect();
        assert!(
            caminhos
                .iter()
                .any(|c| c == std::path::Path::new("/tmp/.ydotool_socket")),
            "{caminhos:?}"
        );
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            assert!(
                caminhos.contains(&PathBuf::from(dir).join(".ydotool_socket")),
                "{caminhos:?}"
            );
        }
    }

    #[test]
    fn a_tecla_de_envio_nenhuma_nao_produz_combinacao() {
        assert!(combinacao_de_envio(TeclaDeEnvio::Nenhuma).is_none());
        let (mods, principal) = combinacao_de_envio(TeclaDeEnvio::CtrlEnter).expect("acorde");
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Codigos, mods, principal),
            vec!["29:1", "28:1", "28:0", "29:0"]
        );
        assert_eq!(
            argumentos_da_combinacao(Dialeto::Nomes, mods, principal),
            vec!["ctrl+enter"]
        );
    }
}
