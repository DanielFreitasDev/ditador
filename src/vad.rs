//! Acha onde a fala começa e onde ela termina, para o Whisper não receber
//! silêncio.
//!
//! ## Por que isto existe
//!
//! Todo ditado deste programa chega ao Whisper com silêncio nas duas pontas, e
//! não por acidente: no começo entram os 300 ms de pré-gravação (`PRE_GRAVACAO_MS`,
//! em `src/audio.rs`) mais o tempo entre apertar a tecla e começar a falar; no
//! fim, o tempo entre calar e soltar a tecla. São facilmente dois segundos de
//! nada em volta de uma frase de cinco.
//!
//! E o Whisper **alucina em silêncio**. É o defeito mais conhecido do modelo:
//! sem fala para transcrever, o decodificador continua produzindo tokens e sai
//! com a frase mais provável do treino — "Legendas pela comunidade Amara.org",
//! "Obrigado.", "Tchau." O `src/stt.rs` já se defende disso duas vezes (o
//! `no_speech_probability` acima de 0,85 e o `is_non_speech_marker`), e as duas
//! agem *depois*, jogando fora o que o modelo já inventou. Este módulo age
//! antes, tirando o silêncio de onde a invenção nasce.
//!
//! ## Por que não o Silero
//!
//! O programa parecido que serviu de referência usa o Silero VAD, uma rede
//! neural em ONNX. Aqui isso custaria o ONNX Runtime inteiro — dezenas de
//! megabytes de biblioteca nativa e uma árvore de dependências maior do que a
//! deste programa inteiro — para decidir, num áudio de microfone próximo, se
//! alguém está falando ou se a sala está calada. O Silero ganha no caso difícil:
//! voz longe, rádio ligada, duas pessoas conversando ao fundo. O caso deste
//! programa é o fácil, e o fácil se resolve com energia por quadro.
//!
//! ## O que o torna confiável sem rede neural
//!
//! **O áudio inteiro já está na mão.** Não estamos decidindo quadro a quadro,
//! ao vivo, com um limiar fixo que precisa servir para qualquer microfone —
//! estamos olhando uma gravação terminada. Isso permite medir o silêncio *deste*
//! ditado, nesta sala, neste microfone, e só então decidir o que é fala: o piso
//! de ruído sai do percentil 10 dos quadros, e o limiar é ele mais uma margem.
//! Um microfone chiado e um microfone limpo produzem pisos diferentes e a mesma
//! decisão certa, sem ninguém calibrar nada.
//!
//! ## As três regras conservadoras
//!
//! Aparar áudio de quem falou é pior do que transcrever silêncio: o erro do
//! silêncio produz uma frase estranha que se apaga, e o erro do corte come uma
//! palavra que ninguém sabe que existiu. Daí as três:
//!
//! 1. **Só as pontas.** O silêncio *no meio* da fala fica onde está. Cortá-lo
//!    emendaria trechos que não eram vizinhos, e o Whisper decodifica em
//!    contexto: uma pausa some e as duas metades da frase viram uma só, com a
//!    prosódia de nenhuma das duas. A pausa também é informação — é dela que
//!    sai boa parte da pontuação.
//! 2. **Na dúvida, não corta.** Não achando fala nenhuma pelo limiar relativo,
//!    o áudio volta inteiro. A única coisa que este módulo descarta por
//!    completo é o silêncio reconhecido por um critério *absoluto* (veja
//!    `PICO_MINIMO_DB`), que não depende de calibragem nenhuma.
//! 3. **Folga nas bordas.** Consoante surda no começo da palavra ("três",
//!    "psiu") e final de frase que morre têm energia baixinha e cairiam fora do
//!    limiar. A folga de 150 ms antes e 250 ms depois devolve isso — e ainda dá
//!    ao modelo um pedaço de silêncio de cada lado, que é o que ele espera ver.

/// Quanto áudio cada quadro de análise cobre.
///
/// Vinte milissegundos é o tamanho clássico de análise de voz: curto o bastante
/// para que o ataque de uma sílaba caia inteiro dentro de um quadro, longo o
/// bastante para que a energia medida seja estável — abaixo de uns 10 ms a
/// conta passa a oscilar com a própria forma de onda da vogal.
const QUADRO_MS: usize = 20;

/// Quanto um quadro precisa estar acima do piso de ruído para ser fala.
///
/// Voz de microfone próximo fica 20 a 40 dB acima do ruído da sala. Oito dB é
/// baixo de propósito: quem decide o que é fala aqui não precisa acertar o
/// meio da vogal — precisa não perder o rabo da palavra, que é justamente a
/// parte que desce em direção ao piso.
const MARGEM_DB: f32 = 8.0;

/// Qual percentil dos quadros é tomado como o silêncio deste ditado.
///
/// Dez por cento porque quase todo ditado tem pelo menos isso de silêncio: as
/// pontas que este módulo existe para tirar já são mais do que isso. Num ditado
/// que fosse fala do primeiro ao último quadro o percentil cairia dentro da
/// fala e o limiar sairia alto demais — e é aí que a regra 2 age, devolvendo o
/// áudio inteiro em vez de comer o começo dele.
const PERCENTIL_DO_PISO: f32 = 0.10;

/// Quantos quadros seguidos acima do limiar começam a fala.
///
/// Dois (40 ms) descartam o estalo do teclado e o clique do mouse, que são
/// impulsos de um quadro só, sem descartar a plosiva que abre uma palavra.
const ATAQUE: usize = 2;

/// Quantos quadros seguidos abaixo do limiar terminam a fala.
///
/// Quinze são 300 ms. É a pausa entre duas frases ditas em sequência —
/// menos do que isso e "o relatório sai na sexta. Depois eu confirmo" seria
/// cortado no ponto final, perdendo a segunda metade.
const SOLTURA: usize = 15;

/// Quanto áudio fica antes do primeiro quadro de fala.
const FOLGA_ANTES_MS: usize = 150;

/// Quanto áudio fica depois do último quadro de fala.
///
/// Maior que a folga da frente porque o fim de frase decai devagar — a última
/// vogal vai morrendo por uns 200 ms — enquanto o começo é um ataque abrupto.
const FOLGA_DEPOIS_MS: usize = 250;

/// Abaixo deste pico, em dBFS, ninguém falou.
///
/// É o único critério absoluto do módulo, e o único que autoriza jogar a
/// gravação fora. Fala em microfone próximo pica entre -25 e -6 dBFS; mesmo um
/// microfone mal ajustado, num sussurro, passa de -45. Cinquenta e cinco abaixo
/// da escala cheia é a tecla apertada sem querer, o microfone mudo no mixer, o
/// cabo fora.
const PICO_MINIMO_DB: f32 = -55.0;

/// Diferença mínima entre o pico e o piso para haver *alguma coisa* acontecendo.
///
/// Pega o caso que o `PICO_MINIMO_DB` não pega: o microfone que chia alto, sem
/// ninguém falando. Ali o pico passa de -55 dBFS sendo puro ruído — e o que
/// denuncia é a planura, porque ruído constante tem pico e piso quase juntos.
/// Fala, mesmo curta, abre pelo menos uns 15 dB entre um e outro.
const DINAMICA_MINIMA_DB: f32 = 10.0;

/// O pedaço do áudio que vale transcrever, em índices de amostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorte {
    pub inicio: usize,
    /// Exclusivo, como todo fim de fatia em Rust.
    pub fim: usize,
}

impl Recorte {
    /// O áudio inteiro — a resposta de quem não tem certeza (regra 2).
    fn inteiro(amostras: usize) -> Self {
        Self {
            inicio: 0,
            fim: amostras,
        }
    }

    pub fn amostras(self) -> usize {
        self.fim.saturating_sub(self.inicio)
    }
}

/// Onde está a fala, ou `None` se não houver nenhuma.
///
/// `None` quer dizer "descarte esta gravação": foi um toque na tecla, um
/// microfone mudo, uma sala em silêncio. Quem chama trata isso como o texto
/// vazio que já trata hoje ("Não identifiquei fala no áudio").
///
/// Todo o resto devolve um recorte — que pode muito bem ser o áudio inteiro.
pub fn achar_a_fala(amostras: &[f32], taxa: u32) -> Option<Recorte> {
    let por_quadro = (taxa as usize * QUADRO_MS) / 1000;
    // Áudio curto demais para render dois quadros não tem o que analisar: o
    // percentil de um quadro só é o próprio quadro, e o limiar sairia dele
    // mesmo. Passa inteiro — o `MIN_SAMPLES` do `stt.rs` cuida do resto.
    if por_quadro == 0 || amostras.len() < por_quadro * 2 {
        return (!amostras.is_empty()).then(|| Recorte::inteiro(amostras.len()));
    }

    let niveis: Vec<f32> = amostras.chunks(por_quadro).map(nivel_db).collect();

    let piso = percentil(&niveis, PERCENTIL_DO_PISO);
    let pico = niveis.iter().copied().fold(f32::MIN, f32::max);

    // Os dois critérios absolutos, que são os únicos que descartam.
    if pico < PICO_MINIMO_DB || pico - piso < DINAMICA_MINIMA_DB {
        return None;
    }

    let limiar = piso + MARGEM_DB;
    let Some((primeiro, ultimo)) = pontas_da_fala(&niveis, limiar) else {
        // Passou nos critérios absolutos — há sinal aqui — mas o limiar
        // relativo não achou onde. É a regra 2: devolve tudo.
        return Some(Recorte::inteiro(amostras.len()));
    };

    let folga_antes = (taxa as usize * FOLGA_ANTES_MS) / 1000;
    let folga_depois = (taxa as usize * FOLGA_DEPOIS_MS) / 1000;
    let inicio = (primeiro * por_quadro).saturating_sub(folga_antes);
    // `ultimo` é índice de quadro; o fim dele é o começo do seguinte.
    let fim = ((ultimo + 1) * por_quadro + folga_depois).min(amostras.len());

    Some(Recorte { inicio, fim })
}

/// O nível de um quadro em dBFS, pela raiz da média dos quadrados.
///
/// O piso de -140 dB existe para o quadro digitalmente zerado (que acontece: é
/// o que sai de um microfone silenciado no mixer). Sem ele o `log10(0)` daria
/// `-inf`, que contamina a média e o percentil — e comparar `-inf` com qualquer
/// coisa deixa de dizer o que se quer.
fn nivel_db(quadro: &[f32]) -> f32 {
    if quadro.is_empty() {
        return -140.0;
    }
    let soma: f32 = quadro.iter().map(|a| a * a).sum();
    let rms = (soma / quadro.len() as f32).sqrt();
    if rms <= 1e-7 {
        -140.0
    } else {
        20.0 * rms.log10()
    }
}

/// O valor abaixo do qual está a fração `p` dos quadros.
///
/// Ordena uma cópia em vez de usar `select_nth_unstable`: são algumas centenas
/// de `f32` (um ditado de dez segundos dá 500 quadros), e a ordenação inteira
/// custa microssegundos ao lado dos segundos que o Whisper vai levar. O que se
/// ganha é uma função que se lê de uma vez.
fn percentil(niveis: &[f32], p: f32) -> f32 {
    let mut ordenados = niveis.to_vec();
    ordenados.sort_by(|a, b| a.total_cmp(b));
    let indice = ((ordenados.len() as f32 * p) as usize).min(ordenados.len() - 1);
    ordenados[indice]
}

/// O primeiro e o último quadro de fala, com ataque e soltura.
///
/// Devolve `None` quando nenhum trecho sobreviveu ao `ATAQUE` — o que, depois
/// dos critérios absolutos, quer dizer que o limiar relativo ficou alto demais
/// para este áudio (fala do começo ao fim, sem silêncio de onde tirar o piso).
fn pontas_da_fala(niveis: &[f32], limiar: f32) -> Option<(usize, usize)> {
    let (mut primeiro, mut ultimo) = (None, None);
    // Quantos quadros seguidos acima do limiar já vimos, e quantos abaixo dele
    // desde o último que estava acima. O segundo só importa depois de a fala
    // ter começado.
    let mut acima = 0usize;
    let mut abaixo = 0usize;
    let mut falando = false;

    for (i, &nivel) in niveis.iter().enumerate() {
        if nivel >= limiar {
            acima += 1;
            abaixo = 0;
            if !falando && acima >= ATAQUE {
                falando = true;
                // O trecho começa no primeiro quadro da sequência, e não neste:
                // os quadros do ataque são fala tanto quanto o que os confirmou.
                primeiro.get_or_insert(i + 1 - ATAQUE);
            }
            if falando {
                ultimo = Some(i);
            }
        } else {
            acima = 0;
            if falando {
                abaixo += 1;
                if abaixo >= SOLTURA {
                    falando = false;
                }
            }
        }
    }

    primeiro.zip(ultimo)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAXA: u32 = 16_000;

    fn amostras(ms: usize) -> usize {
        (TAXA as usize * ms) / 1000
    }

    /// Ruído baixo, do tipo que todo microfone produz parado.
    ///
    /// Determinístico de propósito — um gerador pseudoaleatório de uma linha —
    /// para que o teste que reprovar reprove sempre, e não uma vez em dez.
    fn chiado(quantas: usize, amplitude: f32) -> Vec<f32> {
        let mut semente = 0x2545_F491_4F6C_DD1Du64;
        (0..quantas)
            .map(|_| {
                semente ^= semente << 13;
                semente ^= semente >> 7;
                semente ^= semente << 17;
                let unitario = (semente >> 40) as f32 / 8_388_608.0 - 1.0;
                unitario * amplitude
            })
            .collect()
    }

    /// O que faz as vezes de fala aqui: um tom na região da voz, com envelope.
    ///
    /// O envelope não é enfeite, e o primeiro teste que este módulo reprovou
    /// ensinou por quê: um tom **constante** não é fala e este módulo está certo
    /// em recusá-lo. Voz tem sílaba — sobe e desce umas quatro vezes por
    /// segundo —, e é dessa variação que sai a diferença entre pico e piso de
    /// que o `DINAMICA_MINIMA_DB` vive. Um seno puro tem pico e piso colados,
    /// que é a assinatura de ruído de máquina, não de gente falando.
    ///
    /// O vale do envelope não chega a zero (fica em 15 %) pelo mesmo motivo: nem
    /// entre duas sílabas a voz some por completo.
    fn fala(quantas: usize, amplitude: f32) -> Vec<f32> {
        (0..quantas)
            .map(|i| {
                let t = i as f32 / TAXA as f32;
                let silaba = (2.0 * std::f32::consts::PI * 4.0 * t).sin().abs();
                let envelope = 0.15 + 0.85 * silaba;
                amplitude * envelope * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    fn com_fala_no_meio(silencio_ms: usize, fala_ms: usize) -> Vec<f32> {
        let mut audio = chiado(amostras(silencio_ms), 0.0015);
        audio.extend(fala(amostras(fala_ms), 0.25));
        audio.extend(chiado(amostras(silencio_ms), 0.0015));
        audio
    }

    #[test]
    fn o_silencio_puro_e_descartado() {
        let audio = chiado(amostras(2000), 0.0008);
        assert_eq!(achar_a_fala(&audio, TAXA), None);
    }

    #[test]
    fn o_microfone_mudo_e_descartado() {
        let audio = vec![0.0f32; amostras(1500)];
        assert_eq!(achar_a_fala(&audio, TAXA), None);
    }

    #[test]
    fn o_chiado_alto_e_constante_nao_e_confundido_com_fala() {
        // Pico bem acima do `PICO_MINIMO_DB`, e ainda assim nada: o que denuncia
        // é a planura. É o microfone de má qualidade ou o ganho alto demais numa
        // sala vazia — o caso que um limiar absoluto sozinho transcreveria.
        let audio = chiado(amostras(2000), 0.05);
        assert_eq!(achar_a_fala(&audio, TAXA), None);
    }

    #[test]
    fn a_fala_no_meio_do_silencio_e_recortada_das_duas_pontas() {
        let audio = com_fala_no_meio(1000, 1500);
        let r = achar_a_fala(&audio, TAXA).expect("há fala aqui");

        // O recorte cai dentro do silêncio da frente, não dentro da fala, e
        // respeita a folga: começa antes do primeiro milissegundo de fala.
        assert!(r.inicio > 0, "não aparou nada da frente");
        assert!(
            r.inicio <= amostras(1000),
            "o começo do recorte entrou na fala"
        );
        assert!(
            r.fim >= amostras(2500),
            "o fim do recorte entrou na fala (fim = {})",
            r.fim
        );
        assert!(r.fim < audio.len(), "não aparou nada do fim");
    }

    #[test]
    fn a_folga_das_bordas_e_respeitada() {
        let audio = com_fala_no_meio(1000, 1500);
        let r = achar_a_fala(&audio, TAXA).expect("há fala aqui");

        // A fala começa em 1000 ms; com 150 ms de folga, o recorte não pode
        // começar depois de 850 ms. E termina em 2500 ms, com 250 ms de folga:
        // não pode terminar antes de 2750 ms.
        assert!(
            r.inicio <= amostras(850),
            "a folga da frente encolheu: {} > {}",
            r.inicio,
            amostras(850)
        );
        assert!(
            r.fim >= amostras(2750),
            "a folga do fim encolheu: {} < {}",
            r.fim,
            amostras(2750)
        );
    }

    #[test]
    fn o_silencio_do_meio_nao_e_costurado() {
        // Duas frases com uma pausa de 400 ms entre elas — mais longa que a
        // `SOLTURA`, de propósito. O recorte precisa cobrir as duas: cortar no
        // meio jogaria a segunda frase fora.
        let mut audio = chiado(amostras(800), 0.0015);
        audio.extend(fala(amostras(700), 0.25));
        audio.extend(chiado(amostras(400), 0.0015));
        audio.extend(fala(amostras(700), 0.25));
        audio.extend(chiado(amostras(800), 0.0015));

        let r = achar_a_fala(&audio, TAXA).expect("há fala aqui");
        assert!(
            r.inicio <= amostras(800),
            "cortou o começo da primeira frase"
        );
        assert!(
            r.fim >= amostras(2600),
            "cortou a segunda frase: o recorte termina em {}",
            r.fim
        );
    }

    #[test]
    fn a_fala_do_comeco_ao_fim_volta_inteira() {
        // Sem silêncio de onde tirar o piso, o percentil cai dentro da própria
        // fala e o limiar sai alto demais. A regra 2 manda devolver tudo — e é
        // isto que impede o módulo de comer o começo de um ditado emendado.
        let audio = fala(amostras(2000), 0.25);
        let r = achar_a_fala(&audio, TAXA).expect("há fala aqui");
        assert_eq!(r, Recorte::inteiro(audio.len()));
    }

    #[test]
    fn o_audio_curto_demais_passa_inteiro() {
        let audio = fala(amostras(10), 0.25);
        let r = achar_a_fala(&audio, TAXA).expect("curto, mas tem sinal");
        assert_eq!(r, Recorte::inteiro(audio.len()));
    }

    #[test]
    fn audio_vazio_nao_estoura() {
        assert_eq!(achar_a_fala(&[], TAXA), None);
    }

    #[test]
    fn a_fala_baixinha_de_quem_sussurra_nao_e_descartada() {
        // Pico em torno de -32 dBFS: bem abaixo de uma voz normal e bem acima do
        // `PICO_MINIMO_DB`. Este é o teste que impede alguém de "melhorar" o
        // limiar absoluto para um valor que cala quem fala baixo.
        let mut audio = chiado(amostras(600), 0.0005);
        audio.extend(fala(amostras(1200), 0.025));
        audio.extend(chiado(amostras(600), 0.0005));

        let r = achar_a_fala(&audio, TAXA).expect("sussurro ainda é fala");
        assert!(r.amostras() >= amostras(1200), "aparou fala de verdade");
    }

    #[test]
    fn o_estalo_de_um_quadro_so_nao_vira_fala() {
        // Um clique de teclado no meio do silêncio: alto, curtíssimo. O
        // `ATAQUE` existe para isto — e o critério da dinâmica, para o caso de
        // ele ser o único evento do áudio.
        let mut audio = chiado(amostras(1000), 0.001);
        audio.extend(fala(amostras(10), 0.6));
        audio.extend(chiado(amostras(1000), 0.001));

        // O estalo passa no pico e na dinâmica (é alto), mas não sobrevive ao
        // ataque: sem trecho de fala, a regra 2 devolve o áudio inteiro em vez
        // de recortar em volta do clique.
        let r = achar_a_fala(&audio, TAXA).expect("tem sinal");
        assert_eq!(r, Recorte::inteiro(audio.len()));
    }

    #[test]
    fn o_nivel_de_um_quadro_zerado_nao_e_infinito() {
        assert!(nivel_db(&[0.0; 320]).is_finite());
        assert!(nivel_db(&[]).is_finite());
    }

    #[test]
    fn o_percentil_pega_o_valor_certo() {
        let niveis = [-10.0, -60.0, -20.0, -50.0, -30.0, -40.0, -70.0, -80.0];
        // Ordenados: -80, -70, -60, -50, -40, -30, -20, -10. O índice 0 (8 × 0,1
        // truncado) é o menor.
        assert_eq!(percentil(&niveis, 0.10), -80.0);
        assert_eq!(percentil(&niveis, 0.50), -40.0);
        // Um percentil de 1,0 não pode sair da fatia.
        assert_eq!(percentil(&niveis, 1.0), -10.0);
    }

    #[test]
    fn o_recorte_nunca_passa_do_fim_do_audio() {
        // A folga do fim é maior que o silêncio que sobra depois da fala.
        let mut audio = chiado(amostras(500), 0.0015);
        audio.extend(fala(amostras(1000), 0.25));
        audio.extend(chiado(amostras(50), 0.0015));

        let r = achar_a_fala(&audio, TAXA).expect("há fala aqui");
        assert!(r.fim <= audio.len());
        assert!(r.inicio < r.fim);
    }
}
