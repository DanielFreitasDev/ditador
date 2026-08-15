//! Escuta global de teclado via evdev (`/dev/input/event*`).
//!
//! No Wayland o GNOME não entrega o evento de *soltar* a tecla para aplicativos
//! comuns, o que inviabiliza "segurar para falar". Lendo o evdev direto nós
//! recebemos press e release de verdade. Isso exige que o usuário pertença ao
//! grupo `input` (a leitura é passiva: as teclas continuam chegando normalmente
//! ao aplicativo em foco).
//!
//! Este arquivo é só a *fonte* dos eventos. O que fazer com eles — combinação
//! completa, quem segura o quê, captura de um atalho novo — está em
//! `crate::hotkey`, que é igual nos dois sistemas.

use crate::hotkey::{Acao, HotkeyEvent, HotkeyListener, Origem};
use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// De quanto em quanto tempo a lista de teclados é reenumerada, para pegar os
/// que forem conectados com o programa já rodando.
const RONDA_DOS_TECLADOS: Duration = Duration::from_secs(3);

/// Sobe a vigia dos teclados numa thread própria.
pub fn vigiar(listener: Arc<HotkeyListener>) {
    let vigia = Arc::new(Vigia {
        listener,
        conhecidos: Mutex::new(HashMap::new()),
        proxima_origem: AtomicU64::new(1),
    });

    std::thread::Builder::new()
        .name("hotkey-watch".into())
        .spawn(move || vigia.rondar())
        .expect("spawn hotkey-watch");
}

/// Quantos teclados dá para ler agora.
///
/// É o que o `ditador --diagnostico` pergunta: zero aqui significa, quase
/// sempre, usuário fora do grupo `input` — a falha mais comum e mais silenciosa
/// deste programa.
pub fn teclados_legiveis() -> usize {
    evdev::enumerate()
        .filter(|(_, device)| e_teclado(device))
        .count()
}

/// A linha do `ditador --diagnostico` sobre a leitura do teclado.
///
/// Devolve a linha inteira, e não só um booleano, porque o conselho é o que
/// interessa e ele não tem nada em comum entre os sistemas: aqui a falha quase
/// sempre é o usuário fora do grupo `input`, e a saída é um `usermod` seguido de
/// sair da sessão. No Windows nada disso existe.
pub fn diagnostico() -> (Option<bool>, &'static str, String) {
    let teclados = teclados_legiveis();
    (
        // Aqui a resposta conta para o veredito: qualquer processo do usuário
        // consegue abrir `/dev/input/event*` se ele estiver no grupo `input`, e
        // não conseguir é a falha mais comum e mais silenciosa deste programa.
        Some(teclados > 0),
        "Leitura do teclado (/dev/input)",
        if teclados > 0 {
            format!("{teclados} teclado(s) legível(is).")
        } else {
            "Nenhum. Rode: sudo usermod -aG input $USER — depois saia da sessão \
             e entre de novo."
                .to_string()
        },
    )
}

/// Como o `--diagnostico` diz que não há instância rodando.
pub const COMO_SUBIR_O_SERVICO: &str = "nenhuma. Para subir: systemctl --user start ditador";

struct Vigia {
    listener: Arc<HotkeyListener>,
    /// Cada `/dev/input/event*` que já está sendo lido, e o número que o
    /// representa para a máquina de teclas.
    ///
    /// O número existe porque `crate::hotkey` não pode conhecer `PathBuf`: no
    /// Windows a mesma posição é ocupada por um `HANDLE`. Aqui ele é só um
    /// contador — o que importa é que dois dispositivos nunca recebam o mesmo.
    conhecidos: Mutex<HashMap<PathBuf, Origem>>,
    proxima_origem: AtomicU64,
}

impl Vigia {
    /// Reenumera os teclados periodicamente, cobrindo dispositivos conectados
    /// depois que o programa já estava rodando.
    fn rondar(self: Arc<Self>) {
        let mut avisou_da_falha = false;
        loop {
            let mut achou_algum = false;

            for (caminho, device) in evdev::enumerate() {
                if !e_teclado(&device) {
                    continue;
                }
                achou_algum = true;

                let origem = {
                    let mut conhecidos = trava(&self.conhecidos);
                    if conhecidos.contains_key(&caminho) {
                        continue;
                    }
                    let origem = Origem(self.proxima_origem.fetch_add(1, Ordering::Relaxed));
                    conhecidos.insert(caminho.clone(), origem);
                    origem
                };

                let nome = device.name().unwrap_or("desconhecido").to_string();
                log::info!("escutando teclado: {} ({})", nome, caminho.display());

                let eu = self.clone();
                let dono = caminho.clone();
                if let Err(e) = std::thread::Builder::new()
                    .name("hotkey-read".into())
                    .spawn(move || {
                        eu.ler(device, origem);
                        trava(&eu.conhecidos).remove(&dono);
                        log::info!("parou de escutar {}", dono.display());
                    })
                {
                    log::warn!("não consegui criar a thread de leitura: {e}");
                    trava(&self.conhecidos).remove(&caminho);
                }
            }

            if !achou_algum && !avisou_da_falha {
                avisou_da_falha = true;
                self.listener.avisar(HotkeyEvent::Unavailable(
                    "Nenhum teclado legível em /dev/input. Verifique se seu usuário \
                     está no grupo 'input' (sudo usermod -aG input $USER) e reinicie a sessão."
                        .to_string(),
                ));
            } else if achou_algum && avisou_da_falha {
                avisou_da_falha = false;
                self.listener.avisar(HotkeyEvent::Available);
            }

            std::thread::sleep(RONDA_DOS_TECLADOS);
        }
    }

    fn ler(&self, mut device: Device, origem: Origem) {
        loop {
            let eventos = match device.fetch_events() {
                Ok(eventos) => eventos,
                Err(e) => {
                    log::debug!("leitura do dispositivo encerrada: {e}");
                    // Um teclado que é desconectado com a tecla do atalho
                    // pressionada nunca manda o evento de soltar. Sem isto o
                    // código dela ficaria pressionado para sempre e a gravação
                    // não teria como parar.
                    self.listener.soltar_tudo_de(origem);
                    return;
                }
            };
            for evento in eventos {
                if let EventSummary::Key(_, code, valor) = evento.destructure() {
                    let acao = match valor {
                        0 => Acao::Soltou,
                        1 => Acao::Apertou,
                        // 2 = repetição automática. Qualquer outro valor é
                        // desconhecido e tratá-lo como repetição é o único
                        // desfecho que não inventa um aperto que não houve.
                        _ => Acao::Repetiu,
                    };
                    self.listener.evento(code.code(), acao, origem);
                }
            }
        }
    }
}

fn e_teclado(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_ESC) && keys.contains(KeyCode::KEY_A))
}

fn trava<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
