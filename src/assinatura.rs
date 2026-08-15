//! Quem assina o canal de controle recebe o estado sem perguntar.
//!
//! É a metade do protocolo que o `Ditador.Windows` usa e a linha de comando não:
//! `ditador --status` pergunta, lê uma linha e vai embora; um frontend que
//! desenha um ícone e uma sobreposição precisa saber que a gravação começou **no
//! instante em que ela começa**, e ficar perguntando de 100 em 100 ms seria
//! trocar um problema por um pior — CPU gasta o tempo todo para descobrir tarde.
//!
//! ## Como funciona
//!
//! O cliente manda `assinar` na mesma conexão de sempre. A partir daí a conexão
//! deixa de ser pergunta-e-resposta e passa a receber **uma linha JSON por
//! mensagem**, terminada por `\n`, nesta ordem:
//!
//! ```text
//! {"t":"ola","protocolo":1,"aplicativo":"ditador","versao":"0.5.0","backend":"Vulkan"}
//! {"t":"estado","estado":"pronto","mensagem":"", …}     ← o retrato de agora
//! {"t":"estado", …}                                      ← a cada mudança
//! {"t":"nivel","valor":0.42}                             ← 15 Hz, só gravando
//! ```
//!
//! O `ola` vem primeiro para que um frontend antigo diante de um backend novo
//! possa desistir com uma frase em vez de interpretar campos que não conhece. O
//! `estado` logo em seguida é o *snapshot*: quem conecta não espera a próxima
//! mudança para saber em que pé as coisas estão — o que importa muito no arranque
//! do Windows, em que o frontend costuma conectar durante a carga do modelo e
//! precisa mostrar "carregando" imediatamente.
//!
//! ## O nível do microfone é diferente do resto
//!
//! Ele não é estado: é um fio de água passando. Por isso vai numa mensagem
//! própria, só enquanto o microfone está aberto, a 15 Hz — exatamente as mesmas
//! decisões (e o mesmo intervalo) do sinal `Nivel` do D-Bus, pelos mesmos
//! motivos escritos lá. Fora da gravação esta thread fica parada num `recv`, sem
//! laço acordando para perguntar se já é hora.
//!
//! ## Quem está assinando conta como integração
//!
//! Enquanto houver ao menos um assinante, `Integracoes::frontend` fica ligado —
//! e é isso que faz a janela do egui parar de desenhar o aviso de gravação no
//! Windows (o OSD nativo assume) e o `--diagnostico` responder "o
//! Ditador.Windows está no ar".
//!
//! A garantia é a mesma que o D-Bus dá no Linux, pelo mesmo mecanismo de fundo:
//! quem detém a assinatura é a **conexão**. Se o frontend fechar, travar ou for
//! morto pelo Gerenciador de Tarefas, o sistema derruba a ponta dele, a escrita
//! seguinte falha e a assinatura se solta sozinha. Não há protocolo de "avise
//! quando sair" — ele perderia justamente os casos em que ninguém teve chance de
//! avisar.

use crate::audio::Levels;
use crate::retrato::{PROTOCOLO, Retrato};
use crate::state::{SharedState, Sinal, lock};
use crossbeam_channel::{Sender, TrySendError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// De quanto em quanto tempo o nível do microfone vai para o assinante.
///
/// Quinze por segundo, igual ao D-Bus: o bastante para as barras parecerem
/// acompanhar a voz, e pouco o suficiente para não ser um assunto.
const INTERVALO_DO_NIVEL: Duration = Duration::from_millis(66);

/// Quantas linhas cabem no caminho entre o estado e o transporte.
///
/// Trinta e duas é folga larga para um fluxo de 15 mensagens por segundo. Ela
/// existe para o caso do frontend lento: as mensagens se acumulam um pouco em
/// vez de travarem esta thread na primeira.
const FILA: usize = 32;

/// Quanto esperamos por um assinante entupido antes de desistir dele.
///
/// Um frontend travado — não morto, travado — para de ler sem fechar a conexão,
/// e o buffer do transporte enche. Sem este prazo, a thread da assinatura ficaria
/// pendurada num `send` para sempre, uma por reconexão. Dois segundos é uma
/// eternidade para uma fila de 32 linhas; quem não a esvaziou nesse tempo não
/// está desenhando coisa nenhuma.
const PACIENCIA: Duration = Duration::from_secs(2);

/// Quantos assinantes estão de pé agora.
///
/// Global porque a pergunta é do processo, não de uma conexão: o que interessa a
/// quem desenha é "existe **algum** frontend?". Um frontend que reconecta depois
/// de o Explorer reiniciar não deve fazer o ícone piscar por causa de um instante
/// de zero assinantes, e dois clientes assinando ao mesmo tempo — a interface e
/// um `ditador --assinar` aberto num terminal para depurar — não podem fazer o
/// primeiro a sair apagar a presença do outro.
static ASSINANTES: AtomicUsize = AtomicUsize::new(0);

/// Abre uma assinatura e devolve as linhas que o transporte deve escrever.
///
/// A conexão morre quando o `Receiver` é largado — que é o que o transporte faz
/// quando a escrita falha, ou seja, quando o cliente foi embora.
pub fn abrir(shared: &SharedState, sinal: &Sinal, niveis: &Levels) -> crate::ipc::Fluxo {
    let (tx, rx) = crossbeam_channel::bounded(FILA);
    // O par que avisa desta thread quando o cliente vai embora: o `vivo` viaja
    // dentro do `Fluxo` e morre com ele; o `morreu` fica aqui, no `select!`
    // abaixo. Veja `ipc::Fluxo`, onde está escrito o defeito que isto conserta.
    let (vivo, morreu) = crossbeam_channel::bounded::<()>(0);

    // O `ola` e o primeiro `estado` são postos na fila **antes** de a função
    // voltar. Assim o snapshot já está a caminho mesmo que a thread abaixo
    // demore a ser escalonada, e o frontend nunca vê uma janela de tempo em que
    // está conectado e não sabe de nada.
    let _ = tx.try_send(ola());
    let primeiro = Retrato::tirar(shared, None);
    let _ = tx.try_send(primeiro.linha_json());

    let mudancas = sinal.observar();
    let shared_thread = shared.clone();
    let sinal_thread = sinal.clone();
    let niveis = niveis.clone();

    let subida = std::thread::Builder::new()
        .name("assinatura".into())
        .spawn(move || {
            anotar(
                &shared_thread,
                &sinal_thread,
                ASSINANTES.fetch_add(1, Ordering::SeqCst) + 1,
            );

            let mut anterior = primeiro;
            loop {
                // O estado vai **antes** do nível, e a ordem não é gosto: na
                // volta em que a gravação começa, as duas mensagens saem juntas,
                // e mandando o nível primeiro o frontend receberia uma barra de
                // microfone antes de saber que o microfone abriu — desenhando
                // nível sobre uma tela que ainda diz "pronto". Um teste pegou
                // exatamente isso.
                let agora = Retrato::tirar(&shared_thread, Some(&anterior));
                if agora != anterior {
                    if !entregar(&tx, agora.linha_json()) {
                        break;
                    }
                    anterior = agora;
                }

                let gravando = lock(&shared_thread).gravando();
                if gravando {
                    let valor = niveis
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .back()
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    // O nível é descartável: se a fila estiver cheia, o valor de
                    // agora não vale esperar — daqui a 66 ms vem outro, mais
                    // novo. É a diferença entre ele e uma mudança de estado, que
                    // não pode ser perdida.
                    if let Err(TrySendError::Disconnected(_)) = tx.try_send(nivel_json(valor)) {
                        break;
                    }
                    std::thread::sleep(INTERVALO_DO_NIVEL);
                    continue;
                }

                // Fora da gravação esta thread dorme, e acorda por um de dois
                // motivos: o estado mudou, ou o cliente foi embora. Sem o segundo
                // ramo ela dormiria para sempre num Ditador parado — que é
                // justamente o estado em que a maioria dos frontends é fechada.
                crossbeam_channel::select! {
                    recv(mudancas) -> aviso => if aviso.is_err() {
                        // O programa está encerrando: o `Sinal` foi embora.
                        break;
                    },
                    recv(morreu) -> _ => break,
                }
            }

            anotar(
                &shared_thread,
                &sinal_thread,
                ASSINANTES.fetch_sub(1, Ordering::SeqCst) - 1,
            );
        });

    if let Err(e) = subida {
        log::warn!("não consegui abrir a assinatura do canal de controle: {e}");
    }
    crate::ipc::Fluxo::novo(rx, vivo)
}

/// Põe a linha na fila, esperando um pouco se ela estiver cheia. `false`
/// significa que não há mais ninguém do outro lado.
fn entregar(tx: &Sender<String>, linha: String) -> bool {
    tx.send_timeout(linha, PACIENCIA).is_ok()
}

/// A primeira linha da conversa: quem responde e que versão fala.
fn ola() -> String {
    serde_json::json!({
        "t": "ola",
        "protocolo": PROTOCOLO,
        "aplicativo": "ditador",
        "versao": env!("CARGO_PKG_VERSION"),
        "backend": crate::stt::BACKEND,
    })
    .to_string()
}

fn nivel_json(valor: f32) -> String {
    // Duas casas: o que se desenha com isto é uma barra de uns 100 pixels, e as
    // outras cinco casas do `f32` só engordariam a linha quinze vezes por
    // segundo.
    serde_json::json!({ "t": "nivel", "valor": (f64::from(valor) * 100.0).round() / 100.0 })
        .to_string()
}

/// Liga ou desliga a presença do frontend no estado compartilhado.
///
/// Só escreve quando a resposta muda de fato — o `sinal.mudou()` de cada
/// reconexão redesenharia a interface inteira à toa.
fn anotar(shared: &SharedState, sinal: &Sinal, quantos: usize) {
    let presente = quantos > 0;
    {
        let mut estado = lock(shared);
        if estado.integracoes.frontend == presente {
            return;
        }
        estado.integracoes.frontend = presente;
    }
    log::info!(
        "{}",
        if presente {
            "um frontend assinou o canal de controle"
        } else {
            "o frontend soltou o canal de controle"
        }
    );
    sinal.mudou();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::{ModelState, Shared};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// O contador de assinantes é do processo inteiro, e os testes do Rust
    /// rodam em paralelo dentro de um processo só: sem esta trava, a assinatura
    /// de um teste apareceria na contagem que o outro está conferindo. É a mesma
    /// razão de ele ser global em produção — lá é uma virtude, aqui é preciso
    /// pôr os testes em fila.
    static UM_DE_CADA_VEZ: Mutex<()> = Mutex::new(());

    /// Pega a fila e espera as assinaturas do teste anterior terminarem de sair.
    ///
    /// Pegar a fila não basta: a thread de uma assinatura larga o `ASSINANTES`
    /// **depois** de o teste que a criou já ter terminado, e o teste seguinte
    /// via um contador que ainda não tinha voltado a zero. O sintoma era o
    /// terceiro teste falhar de vez em quando, sem nada de errado no código que
    /// ele cobre — que é o pior tipo de teste que existe.
    fn fila() -> std::sync::MutexGuard<'static, ()> {
        let guarda = UM_DE_CADA_VEZ.lock().unwrap_or_else(|e| e.into_inner());
        for _ in 0..500 {
            if ASSINANTES.load(Ordering::SeqCst) == 0 {
                return guarda;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("sobrou assinatura de um teste anterior");
    }

    fn bancada() -> (SharedState, Sinal, Levels) {
        let shared: SharedState = Arc::new(Mutex::new(Shared::new(Config::default(), Vec::new())));
        lock(&shared).model = ModelState::Ready;
        (
            shared,
            Sinal::default(),
            Arc::new(Mutex::new(VecDeque::new())),
        )
    }

    #[test]
    fn quem_assina_recebe_o_ola_e_o_estado_de_agora_sem_esperar_mudanca() {
        let _fila = fila();
        let (shared, sinal, niveis) = bancada();
        let rx = abrir(&shared, &sinal, &niveis);

        // Sem nenhuma mudança de estado no meio: as duas primeiras linhas têm de
        // estar na fila na hora, porque é delas que o frontend desenha a primeira
        // tela.
        let ola: serde_json::Value =
            serde_json::from_str(&rx.proxima(Duration::from_secs(2)).unwrap()).unwrap();
        assert_eq!(ola["t"], "ola");
        assert_eq!(ola["protocolo"], PROTOCOLO);
        assert_eq!(ola["aplicativo"], "ditador");

        let estado: serde_json::Value =
            serde_json::from_str(&rx.proxima(Duration::from_secs(2)).unwrap()).unwrap();
        assert_eq!(estado["t"], "estado");
        assert_eq!(estado["estado"], "pronto");
    }

    #[test]
    fn a_mudanca_de_estado_chega_sozinha_pelo_fio() {
        let _fila = fila();
        let (shared, sinal, niveis) = bancada();
        let rx = abrir(&shared, &sinal, &niveis);
        let _ola = rx.proxima(Duration::from_secs(2)).unwrap();
        let _snapshot = rx.proxima(Duration::from_secs(2)).unwrap();

        // Isto é o que o controlador faz ao abrir o microfone.
        lock(&shared).recording_since = Some(std::time::Instant::now());
        sinal.mudou();

        let linha = rx.proxima(Duration::from_secs(2)).unwrap();
        let evento: serde_json::Value = serde_json::from_str(&linha).unwrap();
        assert_eq!(evento["t"], "estado");
        assert_eq!(evento["estado"], "gravando");
        assert_ne!(evento["gravandoDesde"], 0);
    }

    #[test]
    fn o_frontend_conta_como_integracao_enquanto_estiver_assinando() {
        let _fila = fila();
        let (shared, sinal, niveis) = bancada();
        assert!(!lock(&shared).integracoes.frontend);

        let rx = abrir(&shared, &sinal, &niveis);
        // A anotação acontece na thread da assinatura; esperar por ela é esperar
        // pela primeira linha mais um instante.
        let _ = rx.proxima(Duration::from_secs(2)).unwrap();
        for _ in 0..200 {
            if lock(&shared).integracoes.frontend {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            lock(&shared).integracoes.frontend,
            "assinar não ligou a presença do frontend"
        );

        // Largar o fluxo é o que o transporte faz quando o cliente morre. E
        // repare no que **não** está escrito aqui: nenhum `sinal.mudou()`. O
        // frontend costuma ser fechado com o Ditador parado, sem nada mudando de
        // estado nem depois — se a thread só acordasse por mudança, ela dormiria
        // para sempre e a presença do frontend ficaria ligada num programa que
        // não tem mais frontend nenhum. Foi assim que este teste falhou da
        // primeira vez.
        drop(rx);
        for _ in 0..200 {
            if !lock(&shared).integracoes.frontend {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !lock(&shared).integracoes.frontend,
            "o frontend morreu e a presença dele ficou ligada"
        );
    }
}
