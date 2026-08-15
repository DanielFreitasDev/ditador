//! O canal de controle local: instância única e linha de comando.
//!
//! É por aqui que `ditador --alternar`, `--status`, `--configuracoes` e
//! `--encerrar` falam com a instância que já está rodando — e é a mesma porta
//! que responde "já tem um Ditador de pé, não suba outro".
//!
//! O transporte muda de sistema para sistema e o protocolo não:
//!
//! * **Linux** — socket Unix em `$XDG_RUNTIME_DIR/ditador.sock`;
//! * **Windows** — named pipe `\\.\pipe\Ditador-<SID>`, com DACL só do usuário.
//!
//! Os dois carregam a mesma coisa: **uma linha de comando, uma linha de
//! resposta, ambas terminadas por `\n`**. É pouco e é de propósito. O volume é
//! de alguns comandos por dia, o conteúdo cabe numa linha de terminal, e o
//! formato é auditável com `nc -U` no Linux ou um `Get-Content` no Windows sem
//! precisar de ferramenta nenhuma. Protobuf ou gRPC aqui seriam três camadas
//! para transportar a palavra "toggle".
//!
//! ## Onde entra a interface do Windows
//!
//! O frontend WinUI precisa de mais do que isto: ele quer o estado inicial e
//! depois um fluxo de eventos, sem ficar perguntando. Isso **não** vira um
//! segundo protocolo nem um segundo canal — é um comando a mais nesta mesma
//! linha (`assinar`), depois do qual a conexão para de ser pergunta-e-resposta e
//! passa a receber uma linha por mudança de estado. Quem manda `status` e fecha
//! continua funcionando exatamente como antes.
//!
//! A regra do `CLAUDE.md` para o contrato D-Bus vale aqui inteira:
//! **acrescentar, nunca renomear**. Um comando novo é invisível para quem não o
//! conhece; um comando renomeado quebra o atalho de teclado que alguém
//! configurou no painel do sistema para chamar `ditador --alternar`.

/// O que o `bind` encontrou.
pub enum Bind {
    /// O canal é nosso, e este é o ouvinte.
    Escutando(Escuta),
    /// Outra instância já responde. Não é erro: é o estado desejado, alcançado
    /// por outro processo. Tratá-lo como falha faria o systemd reiniciar o
    /// serviço sem parar.
    JaRodando,
    /// Não há onde pendurar o canal, e o motivo.
    ///
    /// Também não é erro de inicialização: sem ele ainda dá para ditar, e o que
    /// se perde é o controle por linha de comando. Derrubar o programa inteiro
    /// por causa de um acessório seria trocar o todo pela parte que faltou.
    SemSocket(String),
}

/// O ouvinte de cada plataforma — um `UnixListener` no Linux, a primeira
/// instância do named pipe no Windows.
pub use crate::plataforma::ipc::Escuta;

pub fn bind() -> Bind {
    match crate::plataforma::ipc::bind() {
        Ok(escuta) => Bind::Escutando(escuta),
        Err(crate::plataforma::ipc::Falha::JaRodando) => Bind::JaRodando,
        Err(crate::plataforma::ipc::Falha::SemLugar(motivo)) => Bind::SemSocket(motivo),
    }
}

/// Envia um comando para a instância que já está rodando.
/// `None` significa que não há ninguém escutando.
pub fn send(comando: &str) -> Option<String> {
    crate::plataforma::ipc::send(comando)
}

/// Atende comandos numa thread própria. O handler devolve a resposta.
pub fn serve<F>(escuta: Escuta, handler: F)
where
    F: Fn(&str) -> String + Send + Sync + 'static,
{
    crate::plataforma::ipc::serve(escuta, handler)
}

/// Desfaz o que precisar ser desfeito na saída.
///
/// No Linux é apagar o arquivo do socket; no Windows não é nada, porque o pipe
/// deixa de existir sozinho quando o último handle fecha. A função existe nos
/// dois para que o `main` não precise saber qual é qual.
pub fn cleanup() {
    crate::plataforma::ipc::cleanup()
}
