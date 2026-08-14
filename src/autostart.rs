//! Subir junto com a sessão gráfica.
//!
//! Há dois jeitos de conseguir isso no Linux, e cada um serve melhor a um caso:
//!
//!   * **serviço de usuário do systemd** (`ditador.service`). É o que o pacote
//!     `.deb` e o `instalar.sh` deixam pronto. Ganha reinício automático se o
//!     programa cair e um lugar certo para os registros (`journalctl --user`);
//!   * **autostart do XDG** (`~/.config/autostart/ditador.desktop`). Não depende
//!     de nada estar instalado e funciona em qualquer ambiente gráfico — é a
//!     saída para quem só compilou e rodou.
//!
//! O interruptor das configurações usa o primeiro quando ele existe e cai no
//! segundo quando não. Ler o estado consulta os dois: se qualquer um estiver
//! armado, o programa vai subir.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::Command;

const UNIDADE: &str = "ditador.service";

/// Qual dos dois caminhos está valendo nesta máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metodo {
    Systemd,
    Xdg,
}

/// Está armado para subir com a sessão?
pub fn ligado() -> bool {
    systemd_habilitado().unwrap_or(false) || arquivo_xdg().is_file()
}

/// O caminho que uma mudança vai usar. `Systemd` quando a unidade existe (o
/// programa foi instalado), `Xdg` no resto dos casos.
pub fn metodo() -> Metodo {
    if unidade_existe() {
        Metodo::Systemd
    } else {
        Metodo::Xdg
    }
}

/// Arma ou desarma. Mexe nos dois caminhos ao desarmar, para não sobrar um
/// resquício de uma instalação anterior mandando o programa subir.
pub fn definir(ligar: bool) -> Result<()> {
    if !ligar {
        if unidade_existe() {
            systemctl(&["disable", UNIDADE])?;
        }
        let arquivo = arquivo_xdg();
        if arquivo.is_file() {
            std::fs::remove_file(&arquivo)
                .with_context(|| format!("removendo {}", arquivo.display()))?;
        }
        return Ok(());
    }

    match metodo() {
        Metodo::Systemd => systemctl(&["enable", UNIDADE]).map(|_| ()),
        Metodo::Xdg => escrever_xdg(),
    }
}

// ------------------------------------------------------------------- systemd

fn unidade_existe() -> bool {
    Command::new("systemctl")
        .args(["--user", "cat", UNIDADE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn systemd_habilitado() -> Option<bool> {
    let saida = Command::new("systemctl")
        .args(["--user", "is-enabled", UNIDADE])
        .output()
        .ok()?;
    // `is-enabled` sai com código diferente de zero quando está desabilitado,
    // então quem responde é o texto, não o status.
    Some(String::from_utf8_lossy(&saida.stdout).trim() == "enabled")
}

fn systemctl(args: &[&str]) -> Result<()> {
    let saida = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .context("chamando o systemctl")?;
    if !saida.status.success() {
        bail!(
            "systemctl --user {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&saida.stderr).trim()
        );
    }
    Ok(())
}

// ----------------------------------------------------------------------- XDG

fn arquivo_xdg() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
        .join("ditador.desktop")
}

fn escrever_xdg() -> Result<()> {
    let arquivo = arquivo_xdg();
    let pasta = arquivo.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(pasta).with_context(|| format!("criando {}", pasta.display()))?;

    // O caminho absoluto do binário em execução, porque a sessão gráfica pode
    // não ter o mesmo PATH do terminal de onde o programa foi rodado.
    let executavel = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "ditador".to_string());

    std::fs::write(&arquivo, texto_xdg(&executavel))
        .with_context(|| format!("gravando {}", arquivo.display()))
}

fn texto_xdg(executavel: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Ditador\n\
         Comment=Ditado por voz offline, em segundo plano\n\
         Exec={executavel}\n\
         Icon=ditador\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn o_desktop_de_autostart_aponta_para_o_binario() {
        let texto = super::texto_xdg("/usr/bin/ditador");
        assert!(texto.contains("Exec=/usr/bin/ditador\n"));
        // Sem argumentos: o autostart sobe o serviço, não abre uma janela.
        assert!(!texto.contains("--alternar"));
        // E sem aparecer na lista de aplicativos, que já tem o atalho normal.
        assert!(texto.contains("NoDisplay=true"));
    }
}
