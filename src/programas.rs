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
use std::sync::Mutex;

static MEMORIA: Mutex<BTreeMap<&'static str, bool>> = Mutex::new(BTreeMap::new());

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
        std::env::split_paths(&caminhos).any(|dir| dir.join(programa).is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_o_que_existe_e_nao_inventa_o_que_nao_existe() {
        // O `sh` está no PATH de qualquer sistema onde este programa roda.
        assert!(existe("sh"));
        assert!(!existe("nao-existe-um-programa-com-este-nome"));
        assert_eq!(
            primeiro(&["nao-existe-um-programa-com-este-nome", "sh"]),
            Some("sh")
        );
        assert_eq!(primeiro(&["nao-existe-um-programa-com-este-nome"]), None);
    }

    #[test]
    fn a_resposta_guardada_sobrevive_a_pergunta_repetida() {
        reler();
        assert!(existe("sh"));
        assert!(existe("sh"));
        reler();
        assert!(existe("sh"));
    }
}
