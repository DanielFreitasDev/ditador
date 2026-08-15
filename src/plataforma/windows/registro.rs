//! Onde o log do backend vai parar, no Windows: num arquivo em `LocalAppData`.
//!
//! No Linux o systemd recolhe a saída de erro e o `journalctl` a devolve. No
//! Windows não há nada equivalente para um programa de sessão do usuário — e,
//! pior, o caminho normal de execução é justamente o que não tem para onde
//! escrever: depois de instalado, quem sobe o `ditador.exe` é o
//! `Ditador.Windows`, com `CreateNoWindow`. Não há console, então a saída de
//! erro não vai a lugar nenhum. Durante um tempo a documentação disse que o log
//! do backend estava "no console de quem o iniciou", o que na prática queria
//! dizer que ele não existia.
//!
//! Nada de Event Log: escrever nele exige registrar uma fonte, o que pede
//! administrador. O log do Ditador é do usuário, fica com o usuário, e sai junto
//! quando ele desinstala com `-ApagarDados`.
//!
//! ## Rotação
//!
//! Um arquivo só, trocado por `ditador.log.1` quando passa de 1 MiB — que, no
//! nível `info`, são meses de uso. Duas gerações bastam para o que este log
//! serve: contar o que aconteceu antes de alguma coisa dar errado hoje. Não há
//! laço de manutenção nem thread vigiando; a conta é feita uma vez, na abertura.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// A partir deste tamanho o arquivo vira `.1` e um novo começa.
const LIMITE: u64 = 1024 * 1024;

/// `%LOCALAPPDATA%\ditador\logs\ditador.log`.
///
/// `LocalAppData`, e não `RoamingAppData`: log é diário de bordo de uma máquina,
/// não configuração para levar para outra. É o mesmo raciocínio que põe os
/// modelos do Whisper no local (veja `src/config.rs`).
pub fn caminho() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("ditador").join("logs").join("ditador.log"))
}

/// Abre o arquivo de log, girando o anterior se ele já estiver grande.
///
/// Devolve `None` quando não dá para escrever — disco cheio, pasta sem
/// permissão, perfil móvel indisponível. Nesse caso o `env_logger` fica com o
/// destino padrão dele e o Ditador roda igual: **não ter log não é motivo para
/// não transcrever**.
pub fn destino() -> Option<Box<dyn Write + Send + 'static>> {
    let caminho = caminho()?;
    let pasta = caminho.parent()?;
    std::fs::create_dir_all(pasta).ok()?;

    if std::fs::metadata(&caminho).is_ok_and(|m| m.len() > LIMITE) {
        // Falhar aqui não impede nada: o pior caso é o arquivo passar do limite.
        let _ = std::fs::rename(&caminho, caminho.with_extension("log.1"));
    }

    let arquivo = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&caminho)
        .ok()?;

    Some(Box::new(Duplo { arquivo }))
}

/// Escreve nos dois lugares: no arquivo e na saída de erro.
///
/// A saída de erro continua porque quem roda o `ditador.exe` num terminal para
/// depurar quer ver as linhas na hora, e não descobrir o caminho do arquivo
/// primeiro. Quando não há console — o caso normal, depois de instalado — a
/// escrita nela falha, e falhar ali não pode derrubar o log: o erro é
/// descartado de propósito.
struct Duplo {
    arquivo: File,
}

impl Write for Duplo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        // O erro do arquivo também é engolido: um disco cheio no meio de um
        // ditado deve custar o log, e não a transcrição. O `env_logger` não tem
        // o que fazer com um `Err` aqui além de reclamar em outro log que
        // também não existe.
        let _ = self.arquivo.write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        let _ = self.arquivo.flush();
        Ok(())
    }
}
