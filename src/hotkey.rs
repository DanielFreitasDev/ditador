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
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
    /// Um teclado voltou a ser lido depois de um `Unavailable`.
    ///
    /// Sem este evento o aviso ficava na tela para sempre: quem entrasse no
    /// grupo `input` e reconectasse o teclado continuava vendo o programa dizer
    /// que não conseguia lê-lo.
    Available,
}

pub struct HotkeyListener {
    target: RwLock<Vec<u16>>,
    /// Quem está segurando cada tecla, por dispositivo.
    ///
    /// Guardar só o código não bastava: com dois teclados — ou com o teclado
    /// virtual que o `ydotool` cria para a colagem automática — o `release` de
    /// um apagava a tecla que o outro ainda segurava, cortando um ditado em
    /// curso pela metade. Agora a tecla só deixa de estar pressionada quando o
    /// último dispositivo que a segurava a solta.
    pressed: Mutex<HashMap<u16, HashSet<PathBuf>>>,
    engaged: AtomicBool,
    capturing: AtomicBool,
    capture_buf: Mutex<Vec<u16>>,
    watched: Mutex<HashSet<PathBuf>>,
    tx: Sender<HotkeyEvent>,
}

impl HotkeyListener {
    /// Monta o ouvinte sem subir thread nenhuma.
    ///
    /// Existe separado do `start` para os testes: `watch_devices` entra num
    /// laço infinito e abre um `/dev/input/event*` por teclado de verdade, o
    /// que faria `cargo test` passar a ler as teclas de quem roda.
    pub fn novo(hotkey: &[String], tx: Sender<HotkeyEvent>) -> Arc<Self> {
        Arc::new(Self {
            target: RwLock::new(codes_of(hotkey, Some(&tx))),
            pressed: Mutex::new(HashMap::new()),
            engaged: AtomicBool::new(false),
            capturing: AtomicBool::new(false),
            capture_buf: Mutex::new(Vec::new()),
            watched: Mutex::new(HashSet::new()),
            tx,
        })
    }

    pub fn start(hotkey: &[String], tx: Sender<HotkeyEvent>) -> Arc<Self> {
        let listener = Self::novo(hotkey, tx);

        let watcher = listener.clone();
        std::thread::Builder::new()
            .name("hotkey-watch".into())
            .spawn(move || watcher.watch_devices())
            .expect("spawn hotkey-watch");

        listener
    }

    pub fn set_target(&self, hotkey: &[String]) {
        // Solta o atalho antigo antes de trocá-lo. Sem isto o `engaged` ficava
        // preso em `true` com a combinação nova valendo, e o aperto seguinte
        // era engolido — pior, se o microfone estivesse aberto ele nunca
        // receberia o `Up` que o fecha.
        self.desengatar();
        *self.target.write().unwrap_or_else(|e| e.into_inner()) = codes_of(hotkey, Some(&self.tx));
    }

    /// A próxima combinação pressionada vira o novo atalho.
    pub fn begin_capture(&self) {
        // Idem: segurando o atalho e abrindo a captura pela bandeja, o release
        // caía dentro da captura e o `Up` nunca era emitido — `stop_recording`
        // não rodava e o microfone ficava aberto até o teto de duração.
        self.desengatar();
        lock_mut(&self.capture_buf).clear();
        self.capturing.store(true, Ordering::SeqCst);
    }

    pub fn cancel_capture(&self) {
        self.capturing.store(false, Ordering::SeqCst);
        lock_mut(&self.capture_buf).clear();
    }

    /// Desfaz o atalho em curso, avisando quem precisa saber.
    fn desengatar(&self) {
        if self.engaged.swap(false, Ordering::SeqCst) {
            let _ = self.tx.send(HotkeyEvent::Up);
        }
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
                        let dono = p.clone();
                        me.read_device(device, &dono);
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
            } else if found_any && announced_failure {
                announced_failure = false;
                let _ = self.tx.send(HotkeyEvent::Available);
            }

            std::thread::sleep(RONDA_DOS_TECLADOS);
        }
    }

    fn read_device(&self, mut device: Device, dono: &Path) {
        loop {
            let events = match device.fetch_events() {
                Ok(events) => events,
                Err(e) => {
                    log::debug!("leitura do dispositivo encerrada: {e}");
                    // Um teclado que é desconectado com a tecla do atalho
                    // pressionada nunca manda o evento de soltar. Sem isto o
                    // código dela ficaria em `pressed` para sempre e a gravação
                    // não teria como parar.
                    self.soltar_tudo_de(dono);
                    return;
                }
            };
            for event in events {
                if let EventSummary::Key(_, code, value) = event.destructure() {
                    self.handle_key_de(code, value, dono);
                }
            }
        }
    }

    /// Solta tudo o que este dispositivo estava segurando, como se os eventos
    /// tivessem chegado.
    fn soltar_tudo_de(&self, dono: &Path) {
        let seus: Vec<u16> = lock_mut(&self.pressed)
            .iter()
            .filter(|(_, donos)| donos.contains(dono))
            .map(|(code, _)| *code)
            .collect();
        for code in seus {
            self.handle_key_de(KeyCode::new(code), 0, dono);
        }
    }

    fn handle_key_de(&self, code: KeyCode, value: i32, dono: &Path) {
        // 2 = auto-repeat, não muda o estado de pressionado.
        if value == 2 {
            return;
        }
        let down = value == 1;

        {
            let mut pressed = lock_mut(&self.pressed);
            if down {
                pressed.entry(code.code()).or_default().insert(dono.into());
            } else if let Some(donos) = pressed.get_mut(&code.code()) {
                donos.remove(dono);
                // Só sai do mapa quando o último dispositivo soltou.
                if donos.is_empty() {
                    pressed.remove(&code.code());
                }
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
            target.iter().all(|k| pressed.contains_key(k))
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

        // Só entram teclas cujo nome volta a ser a mesma tecla. O `keys::name`
        // sai do `Debug` do evdev, que devolve "unknown key: 217" para os
        // códigos fora da tabela dele — gravar isso na configuração produziria
        // um atalho que nunca mais dispara, e nada avisaria.
        let mut names: Vec<String> = Vec::new();
        for code in &buf {
            let tecla = KeyCode::new(*code);
            let nome = keys::name(tecla);
            if keys::parse(&nome) == Some(tecla) {
                names.push(nome);
            } else {
                log::warn!("tecla sem nome próprio ignorada na captura: código {code}");
            }
        }
        if names.is_empty() {
            let _ = self.tx.send(HotkeyEvent::Captured(Vec::new()));
            return;
        }

        // Esc sozinho cancela.
        if names.len() == 1 && names[0] == "KEY_ESC" {
            let _ = self.tx.send(HotkeyEvent::Captured(Vec::new()));
            return;
        }

        keys::sort_combo(&mut names);
        let _ = self.tx.send(HotkeyEvent::Captured(names));
    }
}

/// De quanto em quanto tempo a lista de teclados é reenumerada, para pegar os
/// que forem conectados com o programa já rodando.
const RONDA_DOS_TECLADOS: Duration = Duration::from_secs(3);

/// Traduz os nomes gravados na configuração para códigos do evdev.
///
/// O que não for reconhecido vira aviso — e, se nada sobrar, um `Unavailable`.
/// Antes disto um `filter_map` engolia a tecla errada em silêncio: o arquivo é
/// editável à mão, e uma grafia errada encolhia o atalho sem que nada mudasse
/// na tela. Quando o que sobrava era um modificador solto, o casamento por
/// subconjunto fazia o Ditador gravar em todo Ctrl+C da máquina.
fn codes_of(hotkey: &[String], avisar: Option<&Sender<HotkeyEvent>>) -> Vec<u16> {
    let mut codigos = Vec::with_capacity(hotkey.len());
    let mut recusados = Vec::new();
    for nome in hotkey {
        match keys::parse(nome) {
            Some(tecla) => codigos.push(tecla.code()),
            None => {
                log::warn!("tecla desconhecida na configuração do atalho: {nome}");
                recusados.push(nome.as_str());
            }
        }
    }

    if let Some(tx) = avisar
        && !recusados.is_empty()
    {
        let quais = recusados.join(", ");
        let _ = tx.send(HotkeyEvent::Unavailable(if codigos.is_empty() {
            format!(
                "O atalho configurado não existe neste teclado ({quais}). \
                 Escolha outra combinação em Configurações → Atalho."
            )
        } else {
            format!(
                "Parte do atalho não existe neste teclado ({quais}) e foi ignorada. \
                 Confira a combinação em Configurações → Atalho."
            )
        }));
    }
    codigos
}

/// Quantos teclados dá para ler agora.
///
/// É o que o `ditador --diagnostico` pergunta: zero aqui significa, quase
/// sempre, usuário fora do grupo `input` — a falha mais comum e mais silenciosa
/// deste programa.
pub fn teclados_legiveis() -> usize {
    evdev::enumerate()
        .filter(|(_, device)| is_keyboard(device))
        .count()
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

    /// O teclado de mentira dos testes.
    const TECLADO: &str = "/dev/input/event-de-teste";
    /// Um segundo, para os casos em que duas fontes seguram a mesma tecla.
    const OUTRO: &str = "/dev/input/event-de-teste-2";

    /// Um ouvinte sem thread nenhuma, para exercitar só a máquina de teclas.
    fn ouvinte(
        atalho: &[&str],
    ) -> (
        Arc<HotkeyListener>,
        crossbeam_channel::Receiver<HotkeyEvent>,
    ) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let nomes: Vec<String> = atalho.iter().map(|k| k.to_string()).collect();
        (HotkeyListener::novo(&nomes, tx), rx)
    }

    /// Aperta ou solta uma tecla no teclado principal.
    fn tecla(listener: &HotkeyListener, code: KeyCode, value: i32) {
        listener.handle_key_de(code, value, Path::new(TECLADO));
    }

    #[test]
    fn segurar_e_soltar_a_tecla_liga_e_desliga_o_atalho() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, KeyCode::KEY_PAUSE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Repetição automática não conta como um novo aperto.
        tecla(&listener, KeyCode::KEY_PAUSE, 2);
        assert!(rx.try_recv().is_err());
        tecla(&listener, KeyCode::KEY_PAUSE, 0);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn o_teclado_que_some_com_a_tecla_presa_nao_deixa_a_gravacao_correndo() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, KeyCode::KEY_PAUSE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        // O teclado é desconectado agora: o evento de soltar nunca chega, e
        // quem o inventa é a limpeza da leitura.
        listener.soltar_tudo_de(Path::new(TECLADO));
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
        assert!(lock_mut(&listener.pressed).is_empty());
    }

    #[test]
    fn a_combinacao_so_vale_com_todas_as_teclas_juntas() {
        let (listener, rx) = ouvinte(&["KEY_LEFTMETA", "KEY_SPACE"]);
        tecla(&listener, KeyCode::KEY_LEFTMETA, 1);
        assert!(rx.try_recv().is_err(), "meia combinação não grava");
        tecla(&listener, KeyCode::KEY_SPACE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Soltar uma só já desfaz a combinação.
        tecla(&listener, KeyCode::KEY_SPACE, 0);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn o_teclado_de_mentira_do_ydotool_nao_corta_o_ditado_do_teclado_de_verdade() {
        // Com a colagem automática ligada e um atalho com Ctrl, o `ydotool key
        // 29:0` volta pelo evdev num dispositivo virtual. Guardando só o código
        // da tecla, aquele "soltar" apagava o Ctrl que a pessoa ainda segurava.
        let (listener, rx) = ouvinte(&["KEY_LEFTCTRL"]);
        tecla(&listener, KeyCode::KEY_LEFTCTRL, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        // O dispositivo virtual aperta e solta a mesma tecla.
        listener.handle_key_de(KeyCode::KEY_LEFTCTRL, 1, Path::new(OUTRO));
        listener.handle_key_de(KeyCode::KEY_LEFTCTRL, 0, Path::new(OUTRO));
        assert!(
            rx.try_recv().is_err(),
            "o ditado foi cortado por um teclado que não era o de quem fala"
        );

        // Só o teclado de verdade encerra.
        tecla(&listener, KeyCode::KEY_LEFTCTRL, 0);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn abrir_a_captura_com_a_tecla_presa_solta_o_atalho_antes() {
        // Segurando o atalho e abrindo a captura pela bandeja, o release caía
        // dentro da captura: o `Up` nunca saía e o microfone ficava aberto até
        // o teto de duração estourar, quando o resultado arrancava a tela de
        // configurações de quem estava escolhendo a tecla nova.
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, KeyCode::KEY_PAUSE, 1);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        listener.begin_capture();
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn a_tecla_que_nao_existe_avisa_em_vez_de_sumir() {
        let (tx, rx) = crossbeam_channel::unbounded();
        // Sobrou uma tecla: o atalho encolhe e o aviso sai.
        let codigos = codes_of(
            &["KEY_PAUSE".to_string(), "KEY_NAO_EXISTE".to_string()],
            Some(&tx),
        );
        assert_eq!(codigos, [KeyCode::KEY_PAUSE.code()]);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Unavailable(_))));

        // Não sobrou nenhuma: o atalho não existe, e isso precisa aparecer.
        let nada = codes_of(&["KEY_NAO_EXISTE".to_string()], Some(&tx));
        assert!(nada.is_empty());
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Unavailable(_))));

        // Um atalho inteiro válido não gera aviso nenhum.
        let bom = codes_of(&["KEY_PAUSE".to_string()], Some(&tx));
        assert_eq!(bom, [KeyCode::KEY_PAUSE.code()]);
        assert!(rx.try_recv().is_err());
    }
}
