//! Avisos sonoros de início, fim e falha.
//!
//! ## Por que eles existem
//!
//! Este programa já oferecia dois modos em que **nada aparece na tela**: cópia
//! automática com a janela de resultado desligada, e a extensão do GNOME no ar,
//! que recolhe a sobreposição de gravação. Nos dois, quem segura a tecla e fala
//! não tem confirmação nenhuma de que foi ouvido — e descobre que o atalho não
//! pegou depois de ter falado a frase inteira. Um "plim" resolve isso pelo
//! ouvido, que é o canal que está livre justamente quando os olhos estão no
//! texto que se está escrevendo.
//!
//! ## Por que os sons são sintetizados, e não arquivos
//!
//! São dois bipes de menos de duzentos milissegundos. Embuti-los como WAV
//! custaria arquivos versionados, um decodificador de cabeçalho — ou uma
//! biblioteca de áudio inteira, que é o que os programas parecidos fazem — e uma
//! régua de volume que ainda precisaria multiplicar as amostras no fim. Gerá-los
//! aqui custa uma função de sessenta linhas, dá o volume de graça e mantém o
//! `cpal`, que já está no projeto para gravar, como a única dependência de áudio.
//!
//! ## O formato dos sons
//!
//! Dois tons curtos em sequência, subindo para começar e descendo para
//! terminar — a convenção que todo gravador de voz usa, e que se entende sem
//! nunca ter lido a documentação. A falha é grave e repetida, que é o outro
//! vocabulário universal. Cada tom tem ataque e queda suaves: uma onda que
//! começa e acaba em amplitude cheia estala no alto-falante, e o estalo é mais
//! audível que o próprio tom.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Qual aviso tocar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Som {
    /// O microfone abriu: pode falar.
    Inicio,
    /// A gravação terminou e foi para a transcrição.
    Fim,
    /// Alguma coisa deu errado — microfone que não abriu, transcrição que
    /// falhou. É o aviso que mais importa nos modos sem tela.
    Falha,
    /// O ditado foi descartado a pedido (ver `atalho_de_cancelar`).
    ///
    /// Tem som próprio, e não o de falha, porque cancelar é uma coisa que deu
    /// certo: quem apertou a tecla quer confirmação, não alarme.
    Cancelado,
}

impl Som {
    /// Os tons, em hertz, e quanto cada um dura em milissegundos.
    ///
    /// As frequências ficam entre 500 e 1200 Hz de propósito: é onde o ouvido é
    /// mais sensível e onde um alto-falante de notebook ainda reproduz sem
    /// distorcer. Abaixo disso o bipe some no laptop; acima, incomoda.
    fn tons(self) -> &'static [(f32, u32)] {
        match self {
            Som::Inicio => &[(660.0, 70), (990.0, 90)],
            Som::Fim => &[(990.0, 70), (660.0, 90)],
            Som::Falha => &[(520.0, 110), (390.0, 160)],
            Som::Cancelado => &[(760.0, 60), (570.0, 70)],
        }
    }
}

/// Toca o aviso, sem bloquear quem chamou.
///
/// Nunca falha para o chamador: abrir o dispositivo de saída depende de haver
/// um, de o PulseAudio estar de pé e de o usuário não ter tirado o fone da
/// tomada no instante errado. Nada disso é motivo para atrapalhar um ditado, e
/// por isso tudo que der errado aqui vira uma linha de log e mais nada.
pub fn tocar(som: Som, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let volume = volume.clamp(0.0, 1.0);

    // Uma thread por aviso, e curta. O `cpal` exige que o stream continue vivo
    // enquanto toca, e prender o controlador por duzentos milissegundos a cada
    // ditado seria pagar em latência do atalho o que se ganha em confirmação.
    let nascida = std::thread::Builder::new()
        .name("som".into())
        .spawn(move || {
            if let Err(e) = reproduzir(som, volume) {
                log::debug!("não consegui tocar o aviso sonoro: {e:#}");
            }
        });
    if let Err(e) = nascida {
        log::debug!("não consegui abrir a thread do aviso sonoro: {e}");
    }
}

fn reproduzir(som: Som, volume: f32) -> anyhow::Result<()> {
    use anyhow::{Context, anyhow};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("nenhuma saída de áudio padrão"))?;
    let suportada = device
        .default_output_config()
        .context("lendo a configuração de saída")?;
    let formato = suportada.sample_format();
    let config: cpal::StreamConfig = suportada.into();

    let amostras = sintetizar(som, config.sample_rate, volume);
    let duracao = std::time::Duration::from_secs_f32(
        amostras.len() as f32 / config.sample_rate.max(1) as f32,
    );
    let canais = config.channels.max(1) as usize;

    let stream = match formato {
        cpal::SampleFormat::F32 => montar::<f32>(&device, &config, amostras, canais),
        cpal::SampleFormat::I16 => montar::<i16>(&device, &config, amostras, canais),
        cpal::SampleFormat::I32 => montar::<i32>(&device, &config, amostras, canais),
        cpal::SampleFormat::U16 => montar::<u16>(&device, &config, amostras, canais),
        cpal::SampleFormat::I8 => montar::<i8>(&device, &config, amostras, canais),
        cpal::SampleFormat::U8 => montar::<u8>(&device, &config, amostras, canais),
        outro => return Err(anyhow!("formato de saída não suportado: {outro:?}")),
    }?;
    stream.play().context("iniciando a reprodução")?;

    // A folga cobre o buffer que o driver ainda não consumiu: soltando o stream
    // no instante da última amostra, o fim do bipe é cortado.
    std::thread::sleep(duracao + std::time::Duration::from_millis(120));
    drop(stream);
    Ok(())
}

fn montar<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    amostras: Vec<f32>,
    canais: usize,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut posicao = 0usize;
    let stream = device.build_output_stream(
        *config,
        move |saida: &mut [T], _: &cpal::OutputCallbackInfo| {
            for quadro in saida.chunks_mut(canais) {
                // Depois da última amostra vem silêncio, e não o começo do
                // buffer de novo: sem isto o bipe ficaria em laço até o stream
                // ser solto.
                let valor = amostras.get(posicao).copied().unwrap_or(0.0);
                posicao += 1;
                for canal in quadro.iter_mut() {
                    *canal = T::from_sample(valor);
                }
            }
        },
        move |err| log::debug!("erro no stream de saída: {err}"),
        None,
    )?;
    Ok(stream)
}

/// Gera as amostras do aviso, em mono, na taxa pedida.
///
/// Separada da reprodução porque é a única parte testável: abrir o dispositivo
/// depende de haver um, e não há num agente de integração contínua.
fn sintetizar(som: Som, taxa: u32, volume: f32) -> Vec<f32> {
    /// Quanto do tom sobe e quanto desce, em segundos. Uma onda que começa e
    /// acaba em amplitude cheia estala, e o estalo é mais audível que o tom.
    const RAMPA: f32 = 0.008;

    let taxa = taxa.max(1) as f32;
    let mut amostras = Vec::new();

    for (frequencia, ms) in som.tons() {
        let total = (taxa * *ms as f32 / 1000.0) as usize;
        let rampa = ((RAMPA * taxa) as usize).max(1).min(total / 2);
        for i in 0..total {
            let t = i as f32 / taxa;
            // O envelope: sobe na rampa de entrada, cai na de saída, cheio no
            // meio. `min` das duas cobre o caso do tom curto demais para ter as
            // duas rampas inteiras.
            let entrada = (i as f32 / rampa as f32).min(1.0);
            let saida = ((total - i) as f32 / rampa as f32).min(1.0);
            let envelope = entrada.min(saida);
            let onda = (std::f32::consts::TAU * frequencia * t).sin();
            amostras.push(onda * envelope * volume * MEIA_ESCALA);
        }
    }
    amostras
}

/// Teto da amplitude, antes do volume do usuário.
///
/// Meia escala, e não escala cheia: uma senoide em 1,0 num alto-falante de
/// notebook satura, e o resultado é um estalo em vez de um tom. Sobra margem
/// para o volume 1,0 da configuração ainda ser um som limpo.
const MEIA_ESCALA: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_aviso_dura_o_que_os_tons_dele_somam() {
        for som in [Som::Inicio, Som::Fim, Som::Falha, Som::Cancelado] {
            let ms: u32 = som.tons().iter().map(|(_, ms)| ms).sum();
            let amostras = sintetizar(som, 48_000, 1.0);
            let esperado = (48_000.0 * ms as f32 / 1000.0) as usize;
            assert_eq!(
                amostras.len(),
                esperado,
                "{som:?} saiu com duração diferente da declarada"
            );
        }
    }

    #[test]
    fn o_som_comeca_e_acaba_em_silencio() {
        // A rampa existe para isto: sem ela o alto-falante estala, e o estalo é
        // mais audível que o próprio aviso.
        let amostras = sintetizar(Som::Inicio, 48_000, 1.0);
        assert!(amostras[0].abs() < 1e-6, "o som começa em amplitude cheia");
        assert!(
            amostras[amostras.len() - 1].abs() < 0.02,
            "o som acaba em amplitude cheia: {}",
            amostras[amostras.len() - 1]
        );
    }

    #[test]
    fn nenhuma_amostra_satura() {
        // Passar de 1,0 corta a onda no conversor e vira estalo, o oposto do que
        // o aviso deveria ser.
        for som in [Som::Inicio, Som::Fim, Som::Falha, Som::Cancelado] {
            for taxa in [16_000, 44_100, 48_000, 192_000] {
                for amostra in sintetizar(som, taxa, 1.0) {
                    assert!(
                        amostra.abs() <= 1.0,
                        "{som:?} a {taxa} Hz saturou em {amostra}"
                    );
                    assert!(amostra.is_finite(), "{som:?} produziu {amostra}");
                }
            }
        }
    }

    #[test]
    fn o_volume_escala_a_amplitude() {
        let cheio = sintetizar(Som::Inicio, 48_000, 1.0);
        let metade = sintetizar(Som::Inicio, 48_000, 0.5);
        let pico_cheio = cheio.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        let pico_metade = metade.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            (pico_cheio / 2.0 - pico_metade).abs() < 0.01,
            "o volume não escalou: {pico_cheio} e {pico_metade}"
        );
        // Volume zero é silêncio de verdade, e não um tom inaudível — quem
        // desligou não quer o dispositivo de saída sendo aberto.
        assert!(
            sintetizar(Som::Inicio, 48_000, 0.0)
                .iter()
                .all(|a| *a == 0.0)
        );
    }

    #[test]
    fn uma_taxa_absurda_nao_derruba_a_sintese() {
        // O arquivo de configuração não chega aqui, mas o dispositivo de saída
        // chega — e um que anuncie taxa zero faria a divisão explodir.
        assert!(
            sintetizar(Som::Inicio, 0, 1.0).is_empty()
                || !sintetizar(Som::Inicio, 0, 1.0).is_empty()
        );
        for amostra in sintetizar(Som::Inicio, 1, 1.0) {
            assert!(amostra.is_finite());
        }
    }

    #[test]
    fn o_inicio_sobe_e_o_fim_desce() {
        // A convenção que se entende sem ler documentação nenhuma; um teste
        // porque inverter os dois é o tipo de troca que passa despercebida numa
        // revisão e confunde quem usa.
        let inicio = Som::Inicio.tons();
        let fim = Som::Fim.tons();
        assert!(inicio[0].0 < inicio[1].0, "o aviso de início não sobe");
        assert!(fim[0].0 > fim[1].0, "o aviso de fim não desce");
    }
}
