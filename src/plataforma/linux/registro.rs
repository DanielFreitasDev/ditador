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
