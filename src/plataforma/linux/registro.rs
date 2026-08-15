//! Onde o log do backend vai parar, no Linux: no journal.
//!
//! Não há arquivo a abrir aqui, e isso não é uma lacuna. O Ditador do Linux sobe
//! pelo systemd do usuário, que já captura a saída de erro do processo, já a
//! carimba com data e prioridade, já a rotaciona e já a entrega por um comando
//! que a documentação inteira usa (`journalctl --user -u ditador`). Abrir um
//! segundo arquivo por cima disso seria criar uma segunda tabela do mesmo dado —
//! exatamente o que o `CLAUDE.md` proíbe em tantas palavras.
//!
//! Quem roda o binário na mão continua vendo tudo no terminal, como sempre.

use std::io::Write;
use std::path::PathBuf;

/// O journal não é um arquivo nosso, então não há caminho a mostrar.
pub fn caminho() -> Option<PathBuf> {
    None
}

/// Nada a acrescentar: o destino padrão do `env_logger` (a saída de erro) é
/// justamente o que o systemd recolhe.
pub fn destino() -> Option<Box<dyn Write + Send + 'static>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Não é um teste de "chamar função e ver `None`": é o contrato do módulo.
    /// O `main` só desvia o log do `env_logger` quando `destino()` responde
    /// alguma coisa, e o `--diagnostico` só imprime a linha do log quando há
    /// caminho. Devolvendo `Some` por engano aqui, o journal deixaria de receber
    /// o que o systemd recolhe — e ninguém notaria até precisar dele.
    #[test]
    fn no_linux_quem_guarda_o_log_e_o_journal() {
        assert!(caminho().is_none(), "o journal não é arquivo nosso");
        assert!(
            destino().is_none(),
            "desviar a saída de erro tiraria o log do journal"
        );
    }
}
