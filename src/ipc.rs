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

/// O que o atendimento faz com uma conexão depois de ler a linha dela.
///
/// São dois desfechos, e não um, porque há dois públicos: quem pergunta e vai
/// embora (`ditador --status`, o atalho do painel) e quem fica observando (o
/// `Ditador.Windows`). O transporte de cada plataforma sabe escrever uma linha e
/// sabe escrever muitas; o que ele **não** sabe é de onde elas vêm, e é por isso
/// que a decisão chega até ele já tomada, num enum, em vez de o transporte
/// aprender o vocabulário dos comandos.
pub enum Resposta {
    /// Uma linha e a conversa acaba — o caso de quase tudo.
    Linha(String),
    /// O cliente assinou: a conexão fica aberta e recebe uma linha por mensagem
    /// até ele ir embora. Quem produz as linhas é `crate::assinatura`.
    Fluxo(Fluxo),
}

/// As linhas de uma assinatura, e o aviso de quando ela acaba.
///
/// Podia ser um `Receiver<String>` seco, e foi — até um teste mostrar o buraco:
/// quem **produz** as linhas passa a maior parte do tempo dormindo à espera da
/// próxima mudança de estado, e um `Receiver` largado não acorda ninguém. O
/// frontend morria, nada mudava no Ditador (que é o normal: ninguém está
/// ditando), e a thread da assinatura ficava dormindo para sempre — com a
/// presença do frontend ligada, o que faz a janela do egui continuar escondida.
/// O usuário ficava sem ícone e sem aviso de gravação, e nada no programa sabia.
///
/// O `_vivo` resolve isso sem protocolo nenhum: é a ponta de um canal vazio que
/// nunca carrega nada. Quando o transporte larga este `Fluxo` — que é o que ele
/// faz assim que a escrita falha —, o canal fecha, e quem produz acorda na hora
/// pelo `select!`. É o mesmo princípio que faz a conexão ser a fonte da verdade
/// no D-Bus e no pipe: a morte avisa sozinha, sem depender de despedida.
pub struct Fluxo {
    linhas: crossbeam_channel::Receiver<String>,
    _vivo: crossbeam_channel::Sender<()>,
}

impl Fluxo {
    pub fn novo(
        linhas: crossbeam_channel::Receiver<String>,
        vivo: crossbeam_channel::Sender<()>,
    ) -> Self {
        Self {
            linhas,
            _vivo: vivo,
        }
    }

    /// A próxima linha, esperando no máximo o prazo. Existe para os testes: o
    /// transporte usa o iterador, que bloqueia até o fim do fluxo.
    #[cfg(test)]
    pub fn proxima(&self, prazo: std::time::Duration) -> Option<String> {
        self.linhas.recv_timeout(prazo).ok()
    }
}

impl Iterator for Fluxo {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        self.linhas.recv().ok()
    }
}

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
    F: Fn(&str) -> Resposta + Send + Sync + 'static,
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
