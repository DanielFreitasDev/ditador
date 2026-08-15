//! Socket Unix para instância única e para o controle por linha de comando
//! (`ditador --alternar`, usado pelo ícone e por atalhos do GNOME).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Onde o socket de controle mora, ou `None` quando não há lugar seguro.
///
/// A resposta é decidida uma vez só, porque descobri-la cria a pasta de reserva.
pub fn socket_path() -> Option<&'static Path> {
    static CAMINHO: OnceLock<Option<PathBuf>> = OnceLock::new();
    CAMINHO.get_or_init(escolher_o_lugar).as_deref()
}

/// O lugar certo é o `XDG_RUNTIME_DIR`: ele já é uma pasta só do usuário, criada
/// pelo systemd-logind com permissão 0700.
///
/// Sem ele — sessão sem logind, contêiner, `su` para outro usuário —, a reserva
/// é uma pasta nossa dentro do /tmp, criada com 0700 e conferida antes do uso. A
/// conferência não é zelo: o /tmp é gravável por qualquer um, e um socket solto
/// ali poderia ter sido deixado por outro usuário da máquina, que passaria a
/// receber os comandos do Ditador — inclusive o de encerrar — no lugar dele.
fn escolher_o_lugar() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(dir).join("ditador.sock"));
    }

    let reserva = PathBuf::from(format!("/tmp/ditador-{}", unsafe { libc_getuid() }));
    match std::fs::DirBuilder::new().mode(0o700).create(&reserva) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => {
            log::warn!("não consegui criar {}: {e}", reserva.display());
            return None;
        }
    }

    if !so_nossa(&reserva) {
        log::warn!(
            "{} não é uma pasta só sua; sigo sem o socket de controle",
            reserva.display()
        );
        return None;
    }
    Some(reserva.join("ditador.sock"))
}

/// Pasta de verdade, do usuário de agora e fechada para todo o resto do mundo.
fn so_nossa(caminho: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    // `symlink_metadata`, e não `metadata`: um link simbólico apontando para a
    // pasta de outro usuário passaria numa conferência feita no destino.
    match std::fs::symlink_metadata(caminho) {
        Ok(meta) => {
            meta.is_dir()
                && meta.uid() == unsafe { libc_getuid() }
                && meta.permissions().mode() & 0o077 == 0
        }
        Err(_) => false,
    }
}

// Evita puxar a crate `libc` só por isto.
unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

/// Quanto tempo esperamos por um cliente calado antes de desistir dele.
///
/// O atendimento é em série, então uma conexão que nunca mandasse a linha
/// prendia a thread inteira: a partir dali nenhum `ditador --alternar` — nem o
/// do atalho do GNOME, nem o do lançador — era atendido, e cada um deles ficava
/// pendurado esperando resposta. O atalho do evdev e a bandeja continuavam
/// funcionando, o que tornava o sintoma difícil de entender.
const PACIENCIA: Duration = Duration::from_secs(2);

/// Teto do que aceitamos numa linha de comando.
///
/// O maior comando válido tem oito bytes. Sem teto, um cliente que mandasse
/// bytes sem nunca mandar `\n` fazia a `String` crescer até acabar a memória.
const LIMITE_DA_LINHA: u64 = 1024;

/// Envia um comando para a instância que já está rodando.
/// `None` significa que não há ninguém escutando.
pub fn send(command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()?).ok()?;
    // Do lado do cliente os prazos também importam: sem eles, uma instância
    // travada deixava `ditador --status` pendurado para sempre no terminal.
    let _ = stream.set_read_timeout(Some(PACIENCIA));
    let _ = stream.set_write_timeout(Some(PACIENCIA));
    stream.write_all(command.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;

    let mut reply = String::new();
    BufReader::new(stream.take(LIMITE_DA_LINHA))
        .read_line(&mut reply)
        .ok()?;
    Some(reply.trim_end().to_string())
}

pub enum Bind {
    Escutando(UnixListener),
    /// Outra instância já responde no socket.
    JaRodando,
    /// Não há onde pendurar o socket, e o motivo.
    ///
    /// Não é erro de inicialização: sem socket ainda dá para ditar, e o que se
    /// perde é o controle por linha de comando. Derrubar o programa inteiro por
    /// causa de um acessório seria trocar o todo pela parte que faltou.
    SemSocket(String),
}

/// Assume o socket, a menos que outra instância já esteja atendendo nele.
///
/// "Já está rodando" não é erro: é o estado desejado. Tratá-lo como falha faria
/// o systemd reiniciar o serviço sem parar.
pub fn bind() -> Bind {
    let Some(path) = socket_path() else {
        return Bind::SemSocket("sem um lugar seguro para o socket".to_string());
    };

    if UnixStream::connect(path).is_ok() {
        return Bind::JaRodando;
    }
    // Socket órfão de uma execução anterior.
    let _ = std::fs::remove_file(path);

    match UnixListener::bind(path) {
        Ok(listener) => Bind::Escutando(listener),
        // Duas execuções simultâneas chegam aqui juntas e uma perde a corrida.
        // A que perdeu confere de novo: se agora há alguém atendendo, o estado
        // desejado foi alcançado por outro caminho.
        Err(e) => {
            if UnixStream::connect(path).is_ok() {
                Bind::JaRodando
            } else {
                Bind::SemSocket(format!("criando {}: {e}", path.display()))
            }
        }
    }
}

/// Atende comandos numa thread própria. O handler devolve a resposta.
pub fn serve<F>(listener: UnixListener, handler: F)
where
    F: Fn(&str) -> String + Send + 'static,
{
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(PACIENCIA));
                let _ = stream.set_write_timeout(Some(PACIENCIA));

                let mut line = String::new();
                if BufReader::new((&stream).take(LIMITE_DA_LINHA))
                    .read_line(&mut line)
                    .is_err()
                {
                    log::debug!("cliente do socket desistiu antes de mandar o comando");
                    continue;
                }
                let reply = handler(line.trim());
                let _ = writeln!(stream, "{reply}");
            }
        })
        .expect("spawn ipc thread");
}

pub fn cleanup() {
    if let Some(path) = socket_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_serve_a_pasta_que_e_so_nossa() {
        let base = std::env::temp_dir().join(format!("ditador-teste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let fechada = base.join("fechada");
        let aberta = base.join("aberta");
        // As permissões vão depois da criação: o `mode` do `DirBuilder` ainda
        // passa pela umask de quem roda o teste, e o que se quer aqui são dois
        // valores exatos.
        let criar = |caminho: &Path, modo: u32| {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::DirBuilder::new()
                .recursive(true)
                .create(caminho)
                .expect("criando a pasta do teste");
            std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(modo))
                .expect("ajustando a pasta do teste");
        };
        criar(&fechada, 0o700);
        criar(&aberta, 0o755);

        assert!(so_nossa(&fechada));
        // Com a pasta aberta, qualquer um da máquina troca o socket de lugar.
        assert!(!so_nossa(&aberta));

        // Um arquivo comum no lugar da pasta, ou nada, também não servem.
        let arquivo = base.join("arquivo");
        std::fs::write(&arquivo, b"").expect("criando o arquivo do teste");
        assert!(!so_nossa(&arquivo));
        assert!(!so_nossa(&base.join("nao-existe")));

        let _ = std::fs::remove_dir_all(&base);
    }
}
