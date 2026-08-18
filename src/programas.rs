//! Os programas de fora que o Ditador chama: `wl-copy`, `ydotool`, `curl`.
//!
//! Saber se cada um está instalado custa uma varredura do PATH, e quem pergunta
//! é o desenho da interface — a tela de resultado e a de configurações
//! consultam três deles, e o egui repinta várias vezes por segundo enquanto o
//! cursor passa por um controle. Perguntando na hora, cada quadro custava
//! dezenas de chamadas ao sistema (e, antes, um processo `which` por resposta).
//!
//! Por isso a resposta fica guardada. Quem instalar um dos programas com o
//! Ditador aberto não fica preso à resposta velha: `reler` joga a memória fora,
//! e a interface chama isso a cada troca de tela.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

static MEMORIA: Mutex<BTreeMap<&'static str, bool>> = Mutex::new(BTreeMap::new());

/// Prepara um comando de terminal para rodar sem piscar janela nenhuma.
///
/// **Só faz alguma coisa no Windows, e lá ela é obrigatória.** O `ditador.exe` é
/// um programa de console iniciado pelo `Ditador.Windows` com `CreateNoWindow`,
/// ou seja, sem console nenhum. Quando um processo sem console cria outro
/// processo de console — o `curl`, que é como este programa baixa modelo e
/// pergunta por versão nova —, o Windows **aloca um console para o filho**, e
/// isso é uma janela preta piscando na cara de quem estiver usando a máquina.
///
/// No download isso seria feio; na conferência de versão seria um defeito, porque
/// ela acontece sozinha, uma vez por dia, sem ninguém ter pedido nada. A janela
/// apareceria no meio de outra coisa qualquer, sem explicação.
///
/// `CREATE_NO_WINDOW` desliga isso sem mexer no resto: a saída continua sendo
/// capturada pelos canos que o `Command` já cria, que é o que o `output()` lê.
///
/// No Linux não há o que fazer — nem console para alocar — e a função devolve o
/// comando como recebeu.
pub fn sem_janela(cmd: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    cmd
}

/// O programa está no PATH?
pub fn existe(programa: &'static str) -> bool {
    if let Some(lembrado) = memoria().get(programa) {
        return *lembrado;
    }
    let achado = procurar(programa);
    memoria().insert(programa, achado);
    achado
}

/// O primeiro da lista que estiver instalado.
pub fn primeiro(candidatos: &[&'static str]) -> Option<&'static str> {
    candidatos.iter().copied().find(|p| existe(p))
}

/// Esquece tudo que foi respondido até agora.
pub fn reler() {
    memoria().clear();
}

fn memoria() -> std::sync::MutexGuard<'static, BTreeMap<&'static str, bool>> {
    MEMORIA.lock().unwrap_or_else(|e| e.into_inner())
}

fn procurar(programa: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|caminhos| {
        std::env::split_paths(&caminhos)
            .any(|dir| nomes_possiveis(programa).any(|nome| dir.join(&nome).is_file()))
    })
}

/// Os nomes de arquivo que o programa pode ter, em ordem de preferência.
///
/// No Unix é um só, o próprio nome. No Windows um executável quase nunca se
/// chama pelo nome pelado: quem digita `curl` executa `curl.exe`, e é o
/// `PATHEXT` que diz quais sufixos contam. Procurando sem eles, esta função
/// respondia "não instalado" para **tudo** no Windows — inclusive para o `curl`,
/// que vem no próprio sistema desde o Windows 10 e é como o Ditador baixa o
/// modelo de 574 MB. O sintoma seria um botão de baixar que não baixa, dizendo
/// que falta um programa que está ali.
#[cfg(target_os = "windows")]
fn nomes_possiveis(programa: &str) -> impl Iterator<Item = String> {
    // O padrão do Windows, para o caso improvável de a variável não existir.
    const RESERVA: &str = ".COM;.EXE;.BAT;.CMD";

    let extensoes = std::env::var("PATHEXT").unwrap_or_else(|_| RESERVA.to_string());
    // O nome pelado entra primeiro: um programa pode ter sido instalado sem
    // extensão nenhuma, e alguns ambientes de desenvolvimento fazem isso.
    let mut nomes = vec![programa.to_string()];
    nomes.extend(
        extensoes
            .split(';')
            .filter(|extensao| !extensao.is_empty())
            .map(|extensao| format!("{programa}{extensao}")),
    );
    nomes.into_iter()
}

#[cfg(not(target_os = "windows"))]
fn nomes_possiveis(programa: &str) -> impl Iterator<Item = String> {
    std::iter::once(programa.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um programa que está no PATH de qualquer máquina onde o Ditador roda.
    ///
    /// No Unix, o `sh`. No Windows, o `cmd` — que mora no `System32` e é
    /// encontrado **sem** a extensão que ele tem no disco (`cmd.exe`), o que faz
    /// dele a sonda certa: um teste que procurasse "cmd.exe" passaria mesmo com
    /// a busca por `PATHEXT` quebrada.
    #[cfg(target_os = "windows")]
    const SONDA: &str = "cmd";
    #[cfg(not(target_os = "windows"))]
    const SONDA: &str = "sh";

    #[test]
    fn acha_o_que_existe_e_nao_inventa_o_que_nao_existe() {
        assert!(existe(SONDA));
        assert!(!existe("nao-existe-um-programa-com-este-nome"));
        assert_eq!(
            primeiro(&["nao-existe-um-programa-com-este-nome", SONDA]),
            Some(SONDA)
        );
        assert_eq!(primeiro(&["nao-existe-um-programa-com-este-nome"]), None);
    }

    #[test]
    fn a_resposta_guardada_sobrevive_a_pergunta_repetida() {
        reler();
        assert!(existe(SONDA));
        assert!(existe(SONDA));
        reler();
        assert!(existe(SONDA));
    }

    /// O `curl` é como o Ditador baixa o modelo, e o Windows o traz de fábrica
    /// desde o Windows 10 — como `curl.exe`, dentro do System32.
    ///
    /// Este teste existe porque a versão anterior desta busca respondia "não
    /// instalado" para ele: sem consultar o `PATHEXT`, ela procurava um arquivo
    /// chamado exatamente `curl`, que não existe. O botão de baixar o modelo
    /// ficaria desligado numa máquina que tem tudo de que precisa.
    #[test]
    #[cfg(target_os = "windows")]
    fn o_curl_que_o_windows_traz_de_fabrica_e_encontrado() {
        reler();
        assert!(
            existe("curl"),
            "o curl.exe do System32 não foi encontrado; a busca no PATH \
             provavelmente parou de consultar o PATHEXT"
        );
    }
}
