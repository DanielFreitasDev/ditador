//! O ícone da barra no Windows — que **não** é deste processo.
//!
//! ## Um dono só, decidido de uma vez
//!
//! No Linux o Ditador publica o próprio StatusNotifierItem e o recolhe quando
//! uma integração nativa aparece. Isso funciona lá porque o protocolo permite
//! descobrir, em tempo de execução, que outra coisa já está mostrando o Ditador
//! — e porque o barramento avisa sozinho quando ela sai.
//!
//! O Windows não tem equivalente. `Shell_NotifyIcon` não responde "alguém já
//! mostra este aplicativo?", e não há como um processo saber que outro colocou
//! um ícone parecido na área de notificação. Se os dois tentassem, o usuário
//! veria **dois ícones do Ditador lado a lado** — e sem protocolo de descoberta,
//! nenhum dos dois teria como perceber e sair de cena.
//!
//! Então o dono é escolhido em tempo de projeto, e é o frontend:
//!
//! * o ícone precisa reagir a clique com menu, e menu é interface — o
//!   `TrackPopupMenuEx` e o `MenuFlyout` do WinUI vivem lá;
//! * o frontend já precisa de janela e laço de mensagens para o OSD e o popup; o
//!   backend teria de criar uma janela **só** para o ícone;
//! * o `TaskbarCreated` (o reinício do Explorer, que apaga todos os ícones da
//!   bandeja) precisa de um tratador só. Dois processos disputando isso é a
//!   receita para o ícone duplicar depois de cada reinício do Explorer.
//!
//! ## E se o frontend não estiver rodando?
//!
//! Aí não há ícone, e o Ditador continua ditando. O atalho global, a gravação, o
//! Whisper, a área de transferência e a linha de comando são todos deste
//! processo e não dependem de interface nenhuma — é o mesmo isolamento que faz o
//! Raw Input morar aqui e não no C#. Perder a interface custa o ícone; não custa
//! o programa.

use crate::controller::IpcCommand;
use crate::state::{SharedState, Sinal};
use crossbeam_channel::Sender;

/// Não faz nada, e isso é a implementação completa. Veja o comentário do módulo.
pub fn start(_shared: SharedState, _sinal: &Sinal, _comandos: Sender<IpcCommand>) {
    log::debug!("no Windows quem mostra o ícone da barra é o Ditador.Windows");
}
