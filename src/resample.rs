//! Reamostragem para 16 kHz (taxa exigida pelo Whisper).
//!
//! Usa um sinc janelado (Blackman) com tabela pré-computada. O filtro faz o
//! anti-aliasing necessário ao reduzir de 44.1/48 kHz para 16 kHz — decimar sem
//! filtrar dobraria as frequências altas de volta na banda de voz.

use std::f64::consts::PI;

/// Zeros de cada lado do sinc. Mais zeros = transição mais nítida e mais custo.
const ZERO_CROSSINGS: f64 = 16.0;
/// Pontos de tabela por amostra de entrada.
const TABLE_DENSITY: f64 = 128.0;

pub fn resample(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if input.is_empty() || src_rate == 0 || dst_rate == 0 {
        return Vec::new();
    }
    if src_rate == dst_rate {
        return input.to_vec();
    }

    let ratio = dst_rate as f64 / src_rate as f64;
    // Frequência de corte em ciclos por amostra de entrada. Ao reduzir a taxa,
    // o corte acompanha a nova Nyquist; a margem de 0.92 evita o joelho do filtro.
    let cutoff = 0.5 * ratio.min(1.0) * 0.92;
    let half_width = (ZERO_CROSSINGS / (2.0 * cutoff)).ceil();

    let kernel = Kernel::new(cutoff, half_width);
    let step = 1.0 / ratio;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    let last_index = input.len() - 1;
    for n in 0..out_len {
        let center = n as f64 * step;
        let first = (center - half_width).ceil() as i64;
        let last = (center + half_width).floor() as i64;

        let mut acc = 0.0f64;
        for i in first..=last {
            // Fora dos limites, repetimos a borda: evita o "clique" que o
            // preenchimento com zeros produziria no começo e no fim.
            let idx = i.clamp(0, last_index as i64) as usize;
            acc += input[idx] as f64 * kernel.at(i as f64 - center);
        }
        out.push((acc * 2.0 * cutoff) as f32);
    }

    out
}

struct Kernel {
    table: Vec<f64>,
    half_width: f64,
}

impl Kernel {
    fn new(cutoff: f64, half_width: f64) -> Self {
        let len = (2.0 * half_width * TABLE_DENSITY).ceil() as usize + 2;
        let mut table = Vec::with_capacity(len);
        for i in 0..len {
            let t = i as f64 / TABLE_DENSITY - half_width;
            table.push(sinc(2.0 * cutoff * t) * blackman(t / half_width));
        }
        Self { table, half_width }
    }

    /// Valor do núcleo em `t` (em amostras de entrada), com interpolação linear.
    fn at(&self, t: f64) -> f64 {
        let pos = (t + self.half_width) * TABLE_DENSITY;
        if pos < 0.0 {
            return 0.0;
        }
        let i = pos as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let frac = pos - i as f64;
        self.table[i] * (1.0 - frac) + self.table[i + 1] * frac
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Janela de Blackman definida em t ∈ [-1, 1].
fn blackman(t: f64) -> f64 {
    if t.abs() > 1.0 {
        return 0.0;
    }
    0.42 + 0.5 * (PI * t).cos() + 0.08 * (2.0 * PI * t).cos()
}

/// Ajusta o ganho para que o pico fique próximo de 0.9, ajudando microfones
/// fracos. O ganho é limitado para não transformar ruído de fundo em "voz".
pub fn normalize(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak < 1e-4 || peak >= 0.9 {
        return;
    }
    let gain = (0.9 / peak).min(10.0);
    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserva_taxa_igual() {
        let input = vec![0.1, -0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn reduz_o_comprimento_na_proporcao() {
        let input = vec![0.0f32; 48_000];
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn mantem_amplitude_de_um_tom_de_voz() {
        // Seno de 440 Hz a 48 kHz deve sobreviver à redução para 16 kHz.
        let src = 48_000u32;
        let input: Vec<f32> = (0..src)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / src as f64).sin() as f32)
            .collect();
        let out = resample(&input, src, 16_000);
        // Ignora as bordas, onde a janela ainda está entrando.
        let peak = out[2000..out.len() - 2000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.9, "pico caiu demais: {peak}");
        assert!(peak < 1.1, "pico estourou: {peak}");
    }

    #[test]
    fn remove_frequencia_acima_da_nova_nyquist() {
        // 15 kHz não cabe em 16 kHz de taxa (Nyquist 8 kHz): deve ser filtrado,
        // não rebatido para dentro da banda.
        let src = 48_000u32;
        let input: Vec<f32> = (0..src)
            .map(|i| (2.0 * PI * 15_000.0 * i as f64 / src as f64).sin() as f32)
            .collect();
        let out = resample(&input, src, 16_000);
        let peak = out[2000..out.len() - 2000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.05, "aliasing não foi removido: {peak}");
    }
}
