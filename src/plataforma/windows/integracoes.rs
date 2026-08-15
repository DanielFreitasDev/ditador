//! Quem mais está mostrando o Ditador na área de trabalho — no Windows, o
//! frontend WinUI.
//!
//! ## A pergunta é a mesma; a resposta vem de outro lugar
//!
//! No Linux, "a extensão do GNOME está no ar?" é respondida pelo barramento
//! D-Bus: cada integração segura um nome enquanto está carregada, e o barramento
//! **solta o nome sozinho** quando a conexão morre — Shell reiniciado, extensão
//! removida no meio do `disable()`, tanto faz. É por isso que o `CLAUDE.md` diz
//! que quem recolhe o ícone da bandeja é a presença de um nome no barramento, e
//! não um aviso da extensão: um protocolo de "avise quando sair" perderia todos
//! os casos em que ela não teve chance de avisar.
//!
//! O Windows não tem barramento de sessão, mas tem a mesma propriedade no lugar
//! certo: **a conexão do named pipe**. O frontend `Ditador.Windows` conecta e
//! fica conectado; se ele fechar, travar ou for morto pelo Gerenciador de
//! Tarefas, o sistema derruba a ponta dele e o servidor descobre na leitura
//! seguinte. Mesma garantia, mesmo motivo, mecanismo diferente — que é
//! exatamente o que uma camada de plataforma deve fazer.
//!
//! ## Por que ainda não faz nada
//!
//! A vigia da conexão pertence ao marco do IPC orientado a eventos, junto com o
//! comando `assinar` que põe o frontend a receber mudanças de estado. Enquanto
//! ele não existe, a resposta honesta é "nenhuma integração no ar" — que é a
//! verdade: sem frontend, quem mostra o Ditador é ninguém, e o `--diagnostico`
//! deve dizer isso.
//!
//! O que **não** se faz aqui é devolver `None` fingindo que a pergunta não pode
//! ser respondida. `None` no Linux quer dizer "não há barramento de sessão para
//! perguntar"; no Windows sempre há como saber, e a resposta é zero.

use crate::audio::Levels;
use crate::controller::IpcCommand;
use crate::state::{Integracoes, SharedState, Sinal};
use crossbeam_channel::Sender;

/// Sobe o que o Windows tem no lugar do D-Bus.
///
/// Hoje, nada: o servidor do named pipe já é criado pelo `ipc::bind`, no
/// `main.rs`, e o fluxo de eventos para o frontend ainda não existe. A função
/// existe com a assinatura do lado Linux para que o `main.rs` continue sendo um
/// arquivo só, sem `cfg` no meio da inicialização.
pub fn start(_shared: SharedState, _sinal: &Sinal, _comandos: Sender<IpcCommand>, _niveis: Levels) {
}

/// Que integrações estão no ar.
///
/// `Some(nenhuma)` e não `None`: no Windows sempre dá para responder. Veja o
/// comentário do módulo.
pub fn integracoes_no_ar() -> Option<Integracoes> {
    Some(Integracoes::default())
}

/// O que o `--diagnostico` diz quando não há integração nenhuma no ar.
///
/// A frase difere da do Linux num ponto que importa: lá, sem integração o
/// Ditador ainda põe o próprio ícone na bandeja, e a pergunta "cadê o ícone?"
/// tem resposta boa. Aqui não há ícone nenhum sem o `Ditador.Windows` — e dizer
/// isso na cara é melhor do que deixar a pessoa procurar na área de notificação
/// uma coisa que este processo nunca colocou lá.
pub fn sem_nenhuma() -> String {
    "nenhuma. Sem o Ditador.Windows não há ícone na área de notificação nem aviso \
     de gravação na tela — mas o atalho, a transcrição e a área de transferência \
     continuam funcionando por este processo."
        .to_string()
}
