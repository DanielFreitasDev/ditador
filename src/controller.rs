//! Orquestra atalho → gravação → transcrição → resultado.

use crate::audio::{AudioCmd, AudioEvent, AudioHandle, AudioSettings};
use crate::clipboard;
use crate::config::Config;
use crate::hotkey::{HotkeyEvent, HotkeyListener};
use crate::state::{ModelState, SharedState, Sinal, UiAction, View, lock};
use crate::stt::{SttCmd, SttEvent, TranscribeOptions};
use crossbeam_channel::{Receiver, select};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Comandos vindos do socket (ícone do app, atalho do GNOME, terminal).
#[derive(Debug, Clone, Copy)]
pub enum IpcCommand {
    /// Alterna gravar/parar — útil quando não se pode segurar a tecla.
    Toggle,
    Settings,
    Quit,
}

pub struct Channels {
    pub hotkey: Receiver<HotkeyEvent>,
    pub audio: Receiver<AudioEvent>,
    pub stt: Receiver<SttEvent>,
    pub ui: Receiver<UiAction>,
    pub ipc: Receiver<IpcCommand>,
}

pub struct Controller {
    pub shared: SharedState,
    pub sinal: Sinal,
    pub audio: AudioHandle,
    pub stt: crossbeam_channel::Sender<SttCmd>,
    pub hotkey: Arc<HotkeyListener>,
}

impl Controller {
    pub fn run(self, channels: Channels) {
        self.apply_audio_settings();
        self.load_model();

        loop {
            select! {
                recv(channels.hotkey) -> msg => match msg {
                    Ok(event) => self.on_hotkey(event),
                    Err(_) => return self.canal_caiu("atalho"),
                },
                recv(channels.audio) -> msg => match msg {
                    Ok(event) => self.on_audio(event),
                    Err(_) => return self.canal_caiu("áudio"),
                },
                recv(channels.stt) -> msg => match msg {
                    Ok(event) => self.on_stt(event),
                    Err(_) => return self.canal_caiu("transcrição"),
                },
                recv(channels.ui) -> msg => match msg {
                    Ok(action) => self.on_ui(action),
                    Err(_) => return,
                },
                recv(channels.ipc) -> msg => match msg {
                    Ok(command) => self.on_ipc(command),
                    Err(_) => return self.canal_caiu("controle"),
                },
            }
        }
    }

    /// Uma thread de trabalho morreu e levou o canal dela junto.
    ///
    /// Antes isto era um `return` mudo, e o preço era um programa zumbi: o
    /// controlador saía de fininho, a janela ficava em "Transcrevendo…" para
    /// sempre, o atalho e todos os botões paravam de ter efeito, `--encerrar`
    /// respondia "encerrando" sem que nada acontecesse — e o systemd não
    /// reiniciava, porque o processo continuava vivo. Só um `kill` resolvia.
    ///
    /// Encerrar de verdade é melhor: a interface fecha ao ver o `quitting`, o
    /// processo termina, e quem estiver sob o serviço de usuário sobe de novo
    /// inteiro em vez de ficar meio morto.
    fn canal_caiu(&self, qual: &str) {
        log::error!("o canal de {qual} fechou — a thread dele morreu; encerrando");
        let mut state = lock(&self.shared);
        state.quitting = true;
        drop(state);
        self.sinal.mudou();
    }

    // ---------------------------------------------------------------- eventos

    fn on_hotkey(&self, event: HotkeyEvent) {
        match event {
            HotkeyEvent::Down => self.start_recording(),
            HotkeyEvent::Up => self.stop_recording(),

            HotkeyEvent::Captured(keys) => {
                let mut state = lock(&self.shared);
                state.capturing_hotkey = false;
                if !keys.is_empty() {
                    state.draft.hotkey = keys;
                }
                drop(state);
                self.sinal.mudou();
            }

            // O aviso do teclado tem campo próprio e não toma a tela.
            //
            // Tomando `message` e `view` ele disputava o único campo de texto
            // com o aviso do modelo faltando — e os dois nascem no mesmo
            // instante do arranque de uma instalação nova, então o perdedor
            // sumia. Pior: o vencedor era apagado na primeira ação seguinte, e
            // a pessoa ficava achando que o atalho tinha quebrado sozinho.
            HotkeyEvent::Unavailable(message) => {
                log::warn!("atalho global indisponível: {message}");
                let mut state = lock(&self.shared);
                state.aviso_atalho = Some(message);
                drop(state);
                self.sinal.mudou();
            }

            HotkeyEvent::Available => {
                let mut state = lock(&self.shared);
                if state.aviso_atalho.take().is_none() {
                    return;
                }
                drop(state);
                log::info!("atalho global de volta ao ar");
                self.sinal.mudou();
            }
        }
    }

    fn on_audio(&self, event: AudioEvent) {
        match event {
            AudioEvent::Started => {}

            AudioEvent::Failed { ditado, message } => {
                log::warn!("gravação {ditado} falhou: {message}");
                let mut state = lock(&self.shared);
                // Um ditado que já foi substituído por outro não tem mais tela
                // para reclamar.
                if ditado != state.ditado_atual {
                    return;
                }
                state.recording_since = None;
                state.message = format!("Não consegui acessar o microfone: {message}");
                state.erro_e_so_espera = false;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            AudioEvent::Captured {
                ditado,
                samples,
                sample_rate,
                duration_ms,
            } => {
                let (atual, too_short, options) = {
                    let mut state = lock(&self.shared);
                    // Só o ditado mais novo manda na tela e no cronômetro. O
                    // áudio de um anterior ainda é transcrito — ele existe e é
                    // do usuário —, mas em silêncio, por baixo do que estiver
                    // acontecendo agora.
                    let atual = ditado == state.ditado_atual;
                    if atual {
                        // Também fecha a gravação encerrada pelo teto de
                        // duração, que termina sem passar pelo atalho.
                        state.recording_since = None;
                        state.status = format!("{:.1} s de áudio", duration_ms as f64 / 1000.0);
                        // A tela de configurações é intocável: refazer o
                        // rascunho ao reabri-la apagaria o que a pessoa estava
                        // escrevendo neste instante.
                        if state.view != View::Settings {
                            state.view = View::Processing;
                        }
                    }
                    (
                        atual,
                        duration_ms < state.config.min_recording_ms,
                        TranscribeOptions {
                            language: state.config.whisper_language().map(str::to_string),
                            translate: state.config.translate,
                            threads: state.config.threads,
                            initial_prompt: state.config.initial_prompt.clone(),
                            normalize: state.config.normalize_audio,
                        },
                    )
                };

                if too_short {
                    log::debug!("gravação de {duration_ms} ms descartada (curta demais)");
                    if atual {
                        self.voltar_ao_repouso();
                    }
                    return;
                }

                self.sinal.mudou();
                let _ = self.stt.send(SttCmd::Transcribe {
                    ditado,
                    samples,
                    sample_rate,
                    options,
                });
            }
        }
    }

    fn on_stt(&self, event: SttEvent) {
        match event {
            SttEvent::Loading => {
                let mut state = lock(&self.shared);
                state.model = ModelState::Loading;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Ready => {
                let mut state = lock(&self.shared);
                state.model = ModelState::Ready;
                // A tela de erro que era só espera se recolhe sozinha. Quem
                // decide é o campo, e não mais o texto exibido: comparar o
                // prefixo da mensagem deixava a janela presa depois do
                // download, porque aquele caminho escrevia outra frase.
                if state.view == View::Error && state.erro_e_so_espera {
                    state.view = View::Hidden;
                    state.message.clear();
                }
                state.erro_e_so_espera = false;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::LoadFailed(message) => {
                log::error!("o modelo não carregou: {message}");
                let mut state = lock(&self.shared);
                state.model = ModelState::Failed;
                state.message = message;
                state.erro_e_so_espera = false;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Failed { ditado, message } => {
                log::warn!("transcrição {ditado} falhou: {message}");
                let mut state = lock(&self.shared);
                // Mesmo critério do áudio, que faltava aqui: um ditado já
                // substituído não tem mais tela para reclamar, e quem está
                // falando agora manda na janela. Sem isto, uma transcrição que
                // falhasse enquanto a pessoa dita de novo abria a janela de
                // erro sempre-no-topo por cima de quem ainda estava falando.
                // O relato não se perde — ele está no journal, na linha acima.
                if ditado != state.ditado_atual || state.gravando() || state.view == View::Settings
                {
                    return;
                }
                state.message = message;
                state.erro_e_so_espera = false;
                state.view = View::Error;
                drop(state);
                self.sinal.mudou();
            }

            SttEvent::Done {
                ditado,
                text,
                elapsed_ms,
            } => self.on_transcription(ditado, text, elapsed_ms),
        }
    }

    /// Mostrar ou não a janela com o texto, ao terminar de transcrever.
    ///
    /// `a_salvo` é o que manda: só dá para pular a janela se o texto tiver
    /// chegado mesmo à área de transferência, senão a transcrição sumiria sem
    /// ninguém ter visto. Com colagem automática ela nunca aparece — o texto já
    /// foi parar onde o usuário estava digitando; fora isso, quem decide é o
    /// `show_result`, para quem já tem a cópia automática e não quer nada na
    /// frente.
    fn tela_do_resultado(a_salvo: bool, auto_paste: bool, show_result: bool) -> View {
        if a_salvo && (auto_paste || !show_result) {
            View::Hidden
        } else {
            View::Result
        }
    }

    /// Se o resultado pode tomar a janela agora.
    ///
    /// Falar de novo enquanto a frase anterior é transcrita é o uso normal do
    /// programa, e quem está gravando é quem manda na tela: a janela do texto
    /// anterior não pode aparecer por cima de um ditado em andamento. Mexer nas
    /// configurações conta igual — reabrir aquela tela refaz o rascunho, então
    /// uma janela de resultado por cima dela apaga o que a pessoa digitou.
    ///
    /// A exceção, nos dois casos, é a janela ser o único jeito de o texto não se
    /// perder — `tela_do_resultado` devolvendo `Result` —, aí ela aparece assim
    /// mesmo, e o microfone segue aberto por baixo.
    fn resultado_pode_aparecer(tela: View, ocupada: bool) -> bool {
        !ocupada || tela == View::Result
    }

    fn on_transcription(&self, ditado: u64, text: String, elapsed_ms: u128) {
        let (auto_copy, auto_paste, show_result) = {
            let state = lock(&self.shared);
            (
                state.config.auto_copy,
                state.config.auto_paste,
                state.config.show_result,
            )
        };

        let mut copy_error = None;
        if !text.is_empty()
            && (auto_copy || auto_paste)
            && let Err(e) = clipboard::copy(&text)
        {
            log::warn!("não consegui copiar o ditado {ditado}: {e:#}");
            copy_error = Some(format!("{e:#}"));
        }

        {
            let mut state = lock(&self.shared);
            state.status = format!(
                "{:.1} s · {} · {}",
                elapsed_ms as f64 / 1000.0,
                crate::stt::BACKEND,
                if state.config.use_gpu && crate::stt::GPU_CAPABLE {
                    "GPU"
                } else {
                    "CPU"
                }
            );

            // "Ocupada" é quem grava agora e também quem está no meio das
            // configurações: nos dois casos a janela pertence a outra coisa.
            let ocupada = state.gravando() || state.view == View::Settings;
            if text.is_empty() {
                state.text.clear();
                state.message = "Não identifiquei fala no áudio.".to_string();
                state.erro_e_so_espera = false;
                if !ocupada {
                    state.view = View::Error;
                }
            } else {
                state.text = text.clone();
                state.message = copy_error.clone().unwrap_or_default();
                let a_salvo = (auto_copy || auto_paste) && copy_error.is_none();
                state.copied_at = a_salvo.then(Instant::now);
                let tela = Self::tela_do_resultado(a_salvo, auto_paste, show_result);
                if Self::resultado_pode_aparecer(tela, ocupada) {
                    state.view = tela;
                }
                state.result_shown_at = Some(Instant::now());
            }
        }
        self.sinal.mudou();

        if auto_paste && copy_error.is_none() && !text.is_empty() {
            self.colar_depois();
        }
    }

    /// Espera antes de mandar o Ctrl+V.
    ///
    /// A nossa janela precisa sumir primeiro, senão o foco ainda é dela e a
    /// colagem cai no nada — ou, pior, dentro do próprio campo de resultado.
    const ESPERA_ANTES_DE_COLAR: Duration = Duration::from_millis(250);

    /// Cola o que já está na área de transferência, numa thread à parte.
    ///
    /// Só é chamada depois de uma cópia bem-sucedida. Isso é o contrato: colar
    /// sem ter copiado joga na janela do usuário o conteúdo anterior da área de
    /// transferência, que é dele e não tem nada a ver com o que ele acabou de
    /// falar.
    fn colar_depois(&self) {
        let shared = self.shared.clone();
        let sinal = self.sinal.clone();
        let _ = std::thread::Builder::new()
            .name("colar".into())
            .spawn(move || {
                std::thread::sleep(Self::ESPERA_ANTES_DE_COLAR);
                if let Err(e) = clipboard::paste() {
                    log::warn!("copiei, mas a colagem falhou: {e:#}");
                    let mut state = lock(&shared);
                    state.message = format!("Copiei, mas não consegui colar: {e:#}");
                    state.erro_e_so_espera = false;
                    state.view = View::Result;
                    drop(state);
                    sinal.mudou();
                }
            });
    }

    fn on_ui(&self, action: UiAction) {
        match action {
            UiAction::Hide => self.voltar_ao_repouso(),

            UiAction::Copy => self.copy_current(false),
            UiAction::Paste => self.copy_current(true),

            // Abrir as configurações fecha o microfone primeiro.
            //
            // Deixar a gravação correndo por baixo terminava mal dos dois
            // lados: o `Captured` que chega no meio troca a tela e o rascunho
            // digitado se perde ao reabrir; e, se a tela segurasse a troca, o
            // microfone ficaria aberto sem nada na tela dizendo isso, até o
            // teto de duração estourar. O que a pessoa já falou não se perde —
            // `stop_recording` entrega o áudio e ele segue para a transcrição.
            UiAction::OpenSettings => {
                if lock(&self.shared).gravando() {
                    self.stop_recording();
                }
                // A enumeração dos dispositivos abre o ALSA e é a parte cara
                // daqui; fica fora do mutex para não prender a interface e o
                // controlador enquanto isso acontece.
                let devices = crate::audio::list_input_devices();
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.devices = devices;
                state.view = View::Settings;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CloseSettings => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.draft = state.config.clone();
                state.capturing_hotkey = false;
                let repouso = tela_de_repouso(&state);
                state.view = repouso;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::ApplyDraft => self.apply_draft(),

            UiAction::StartHotkeyCapture => {
                self.hotkey.begin_capture();
                let mut state = lock(&self.shared);
                state.capturing_hotkey = true;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::CancelHotkeyCapture => {
                self.hotkey.cancel_capture();
                let mut state = lock(&self.shared);
                state.capturing_hotkey = false;
                drop(state);
                self.sinal.mudou();
            }

            UiAction::ReloadModel => self.load_model(),

            UiAction::DownloadModel => self.download_model(),

            UiAction::CancelDownload => {
                let state = lock(&self.shared);
                if let Some(andamento) = &state.download {
                    andamento
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .cancelado = true;
                }
                drop(state);
                self.sinal.mudou();
            }

            UiAction::Quit => {
                let mut state = lock(&self.shared);
                state.quitting = true;
                drop(state);
                self.sinal.mudou();
            }
        }
    }

    fn on_ipc(&self, command: IpcCommand) {
        match command {
            IpcCommand::Toggle => {
                let recording = lock(&self.shared).recording_since.is_some();
                if recording {
                    self.stop_recording();
                } else {
                    self.start_recording();
                }
            }
            IpcCommand::Settings => self.on_ui(UiAction::OpenSettings),
            IpcCommand::Quit => self.on_ui(UiAction::Quit),
        }
    }

    // ------------------------------------------------------------- gravação

    fn start_recording(&self) {
        let ditado = {
            let mut state = lock(&self.shared);

            // Não atrapalha quem está mexendo nas configurações.
            if state.view == View::Settings || state.capturing_hotkey {
                return;
            }
            // Quem responde por "já estamos ouvindo" é o `recording_since`, e
            // não a tela: a janela do ditado anterior pode ter aparecido por
            // cima. Transcrever o anterior, por outro lado, não impede nada —
            // isso acontece numa thread só dele.
            if state.recording_since.is_some() {
                return;
            }

            match state.model {
                ModelState::Loading => {
                    state.message = "Carregando o modelo, só um instante…".to_string();
                    // Espera, não falha: a tela se recolhe sozinha assim que o
                    // modelo ficar pronto.
                    state.erro_e_so_espera = true;
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Failed => {
                    state.erro_e_so_espera = false;
                    state.view = View::Error;
                    drop(state);
                    self.sinal.mudou();
                    return;
                }
                ModelState::Ready => {}
            }

            state.text.clear();
            state.message.clear();
            state.erro_e_so_espera = false;
            state.copied_at = None;
            state.recording_since = Some(Instant::now());
            state.view = View::Recording;
            state.ditado_atual += 1;
            state.ditado_atual
        };
        self.audio.send(AudioCmd::Start { ditado });
        self.sinal.mudou();
    }

    fn stop_recording(&self) {
        {
            let mut state = lock(&self.shared);
            // De novo o `recording_since` no lugar da tela: olhando a tela, um
            // resultado que tivesse aparecido no meio do ditado faria este
            // `return` acontecer e o microfone ficaria aberto para sempre.
            if state.recording_since.is_none() {
                return;
            }
            state.recording_since = None;
            // As configurações continuam de pé: quem parou a gravação para
            // abri-las não quer a tela trocada por baixo do que está digitando.
            if state.view != View::Settings {
                state.view = View::Processing;
            }
        }
        self.audio.send(AudioCmd::Stop);
        self.sinal.mudou();
    }

    // ---------------------------------------------------------------- apoio

    fn copy_current(&self, then_paste: bool) {
        let text = lock(&self.shared).text.clone();
        if text.is_empty() {
            return;
        }

        // A colagem depende de a cópia ter dado certo, e essa guarda antes só
        // existia no caminho automático. Sem ela, uma cópia falha ainda mandava
        // o Ctrl+V — colando o conteúdo anterior da área de transferência — e a
        // mensagem "Copiei, mas não consegui colar" apagava a verdadeira, "Não
        // consegui copiar": o programa afirmava ter copiado justamente quando
        // não copiou.
        let copiou = match clipboard::copy(&text) {
            Ok(()) => {
                let mut state = lock(&self.shared);
                state.copied_at = Some(Instant::now());
                state.message.clear();
                if then_paste {
                    state.view = View::Hidden;
                }
                true
            }
            Err(e) => {
                log::warn!("cópia manual falhou: {e:#}");
                let mut state = lock(&self.shared);
                state.message = format!("Não consegui copiar: {e:#}");
                false
            }
        };
        self.sinal.mudou();

        if then_paste && copiou {
            self.colar_depois();
        }
    }

    fn apply_draft(&self) {
        let (draft, previous) = {
            let state = lock(&self.shared);
            (state.draft.clone(), state.config.clone())
        };

        if let Err(e) = draft.save() {
            // Sem este registro a falha não deixava rastro em lugar nenhum: a
            // tela de configurações não desenhava `message` (agora desenha) e o
            // journal não recebia uma linha. O Salvar simplesmente não fazia
            // nada, em silêncio.
            log::error!("não consegui gravar a configuração: {e:#}");
            let mut state = lock(&self.shared);
            state.message = format!("Não consegui gravar a configuração: {e:#}");
            drop(state);
            self.sinal.mudou();
            return;
        }

        if draft.hotkey != previous.hotkey {
            self.hotkey.set_target(&draft.hotkey);
        }

        let reload_model =
            draft.model_path != previous.model_path || draft.use_gpu != previous.use_gpu;

        {
            let mut state = lock(&self.shared);
            state.config = draft;
            state.capturing_hotkey = false;
            let repouso = tela_de_repouso(&state);
            state.view = repouso;
            state.message.clear();
        }
        self.hotkey.cancel_capture();
        self.apply_audio_settings();

        if reload_model {
            self.load_model();
        }
        self.sinal.mudou();
    }

    fn apply_audio_settings(&self) {
        let config: Config = lock(&self.shared).config.clone();
        self.audio.send(AudioCmd::Configure(AudioSettings {
            device: config.input_device.clone(),
            max_secs: config.max_recording_secs,
        }));
    }

    fn load_model(&self) {
        let (model_path, use_gpu) = {
            let mut state = lock(&self.shared);
            state.model = ModelState::Loading;
            (state.config.model_path.clone(), state.config.use_gpu)
        };
        self.sinal.mudou();
        let _ = self.stt.send(SttCmd::Load {
            model_path,
            use_gpu,
        });
    }

    /// Baixa o modelo sugerido e, quando ele chegar, passa a usá-lo — inclusive
    /// gravando o caminho na configuração, porque quem clicou no botão não
    /// deveria precisar apontar o arquivo depois.
    fn download_model(&self) {
        {
            let mut state = lock(&self.shared);
            if state
                .download
                .as_ref()
                .is_some_and(|d| d.lock().unwrap_or_else(|e| e.into_inner()).andando())
            {
                return;
            }
            state.message = "Leva alguns minutos, dependendo da conexão. Pode fechar esta \
                             janela — o download continua."
                .to_string();
        }

        let (andamento, pronto) = crate::modelo::baixar(crate::modelo::PADRAO, self.sinal.clone());
        lock(&self.shared).download = Some(andamento);
        self.sinal.mudou();

        let shared = self.shared.clone();
        let sinal = self.sinal.clone();
        let stt = self.stt.clone();
        let vigia = std::thread::Builder::new()
            .name("modelo-pronto".into())
            .spawn(move || {
                let Ok(model_path) = pronto.recv() else {
                    return;
                };
                let use_gpu = {
                    let mut state = lock(&shared);
                    state.config.model_path = model_path.clone();
                    state.draft.model_path = model_path.clone();
                    state.model = ModelState::Loading;
                    state.message = "Modelo baixado; carregando…".to_string();
                    // É espera, não falha — a tela se recolhe sozinha quando o
                    // modelo terminar de carregar. Sem esta marca ela ficava no
                    // ar com o emblema vermelho de erro ao lado desta frase, e
                    // era assim que terminava a primeira execução de quem
                    // acabara de esperar 574 MB.
                    state.erro_e_so_espera = true;
                    if let Err(e) = state.config.save() {
                        log::warn!("modelo baixado, mas não consegui gravar a config: {e:#}");
                    }
                    state.config.use_gpu
                };
                sinal.mudou();
                let _ = stt.send(SttCmd::Load {
                    model_path,
                    use_gpu,
                });
            });
        if let Err(e) = vigia {
            // Sem esta thread o download continua, mas ninguém passa a usar o
            // arquivo quando ele chegar: a barra completa 100 % e nada mais
            // acontece.
            log::error!("não consegui acompanhar o download até o fim: {e}");
        }
    }

    /// Dispensa a tela atual e volta para onde o programa fica quando não tem
    /// nada a dizer.
    fn voltar_ao_repouso(&self) {
        let mut state = lock(&self.shared);
        let repouso = tela_de_repouso(&state);
        state.view = repouso;
        drop(state);
        self.sinal.mudou();
    }
}

/// Para onde a janela volta quando uma tela é dispensada.
///
/// Escondida, no caso normal. Mas se o microfone estiver aberto é a tela da
/// gravação que volta: fechar uma janela não pode deixar a pessoa falando sem
/// nada na tela dizendo que ela está sendo ouvida.
fn tela_de_repouso(state: &crate::state::Shared) -> View {
    if state.gravando() {
        View::Recording
    } else {
        View::Hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioCmd;
    use crate::state::Shared;
    use crossbeam_channel::Receiver;

    /// Um controlador de verdade, sem thread nenhuma.
    ///
    /// Os manipuladores (`on_hotkey`, `on_audio`, `on_stt`, `on_ui`) são
    /// chamados direto: o `run` só existe para escolher de qual canal ler, e o
    /// que quebrou duas vezes na história deste projeto foi a máquina de
    /// estados, não o `select!`.
    struct Bancada {
        controlador: Controller,
        audio: Receiver<AudioCmd>,
        stt: Receiver<SttCmd>,
    }

    impl Bancada {
        fn nova() -> Self {
            // Obrigatório: com a cópia automática ligada — que é o padrão — o
            // desfecho de um ditado mexe na área de transferência da máquina de
            // quem roda `cargo test`, e sem sessão gráfica o arboard trava.
            let config = Config {
                auto_copy: false,
                auto_paste: false,
                ..Config::default()
            };

            let shared: SharedState =
                Arc::new(std::sync::Mutex::new(Shared::new(config, Vec::new())));
            let (audio_tx, audio_rx) = crossbeam_channel::unbounded();
            let (stt_tx, stt_rx) = crossbeam_channel::unbounded();
            let (hotkey_tx, _) = crossbeam_channel::unbounded();

            let mut estado = lock(&shared);
            estado.model = ModelState::Ready;
            drop(estado);

            Self {
                controlador: Controller {
                    shared: shared.clone(),
                    sinal: Sinal::default(),
                    audio: crate::audio::AudioHandle {
                        tx: audio_tx,
                        levels: Default::default(),
                    },
                    stt: stt_tx,
                    hotkey: HotkeyListener::novo(&Config::default().hotkey, hotkey_tx),
                },
                audio: audio_rx,
                stt: stt_rx,
            }
        }

        fn estado(&self) -> std::sync::MutexGuard<'_, Shared> {
            lock(&self.controlador.shared)
        }

        /// Esvazia o que o `run` inicial teria mandado (Configure e Load).
        fn limpar(&self) {
            while self.audio.try_recv().is_ok() {}
            while self.stt.try_recv().is_ok() {}
        }
    }

    #[test]
    fn o_stop_sai_mesmo_com_a_janela_do_resultado_por_cima() {
        // O bug do commit 770d74e: falar de novo enquanto a frase anterior é
        // transcrita é o uso normal, e nesse intervalo a janela do resultado
        // anterior pode aparecer por cima. Decidindo pela tela, o
        // `stop_recording` desistia e o microfone ficava aberto para sempre.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        assert!(b.estado().gravando(), "o microfone devia ter aberto");
        b.limpar();

        // O resultado do ditado anterior toma a tela.
        b.estado().view = View::Result;

        b.controlador.on_hotkey(HotkeyEvent::Up);
        assert!(
            !b.estado().gravando(),
            "o microfone ficou aberto: alguém voltou a decidir pela tela"
        );
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Stop)),
            "o comando de parar não chegou ao áudio"
        );
    }

    #[test]
    fn o_audio_de_um_ditado_atropelado_nao_mexe_na_tela() {
        // Do mesmo commit: o áudio de um ditado antigo ainda é transcrito — ele
        // existe e é do usuário —, mas em silêncio, sem tocar na tela nem no
        // cronômetro de quem está falando agora.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 1
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 2, em curso
        b.limpar();
        assert_eq!(b.estado().ditado_atual, 2);

        let quando = b.estado().recording_since;
        b.controlador.on_audio(AudioEvent::Captured {
            ditado: 1,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        let estado = b.estado();
        assert_eq!(
            estado.view,
            View::Recording,
            "a tela do ditado 2 foi trocada"
        );
        assert_eq!(
            estado.recording_since, quando,
            "o cronômetro foi reiniciado"
        );
        drop(estado);
        assert!(
            matches!(b.stt.try_recv(), Ok(SttCmd::Transcribe { ditado: 1, .. })),
            "o áudio do ditado 1 devia ter sido transcrito assim mesmo"
        );
    }

    #[test]
    fn a_falha_de_uma_transcricao_antiga_nao_toma_a_tela_de_quem_fala_agora() {
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 1
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_hotkey(HotkeyEvent::Down); // ditado 2, em curso
        b.limpar();

        b.controlador.on_stt(SttEvent::Failed {
            ditado: 1,
            message: "deu ruim".to_string(),
        });
        assert_eq!(
            b.estado().view,
            View::Recording,
            "a janela de erro apareceu por cima de um ditado em andamento"
        );

        // Sem ninguém falando, a mesma falha aparece.
        b.controlador.on_hotkey(HotkeyEvent::Up);
        b.controlador.on_stt(SttEvent::Failed {
            ditado: 2,
            message: "deu ruim".to_string(),
        });
        assert_eq!(b.estado().view, View::Error);
    }

    #[test]
    fn a_tela_de_espera_do_modelo_se_recolhe_quando_ele_fica_pronto() {
        // O caminho da primeira execução: baixar → carregar → pronto. A decisão
        // de fechar já foi tomada comparando o prefixo da mensagem exibida, e a
        // frase do download não casava — a tela ficava presa com o emblema de
        // erro ao lado de "Modelo baixado; carregando…".
        let b = Bancada::nova();
        {
            let mut estado = b.estado();
            estado.model = ModelState::Loading;
            estado.message = "Modelo baixado; carregando…".to_string();
            estado.erro_e_so_espera = true;
            estado.view = View::Error;
        }

        b.controlador.on_stt(SttEvent::Ready);

        let estado = b.estado();
        assert_eq!(estado.view, View::Hidden, "a tela de espera ficou presa");
        assert!(estado.message.is_empty());
        assert_eq!(estado.model, ModelState::Ready);
    }

    #[test]
    fn uma_falha_de_verdade_nao_e_apagada_quando_o_modelo_carrega() {
        // O outro lado da mesma moeda: a tela que está no ar por uma falha de
        // verdade continua lá.
        let b = Bancada::nova();
        {
            let mut estado = b.estado();
            estado.message = "Não consegui acessar o microfone".to_string();
            estado.erro_e_so_espera = false;
            estado.view = View::Error;
        }

        b.controlador.on_stt(SttEvent::Ready);
        assert_eq!(b.estado().view, View::Error);
    }

    #[test]
    fn abrir_as_configuracoes_fecha_o_microfone_antes() {
        // Deixar a gravação correndo por baixo das configurações terminava mal
        // dos dois lados: ou o `Captured` trocava a tela e o rascunho digitado
        // se perdia, ou o microfone ficava aberto sem nada dizendo isso.
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        b.limpar();

        b.controlador.on_ui(UiAction::OpenSettings);

        assert!(!b.estado().gravando(), "o microfone continuou aberto");
        assert_eq!(b.estado().view, View::Settings);
        assert!(
            matches!(b.audio.try_recv(), Ok(AudioCmd::Stop)),
            "o áudio já falado precisa ser entregue, não descartado"
        );
    }

    #[test]
    fn o_fim_da_gravacao_nao_atropela_a_tela_de_configuracoes() {
        let b = Bancada::nova();
        b.controlador.on_ui(UiAction::OpenSettings);
        b.limpar();
        // Uma gravação que já estava a caminho termina agora. O número sai
        // antes da chamada: o guard vive até o fim da expressão, e passá-lo
        // dentro dos argumentos travaria o mutex contra o próprio `on_audio`.
        let ditado = {
            let mut estado = b.estado();
            estado.recording_since = Some(Instant::now());
            estado.ditado_atual
        };
        b.controlador.on_audio(AudioEvent::Captured {
            ditado,
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
            duration_ms: 1_000,
        });

        assert_eq!(
            b.estado().view,
            View::Settings,
            "as configurações foram trocadas por baixo de quem digitava"
        );
    }

    #[test]
    fn fechar_uma_janela_com_o_microfone_aberto_volta_para_a_tela_da_gravacao() {
        let b = Bancada::nova();
        b.controlador.on_hotkey(HotkeyEvent::Down);
        b.limpar();
        b.estado().view = View::Result;

        b.controlador.on_ui(UiAction::Hide);

        assert_eq!(
            b.estado().view,
            View::Recording,
            "a pessoa ficou falando sem nada na tela dizendo que estava sendo ouvida"
        );
    }

    /// (a_salvo, auto_paste, show_result) → tela
    fn tela(a_salvo: bool, auto_paste: bool, show_result: bool) -> View {
        Controller::tela_do_resultado(a_salvo, auto_paste, show_result)
    }

    #[test]
    fn sem_a_janela_quando_a_copia_automatica_ja_resolveu() {
        // O caso novo: o texto está na área de transferência e o usuário não
        // quer nada na frente.
        assert_eq!(tela(true, false, false), View::Hidden);
        // Com a janela ligada, ela aparece, que é o padrão.
        assert_eq!(tela(true, false, true), View::Result);
    }

    #[test]
    fn a_colagem_automatica_esconde_a_janela_de_qualquer_jeito() {
        // O texto já foi para onde o usuário estava digitando; mostrar a janela
        // depois disso só atrapalharia.
        assert_eq!(tela(true, true, true), View::Hidden);
        assert_eq!(tela(true, true, false), View::Hidden);
    }

    #[test]
    fn um_ditado_em_andamento_fica_com_a_tela() {
        // Falar de novo enquanto a frase anterior é transcrita: a janela do
        // texto anterior não pode aparecer por cima de quem está falando…
        assert!(!Controller::resultado_pode_aparecer(View::Hidden, true));
        // …a menos que ela seja o único jeito de o texto não se perder.
        assert!(Controller::resultado_pode_aparecer(View::Result, true));
        // Sem ninguém gravando, as duas telas valem como sempre.
        assert!(Controller::resultado_pode_aparecer(View::Hidden, false));
        assert!(Controller::resultado_pode_aparecer(View::Result, false));
    }

    #[test]
    fn a_janela_aparece_quando_o_texto_nao_esta_a_salvo() {
        // Nada foi para a área de transferência (cópia desligada, ou a cópia
        // falhou): a janela é o único jeito de pegar o texto, então ela aparece
        // mesmo com todas as chaves pedindo o contrário.
        for auto_paste in [false, true] {
            for show_result in [false, true] {
                assert_eq!(
                    tela(false, auto_paste, show_result),
                    View::Result,
                    "auto_paste={auto_paste} show_result={show_result}"
                );
            }
        }
    }
}
