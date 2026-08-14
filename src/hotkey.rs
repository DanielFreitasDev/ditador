//! Escuta global de teclado via evdev (/dev/input/event*).
//!
//! No Wayland o GNOME não entrega o evento de *soltar* a tecla para aplicativos
//! comuns, o que inviabiliza "segurar para falar". Lendo o evdev direto nós
//! recebemos press e release de verdade. Isso exige que o usuário pertença ao
//! grupo `input` (a leitura é passiva: as teclas continuam chegando normalmente
//! ao aplicativo em foco).

use crate::keys;
use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    /// Todas as teclas do atalho ficaram pressionadas.
    Down,
    /// O atalho deixou de estar completo.
    Up,
    /// Uma nova combinação foi capturada nas configurações (vazio = cancelado).
    Captured(Vec<String>),
    /// Nenhum teclado pôde ser lido.
    Unavailable(String),
}

pub struct HotkeyListener {
    target: RwLock<Vec<u16>>,
    pressed: Mutex<HashSet<u16>>,
    engaged: AtomicBool,
    capturing: AtomicBool,
    capture_buf: Mutex<Vec<u16>>,
    watched: Mutex<HashSet<PathBuf>>,
    tx: Sender<HotkeyEvent>,
}

impl HotkeyListener {
    pub fn start(hotkey: &[String], tx: Sender<HotkeyEvent>) -> Arc<Self> {
        let listener = Arc::new(Self {
            target: RwLock::new(codes_of(hotkey)),
            pressed: Mutex::new(HashSet::new()),
            engaged: AtomicBool::new(false),
            capturing: AtomicBool::new(false),
            capture_buf: Mutex::new(Vec::new()),
            watched: Mutex::new(HashSet::new()),
            tx,
        });

        let watcher = listener.clone();
        std::thread::Builder::new()
            .name("hotkey-watch".into())
            .spawn(move || watcher.watch_devices())
            .expect("spawn hotkey-watch");

        listener
    }

    pub fn set_target(&self, hotkey: &[String]) {
        *self.target.write().unwrap_or_else(|e| e.into_inner()) = codes_of(hotkey);
        self.engaged.store(false, Ordering::SeqCst);
    }

    /// A próxima combinação pressionada vira o novo atalho.
    pub fn begin_capture(&self) {
        lock_mut(&self.capture_buf).clear();
        self.capturing.store(true, Ordering::SeqCst);
    }

    pub fn cancel_capture(&self) {
        self.capturing.store(false, Ordering::SeqCst);
        lock_mut(&self.capture_buf).clear();
    }

    /// Reenumera os teclados periodicamente, cobrindo dispositivos conectados
    /// depois que o programa já estava rodando.
    fn watch_devices(self: Arc<Self>) {
        let mut announced_failure = false;
        loop {
            let mut found_any = false;

            for (path, device) in evdev::enumerate() {
                if !is_keyboard(&device) {
                    continue;
                }
                found_any = true;

                {
                    let mut watched = lock_mut(&self.watched);
                    if !watched.insert(path.clone()) {
                        continue;
                    }
                }

                let me = self.clone();
                let p = path.clone();
                let name = device.name().unwrap_or("desconhecido").to_string();
                log::info!("escutando teclado: {} ({})", name, p.display());
                if let Err(e) = std::thread::Builder::new()
                    .name("hotkey-read".into())
                    .spawn(move || {
                        me.read_device(device);
                        lock_mut(&me.watched).remove(&p);
                        log::info!("parou de escutar {}", p.display());
                    })
                {
                    log::warn!("não consegui criar a thread de leitura: {e}");
                    lock_mut(&self.watched).remove(&path);
                }
            }

            if !found_any && !announced_failure {
                announced_failure = true;
                let _ = self.tx.send(HotkeyEvent::Unavailable(
                    "Nenhum teclado legível em /dev/input. Verifique se seu usuário \
                     está no grupo 'input' (sudo usermod -aG input $USER) e reinicie a sessão."
                        .to_string(),
                ));
            } else if found_any {
                announced_failure = false;
            }

            std::thread::sleep(Duration::from_secs(3));
        }
    }

    fn read_device(&self, mut device: Device) {
        // O que este teclado, e só ele, deixou pressionado.
        let mut minhas: HashSet<u16> = HashSet::new();

        loop {
            let events = match device.fetch_events() {
                Ok(events) => events,
                Err(e) => {
                    log::debug!("leitura do dispositivo encerrada: {e}");
                    // Um teclado que é desconectado com a tecla do atalho
                    // pressionada nunca manda o evento de soltar. Sem isto o
                    // código dela ficaria em `pressed` para sempre e a gravação
                    // não teria como parar.
                    self.soltar(minhas);
                    return;
                }
            };
            for event in events {
                if let EventSummary::Key(_, code, value) = event.destructure() {
                    match value {
                        0 => {
                            minhas.remove(&code.code());
                        }
                        1 => {
                            minhas.insert(code.code());
                        }
                        _ => {}
                    }
                    self.handle_key(code, value);
                }
            }
        }
    }

    /// Solta as teclas indicadas, como se os eventos tivessem chegado.
    fn soltar(&self, codigos: HashSet<u16>) {
        for code in codigos {
            self.handle_key(KeyCode::new(code), 0);
        }
    }

    fn handle_key(&self, code: KeyCode, value: i32) {
        // 2 = auto-repeat, não muda o estado de pressionado.
        if value == 2 {
            return;
        }
        let down = value == 1;

        {
            let mut pressed = lock_mut(&self.pressed);
            if down {
                pressed.insert(code.code());
            } else {
                pressed.remove(&code.code());
            }
        }

        if self.capturing.load(Ordering::SeqCst) {
            self.handle_capture(code, down);
            return;
        }

        let target = lock(&self.target).clone();
        if target.is_empty() {
            return;
        }
        let complete = {
            let pressed = lock_mut(&self.pressed);
            target.iter().all(|k| pressed.contains(k))
        };

        let was = self.engaged.load(Ordering::SeqCst);
        if complete && !was {
            self.engaged.store(true, Ordering::SeqCst);
            let _ = self.tx.send(HotkeyEvent::Down);
        } else if !complete && was {
            self.engaged.store(false, Ordering::SeqCst);
            let _ = self.tx.send(HotkeyEvent::Up);
        }
    }

    fn handle_capture(&self, code: KeyCode, down: bool) {
        if down {
            let mut buf = lock_mut(&self.capture_buf);
            if !buf.contains(&code.code()) {
                buf.push(code.code());
            }
            return;
        }

        // A primeira tecla solta encerra a captura.
        let buf = std::mem::take(&mut *lock_mut(&self.capture_buf));
        if buf.is_empty() {
            return;
        }
        self.capturing.store(false, Ordering::SeqCst);

        let mut names: Vec<String> = buf.iter().map(|c| keys::name(KeyCode::new(*c))).collect();

        // Esc sozinho cancela.
        if names.len() == 1 && names[0] == "KEY_ESC" {
            let _ = self.tx.send(HotkeyEvent::Captured(Vec::new()));
            return;
        }

        keys::sort_combo(&mut names);
        let _ = self.tx.send(HotkeyEvent::Captured(names));
    }
}

fn codes_of(hotkey: &[String]) -> Vec<u16> {
    hotkey
        .iter()
        .filter_map(|name| keys::parse(name).map(|k| k.code()))
        .collect()
}

fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_ESC) && keys.contains(KeyCode::KEY_A))
}

fn lock<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

fn lock_mut<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um ouvinte sem thread nenhuma, para exercitar só a máquina de teclas.
    fn ouvinte(atalho: &[&str]) -> (HotkeyListener, crossbeam_channel::Receiver<HotkeyEvent>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let nomes: Vec<String> = atalho.iter().map(|k| k.to_string()).collect();
        let listener = HotkeyListener {
            target: RwLock::new(codes_of(&nomes)),
            pressed: Mutex::new(HashSet::new()),
            engaged: AtomicBool::new(false),
            capturing: AtomicBool::new(false),
            capture_buf: Mutex::new(Vec::new()),
            watched: Mutex::new(HashSet::new()),
            tx,
        };
        (listener, rx)
    }

    #[test]
    fn segurar_e_soltar_a_tecla_liga_e_desliga_o_atalho() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        listener.handle_key(KeyCode::KEY_PAUSE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Repetição automática não conta como um novo aperto.
        listener.handle_key(KeyCode::KEY_PAUSE, 2);
        assert!(rx.try_recv().is_err());
        listener.handle_key(KeyCode::KEY_PAUSE, 0);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn o_teclado_que_some_com_a_tecla_presa_nao_deixa_a_gravacao_correndo() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        listener.handle_key(KeyCode::KEY_PAUSE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        // O teclado é desconectado agora: o evento de soltar nunca chega, e
        // quem o inventa é a limpeza da leitura.
        listener.soltar(HashSet::from([KeyCode::KEY_PAUSE.code()]));
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
        assert!(lock_mut(&listener.pressed).is_empty());
    }

    #[test]
    fn a_combinacao_so_vale_com_todas_as_teclas_juntas() {
        let (listener, rx) = ouvinte(&["KEY_LEFTMETA", "KEY_SPACE"]);
        listener.handle_key(KeyCode::KEY_LEFTMETA, 1);
        assert!(rx.try_recv().is_err(), "meia combinação não grava");
        listener.handle_key(KeyCode::KEY_SPACE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Soltar uma só já desfaz a combinação.
        listener.handle_key(KeyCode::KEY_SPACE, 0);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }
}
