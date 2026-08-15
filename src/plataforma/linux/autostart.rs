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
use std::sync::OnceLock;

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
///
/// A resposta é guardada porque quem pergunta é o desenho da tela de
/// configurações, a cada quadro, e descobrir custa um processo `systemctl`.
/// Guardar é seguro: a unidade só aparece numa instalação, e tanto o
/// `instalar.sh` quanto o `prerm` do pacote encerram o Ditador antes de mexer
/// em qualquer arquivo — nenhuma instalação acontece com este processo vivo.
pub fn metodo() -> Metodo {
    static MEMORIA: OnceLock<Metodo> = OnceLock::new();
    *MEMORIA.get_or_init(|| {
        if unidade_existe() {
            Metodo::Systemd
        } else {
            Metodo::Xdg
        }
    })
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

    std::fs::write(&arquivo, texto_xdg(&executavel_atual()))
        .with_context(|| format!("gravando {}", arquivo.display()))
}

/// O caminho absoluto do binário em execução, porque a sessão gráfica pode não
/// ter o mesmo PATH do terminal de onde o programa foi rodado.
///
/// Depois de um `cargo build` por cima do binário que está rodando, o Linux
/// devolve aqui o caminho antigo com " (deleted)" no fim. Escrever isso no
/// `Exec=` produziria um autostart que não sobe — e, como `ligado()` só confere
/// se o arquivo existe, o interruptor continuaria dizendo que está ligado.
fn executavel_atual() -> String {
    match std::env::current_exe() {
        Ok(caminho) if caminho.exists() => caminho.display().to_string(),
        Ok(caminho) => {
            log::warn!(
                "{} não existe mais; o autostart vai procurar o ditador no PATH",
                caminho.display()
            );
            "ditador".to_string()
        }
        Err(e) => {
            log::warn!("não descobri o caminho do próprio binário ({e}); usando o PATH");
            "ditador".to_string()
        }
    }
}

/// Cita o comando conforme a especificação Desktop Entry.
///
/// Sem as aspas, um caminho com espaço — que é o caso normal de quem só
/// compilou o projeto dentro de uma pasta com nome de verdade — era quebrado
/// pelo lançador e só o primeiro pedaço virava o programa a executar. A spec
/// pede a barra invertida antes de `"`, `` ` ``, `$` e `\`, e o `%` dobrado.
fn citar(comando: &str) -> String {
    let mut saida = String::with_capacity(comando.len() + 2);
    saida.push('"');
    for c in comando.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                saida.push('\\');
                saida.push(c);
            }
            '%' => saida.push_str("%%"),
            _ => saida.push(c),
        }
    }
    saida.push('"');
    saida
}

fn texto_xdg(executavel: &str) -> String {
    let comando = citar(executavel);
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Ditador\n\
         Comment=Ditado por voz offline, em segundo plano\n\
         Exec={comando}\n\
         Icon=ditador\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_desktop_de_autostart_aponta_para_o_binario() {
        let texto = texto_xdg("/usr/bin/ditador");
        assert!(texto.contains("Exec=\"/usr/bin/ditador\"\n"));
        // Sem argumentos: o autostart sobe o serviço, não abre uma janela.
        assert!(!texto.contains("--alternar"));
        // E sem aparecer na lista de aplicativos, que já tem o atalho normal.
        assert!(texto.contains("NoDisplay=true"));
    }

    #[test]
    fn o_exec_com_espaco_no_caminho_sai_entre_aspas() {
        // É o caso de quem só compilou: ~/Meus Projetos/ditador/target/release.
        let texto = texto_xdg("/home/ana/Meus Projetos/ditador");
        assert!(
            texto.contains("Exec=\"/home/ana/Meus Projetos/ditador\"\n"),
            "{texto}"
        );
    }

    #[test]
    fn os_caracteres_que_a_especificacao_manda_escapar_saem_escapados() {
        assert_eq!(citar("/opt/di$ador"), "\"/opt/di\\$ador\"");
        assert_eq!(citar("/opt/100% puro"), "\"/opt/100%% puro\"");
        assert_eq!(citar(r"/opt/a\b"), "\"/opt/a\\\\b\"");
    }
}
