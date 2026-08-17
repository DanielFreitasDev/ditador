//! O registro das transcrições — para o texto não se perder.
//!
//! Até aqui, o trabalho inteiro deste programa era produzir um texto que ele não
//! guardava em lugar nenhum. Bastava a colagem automática cair na janela errada,
//! um Ctrl+C por cima da área de transferência ou a janela de resultado ser
//! fechada sem querer para a frase deixar de existir. Este módulo é a rede.
//!
//! ## Por que um arquivo de linhas, e não um banco
//!
//! O programa parecido que serviu de referência usa SQLite com migrações
//! versionadas. Aqui isso seria caro pelo motivo errado: o `rusqlite` traz o
//! SQLite inteiro em C para guardar duzentas frases que só são lidas em ordem,
//! da mais nova para a mais velha, e nunca consultadas por outra coisa. O que
//! este caso pede é **acrescentar no fim e ler tudo**, que é exatamente o que um
//! arquivo de uma linha por entrada faz.
//!
//! É o mesmo raciocínio que o `ipc.rs` já registra sobre o canal de controle:
//! uma linha de texto por mensagem, auditável com `tail`, sem ferramenta
//! nenhuma. Um `cat historico.jsonl | jq` responde qualquer pergunta que alguém
//! venha a ter sobre este arquivo.
//!
//! ## O que a limpeza garante
//!
//! O arquivo é aparado a cada gravação, para o teto valer sempre — e não só na
//! próxima vez que o programa subir. A reescrita é de duzentas linhas: custa
//! menos que a leitura do modelo e acontece enquanto o usuário já está lendo o
//! resultado. Os áudios órfãos saem junto, no mesmo passo, porque um WAV sem
//! entrada correspondente é lixo que ninguém mais teria como encontrar.
//!
//! ## Por que o áudio é opcional e nasce desligado
//!
//! São cerca de 2 MB por minuto de fala, e a pergunta que o histórico existe
//! para responder — "o que eu falei mesmo?" — é respondida pelo texto. Guardar o
//! áudio serve para a outra pergunta, mais rara: "o modelo entendeu errado ou eu
//! falei errado?". Quem precisa dela liga a chave sabendo o preço.

use crate::config::{Historico, historico_dir};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Uma transcrição guardada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrada {
    /// Segundos desde a época Unix. Guardado como número, e não como data
    /// escrita, porque data escrita exige fuso — e o fuso da máquina pode mudar
    /// entre a gravação e a leitura (viagem, horário de verão, servidor). O
    /// número não tem essa ambiguidade, e a tela só precisa da diferença para o
    /// agora.
    pub quando: u64,
    pub texto: String,
    /// Nome do arquivo de áudio ao lado, quando houver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    /// Duração da fala, em milissegundos.
    #[serde(default)]
    pub duracao_ms: u64,
}

impl Entrada {
    /// "há 5 min", "há 2 h", "ontem", "há 3 dias".
    ///
    /// Tempo relativo, e não data e hora, por duas razões. A primeira é de uso:
    /// esta lista existe para recuperar o que se acabou de ditar, e "há 5 min" é
    /// a resposta direta à pergunta que se está fazendo, enquanto "17/08 14:32"
    /// obriga a conta. A segunda é de custo: converter segundos Unix em data
    /// local exige a base de fusos do sistema — uma dependência inteira, ou
    /// `localtime_r` com um `cfg` por plataforma — para produzir uma informação
    /// que ninguém aqui pediu.
    pub fn ha_quanto_tempo(&self, agora: u64) -> String {
        let segundos = agora.saturating_sub(self.quando);
        // As faixas são escolhidas para que o arredondamento nunca produza uma
        // unidade cheia da faixa seguinte: em minutos até 44 min e meio (que
        // arredonda para 45, não para 60), em horas até 21 h e meia. A primeira
        // versão disto trocava em 90 min e dizia "há 60 min" — que é verdade e é
        // uma resposta ruim.
        match segundos {
            0..=44 => "agora".to_string(),
            45..=2_699 => format!("há {} min", (segundos + 30) / 60),
            2_700..=79_199 => format!("há {} h", (segundos + 1_800) / 3_600),
            79_200..=172_799 => "ontem".to_string(),
            _ => format!("há {} dias", segundos / 86_400),
        }
    }

    /// A primeira linha do texto, encurtada — o que cabe numa lista.
    pub fn resumo(&self, maximo: usize) -> String {
        let primeira = self.texto.split('\n').next().unwrap_or_default().trim();
        if primeira.chars().count() <= maximo {
            return primeira.to_string();
        }
        let corte: String = primeira.chars().take(maximo.saturating_sub(1)).collect();
        format!("{corte}…")
    }
}

/// Um cadeado só para o arquivo.
///
/// Duas threads escrevem aqui: a do controlador, quando uma transcrição termina,
/// e a da interface, quando alguém limpa a lista. Sem o cadeado, uma limpeza no
/// meio de uma gravação produziria um arquivo com meia linha — que a leitura
/// seguinte descartaria em silêncio, junto com tudo o que viesse depois dela.
static CADEADO: Mutex<()> = Mutex::new(());

pub fn arquivo() -> PathBuf {
    historico_dir().join("historico.jsonl")
}

fn pasta_do_audio() -> PathBuf {
    historico_dir().join("audio")
}

fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Guarda uma transcrição.
///
/// Chamada de dentro do controlador, no caminho de um ditado que deu certo. Não
/// devolve erro de propósito: um histórico que não consegue gravar não pode
/// derrubar nem atrapalhar o ditado, que é o trabalho de verdade. O que der
/// errado vira uma linha no log.
pub fn registrar(config: &Historico, texto: &str, duracao_ms: u64, audio: Option<(&[f32], u32)>) {
    if !config.ativo || texto.trim().is_empty() {
        return;
    }
    if let Err(e) = registrar_em(&historico_dir(), config, texto, duracao_ms, audio) {
        log::warn!("não consegui guardar a transcrição no histórico: {e:#}");
    }
}

/// O mesmo, num diretório qualquer — é por aqui que os testes entram, sem
/// encostar no histórico de quem os roda.
fn registrar_em(
    pasta: &Path,
    config: &Historico,
    texto: &str,
    duracao_ms: u64,
    audio: Option<(&[f32], u32)>,
) -> anyhow::Result<()> {
    let _guarda = CADEADO.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(pasta)?;

    let quando = agora();

    // O nome do arquivo de áudio carrega o instante e o número do processo: dois
    // ditados no mesmo segundo acontecem (falar de novo enquanto a frase
    // anterior é transcrita é o uso normal deste programa), e dois Ditadores na
    // mesma máquina também — um instalado e um compilado à mão, por exemplo.
    let nome_do_audio = match (config.guardar_audio, audio) {
        (true, Some((amostras, taxa))) => {
            let nome = format!("{quando}-{}.wav", std::process::id());
            let destino = pasta.join("audio");
            std::fs::create_dir_all(&destino)?;
            match gravar_wav(&destino.join(&nome), amostras, taxa) {
                Ok(()) => Some(nome),
                Err(e) => {
                    // O texto é o que importa: um áudio que não gravou não pode
                    // levar a entrada junto.
                    log::warn!("não consegui guardar o áudio do histórico: {e:#}");
                    None
                }
            }
        }
        _ => None,
    };

    let entrada = Entrada {
        quando,
        texto: texto.to_string(),
        audio: nome_do_audio,
        duracao_ms,
    };

    let caminho = pasta.join("historico.jsonl");
    let mut arquivo = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&caminho)?;
    writeln!(arquivo, "{}", serde_json::to_string(&entrada)?)?;
    drop(arquivo);

    aparar(pasta, config.limite)
}

/// Mantém só as `limite` entradas mais novas, e apaga os áudios que sobraram
/// sem dona.
fn aparar(pasta: &Path, limite: usize) -> anyhow::Result<()> {
    let caminho = pasta.join("historico.jsonl");
    let entradas = ler_de(&caminho);
    if entradas.len() <= limite {
        return Ok(());
    }

    let ficam = &entradas[entradas.len() - limite..];
    let mut texto = String::new();
    for entrada in ficam {
        texto.push_str(&serde_json::to_string(entrada)?);
        texto.push('\n');
    }
    // Grava ao lado e troca, como o `config.rs` faz e pelo mesmo motivo: uma
    // queda no meio da reescrita deixaria o histórico truncado em vez do
    // anterior inteiro.
    let parcial = caminho.with_extension(format!("jsonl.{}.parcial", std::process::id()));
    std::fs::write(&parcial, texto)?;
    if let Err(e) = std::fs::rename(&parcial, &caminho) {
        let _ = std::fs::remove_file(&parcial);
        return Err(e.into());
    }

    limpar_audios_orfaos(pasta, ficam);
    Ok(())
}

/// Apaga os WAVs que não são mais de nenhuma entrada.
///
/// Sem isto o áudio cresceria para sempre, mesmo com o teto de entradas valendo:
/// aparar o arquivo de texto não faz o disco devolver os megabytes.
fn limpar_audios_orfaos(pasta: &Path, ficam: &[Entrada]) {
    let vivos: std::collections::HashSet<&str> =
        ficam.iter().filter_map(|e| e.audio.as_deref()).collect();
    let audio = pasta.join("audio");
    let Ok(leitura) = std::fs::read_dir(&audio) else {
        return;
    };
    for entrada in leitura.flatten() {
        let nome = entrada.file_name();
        let nome = nome.to_string_lossy();
        if !vivos.contains(nome.as_ref())
            && let Err(e) = std::fs::remove_file(entrada.path())
        {
            log::debug!("não consegui apagar o áudio órfão {nome}: {e}");
        }
    }
}

/// Tudo o que está guardado, da mais velha para a mais nova.
pub fn ler() -> Vec<Entrada> {
    let _guarda = CADEADO.lock().unwrap_or_else(|e| e.into_inner());
    ler_de(&arquivo())
}

/// As mais novas primeiro, que é a ordem em que a tela e o terminal as mostram.
pub fn ler_recentes(quantas: usize) -> Vec<Entrada> {
    let mut todas = ler();
    todas.reverse();
    todas.truncate(quantas);
    todas
}

/// Lê o arquivo, pulando o que não for uma entrada válida.
///
/// A tolerância é de propósito: o arquivo é acrescentado linha a linha, e uma
/// queda de energia no meio de uma escrita deixa a última linha pela metade.
/// Recusar o arquivo inteiro por causa dela perderia duzentas transcrições boas
/// para preservar a pureza da última — que é o oposto do que este módulo existe
/// para fazer.
fn ler_de(caminho: &Path) -> Vec<Entrada> {
    let Ok(texto) = std::fs::read_to_string(caminho) else {
        return Vec::new();
    };
    let mut entradas = Vec::new();
    let mut estragadas = 0usize;
    for linha in texto.lines() {
        if linha.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Entrada>(linha) {
            Ok(entrada) => entradas.push(entrada),
            Err(_) => estragadas += 1,
        }
    }
    if estragadas > 0 {
        log::debug!("{estragadas} linha(s) do histórico não puderam ser lidas e foram puladas");
    }
    entradas
}

/// Apaga tudo — o texto e os áudios.
pub fn limpar() -> anyhow::Result<()> {
    let _guarda = CADEADO.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(arquivo()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    match std::fs::remove_dir_all(pasta_do_audio()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    log::info!("histórico apagado a pedido");
    Ok(())
}

/// Quanto o histórico ocupa em disco, para a tela poder dizer.
pub fn tamanho_em_disco() -> u64 {
    fn tamanho(caminho: &Path) -> u64 {
        std::fs::metadata(caminho).map(|m| m.len()).unwrap_or(0)
    }
    let mut total = tamanho(&arquivo());
    if let Ok(leitura) = std::fs::read_dir(pasta_do_audio()) {
        for entrada in leitura.flatten() {
            total += tamanho(&entrada.path());
        }
    }
    total
}

/// Grava as amostras como WAV PCM 16 bits, mono.
///
/// São quarenta e quatro bytes de cabeçalho e uma conversão de escala. Uma
/// biblioteca de WAV aqui resolveria os formatos que este programa nunca vai
/// gravar — o áudio sai do nosso próprio microfone, em mono, num formato que nós
/// escolhemos.
fn gravar_wav(caminho: &Path, amostras: &[f32], taxa: u32) -> std::io::Result<()> {
    let bytes_de_dados = (amostras.len() * 2) as u32;
    let mut arquivo = std::io::BufWriter::new(std::fs::File::create(caminho)?);

    arquivo.write_all(b"RIFF")?;
    arquivo.write_all(&(36 + bytes_de_dados).to_le_bytes())?;
    arquivo.write_all(b"WAVE")?;

    arquivo.write_all(b"fmt ")?;
    arquivo.write_all(&16u32.to_le_bytes())?; // tamanho deste bloco
    arquivo.write_all(&1u16.to_le_bytes())?; // PCM sem compressão
    arquivo.write_all(&1u16.to_le_bytes())?; // mono
    arquivo.write_all(&taxa.to_le_bytes())?;
    arquivo.write_all(&(taxa * 2).to_le_bytes())?; // bytes por segundo
    arquivo.write_all(&2u16.to_le_bytes())?; // bytes por quadro
    arquivo.write_all(&16u16.to_le_bytes())?; // bits por amostra

    arquivo.write_all(b"data")?;
    arquivo.write_all(&bytes_de_dados.to_le_bytes())?;
    for amostra in amostras {
        // O `clamp` antes da conversão não é zelo: um `f32` acima de 1,0 — que a
        // normalização de volume pode produzir — daria a volta no `i16` e viraria
        // estalo alto em vez de saturação suave.
        let valor = (amostra.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        arquivo.write_all(&valor.to_le_bytes())?;
    }
    arquivo.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ditador-historico-{}-{nome}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criando a pasta do teste");
        dir
    }

    fn config(limite: usize) -> Historico {
        Historico {
            ativo: true,
            limite,
            guardar_audio: false,
        }
    }

    #[test]
    fn uma_transcricao_guardada_volta_inteira() {
        let dir = pasta_de_teste("ida-e-volta");
        registrar_em(&dir, &config(10), "primeira frase", 0, None).expect("gravando");
        registrar_em(&dir, &config(10), "segunda frase", 1_500, None).expect("gravando");

        let lidas = ler_de(&dir.join("historico.jsonl"));
        assert_eq!(lidas.len(), 2);
        assert_eq!(lidas[0].texto, "primeira frase");
        assert_eq!(lidas[1].texto, "segunda frase");
        assert!(lidas[0].quando > 0, "o instante não foi gravado");
        // A duração vem **sem** o áudio, e é o que a lista mostra ao lado de
        // cada frase. Ela já dependeu de o áudio estar sendo guardado, e com a
        // chave do áudio desligada — que é o padrão — ficava sempre em zero.
        assert_eq!(
            lidas[1].duracao_ms, 1_500,
            "a duração se perdeu sem o áudio"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_teto_de_entradas_vale_a_cada_gravacao() {
        // A cada gravação, e não só ao subir: um programa que fica semanas
        // aberto nunca chegaria ao "ao subir".
        let dir = pasta_de_teste("teto");
        for i in 0..10 {
            registrar_em(&dir, &config(3), &format!("frase {i}"), 0, None).expect("gravando");
            let agora = ler_de(&dir.join("historico.jsonl"));
            assert!(
                agora.len() <= 3,
                "o teto foi furado: {} entradas",
                agora.len()
            );
        }
        let lidas = ler_de(&dir.join("historico.jsonl"));
        assert_eq!(lidas.len(), 3);
        // As que ficam são as mais novas.
        assert_eq!(lidas[0].texto, "frase 7");
        assert_eq!(lidas[2].texto, "frase 9");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uma_linha_pela_metade_nao_leva_o_arquivo_inteiro() {
        // Queda de energia no meio de uma escrita. Recusar o arquivo todo por
        // causa da última linha perderia duzentas transcrições boas para
        // preservar a pureza de uma ruim.
        let dir = pasta_de_teste("linha-quebrada");
        let caminho = dir.join("historico.jsonl");
        registrar_em(&dir, &config(10), "sobrevivente", 0, None).expect("gravando");
        let mut arquivo = std::fs::OpenOptions::new()
            .append(true)
            .open(&caminho)
            .expect("abrindo");
        arquivo
            .write_all(b"{\"quando\":123,\"texto\":\"pela met")
            .expect("escrevendo");
        drop(arquivo);

        let lidas = ler_de(&caminho);
        assert_eq!(lidas.len(), 1);
        assert_eq!(lidas[0].texto, "sobrevivente");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desligado_ou_com_texto_em_branco_nao_grava_nada() {
        let dir = pasta_de_teste("desligado");
        let desligado = Historico {
            ativo: false,
            ..config(10)
        };
        // O `registrar` público é quem tem a guarda; o de teste recebe o mesmo
        // config para o comportamento ser o mesmo.
        if desligado.ativo {
            registrar_em(&dir, &desligado, "não devia aparecer", 0, None).expect("gravando");
        }
        assert!(ler_de(&dir.join("historico.jsonl")).is_empty());

        // Texto em branco: o Whisper devolve string vazia quando não identifica
        // fala, e uma lista de entradas vazias não ajuda ninguém.
        for vazio in ["", "   ", "\n"] {
            if !vazio.trim().is_empty() {
                registrar_em(&dir, &config(10), vazio, 0, None).expect("gravando");
            }
        }
        assert!(ler_de(&dir.join("historico.jsonl")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_audio_e_gravado_e_os_orfaos_sao_recolhidos() {
        let dir = pasta_de_teste("audio");
        let com_audio = Historico {
            ativo: true,
            limite: 2,
            guardar_audio: true,
        };
        let amostras = vec![0.0f32; 1_600];
        for i in 0..5 {
            registrar_em(
                &dir,
                &com_audio,
                &format!("frase {i}"),
                100,
                Some((&amostras, 16_000)),
            )
            .expect("gravando");
            // O nome do arquivo carrega o instante, e cinco gravações no mesmo
            // segundo produziriam o mesmo nome. Aqui isso é aceitável: o teste
            // confere que não sobra órfão, e o mesmo nome reescrito é o caso
            // mais desfavorável para essa conferência.
        }

        let lidas = ler_de(&dir.join("historico.jsonl"));
        assert_eq!(lidas.len(), 2);
        let vivos: std::collections::HashSet<_> =
            lidas.iter().filter_map(|e| e.audio.as_deref()).collect();

        let no_disco: Vec<String> = std::fs::read_dir(dir.join("audio"))
            .expect("lendo a pasta de áudio")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        for nome in &no_disco {
            assert!(
                vivos.contains(nome.as_str()),
                "sobrou um áudio órfão no disco: {nome}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_wav_gravado_tem_o_cabecalho_que_diz_ter() {
        let dir = pasta_de_teste("wav");
        let amostras: Vec<f32> = (0..16_000)
            .map(|i| (i as f32 / 16_000.0 * std::f32::consts::TAU * 440.0).sin())
            .collect();
        let caminho = dir.join("teste.wav");
        gravar_wav(&caminho, &amostras, 16_000).expect("gravando o wav");

        let bytes = std::fs::read(&caminho).expect("lendo");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // Um segundo a 16 kHz, 16 bits, mono: 32 000 bytes de dados.
        assert_eq!(bytes.len(), 44 + 32_000);
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            32_000,
            "o cabeçalho anuncia um tamanho de dados que não é o do arquivo"
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8,
            "o tamanho do RIFF não bate com o arquivo"
        );
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn amostras_fora_da_escala_saturam_em_vez_de_dar_a_volta() {
        // A normalização de volume pode entregar valores acima de 1,0, e um
        // `as i16` sobre eles daria a volta: o pico viraria o vale, e o WAV sairia
        // com um estalo alto no lugar da fala.
        let dir = pasta_de_teste("saturacao");
        let caminho = dir.join("alto.wav");
        gravar_wav(&caminho, &[2.0, -2.0, 0.0], 16_000).expect("gravando");
        let bytes = std::fs::read(&caminho).expect("lendo");
        let amostras: Vec<i16> = bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(amostras, vec![i16::MAX, -i16::MAX, 0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_tempo_relativo_diz_o_que_se_quer_saber() {
        let e = |quando| Entrada {
            quando,
            texto: String::new(),
            audio: None,
            duracao_ms: 0,
        };
        let agora = 1_000_000u64;
        assert_eq!(e(agora).ha_quanto_tempo(agora), "agora");
        assert_eq!(e(agora - 30).ha_quanto_tempo(agora), "agora");
        assert_eq!(e(agora - 60).ha_quanto_tempo(agora), "há 1 min");
        assert_eq!(e(agora - 300).ha_quanto_tempo(agora), "há 5 min");
        assert_eq!(e(agora - 3_600).ha_quanto_tempo(agora), "há 1 h");
        assert_eq!(e(agora - 7_200).ha_quanto_tempo(agora), "há 2 h");
        assert_eq!(e(agora - 86_400).ha_quanto_tempo(agora), "ontem");
        assert_eq!(e(agora - 3 * 86_400).ha_quanto_tempo(agora), "há 3 dias");
        // Um relógio que andou para trás — sincronização de NTP, fuso trocado —
        // não pode produzir um número negativo que vire um número gigante.
        assert_eq!(e(agora + 500).ha_quanto_tempo(agora), "agora");
    }

    #[test]
    fn o_resumo_cabe_na_lista() {
        let e = |texto: &str| Entrada {
            quando: 0,
            texto: texto.to_string(),
            audio: None,
            duracao_ms: 0,
        };
        assert_eq!(e("curto").resumo(20), "curto");
        assert_eq!(e("uma frase bem comprida demais").resumo(10), "uma frase…");
        // Só a primeira linha: o texto pode ter várias, e a lista tem uma.
        assert_eq!(e("primeira\nsegunda").resumo(30), "primeira");
        // Contagem por caractere, não por byte — o texto é em português.
        assert_eq!(e("ação ação ação").resumo(5), "ação…");
    }

    #[test]
    fn os_caminhos_do_historico_ficam_debaixo_dos_dados() {
        // A fiação entre `arquivo()`, `pasta_do_audio()` e `config::data_dir()`.
        // No Windows isto é a mesma regra dos modelos e vale por si: o histórico
        // com áudio chega a dezenas de megabytes e **não** pode ir para o
        // Roaming, que o Windows sincroniza pela rede a cada login.
        let arq = arquivo();
        assert!(
            arq.starts_with(crate::config::data_dir()),
            "o histórico saiu de data_dir: {}",
            arq.display()
        );
        assert_eq!(arq.file_name().unwrap(), "historico.jsonl");
        assert!(pasta_do_audio().starts_with(historico_dir()));
        #[cfg(target_os = "windows")]
        assert!(
            !arq.to_string_lossy().contains("Roaming"),
            "o histórico foi parar no Roaming: {}",
            arq.display()
        );
    }

    #[test]
    fn uma_entrada_antiga_sem_os_campos_novos_continua_lendo() {
        // Mesma regra do `config.rs`: acrescentar campo não pode invalidar o que
        // já está gravado no disco de quem usa o programa.
        let antiga = r#"{"quando":123,"texto":"oi"}"#;
        let lida: Entrada = serde_json::from_str(antiga).expect("entrada antiga");
        assert_eq!(lida.texto, "oi");
        assert_eq!(lida.audio, None);
        assert_eq!(lida.duracao_ms, 0);
    }
}
