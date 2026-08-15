//! O atalho global: a parte que não depende de sistema operacional.
//!
//! Aqui mora a máquina de teclas — quem está segurando o quê, quando a
//! combinação fica completa, e a captura de uma combinação nova na tela de
//! configurações. Ela não sabe de onde os eventos vêm.
//!
//! Quem os traz é `plataforma::teclado`: no Linux, o evdev lendo
//! `/dev/input/event*`; no Windows, o Raw Input entregando `WM_INPUT` numa
//! janela de mensagens escondida. Os dois chamam `evento()` com o mesmo código
//! canônico de tecla (veja `plataforma/mod.rs` sobre por que a numeração é a do
//! evdev nos dois lados) e com uma *origem*, que é o dispositivo que mandou.
//!
//! ## Por que a origem importa nas duas plataformas
//!
//! Este arquivo guarda, para cada tecla, o conjunto de dispositivos que a estão
//! segurando — não um simples "está apertada". Isso nasceu de um bug do Linux: a
//! colagem automática usa o `ydotool`, que cria um teclado virtual, e o "soltar"
//! dele apagava a tecla que a pessoa ainda segurava de verdade, cortando o
//! ditado pela metade.
//!
//! O Windows tem exatamente o mesmo problema com outro nome. O Raw Input entrega
//! o `hDevice` de quem originou cada tecla, e input sintético — de um `SendInput`
//! qualquer, do próprio Windows, de um software de macro — chega com um handle
//! diferente do teclado físico. Guardar por origem resolve os dois casos com o
//! mesmo código, o que é o melhor argumento que se pode ter de que a abstração
//! está no lugar certo.

use crate::keys;
use crossbeam_channel::Sender;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// De quem veio a tecla.
///
/// Um número opaco que só a plataforma sabe interpretar: no Linux é um contador
/// por `/dev/input/event*` aberto, no Windows é o `hDevice` que o Raw Input
/// carimba em cada evento. O que a máquina de teclas precisa é só conseguir
/// distinguir um do outro e comparar por igualdade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Origem(pub u64);

/// O que aconteceu com a tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Apertou,
    Soltou,
    /// Repetição automática de uma tecla que já estava apertada.
    ///
    /// Não muda o estado de nada, e é por isso que existe em vez de virar um
    /// segundo `Apertou`: contá-la abriria e fecharia a gravação a cada repique
    /// enquanto a pessoa segura a tecla.
    Repetiu,
}

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
    /// virtual que o `ydotool` cria para a colagem automática, ou com um
    /// `SendInput` no Windows — o `release` de um apagava a tecla que o outro
    /// ainda segurava, cortando um ditado em curso pela metade. Agora a tecla só
    /// deixa de estar pressionada quando o último dispositivo que a segurava a
    /// solta.
    pressed: Mutex<HashMap<u16, HashSet<Origem>>>,
    engaged: AtomicBool,
    capturing: AtomicBool,
    capture_buf: Mutex<Vec<u16>>,
    tx: Sender<HotkeyEvent>,
}

impl HotkeyListener {
    /// Monta o ouvinte sem subir thread nenhuma.
    ///
    /// Existe separado do `start` para os testes: a vigia da plataforma entra
    /// num laço infinito e abre os teclados de verdade da máquina, o que faria
    /// `cargo test` passar a ler as teclas de quem roda.
    pub fn novo(hotkey: &[String], tx: Sender<HotkeyEvent>) -> Arc<Self> {
        Arc::new(Self {
            target: RwLock::new(codes_of(hotkey, Some(&tx))),
            pressed: Mutex::new(HashMap::new()),
            engaged: AtomicBool::new(false),
            capturing: AtomicBool::new(false),
            capture_buf: Mutex::new(Vec::new()),
            tx,
        })
    }

    pub fn start(hotkey: &[String], tx: Sender<HotkeyEvent>) -> Arc<Self> {
        let listener = Self::novo(hotkey, tx);
        crate::plataforma::teclado::vigiar(listener.clone());
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

    /// Manda um aviso para quem escuta o atalho. É por aqui que a plataforma
    /// relata teclado ilegível e teclado de volta.
    pub(crate) fn avisar(&self, evento: HotkeyEvent) {
        let _ = self.tx.send(evento);
    }

    /// Desfaz o atalho em curso, avisando quem precisa saber.
    fn desengatar(&self) {
        if self.engaged.swap(false, Ordering::SeqCst) {
            let _ = self.tx.send(HotkeyEvent::Up);
        }
    }

    /// Solta tudo o que esta origem estava segurando, como se os eventos
    /// tivessem chegado.
    ///
    /// A plataforma chama isto quando perde o dispositivo de vista: um teclado
    /// desconectado com a tecla do atalho pressionada nunca manda o evento de
    /// soltar, e sem isto o código dela ficaria em `pressed` para sempre — a
    /// gravação não teria como parar.
    pub(crate) fn soltar_tudo_de(&self, origem: Origem) {
        let seus: Vec<u16> = lock_mut(&self.pressed)
            .iter()
            .filter(|(_, origens)| origens.contains(&origem))
            .map(|(code, _)| *code)
            .collect();
        for code in seus {
            self.evento(code, Acao::Soltou, origem);
        }
    }

    /// A porta de entrada de tudo: uma tecla mudou de estado em algum
    /// dispositivo.
    pub(crate) fn evento(&self, codigo: u16, acao: Acao, origem: Origem) {
        // Repetição automática não muda o estado de pressionado.
        let down = match acao {
            Acao::Repetiu => return,
            Acao::Apertou => true,
            Acao::Soltou => false,
        };

        {
            let mut pressed = lock_mut(&self.pressed);
            if down {
                pressed.entry(codigo).or_default().insert(origem);
            } else if let Some(origens) = pressed.get_mut(&codigo) {
                origens.remove(&origem);
                // Só sai do mapa quando o último dispositivo soltou.
                if origens.is_empty() {
                    pressed.remove(&codigo);
                }
            }
        }

        if self.capturing.load(Ordering::SeqCst) {
            self.handle_capture(codigo, down);
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

    fn handle_capture(&self, codigo: u16, down: bool) {
        if down {
            let mut buf = lock_mut(&self.capture_buf);
            if !buf.contains(&codigo) {
                buf.push(codigo);
            }
            return;
        }

        // A primeira tecla solta encerra a captura.
        let buf = std::mem::take(&mut *lock_mut(&self.capture_buf));
        if buf.is_empty() {
            return;
        }
        self.capturing.store(false, Ordering::SeqCst);

        // Só entram teclas cujo nome volta a ser a mesma tecla. Gravar na
        // configuração um código sem nome próprio produziria um atalho que nunca
        // mais dispara, e nada avisaria.
        let mut names: Vec<String> = Vec::new();
        for codigo in &buf {
            match keys::name(*codigo) {
                Some(nome) if keys::parse(&nome) == Some(*codigo) => names.push(nome),
                _ => log::warn!("tecla sem nome próprio ignorada na captura: código {codigo}"),
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

/// Traduz os nomes gravados na configuração para códigos canônicos.
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
            Some(tecla) => codigos.push(tecla),
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
    const TECLADO: Origem = Origem(1);
    /// Um segundo, para os casos em que duas fontes seguram a mesma tecla.
    const OUTRO: Origem = Origem(2);

    /// Os códigos que os testes usam, escritos pelo nome para não dependerem de
    /// qual plataforma está compilando: `keys::parse` é a mesma porta que a
    /// configuração de verdade atravessa.
    fn codigo(nome: &str) -> u16 {
        keys::parse(nome).unwrap_or_else(|| panic!("tecla desconhecida no teste: {nome}"))
    }

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
    fn tecla(listener: &HotkeyListener, nome: &str, acao: Acao) {
        listener.evento(codigo(nome), acao, TECLADO);
    }

    #[test]
    fn segurar_e_soltar_a_tecla_liga_e_desliga_o_atalho() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, "KEY_PAUSE", Acao::Apertou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Repetição automática não conta como um novo aperto.
        tecla(&listener, "KEY_PAUSE", Acao::Repetiu);
        assert!(rx.try_recv().is_err());
        tecla(&listener, "KEY_PAUSE", Acao::Soltou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn o_teclado_que_some_com_a_tecla_presa_nao_deixa_a_gravacao_correndo() {
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, "KEY_PAUSE", Acao::Apertou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        // O teclado é desconectado agora: o evento de soltar nunca chega, e
        // quem o inventa é a limpeza da leitura.
        listener.soltar_tudo_de(TECLADO);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
        assert!(lock_mut(&listener.pressed).is_empty());
    }

    #[test]
    fn a_combinacao_so_vale_com_todas_as_teclas_juntas() {
        let (listener, rx) = ouvinte(&["KEY_LEFTMETA", "KEY_SPACE"]);
        tecla(&listener, "KEY_LEFTMETA", Acao::Apertou);
        assert!(rx.try_recv().is_err(), "meia combinação não grava");
        tecla(&listener, "KEY_SPACE", Acao::Apertou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));
        // Soltar uma só já desfaz a combinação.
        tecla(&listener, "KEY_SPACE", Acao::Soltou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn o_teclado_de_mentira_nao_corta_o_ditado_do_teclado_de_verdade() {
        // No Linux é o `ydotool` da colagem automática, que devolve `29:0` por
        // um dispositivo virtual; no Windows é qualquer `SendInput`, que chega
        // com outro `hDevice`. Guardando só o código da tecla, aquele "soltar"
        // apagava o Ctrl que a pessoa ainda segurava.
        let (listener, rx) = ouvinte(&["KEY_LEFTCTRL"]);
        tecla(&listener, "KEY_LEFTCTRL", Acao::Apertou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Down)));

        // O dispositivo virtual aperta e solta a mesma tecla.
        let ctrl = codigo("KEY_LEFTCTRL");
        listener.evento(ctrl, Acao::Apertou, OUTRO);
        listener.evento(ctrl, Acao::Soltou, OUTRO);
        assert!(
            rx.try_recv().is_err(),
            "o ditado foi cortado por um teclado que não era o de quem fala"
        );

        // Só o teclado de verdade encerra.
        tecla(&listener, "KEY_LEFTCTRL", Acao::Soltou);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Up)));
    }

    #[test]
    fn abrir_a_captura_com_a_tecla_presa_solta_o_atalho_antes() {
        // Segurando o atalho e abrindo a captura pela bandeja, o release caía
        // dentro da captura: o `Up` nunca saía e o microfone ficava aberto até
        // o teto de duração estourar, quando o resultado arrancava a tela de
        // configurações de quem estava escolhendo a tecla nova.
        let (listener, rx) = ouvinte(&["KEY_PAUSE"]);
        tecla(&listener, "KEY_PAUSE", Acao::Apertou);
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
        assert_eq!(codigos, [codigo("KEY_PAUSE")]);
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Unavailable(_))));

        // Não sobrou nenhuma: o atalho não existe, e isso precisa aparecer.
        let nada = codes_of(&["KEY_NAO_EXISTE".to_string()], Some(&tx));
        assert!(nada.is_empty());
        assert!(matches!(rx.try_recv(), Ok(HotkeyEvent::Unavailable(_))));

        // Um atalho inteiro válido não gera aviso nenhum.
        let bom = codes_of(&["KEY_PAUSE".to_string()], Some(&tx));
        assert_eq!(bom, [codigo("KEY_PAUSE")]);
        assert!(rx.try_recv().is_err());
    }
}
