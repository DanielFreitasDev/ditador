//! Captura do microfone com cpal, convertida para mono 16 kHz.

use crate::config::WHISPER_SAMPLE_RATE;
use crate::resample;
use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Quantas amostras de nível guardamos para desenhar a animação.
pub const LEVEL_HISTORY: usize = 36;

pub type Levels = Arc<Mutex<VecDeque<f32>>>;

#[derive(Debug, Clone)]
pub struct AudioSettings {
    pub device: Option<String>,
    pub max_secs: u64,
    pub normalize: bool,
}

#[derive(Debug)]
pub enum AudioCmd {
    Configure(AudioSettings),
    Start,
    Stop,
}

#[derive(Debug)]
pub enum AudioEvent {
    Started,
    Captured { samples: Vec<f32>, duration_ms: u64 },
    Failed(String),
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
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| device_name(&d)).collect(),
        Err(e) => {
            log::warn!("não consegui listar microfones: {e}");
            Vec::new()
        }
    }
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|d| d.name().to_string())
}

struct Recording {
    // O stream precisa continuar vivo enquanto gravamos; ele nunca sai desta thread.
    _stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    started: Instant,
}

fn run(
    mut settings: AudioSettings,
    rx: Receiver<AudioCmd>,
    events: Sender<AudioEvent>,
    levels: Levels,
) {
    let mut recording: Option<Recording> = None;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AudioCmd::Configure(new) => settings = new,

            AudioCmd::Start => {
                if recording.is_some() {
                    continue;
                }
                clear(&levels);
                match start(&settings, &levels) {
                    Ok(rec) => {
                        recording = Some(rec);
                        let _ = events.send(AudioEvent::Started);
                    }
                    Err(e) => {
                        let _ = events.send(AudioEvent::Failed(format!("{e:#}")));
                    }
                }
            }

            AudioCmd::Stop => {
                let Some(rec) = recording.take() else { continue };
                let duration_ms = rec.started.elapsed().as_millis() as u64;
                let sample_rate = rec.sample_rate;
                let buffer = rec.buffer.clone();
                // Fecha o stream antes de ler: garante que nenhum callback ainda escreve.
                drop(rec);
                clear(&levels);

                let raw = std::mem::take(&mut *lock(&buffer));
                let mut samples = resample::resample(&raw, sample_rate, WHISPER_SAMPLE_RATE);
                if settings.normalize {
                    resample::normalize(&mut samples);
                }
                let _ = events.send(AudioEvent::Captured {
                    samples,
                    duration_ms,
                });
            }
        }
    }
}

fn start(settings: &AudioSettings, levels: &Levels) -> Result<Recording> {
    let host = cpal::default_host();

    let device = match &settings.device {
        Some(name) => host
            .input_devices()?
            .find(|d| device_name(d).as_deref() == Some(name.as_str()))
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

    let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
        sample_rate as usize * 4,
    )));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            build::<f32>(&device, &config, &buffer, levels, channels, max_samples)
        }
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, &config, &buffer, levels, channels, max_samples)
        }
        cpal::SampleFormat::I32 => {
            build::<i32>(&device, &config, &buffer, levels, channels, max_samples)
        }
        cpal::SampleFormat::I8 => {
            build::<i8>(&device, &config, &buffer, levels, channels, max_samples)
        }
        cpal::SampleFormat::U8 => {
            build::<u8>(&device, &config, &buffer, levels, channels, max_samples)
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, &config, &buffer, levels, channels, max_samples)
        }
        other => return Err(anyhow!("formato de amostra não suportado: {other:?}")),
    }?;

    stream.play()?;

    Ok(Recording {
        _stream: stream,
        buffer,
        sample_rate,
        started: Instant::now(),
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

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: &Arc<Mutex<Vec<f32>>>,
    levels: &Levels,
    channels: usize,
    max_samples: usize,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let buffer = buffer.clone();
    let levels = levels.clone();
    let channels = channels.max(1);

    let stream = device.build_input_stream(
        config.clone(),
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut peak = 0.0f32;
            {
                let mut buf = lock(&buffer);
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
        |err| log::warn!("erro no stream de entrada: {err}"),
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
