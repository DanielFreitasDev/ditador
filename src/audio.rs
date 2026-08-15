//! Captura do microfone com cpal, reduzida a mono.
//!
//! O áudio sai daqui na taxa do próprio dispositivo, e não nos 16 kHz que o
//! Whisper exige: reamostrar custa uma centena de multiplicações por amostra de
//! saída, e fazer isso aqui prenderia a thread que atende os comandos — que é a
//! mesma que precisa estar pronta para abrir o microfone de novo. Falar outra
//! frase enquanto a anterior é transcrita é o uso normal do programa, então a
//! conversão foi para o lado de quem transcreve (ver `stt.rs`), que já ia
//! esperar mesmo.

use crate::config::WHISPER_SAMPLE_RATE;
use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Quantas amostras de nível guardamos para desenhar a animação.
pub const LEVEL_HISTORY: usize = 36;

pub type Levels = Arc<Mutex<VecDeque<f32>>>;

#[derive(Debug, Clone)]
pub struct AudioSettings {
    pub device: Option<String>,
    pub max_secs: u64,
}

#[derive(Debug)]
pub enum AudioCmd {
    Configure(AudioSettings),
    /// `ditado` é o número que o controlador deu a esta gravação; ele volta nos
    /// eventos para que uma gravação antiga não seja confundida com a atual.
    Start {
        ditado: u64,
    },
    Stop,
}

#[derive(Debug)]
pub enum AudioEvent {
    Started,
    Captured {
        ditado: u64,
        /// Mono, na taxa do dispositivo — quem reamostra é o `stt`.
        samples: Vec<f32>,
        sample_rate: u32,
        duration_ms: u64,
    },
    Failed {
        ditado: u64,
        message: String,
    },
}

pub struct AudioHandle {
    pub tx: Sender<AudioCmd>,
    pub levels: Levels,
}

impl AudioHandle {
    pub fn send(&self, cmd: AudioCmd) {
        let _ = self.tx.send(cmd);
    }
}

pub fn spawn(settings: AudioSettings, events: Sender<AudioEvent>) -> AudioHandle {
    let (tx, rx) = crossbeam_channel::unbounded();
    let levels: Levels = Arc::new(Mutex::new(VecDeque::with_capacity(LEVEL_HISTORY)));

    let thread_levels = levels.clone();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || run(settings, rx, events, thread_levels))
        .expect("spawn audio thread");

    AudioHandle { tx, levels }
}

/// Lista os microfones disponíveis, para o seletor das configurações.
///
/// Os nomes repetidos são desfeitos aqui. O ALSA anuncia o mesmo microfone em
/// vários PCMs — `hw:`, `plughw:`, `sysdefault:`, `dsnoop:` — e a primeira linha
/// da descrição é idêntica em todos: num aparelho comum saíam sete entradas
/// iguais na lista, todas marcadas como escolhidas ao mesmo tempo. Pior, como o
/// que se grava na configuração é o nome, a busca sempre recaía sobre a
/// primeira da enumeração, que costuma ser o `hw:` cru — acesso exclusivo, sem
/// conversão de taxa — e não havia como escolher a que funcionaria.
pub fn list_input_devices() -> Vec<String> {
    rotular(cpal::default_host().input_devices().ok())
        .into_iter()
        .map(|(rotulo, _)| rotulo)
        .collect()
}

/// Acha o dispositivo pelo rótulo que a configuração guardou.
///
/// Se o rótulo exato não estiver mais lá, vale o nome sem o sufixo de desempate:
/// o ALSA renumera os PCMs entre sessões (`hw:CARD=2` hoje, `hw:CARD=Generic_1`
/// amanhã), e é o mesmo microfone. Melhor gravar no aparelho certo por um
/// caminho diferente do que dizer que ele sumiu.
fn achar_dispositivo(host: &cpal::Host, procurado: &str) -> Option<cpal::Device> {
    let candidatos = rotular(host.input_devices().ok());
    if let Some((_, device)) = candidatos.iter().find(|(rotulo, _)| rotulo == procurado) {
        return Some(device.clone());
    }

    let base = procurado
        .split_once(" (")
        .map_or(procurado, |(nome, _)| nome);
    let achado = candidatos
        .iter()
        .find(|(rotulo, _)| rotulo.split_once(" (").map_or(rotulo.as_str(), |(n, _)| n) == base);
    if let Some((rotulo, device)) = achado {
        log::info!("o microfone \"{procurado}\" mudou de endereço; usando \"{rotulo}\"");
        return Some(device.clone());
    }
    None
}

/// Dá a cada microfone um rótulo único, na ordem em que o cpal os anuncia.
///
/// A lista e a busca passam as duas por aqui de propósito: se as duas regras
/// vivessem separadas, o dia em que uma mudasse a configuração de alguém
/// passaria a apontar para um dispositivo que a tela nunca mostrou.
///
/// O primeiro de cada nome fica com o nome limpo, sem sufixo, porque é ele que
/// já está gravado nas configurações de quem usa o programa hoje.
fn rotular(devices: Option<impl Iterator<Item = cpal::Device>>) -> Vec<(String, cpal::Device)> {
    let Some(devices) = devices else {
        log::warn!("não consegui listar os microfones do sistema");
        return Vec::new();
    };

    let mut saida: Vec<(String, cpal::Device)> = Vec::new();
    let mut vistos: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for device in devices {
        let Some(descricao) = device.description().ok() else {
            continue;
        };
        let base = descricao.name().to_string();
        let quantos = vistos.entry(base.clone()).or_insert(0);
        *quantos += 1;
        let rotulo = match (*quantos, descricao.driver()) {
            (1, _) => base,
            // No ALSA o `driver` é o identificador do PCM (`hw:0,0`,
            // `plughw:0,0`, `dsnoop:…`), que é exatamente o que distingue as
            // sete entradas idênticas de um mesmo microfone.
            (_, Some(pcm)) if !pcm.is_empty() => format!("{base} ({pcm})"),
            (n, _) => format!("{base} #{n}"),
        };
        saida.push((rotulo, device));
    }
    saida
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

/// De quanto em quanto tempo a thread acorda, enquanto grava, para conferir se
/// a gravação já bateu o teto de duração.
const RONDA: Duration = Duration::from_millis(200);

struct Recording {
    // O stream precisa continuar vivo enquanto gravamos; ele nunca sai desta thread.
    _stream: cpal::Stream,
    ditado: u64,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    max_samples: usize,
    /// O dispositivo sumiu no meio da gravação.
    ///
    /// Marcada pelo callback de erro do cpal e lida pela ronda de `run`. É por
    /// bandeira, e não por evento mandado dali de dentro, porque quem precisa
    /// largar o `Recording` é a thread de comandos: um `Failed` vindo do
    /// callback zeraria o `recording_since` do controlador com o stream ainda
    /// de pé, o `stop_recording` seguinte sairia cedo, o `Stop` nunca chegaria
    /// aqui — e todo ditado depois disso ficaria preso em "Transcrevendo…".
    perdido: Arc<AtomicBool>,
}

impl Recording {
    /// Bateu o teto de duração.
    fn cheia(&self) -> bool {
        lock(&self.buffer).len() >= self.max_samples
    }

    /// O microfone sumiu.
    fn perdeu_o_dispositivo(&self) -> bool {
        self.perdido.load(Ordering::Relaxed)
    }

    /// Quanto tempo de áudio já foi capturado.
    fn duracao_ms(&self) -> u64 {
        lock(&self.buffer).len() as u64 * 1000 / self.sample_rate.max(1) as u64
    }
}

fn run(
    mut settings: AudioSettings,
    rx: Receiver<AudioCmd>,
    events: Sender<AudioEvent>,
    levels: Levels,
) {
    let mut recording: Option<Recording> = None;

    loop {
        // Parada só enquanto grava: o teto de duração precisa ser conferido de
        // tempos em tempos, senão o buffer cheio pararia de aceitar amostras e
        // a gravação seguiria de olhos abertos, sem gravar nada, até alguém
        // soltar a tecla.
        let cmd = if recording.is_some() {
            match rx.recv_timeout(RONDA) {
                Ok(cmd) => Some(cmd),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => return,
            }
        };

        match cmd {
            Some(AudioCmd::Configure(new)) => settings = new,

            Some(AudioCmd::Start { ditado }) => {
                if recording.is_some() {
                    continue;
                }
                clear(&levels);
                match start(ditado, &settings, &levels) {
                    Ok(rec) => {
                        recording = Some(rec);
                        let _ = events.send(AudioEvent::Started);
                    }
                    Err(e) => {
                        // O erro cru primeiro, a ajuda depois: o texto do
                        // sistema é o que se procura numa busca, e a frase da
                        // plataforma é o que se faz a respeito. No Linux não há
                        // ajuda a acrescentar e a mensagem sai como sempre saiu.
                        let cru = format!("{e:#}");
                        let message = match crate::plataforma::microfone::explicar_falha(&cru) {
                            Some(ajuda) => format!("{cru}\n\n{ajuda}"),
                            None => cru,
                        };
                        let _ = events.send(AudioEvent::Failed { ditado, message });
                    }
                }
            }

            Some(AudioCmd::Stop) => {
                if let Some(rec) = recording.take() {
                    entregar(rec, &levels, &events);
                }
            }

            // A ronda: nada chegou pelo canal.
            None => {
                let Some(rec) = &recording else { continue };
                if rec.perdeu_o_dispositivo() {
                    // O microfone sumiu no meio da frase. Antes disto o
                    // callback de erro só escrevia no log: o `Recording`
                    // continuava de pé, o buffer congelava, e como ele nunca
                    // mais encheria o teto de duração também não servia de
                    // rede. A tela seguia dizendo "Ouvindo" com o cronômetro
                    // correndo, e o pedaço já gravado morria calado no filtro
                    // de duração mínima.
                    let rec = recording.take().expect("acabou de ser conferido");
                    let ditado = rec.ditado;
                    let tinha = rec.duracao_ms();
                    log::warn!("o microfone sumiu no meio da gravação ({tinha} ms capturados)");
                    // O que já foi falado não se perde: se dá uma frase, vai
                    // para a transcrição; se não dá, aí sim é uma falha para
                    // contar na tela.
                    if tinha >= AVULSO_MINIMO_MS {
                        entregar(rec, &levels, &events);
                    } else {
                        drop(rec);
                        clear(&levels);
                        let _ = events.send(AudioEvent::Failed {
                            ditado,
                            message: "o microfone foi desconectado".to_string(),
                        });
                    }
                } else if rec.cheia() {
                    let rec = recording.take().expect("acabou de ser conferido");
                    log::info!(
                        "teto de {} s atingido; encerrando a gravação",
                        settings.max_secs
                    );
                    entregar(rec, &levels, &events);
                }
            }
        }
    }
}

/// Abaixo disto, um ditado interrompido não vale a pena mandar para o Whisper —
/// é menos que uma palavra, e o modelo devolveria alucinação.
const AVULSO_MINIMO_MS: u64 = 400;

/// Fecha a gravação e manda o áudio, ainda na taxa do dispositivo, para quem
/// transcreve. A conversão para 16 kHz acontece lá (ver o bloco `//!`).
fn entregar(rec: Recording, levels: &Levels, events: &Sender<AudioEvent>) {
    let ditado = rec.ditado;
    let sample_rate = rec.sample_rate;
    let buffer = rec.buffer.clone();
    // Fecha o stream antes de ler: garante que nenhum callback ainda escreve.
    drop(rec);
    clear(levels);

    let samples = std::mem::take(&mut *lock(&buffer));
    // A duração sai da contagem de amostras, e não do relógio: ao bater o teto
    // a gravação termina antes de a tecla ser solta, e o relógio contaria um
    // tempo de áudio que não existe.
    let duration_ms = samples.len() as u64 * 1000 / sample_rate.max(1) as u64;

    let _ = events.send(AudioEvent::Captured {
        ditado,
        samples,
        sample_rate,
        duration_ms,
    });
}

fn start(ditado: u64, settings: &AudioSettings, levels: &Levels) -> Result<Recording> {
    let host = cpal::default_host();

    let device = match &settings.device {
        Some(name) => achar_dispositivo(&host, name)
            .ok_or_else(|| anyhow!("microfone \"{name}\" não encontrado"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("nenhum microfone padrão disponível"))?,
    };

    let (config, sample_format) = pick_config(&device)?;
    let sample_rate = config.sample_rate;
    let channels = config.channels as usize;
    let max_samples = (settings.max_secs.max(1) as usize) * sample_rate as usize;

    log::info!(
        "gravando em {} — {} Hz, {} canal(is), {:?}",
        device_name(&device).unwrap_or_else(|| "?".into()),
        sample_rate,
        channels,
        sample_format
    );

    // O buffer nasce já do tamanho do teto de duração. Crescer sob demanda
    // significava `realloc` — com cópia de tudo — dentro do callback de áudio,
    // que roda em tempo real e não pode esperar o alocador. Ao teto padrão de
    // 120 s isso é meio megabyte, reservado enquanto o microfone está aberto.
    let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(max_samples)));
    let perdido = Arc::new(AtomicBool::new(false));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build::<f32>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        cpal::SampleFormat::I16 => build::<i16>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        cpal::SampleFormat::I32 => build::<i32>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        cpal::SampleFormat::I8 => build::<i8>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        cpal::SampleFormat::U8 => build::<u8>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        cpal::SampleFormat::U16 => build::<u16>(
            &device,
            &config,
            &buffer,
            levels,
            &perdido,
            channels,
            max_samples,
        ),
        other => return Err(anyhow!("formato de amostra não suportado: {other:?}")),
    }?;

    stream.play()?;

    Ok(Recording {
        _stream: stream,
        ditado,
        buffer,
        sample_rate,
        max_samples,
        perdido,
    })
}

/// Prefere 16 kHz nativo (evita reamostrar), no melhor formato de amostra
/// disponível; cai para a configuração padrão do dispositivo.
fn pick_config(device: &cpal::Device) -> Result<(cpal::StreamConfig, cpal::SampleFormat)> {
    if let Ok(ranges) = device.supported_input_configs() {
        let mut candidates: Vec<_> = ranges
            .filter(|r| {
                r.min_sample_rate() <= WHISPER_SAMPLE_RATE
                    && r.max_sample_rate() >= WHISPER_SAMPLE_RATE
            })
            .collect();
        // Qualidade da amostra primeiro; entre iguais, menos canais.
        candidates.sort_by_key(|r| (format_rank(r.sample_format()), r.channels()));
        if let Some(range) = candidates.into_iter().next() {
            let format = range.sample_format();
            let supported = range.with_sample_rate(WHISPER_SAMPLE_RATE);
            return Ok((supported.into(), format));
        }
    }

    let default = device.default_input_config()?;
    let format = default.sample_format();
    Ok((default.into(), format))
}

/// Menor é melhor. O ALSA costuma anunciar U8 junto com formatos bons, e pegar
/// o primeiro da lista jogaria a gravação para 8 bits sem necessidade.
fn format_rank(format: cpal::SampleFormat) -> u8 {
    match format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I32 => 1,
        cpal::SampleFormat::I16 => 2,
        cpal::SampleFormat::U16 => 3,
        cpal::SampleFormat::I8 => 4,
        cpal::SampleFormat::U8 => 5,
        _ => 6,
    }
}

#[allow(clippy::too_many_arguments)]
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: &Arc<Mutex<Vec<f32>>>,
    levels: &Levels,
    perdido: &Arc<AtomicBool>,
    channels: usize,
    max_samples: usize,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let buffer = buffer.clone();
    let levels = levels.clone();
    let perdido = perdido.clone();
    let channels = channels.max(1);

    let stream = device.build_input_stream(
        *config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut peak = 0.0f32;
            {
                let mut buf = lock(&buffer);
                // Teto batido: para de acumular e deixa a ronda de `run`
                // encerrar a gravação, o que acontece na volta seguinte dela.
                if buf.len() >= max_samples {
                    return;
                }
                for frame in data.chunks(channels) {
                    let mut sum = 0.0f32;
                    for sample in frame {
                        sum += sample.to_sample::<f32>();
                    }
                    let mono = sum / frame.len() as f32;
                    peak = peak.max(mono.abs());
                    buf.push(mono);
                }
            }
            let mut lv = lock(&levels);
            if lv.len() >= LEVEL_HISTORY {
                lv.pop_front();
            }
            lv.push_back(peak);
        },
        move |err| {
            // `Xrun` é rotina: o cpal reprepara o dispositivo sozinho e a
            // gravação continua. Estes dois não — no ALSA o worker chama este
            // callback e faz `return`, então a thread do stream morre ali e
            // nenhuma amostra chega mais. Filtrar importa: marcar a bandeira
            // num Xrun encerraria a gravação a cada engasgo do sistema.
            if matches!(
                err.kind(),
                cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated
            ) {
                log::warn!("o microfone deixou de responder: {err}");
                perdido.store(true, Ordering::Relaxed);
            } else {
                log::warn!("erro no stream de entrada: {err}");
            }
        },
        None,
    )?;

    Ok(stream)
}

fn clear(levels: &Levels) {
    lock(levels).clear();
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_melhor_formato_de_amostra_ganha_do_pior() {
        // Esta ordem existe porque o ALSA anuncia U8 junto com formatos bons, e
        // pegar o primeiro da lista jogaria a gravação para 8 bits sem
        // necessidade — algo que ninguém percebe até ouvir o resultado.
        let mut oferta = [
            cpal::SampleFormat::U8,
            cpal::SampleFormat::I16,
            cpal::SampleFormat::F32,
            cpal::SampleFormat::I8,
        ];
        oferta.sort_by_key(|f| format_rank(*f));
        assert_eq!(oferta[0], cpal::SampleFormat::F32);
        assert_eq!(oferta[oferta.len() - 1], cpal::SampleFormat::U8);
        // Um formato que não conhecemos vai para o fim da fila, nunca para o
        // começo.
        assert!(format_rank(cpal::SampleFormat::F64) >= format_rank(cpal::SampleFormat::U8));
    }
}
