//! Socket Unix para instância única e para o controle por linha de comando
//! (`ditador --alternar`, usado pelo ícone e por atalhos do GNOME).

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("ditador.sock")
    } else {
        PathBuf::from(format!("/tmp/ditador-{}.sock", unsafe { libc_getuid() }))
    }
}

// Evita puxar a crate `libc` só por isto.
unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

/// Envia um comando para a instância que já está rodando.
/// `None` significa que não há ninguém escutando.
pub fn send(command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;

    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply).ok()?;
    Some(reply.trim_end().to_string())
}

pub enum Bind {
    Ready(UnixListener),
    /// Outra instância já responde no socket.
    AlreadyRunning,
}

/// Assume o socket, a menos que outra instância já esteja atendendo nele.
///
/// "Já está rodando" não é erro: é o estado desejado. Tratá-lo como falha faria
/// o systemd reiniciar o serviço sem parar.
pub fn bind() -> Result<Bind> {
    let path = socket_path();

    if UnixStream::connect(&path).is_ok() {
        return Ok(Bind::AlreadyRunning);
    }
    // Socket órfão de uma execução anterior.
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("criando o socket {}", path.display()))?;
    Ok(Bind::Ready(listener))
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
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let reply = handler(line.trim());
                let _ = writeln!(stream, "{reply}");
            }
        })
        .expect("spawn ipc thread");
}

pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}
