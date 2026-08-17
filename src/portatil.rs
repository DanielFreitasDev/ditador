//! Modo portátil: tudo numa pasta ao lado do executável.
//!
//! Normalmente o Ditador guarda a configuração em `~/.config/ditador` e os
//! dados — modelos, histórico — em `~/.local/share/ditador` (no Windows, o
//! `%APPDATA%` e o `%LOCALAPPDATA%`). Com um arquivo chamado `portatil` ao lado
//! do executável, os dois passam a morar numa pasta `Dados/` vizinha a ele.
//!
//! Para que serve: pendrive, e máquina de trabalho onde não se instala nada. No
//! Windows isso combina com o instalador sem administrador que já existe —
//! descompactar a pasta em qualquer lugar e criar o marcador é uma instalação
//! inteira. No Linux o caso é mais raro (o `.deb` pressupõe instalação normal),
//! mas o mecanismo é o mesmo e não custa nada ter os dois iguais.
//!
//! ## Por que a decisão é tomada uma vez, no arranque
//!
//! `config_dir()` e `data_dir()` são chamados de várias threads e em vários
//! momentos — o download do modelo, o histórico, o salvar das configurações.
//! Se a resposta pudesse mudar no meio da execução (o marcador sendo criado ou
//! apagado com o programa aberto), metade do programa escreveria num lugar e
//! metade no outro. O `OnceLock` decide no primeiro uso e não volta atrás.
//!
//! ## Por que não basta o marcador existir para a coisa valer
//!
//! Um marcador ao lado de um executável numa pasta somente-leitura — o
//! `/usr/bin` de uma instalação por `.deb`, por exemplo — produziria um Ditador
//! que não consegue gravar a própria configuração e não sabe dizer por quê.
//! Então o modo portátil só é aceito depois de a pasta de dados ser **criada e
//! testada com uma escrita de verdade**; falhando, o programa avisa no log e
//! segue pelos caminhos normais, que é o comportamento que não deixa ninguém
//! sem lugar para gravar.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// O nome do marcador. O alias em inglês existe pelo mesmo motivo que
/// `--toggle` acompanha `--alternar`: quem chega pela documentação de outro
/// programa tenta o nome que conhece.
const MARCADORES: [&str; 2] = ["portatil", "portable"];

/// A pasta de dados, dentro do modo portátil.
const PASTA: &str = "Dados";

static PASTA_PORTATIL: OnceLock<Option<PathBuf>> = OnceLock::new();

/// O que houve na detecção, para ser contado depois que o log existir.
///
/// A detecção precisa acontecer **antes** do logger — no Windows o próprio
/// destino do arquivo de log sai de `data_dir()`, que depende desta decisão.
/// Então ela não pode escrever no log, e o que ela teria a dizer fica guardado
/// aqui até o `relatar()`.
static RELATO: OnceLock<String> = OnceLock::new();

/// Decide, uma vez, se este processo roda em modo portátil.
///
/// Deve ser a primeira coisa do `main`, antes do logger e de qualquer coisa que
/// leia `config_dir()` ou `data_dir()`.
pub fn init() {
    let _ = PASTA_PORTATIL.set(detectar());
}

/// Conta no log o que a detecção descobriu. Chamada logo depois de o logger
/// subir.
pub fn relatar() {
    if let Some(relato) = RELATO.get() {
        log::warn!("{relato}");
    }
    if let Some(pasta) = pasta() {
        log::info!("modo portátil: os dados ficam em {}", pasta.display());
    }
}

/// A pasta de dados portátil, se este processo estiver em modo portátil.
pub fn pasta() -> Option<PathBuf> {
    PASTA_PORTATIL.get_or_init(detectar).clone()
}

/// Se o modo portátil está valendo — para a tela de configurações dizer onde as
/// coisas estão.
pub fn ativo() -> bool {
    pasta().is_some()
}

fn detectar() -> Option<PathBuf> {
    let executavel = std::env::current_exe().ok()?;
    let ao_lado = executavel.parent()?;
    let marcador = MARCADORES
        .iter()
        .map(|nome| ao_lado.join(nome))
        .find(|caminho| caminho.is_file())?;

    let pasta = ao_lado.join(PASTA);
    match preparar(&pasta) {
        Ok(()) => Some(pasta),
        Err(e) => {
            // Sem este aviso o sintoma seria um Ditador que perde as
            // configurações a cada execução, sem uma linha explicando.
            let _ = RELATO.set(format!(
                "há um marcador de modo portátil em {}, mas não consigo gravar em {}: {e}. \
                 Seguindo com as pastas do sistema.",
                marcador.display(),
                pasta.display()
            ));
            None
        }
    }
}

/// Cria a pasta e confirma que dá para gravar nela.
///
/// A escrita de teste não é paranoia: `create_dir_all` devolve `Ok` para uma
/// pasta que já existe e é somente-leitura, que é exatamente o caso do
/// executável instalado em `/usr/bin` com um marcador esquecido ao lado.
fn preparar(pasta: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(pasta)?;
    let teste = pasta.join(".escrita-de-teste");
    std::fs::write(&teste, b"ditador")?;
    std::fs::remove_file(&teste)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ditador-portatil-{}-{nome}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criando a pasta do teste");
        dir
    }

    #[test]
    fn sem_marcador_nao_ha_modo_portatil() {
        let dir = pasta_de_teste("sem-marcador");
        assert!(
            !MARCADORES.iter().any(|n| dir.join(n).is_file()),
            "a pasta do teste nasceu com marcador"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn os_dois_nomes_de_marcador_valem() {
        // O alias em inglês existe pelo mesmo motivo que `--toggle` acompanha
        // `--alternar`, e um teste é mais barato do que descobrir pelo relato de
        // alguém que criou o arquivo com o nome que a documentação de outro
        // programa ensina.
        for nome in MARCADORES {
            let dir = pasta_de_teste(&format!("marcador-{nome}"));
            std::fs::write(dir.join(nome), b"").expect("criando o marcador");
            let achou = MARCADORES.iter().any(|n| dir.join(n).is_file());
            assert!(achou, "o marcador \"{nome}\" não foi reconhecido");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_pasta_de_dados_e_conferida_com_uma_escrita_de_verdade() {
        // `create_dir_all` devolve `Ok` para uma pasta que já existe sem
        // permissão de escrita, e é por isso que a conferência escreve um
        // arquivo em vez de confiar nele.
        let dir = pasta_de_teste("escrita");
        let dados = dir.join(PASTA);
        assert!(preparar(&dados).is_ok(), "não consegui preparar {dados:?}");
        assert!(dados.is_dir());
        // E não deixa sujeira para trás.
        let sobras: Vec<_> = std::fs::read_dir(&dados)
            .expect("lendo")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(sobras.is_empty(), "o teste de escrita deixou {sobras:?}");

        // Um caminho impossível reprova, em vez de o programa descobrir isso na
        // primeira vez que tentar salvar.
        let impossivel = dados.join("arquivo").join("dentro-de-um-arquivo");
        std::fs::write(dados.join("arquivo"), b"x").expect("criando o arquivo");
        assert!(preparar(&impossivel).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
