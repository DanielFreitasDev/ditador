//! Named pipe local: instância única e controle por linha de comando.
//!
//! O equivalente Windows do socket Unix que o Linux usa. Mesmo propósito, mesma
//! forma vista de fora (`bind`, `serve`, `send`, `cleanup`), API completamente
//! diferente por baixo — que é a razão de este arquivo existir em vez de uma
//! camada de compatibilidade fingindo que os dois são a mesma coisa.
//!
//! ## O nome do pipe carrega o SID
//!
//! `\\.\pipe\Ditador-<SID do usuário>`.
//!
//! Não é enfeite. O espaço de nomes de pipes é **global na máquina**: um pipe
//! chamado só `Ditador` seria um nome único para todos os usuários logados ao
//! mesmo tempo — e no Windows isso é rotina, com troca rápida de usuário e
//! sessões de área de trabalho remota. O segundo usuário a entrar não
//! conseguiria criar o dele, e ficaria sem Ditador sem entender por quê.
//!
//! Com o SID no nome, cada sessão tem o seu, que é o que o `CLAUDE.md` do
//! projeto já pedia do lado Linux ("cada login tem sua própria instância").
//!
//! ## A ACL é explícita, e isso não é excesso de zelo
//!
//! A documentação da Microsoft avisa que o descritor de segurança **padrão** de
//! um named pipe concede leitura a grupos amplos — em certos cenários, inclusive
//! a `Everyone` e a sessão anônima. Aceitá-lo em silêncio aqui significaria que
//! qualquer conta da máquina poderia mandar `quit` no Ditador de outra pessoa,
//! ou abrir o microfone dela com `toggle`. O nome com SID esconderia o pipe, mas
//! esconder não é proteger: o nome é enumerável.
//!
//! Então a DACL é escrita à mão, e é uma lista de permissão — quem não está
//! nela não entra. Só o usuário que criou o pipe. Sem SYSTEM e sem
//! Administradores: nenhum dos dois tem o que fazer com "começar a gravar", e a
//! regra do prompt (e do bom senso) é conceder o mínimo. Um administrador que
//! precise depurar continua podendo assumir a posse do objeto — o que é uma ação
//! deliberada e auditável, e não um acesso que já viesse aberto.
//!
//! ## Instância única de graça
//!
//! `FILE_FLAG_FIRST_PIPE_INSTANCE` faz o `CreateNamedPipeW` falhar se alguém já
//! tiver criado um pipe com esse nome. Isso é exatamente a pergunta "já existe
//! um Ditador rodando nesta sessão?", respondida pelo próprio sistema, sem mutex
//! separado e sem arquivo de trava que sobrevive a um travamento. O primeiro a
//! chegar vira o servidor; o segundo descobre no mesmo instante que perdeu e
//! vira cliente.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
    GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Quanto tempo esperamos por um cliente calado antes de desistir dele.
///
/// O atendimento de cada cliente acontece na thread dele, mas o prazo continua
/// valendo: uma conexão que nunca mandasse a linha prenderia uma instância do
/// pipe para sempre, e elas são um recurso finito.
const PACIENCIA: Duration = Duration::from_secs(2);

/// Teto do que aceitamos numa linha de comando.
///
/// O maior comando válido tem oito bytes. Sem teto, um cliente que mandasse
/// bytes sem nunca mandar `\n` fazia a `String` crescer até acabar a memória.
const LIMITE_DA_LINHA: u64 = 1024;

/// Quantas instâncias do pipe podem existir ao mesmo tempo.
///
/// Cada cliente conectado ocupa uma, e quem assina ocupa a dele **enquanto
/// estiver de pé** — o `Ditador.Windows` mantém uma conexão aberta o dia
/// inteiro. Oito deixa folga para o frontend, para um segundo assinante de
/// depuração e para os comandos de linha de comando, que duram milissegundos.
///
/// O limite existe para que um cliente em laço não consiga consumir handles do
/// sistema sem parar; ele é por nome de pipe, e o nome já é só do usuário.
const INSTANCIAS: u32 = 8;

const TAMANHO_DO_BUFFER: u32 = 4096;

/// Quanto o laço de atendimento espera quando não consegue mais criar instâncias.
///
/// Existe para que uma falha persistente do sistema não vire laço quente. Um
/// quinto de segundo é imperceptível para quem digita `ditador --status` e é uma
/// eternidade para um núcleo de CPU.
const RESPIRO: Duration = Duration::from_millis(200);

/// `\\.\pipe\Ditador-<SID>`, decidido uma vez só.
fn caminho_do_pipe() -> Option<&'static str> {
    static CAMINHO: OnceLock<Option<String>> = OnceLock::new();
    CAMINHO
        .get_or_init(|| {
            let sid = sid_do_usuario()?;
            // Nos testes o nome leva o número do processo. Sem isso, rodar
            // `cargo test` com o Ditador de pé faria o teste disputar o pipe da
            // instância de verdade — o `bind` falharia com "já rodando" e, pior,
            // o teste passaria a mandar comandos para o programa que a pessoa
            // está usando. A forma de produção continua sendo conferida, e por
            // `montar_caminho`, logo abaixo.
            if cfg!(test) {
                Some(format!(
                    r"\\.\pipe\Ditador-teste-{}-{sid}",
                    std::process::id()
                ))
            } else {
                Some(montar_caminho(&sid))
            }
        })
        .as_deref()
}

/// O nome do pipe como ele é em produção.
fn montar_caminho(sid: &str) -> String {
    format!(r"\\.\pipe\Ditador-{sid}")
}

/// O SID textual do usuário deste processo, ex.: `S-1-5-21-…-1001`.
fn sid_do_usuario() -> Option<String> {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            log::warn!("não consegui abrir o token do processo: {}", ultimo_erro());
            return None;
        }
        let _fecha_token = AoSair(|| {
            CloseHandle(token);
        });

        // Duas chamadas: a primeira só para descobrir o tamanho. O
        // `GetTokenInformation` sempre falha nessa primeira, com
        // ERROR_INSUFFICIENT_BUFFER, e é assim que ele foi feito para ser usado.
        let mut tamanho = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut tamanho);
        if tamanho == 0 {
            log::warn!("token sem tamanho de TokenUser: {}", ultimo_erro());
            return None;
        }

        let mut buffer = vec![0u8; tamanho as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            tamanho,
            &mut tamanho,
        ) == 0
        {
            log::warn!("não consegui ler o TokenUser: {}", ultimo_erro());
            return None;
        }

        let info = buffer.as_ptr().cast::<TOKEN_USER>();
        let mut texto: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW((*info).User.Sid, &mut texto) == 0 {
            log::warn!("não consegui converter o SID: {}", ultimo_erro());
            return None;
        }
        let _libera = AoSair(|| {
            LocalFree(texto.cast());
        });

        Some(de_utf16(texto))
    }
}

/// O descritor de segurança do pipe, em SDDL.
///
/// * `D:` — o que vem a seguir é a DACL.
/// * `P` — *protected*: nada é herdado do contêiner. Sem isto, permissões do
///   objeto pai poderiam acrescentar acesso que não escolhemos.
/// * `(A;;GA;;;<SID>)` — Allow, Generic All, para este usuário e mais ninguém.
///
/// Como a DACL é uma lista de permissão, tudo o que não está escrito aqui está
/// negado: outros usuários, contas de serviço, e a sessão anônima em particular.
fn sddl(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

/// Constrói o `SECURITY_ATTRIBUTES` do pipe. O descritor precisa continuar vivo
/// enquanto o `CreateNamedPipeW` roda, e é por isso que os dois voltam juntos.
struct Seguranca {
    atributos: SECURITY_ATTRIBUTES,
    descritor: PSECURITY_DESCRIPTOR,
}

impl Seguranca {
    fn nova(sid: &str) -> Option<Self> {
        unsafe {
            let mut descritor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            let texto = para_utf16(&sddl(sid));
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                texto.as_ptr(),
                SDDL_REVISION_1,
                &mut descritor,
                std::ptr::null_mut(),
            ) == 0
            {
                log::warn!(
                    "não consegui montar a segurança do pipe ({}); \
                     sigo sem o canal de controle em vez de abri-lo para a máquina inteira",
                    ultimo_erro()
                );
                return None;
            }

            Some(Self {
                atributos: SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: descritor,
                    // O handle não é herdado por processos filhos. O Ditador
                    // lança `wl-copy` no Linux e nada no Windows, mas herdar um
                    // servidor de pipe sem precisar é dar acesso de graça.
                    bInheritHandle: 0,
                },
                descritor,
            })
        }
    }
}

impl Drop for Seguranca {
    fn drop(&mut self) {
        if !self.descritor.is_null() {
            unsafe { LocalFree(self.descritor.cast()) };
        }
    }
}

/// Uma ponta do pipe — a instância que o servidor oferece ou o handle que o
/// cliente abriu.
///
/// Existe como tipo próprio para que o `HANDLE` tenha dono: um `CloseHandle`
/// esquecido num caminho de erro vaza uma das quatro instâncias, e depois de
/// quatro erros o Ditador para de aceitar comandos sem nada explicar.
///
/// O `Lado` não é decoração: `DisconnectNamedPipe` é uma chamada do servidor, e
/// pedi-la sobre o handle de um cliente só devolve erro — silencioso, porque o
/// `Drop` não tem a quem contar.
pub struct Instancia(HANDLE, Lado);

/// Qual das duas pontas do pipe este handle é.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lado {
    Servidor,
    Cliente,
}

// O `HANDLE` é um ponteiro opaco e por isso o Rust não o considera enviável
// entre threads sozinho. Um handle de pipe do Windows é seguro de usar de
// qualquer thread — é o próprio sistema que serializa —, e o `serve` precisa
// mandá-lo para a thread que vai atender aquele cliente.
unsafe impl Send for Instancia {}

impl Drop for Instancia {
    fn drop(&mut self) {
        unsafe {
            // `DisconnectNamedPipe` antes de fechar: sem isso, um cliente que
            // ainda esteja lendo recebe o fim da conexão como erro de sistema em
            // vez de fim de arquivo. Só o servidor pode fazê-lo.
            if self.1 == Lado::Servidor {
                DisconnectNamedPipe(self.0);
            }
            CloseHandle(self.0);
        }
    }
}

/// Cria uma instância do pipe. `primeira` liga o
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`, que é o que detecta outra instância viva.
fn criar_instancia(primeira: bool) -> Result<Instancia, u32> {
    let Some(caminho) = caminho_do_pipe() else {
        return Err(ERROR_ACCESS_DENIED);
    };
    let Some(sid) = sid_do_usuario() else {
        return Err(ERROR_ACCESS_DENIED);
    };
    let Some(seguranca) = Seguranca::nova(&sid) else {
        return Err(ERROR_ACCESS_DENIED);
    };

    let nome = para_utf16(caminho);
    let modo_de_abertura = PIPE_ACCESS_DUPLEX
        | if primeira {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };

    let handle = unsafe {
        CreateNamedPipeW(
            nome.as_ptr(),
            modo_de_abertura,
            // Fluxo de bytes, não de mensagens: o protocolo é uma linha
            // terminada por `\n`, igual ao do socket Unix, e enquadrar por
            // mensagem só acrescentaria um segundo enquadramento por cima.
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            INSTANCIAS,
            TAMANHO_DO_BUFFER,
            TAMANHO_DO_BUFFER,
            0,
            &seguranca.atributos,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        Err(codigo_do_erro())
    } else {
        Ok(Instancia(handle, Lado::Servidor))
    }
}

// --------------------------------------------------------------- a porta aberta

/// O que o `bind` devolve quando não conseguiu ficar com o pipe.
pub enum Falha {
    /// Outra instância desta sessão já é a dona. Não é erro: é o estado
    /// desejado, alcançado por outro processo.
    JaRodando,
    /// Não dá para ter o canal de controle, e o motivo.
    SemLugar(String),
}

/// A primeira instância do pipe, já criada e ainda sem cliente.
pub type Escuta = Instancia;

/// Assume o pipe, a menos que outra instância desta sessão já seja a dona.
///
/// Quem responde "já tem dono" é o próprio Windows, pelo
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`: não há corrida entre conferir e criar,
/// porque as duas coisas são a mesma chamada. O socket Unix precisa de um
/// segundo `connect` depois do erro justamente por não ter isso.
pub fn bind() -> Result<Escuta, Falha> {
    match criar_instancia(true) {
        Ok(instancia) => {
            log::debug!("pipe de controle em {}", caminho_do_pipe().unwrap_or("?"));
            Ok(instancia)
        }
        // `ERROR_ACCESS_DENIED` é o que o `FILE_FLAG_FIRST_PIPE_INSTANCE`
        // devolve quando o nome já existe. É contraintuitivo — parece problema
        // de permissão — e custou um bom tempo de depuração na primeira vez.
        Err(ERROR_ACCESS_DENIED) => Err(Falha::JaRodando),
        Err(codigo) => Err(Falha::SemLugar(format!(
            "criando {}: {} (código {codigo})",
            caminho_do_pipe().unwrap_or("?"),
            std::io::Error::from_raw_os_error(codigo as i32)
        ))),
    }
}

/// Atende comandos numa thread própria. O handler devolve a resposta.
///
/// Cada cliente é atendido na thread dele. No Linux o atendimento é em série e
/// por isso lá há um prazo para o cliente calado — sem ele, uma conexão que
/// nunca mandasse a linha travava *todos* os comandos seguintes. Aqui o estrago
/// de um cliente calado é menor (ele segura uma das quatro instâncias, não a
/// fila inteira), mas quatro clientes calados fechariam a porta do mesmo jeito.
/// Daí o cão de guarda abaixo.
pub fn serve<F>(primeira: Escuta, handler: F)
where
    F: Fn(&str) -> crate::ipc::Resposta + Send + Sync + 'static,
{
    use std::sync::Arc;

    let handler = Arc::new(handler);
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            let mut atual = primeira;
            loop {
                // `ConnectNamedPipe` devolvendo zero com `ERROR_PIPE_CONNECTED`
                // significa que o cliente chegou entre a criação da instância e
                // esta chamada. É sucesso, não falha — e é o caso mais comum
                // quando a interface sobe junto com o Ditador.
                let conectou = unsafe { ConnectNamedPipe(atual.0, std::ptr::null_mut()) } != 0
                    || codigo_do_erro() == ERROR_PIPE_CONNECTED;

                if !conectou {
                    // Aqui havia um `continue` seco, e ele custou o canal de
                    // controle inteiro.
                    //
                    // O que acontece de verdade: um cliente abre o pipe e vai
                    // embora sem dizer nada — o `Get-Acl` do PowerShell faz
                    // exatamente isso ao ler o descritor de segurança, e
                    // qualquer ferramenta que "espie" pipes faz igual. A
                    // instância fica no estado *cliente foi embora*, e a partir
                    // daí todo `ConnectNamedPipe` nela devolve
                    // `ERROR_NO_DATA`. Com o `continue` seco, o laço girava
                    // nesse erro para sempre: um núcleo a 100% e nenhum
                    // `ditador --status` respondendo nunca mais, num Ditador que
                    // continuava gravando e transcrevendo normalmente. Levou
                    // dois minutos para reproduzir e não aparecia em teste
                    // nenhum.
                    //
                    // A instância só volta a servir depois de um
                    // `DisconnectNamedPipe`, e é isso que falta abaixo.
                    let codigo = codigo_do_erro();
                    log::debug!("ConnectNamedPipe falhou: {}", ultimo_erro());
                    unsafe { DisconnectNamedPipe(atual.0) };

                    if codigo != ERROR_NO_DATA {
                        // Outro erro qualquer: a instância pode estar
                        // inutilizável. Troca-se por uma nova, e a espera curta
                        // impede que uma falha permanente vire laço quente —
                        // que é a mesma lição, aplicada ao caso geral.
                        if let Ok(nova) = criar_instancia(false).or_else(|_| criar_instancia(true))
                        {
                            atual = nova;
                        } else {
                            std::thread::sleep(RESPIRO);
                        }
                    }
                    continue;
                }

                // A instância seguinte é criada *antes* de atender esta, para
                // que nunca haja um instante sem ninguém escutando: um
                // `ditador --status` disparado exatamente nesse intervalo
                // receberia "não está rodando" sobre um Ditador vivo.
                let proxima = match criar_instancia(false) {
                    Ok(instancia) => instancia,
                    Err(codigo) => {
                        // Sem instância nova, atende esta assim mesmo e tenta
                        // recomeçar. Fica um instante sem ninguém escutando — o
                        // preço de não ter handles — mas responder a este
                        // cliente é melhor do que abandoná-lo para preservar a
                        // continuidade de um serviço que já está degradado.
                        log::warn!(
                            "não consegui abrir outra instância do pipe (código {codigo}); \
                             atendo esta e tento de novo"
                        );
                        atender(atual, handler.clone());
                        match criar_instancia(true).or_else(|_| criar_instancia(false)) {
                            Ok(instancia) => {
                                atual = instancia;
                                continue;
                            }
                            Err(_) => {
                                log::error!(
                                    "o canal de controle caiu de vez; \
                                     `ditador --status` e afins pararam de responder"
                                );
                                return;
                            }
                        }
                    }
                };

                let anterior = std::mem::replace(&mut atual, proxima);
                atender(anterior, handler.clone());
            }
        })
        .expect("spawn ipc thread");
}

/// Lê a linha de um cliente, responde e encerra a conexão — ou fica escrevendo,
/// se ele tiver assinado.
fn atender<F>(instancia: Instancia, handler: std::sync::Arc<F>)
where
    F: Fn(&str) -> crate::ipc::Resposta + Send + Sync + 'static,
{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let respondido = Arc::new(AtomicBool::new(false));

    // O cão de guarda. Não existe `set_read_timeout` para named pipe em modo
    // bloqueante; o jeito suportado de acordar uma leitura presa é derrubar a
    // conexão de outra thread, e é o que ele faz. Sem isso, um cliente que
    // conecta e emudece segura uma das quatro instâncias para sempre.
    {
        let respondido = respondido.clone();
        let handle = instancia.0 as usize;
        let _ = std::thread::Builder::new()
            .name("ipc-prazo".into())
            .spawn(move || {
                std::thread::sleep(PACIENCIA);
                if !respondido.load(Ordering::SeqCst) {
                    log::debug!("cliente do pipe desistiu antes de mandar o comando");
                    unsafe { DisconnectNamedPipe(handle as HANDLE) };
                }
            });
    }

    // O cão de guarda guarda o valor do `HANDLE`, e não a posse dele — se ele
    // fosse dono, a instância só voltaria para o rodízio depois dos dois
    // segundos de paciência, e quatro `--status` seguidos esgotariam as quatro.
    // O preço de não ser dono é que existe **um** caminho em que o handle pode
    // fechar sem ninguém marcar `respondido`: o `spawn` abaixo falhar. Aí a
    // closure volta dentro do `Err`, é destruída, o `Drop` da `Instancia` fecha
    // o handle — e dois segundos depois o cão de guarda desconectaria um valor
    // que o Windows já pode ter reentregue a outro objeto deste processo. É por
    // isso que o `Err` é tratado em vez de virar `let _ =`.
    let atendimento = {
        let respondido = respondido.clone();
        std::thread::Builder::new()
            .name("ipc-cliente".into())
            .spawn(move || {
                let mut conversa = Conversa(&instancia);
                let mut linha = String::new();
                let leu = BufReader::new(Read::by_ref(&mut conversa).take(LIMITE_DA_LINHA))
                    .read_line(&mut linha)
                    .is_ok();
                respondido.store(true, Ordering::SeqCst);

                if leu && !linha.trim().is_empty() {
                    match handler(linha.trim()) {
                        crate::ipc::Resposta::Linha(resposta) => {
                            let _ = writeln!(conversa, "{resposta}");
                            // O cliente lê exatamente uma linha e fecha; esperar
                            // que ele termine evita que o `Drop` derrube a
                            // conexão antes de a resposta ter saído do buffer do
                            // sistema.
                            let _ = conversa.flush();
                        }
                        crate::ipc::Resposta::Fluxo(linhas) => escoar(&mut conversa, linhas),
                    }
                }
                drop(instancia);
            })
    };

    if atendimento.is_err() {
        // A instância já foi fechada junto com a closure devolvida no `Err`.
        // Marcar aqui é o que impede o cão de guarda de tocar num handle morto.
        respondido.store(true, Ordering::SeqCst);
        log::error!("não consegui criar a thread que atende o cliente do canal de controle");
    }
}

/// Escreve as linhas de uma assinatura até o cliente ir embora.
///
/// A saída normal deste laço é a **escrita falhar**: o frontend fechou, travou e
/// foi morto, ou o Windows encerrou a sessão. É de propósito que não há
/// protocolo de despedida — ele perderia justamente os casos em que ninguém teve
/// chance de se despedir, que são a maioria (veja o `integracoes.rs` desta
/// pasta). Largar o `Receiver` ao sair é o que avisa a thread da assinatura de
/// que não há mais para quem escrever.
///
/// Um cliente vivo que pare de ler prende esta thread num `WriteFile` quando o
/// buffer do pipe enche — quatro quilobytes, umas quarenta linhas de estado.
/// Prende só a conexão dele: a instância seguinte já foi criada antes de chegar
/// aqui, e os `ditador --status` da vida continuam sendo atendidos. Do outro
/// lado, a fila da assinatura tem prazo próprio e desiste sozinha.
fn escoar(conversa: &mut Conversa<'_>, linhas: crate::ipc::Fluxo) {
    log::debug!("um cliente assinou o canal de controle");
    for linha in linhas {
        if writeln!(conversa, "{linha}").is_err() {
            break;
        }
    }
    log::debug!("a assinatura do canal de controle terminou");
}

/// Empresta o handle da instância para o `std::io`, sem lhe dar a posse.
///
/// Um `File::from_raw_handle` fecharia o handle no `Drop` dele, e a posse é da
/// `Instancia` — que também precisa desconectar antes de fechar. Este
/// intermediário existe só para poder usar `BufReader`/`writeln!` em cima de
/// `ReadFile`/`WriteFile`.
struct Conversa<'a>(&'a Instancia);

impl std::io::Read for Conversa<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut lidos = 0u32;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReadFile(
                self.0.0,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut lidos,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // Cliente que fechou a ponta dele é fim de arquivo, não erro: é o
            // desfecho normal de todo `ditador --status`.
            return match codigo_do_erro() {
                109 | 233 => Ok(0), // ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED
                codigo => Err(std::io::Error::from_raw_os_error(codigo as i32)),
            };
        }
        Ok(lidos as usize)
    }
}

impl std::io::Write for Conversa<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut escritos = 0u32;
        let ok = unsafe {
            WriteFile(
                self.0.0,
                buf.as_ptr(),
                buf.len() as u32,
                &mut escritos,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::from_raw_os_error(codigo_do_erro() as i32));
        }
        Ok(escritos as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Envia um comando para a instância que já está rodando.
/// `None` significa que não há ninguém escutando.
pub fn send(comando: &str) -> Option<String> {
    let caminho = caminho_do_pipe()?;
    let nome = para_utf16(caminho);

    // Duas tentativas: se todas as instâncias estiverem ocupadas no instante da
    // primeira, o `WaitNamedPipeW` espera uma liberar. Não é laço infinito de
    // propósito — um Ditador travado com quatro clientes presos deve devolver
    // "não respondeu" ao terminal, e não pendurá-lo.
    for tentativa in 0..2 {
        let handle = unsafe {
            CreateFileW(
                nome.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };

        if handle != INVALID_HANDLE_VALUE {
            let instancia = Instancia(handle, Lado::Cliente);
            let mut conversa = Conversa(&instancia);
            conversa.write_all(comando.as_bytes()).ok()?;
            conversa.write_all(b"\n").ok()?;

            let mut resposta = String::new();
            BufReader::new(Read::by_ref(&mut conversa).take(LIMITE_DA_LINHA))
                .read_line(&mut resposta)
                .ok()?;
            return Some(resposta.trim_end().to_string());
        }

        if codigo_do_erro() != ERROR_PIPE_BUSY || tentativa == 1 {
            return None;
        }
        unsafe { WaitNamedPipeW(nome.as_ptr(), PACIENCIA.as_millis() as u32) };
    }
    None
}

/// No Windows não há o que limpar.
///
/// O pipe não é um arquivo no sistema de arquivos: ele deixa de existir quando o
/// último handle fecha, o que o próprio Windows faz mesmo se o processo morrer
/// de morte violenta. É a diferença que faz o socket Unix precisar do
/// `remove_file` e de tratar socket órfão de uma execução anterior — problemas
/// que simplesmente não existem deste lado.
pub fn cleanup() {}

// ------------------------------------------------------------------ utilidades

fn para_utf16(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

unsafe fn de_utf16(p: *const u16) -> String {
    unsafe {
        let mut fim = 0;
        while *p.add(fim) != 0 {
            fim += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, fim))
    }
}

fn codigo_do_erro() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

/// O último erro do Windows já traduzido para uma frase.
fn ultimo_erro() -> String {
    let codigo = codigo_do_erro();
    format!(
        "{} (código {codigo})",
        std::io::Error::from_raw_os_error(codigo as i32)
    )
}

/// Roda o fechamento quando sai de escopo, inclusive por `return` no meio.
///
/// Existe para que cada `CloseHandle` fique escrito ao lado do `Open` que o
/// exige, em vez de no fim de uma função com seis saídas — onde é exatamente o
/// tipo de linha que se esquece.
struct AoSair<F: FnMut()>(F);

impl<F: FnMut()> Drop for AoSair<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dacl_nao_deixa_ninguem_de_fora_entrar() {
        // A forma exata importa: `D:` abre a DACL, `P` a protege de herança, e
        // `(A;;GA;;;S-…)` é a única entrada. Qualquer coisa a mais nesta linha é
        // alguém a mais podendo abrir o microfone de outra pessoa.
        let s = sddl("S-1-5-21-1-2-3-1001");
        assert_eq!(s, "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)");
        assert!(s.starts_with("D:P"), "a DACL precisa ser protegida (P)");
        assert_eq!(s.matches("(A;").count(), 1, "entrou mais alguém na DACL");
        // Os suspeitos de sempre, que o descritor padrão do Windows costuma
        // incluir e que este aqui não pode incluir.
        for indesejado in ["WD", "AN", "BU", "BA", "SY"] {
            assert!(
                !s.contains(&format!(";{indesejado})")),
                "{indesejado} entrou na DACL do pipe"
            );
        }
    }

    #[test]
    fn o_windows_aceita_a_dacl_que_escrevemos() {
        // O teste acima confere o texto; este confere que o Windows concorda
        // com ele. Uma vírgula fora do lugar no SDDL só aparece aqui.
        let sid = sid_do_usuario().expect("o processo do teste precisa ter um SID");
        assert!(sid.starts_with("S-1-"), "SID com forma estranha: {sid}");
        let seguranca = Seguranca::nova(&sid);
        assert!(
            seguranca.is_some(),
            "o Windows recusou o SDDL: {}",
            sddl(&sid)
        );
    }

    #[test]
    fn o_nome_do_pipe_e_por_usuario() {
        let sid = sid_do_usuario().expect("o processo do teste precisa ter um SID");
        let caminho = montar_caminho(&sid);
        assert!(caminho.starts_with(r"\\.\pipe\Ditador-"));
        // Sem o SID, dois usuários logados ao mesmo tempo disputariam um nome só
        // e o segundo ficaria sem Ditador.
        assert!(caminho.contains("S-1-"), "o nome do pipe perdeu o SID");
    }

    /// Um cliente que abre a conexão e vai embora sem dizer nada não pode
    /// derrubar o canal de controle.
    ///
    /// Este teste existe porque isso aconteceu. O `Get-Acl` do PowerShell — e
    /// qualquer ferramenta que inspecione pipes — abre e fecha exatamente assim.
    /// A instância ficava presa devolvendo `ERROR_NO_DATA` em todo
    /// `ConnectNamedPipe`, o laço de atendimento girava nesse erro consumindo um
    /// núcleo inteiro, e nenhum `ditador --status` era atendido nunca mais. O
    /// programa seguia gravando e transcrevendo, o que tornava o sintoma ainda
    /// mais confuso: "o Ditador funciona, mas a linha de comando diz que ele não
    /// está rodando".
    #[test]
    fn um_cliente_que_some_nao_derruba_o_canal() {
        use std::time::Duration;

        let escuta = match bind() {
            Ok(escuta) => escuta,
            Err(_) => panic!("o teste precisa conseguir criar o próprio pipe"),
        };
        serve(escuta, |linha| {
            crate::ipc::Resposta::Linha(format!("eco: {linha}"))
        });

        // Antes: o canal atende.
        assert_eq!(send("oi").as_deref(), Some("eco: oi"));

        // O cliente malcomportado: abre e fecha sem escrever nem ler.
        let nome = para_utf16(caminho_do_pipe().expect("sem caminho"));
        let handle = unsafe {
            CreateFileW(
                nome.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE, "não consegui abrir o pipe");
        unsafe { CloseHandle(handle) };

        // Depois: o canal continua atendendo. Sem a correção, daqui para a
        // frente todo `send` devolvia `None`.
        //
        // A espera é para dar tempo de o laço perceber e se recompor; ela é
        // generosa de propósito, porque o custo de um teste instável é maior do
        // que o de duas voltas a mais.
        let mut resposta = None;
        for _ in 0..40 {
            resposta = send("oi");
            if resposta.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            resposta.as_deref(),
            Some("eco: oi"),
            "o canal de controle morreu depois de um cliente que só abriu e fechou"
        );
    }
}
