//! Baixar o modelo do Whisper sem sair do programa.
//!
//! O modelo tem centenas de megabytes e não cabe num pacote de instalação, mas
//! exigir um comando no terminal para o programa começar a funcionar derruba
//! qualquer instalação feita por quem só quer usar. Então a tela de erro
//! oferece baixá-lo, com barra de progresso.
//!
//! Quem baixa é o `curl` (ou o `wget`), que todo Ubuntu tem: ele resolve TLS,
//! redirecionamento e retomada de download melhor do que valeria a pena
//! reimplementar, e evita arrastar uma pilha de HTTP inteira para dentro de um
//! programa que, fora isto, não fala com a rede.
//!
//! ## Como se sabe que o arquivo veio inteiro
//!
//! São três conferências, da mais barata para a mais cara, e todas antes de o
//! arquivo tomar o lugar do definitivo:
//!
//! 1. **o tamanho** bate com o que o servidor anunciou;
//! 2. **os quatro primeiros bytes** são a assinatura GGML — pega o caso do
//!    portal cativo que devolve uma página HTML com status 200, que o `--fail`
//!    do curl não recusa;
//! 3. **a soma SHA-256** bate com a esperada.
//!
//! A terceira é a que faltava, e é a única que pega o resto: um download
//! truncado que ainda assim tenha o tamanho certo no cabeçalho, um setor ruim
//! no disco, um proxy que reescreve bytes no meio. Sem ela o sintoma era o pior
//! possível — 574 MB no lugar certo, com o nome certo, que o whisper.cpp recusa
//! carregar com uma mensagem sobre formato inválido, e a instalação travada
//! sem ninguém entender por quê.
//!
//! ### De onde vem a soma esperada
//!
//! De dois lugares, nesta ordem. A tabela [`SOMAS`] é a fonte forte: são as
//! somas dos modelos que este programa oferece, escritas aqui e conferidas
//! contra o `lfs.oid` que a API da Hugging Face publica. A reserva é o
//! cabeçalho **`x-linked-etag`** da própria resposta, que para um arquivo LFS
//! *é* o SHA-256 — ele cobre qualquer modelo fora da tabela (quem apontar o
//! `--baixar-modelo` para outro nome) e continua pegando corrupção no
//! caminho, ainda que não sirva contra um servidor mentindo por inteiro.
//!
//! Sem nenhuma das duas, o download acontece com as conferências 1 e 2 e uma
//! linha no log dizendo que a soma não pôde ser verificada. É o certo: recusar
//! um modelo por não saber a soma dele deixaria sem saída quem baixa um modelo
//! afinado por conta própria.

use crate::config::models_dir;
use crate::state::Sinal;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Modelo sugerido: rápido o bastante para ditado e preciso em português.
pub const PADRAO: &str = "large-v3-turbo-q5_0";

/// As somas SHA-256 dos modelos que este programa oferece.
///
/// Tiradas do `lfs.oid` da API da Hugging Face
/// (`POST /api/models/ggerganov/whisper.cpp/paths-info/main`), que é onde ela
/// publica o hash de cada arquivo do Git LFS. Para conferir ou acrescentar um:
///
/// ```text
/// curl -s -X POST https://huggingface.co/api/models/ggerganov/whisper.cpp/paths-info/main \
///   -H 'Content-Type: application/json' \
///   -d '{"paths":["ggml-medium-q5_0.bin"]}' | jq -r '.[].lfs.oid'
/// ```
///
/// Os nomes são os mesmos que o `baixar-modelo.sh --lista` mostra, e há um
/// teste que confere que os dois lados não se separaram.
const SOMAS: &[(&str, &str)] = &[
    (
        "large-v3-turbo-q5_0",
        "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    ),
    (
        "large-v3-turbo",
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    ),
    (
        "large-v3-q5_0",
        "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
    ),
    (
        "large-v3",
        "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
    ),
    (
        "medium-q5_0",
        "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
    ),
    (
        "medium",
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    ),
    (
        "small-q5_1",
        "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
    ),
    (
        "small",
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    ),
    (
        "base",
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    ),
    (
        "tiny",
        "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    ),
];

/// A soma que este modelo deve ter, se ela for conhecida.
fn soma_conhecida(modelo: &str) -> Option<&'static str> {
    SOMAS
        .iter()
        .find(|(nome, _)| *nome == modelo)
        .map(|(_, soma)| *soma)
}

fn url(modelo: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{modelo}.bin")
}

pub fn caminho(modelo: &str) -> PathBuf {
    models_dir().join(format!("ggml-{modelo}.bin"))
}

/// O que a interface mostra enquanto o download acontece.
#[derive(Debug, Clone, Default)]
pub struct Progresso {
    pub baixados: u64,
    /// Zero enquanto o tamanho ainda não foi descoberto.
    pub total: u64,
    /// `None` enquanto anda; depois, o resultado.
    pub fim: Option<Result<PathBuf, String>>,
    /// Alguém pediu para parar. A ronda de `executar` lê isto, mata o curl e
    /// apaga o arquivo pela metade.
    ///
    /// São centenas de megabytes numa conexão doméstica: sem uma saída, quem
    /// clicou por engano — ou escolheu o modelo errado — ficava preso ao
    /// download por cinco a dez minutos, porque começar outro é recusado
    /// enquanto este ainda anda.
    pub cancelado: bool,
}

impl Progresso {
    /// Fração de 0 a 1, ou `None` quando o tamanho total é desconhecido.
    pub fn fracao(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.baixados as f32 / self.total as f32).clamp(0.0, 1.0))
    }

    pub fn andando(&self) -> bool {
        self.fim.is_none()
    }
}

pub type Andamento = Arc<Mutex<Progresso>>;

/// Verdadeiro se dá para baixar nesta máquina.
pub fn disponivel() -> bool {
    programa().is_some()
}

fn programa() -> Option<&'static str> {
    crate::programas::primeiro(&["curl", "wget"])
}

/// Começa o download numa thread. Devolve o andamento, que a interface lê a
/// cada repintura, e um canal que entrega o resultado uma vez, para quem
/// precisa agir quando terminar. `sinal` acorda a interface a cada avanço.
pub fn baixar(modelo: &str, sinal: Sinal) -> (Andamento, crossbeam_channel::Receiver<PathBuf>) {
    let andamento: Andamento = Arc::new(Mutex::new(Progresso::default()));
    let (tx, rx) = crossbeam_channel::bounded(1);
    let destino = caminho(modelo);
    let endereco = url(modelo);
    let esperada = soma_conhecida(modelo).map(str::to_string);
    let saida = andamento.clone();

    let thread = std::thread::Builder::new()
        .name("baixar-modelo".into())
        .spawn({
            let saida = saida.clone();
            move || {
                let resultado = executar(&endereco, &destino, esperada.as_deref(), &saida, &sinal);
                if let Ok(pronto) = &resultado {
                    let _ = tx.send(pronto.clone());
                }
                if let Err(e) = &resultado {
                    log::error!("o download do modelo falhou: {e:#}");
                }
                saida
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .fim
                    .replace(resultado.map_err(|e| format!("{e:#}")));
                sinal.mudou();
            }
        });

    // A thread que não nasce precisa marcar o fim, senão `andando()` fica
    // verdadeiro para sempre: a barra congela em zero, o botão de baixar some
    // (porque um download está "em curso") e a tela vira um beco sem saída até
    // alguém reiniciar o programa.
    if let Err(e) = thread {
        log::error!("não consegui iniciar o download: {e}");
        saida
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .fim
            .replace(Err(format!("não consegui iniciar o download: {e}")));
    }

    (andamento, rx)
}

fn executar(
    endereco: &str,
    destino: &Path,
    soma_esperada: Option<&str>,
    andamento: &Andamento,
    sinal: &Sinal,
) -> anyhow::Result<PathBuf> {
    let Some(prog) = programa() else {
        anyhow::bail!("preciso do curl ou do wget para baixar (sudo apt install curl)");
    };

    if let Some(pasta) = destino.parent() {
        std::fs::create_dir_all(pasta)?;
    }

    // O temporário leva o número do processo. Com um nome fixo, o botão da
    // janela e um `ditador --baixar-modelo` num terminal — que é justamente o
    // que a pessoa faz quando a janela parece travada — apontavam para o mesmo
    // arquivo: o segundo apagava o do primeiro, o progresso passava a medir o
    // arquivo errado, e quem terminasse antes renomeava para o destino um
    // arquivo que o outro ainda estava escrevendo.
    let parcial = destino.with_extension(format!("{}.parcial", std::process::id()));
    // Um download interrompido antes deixaria um arquivo pela metade, e a
    // retomada precisaria de um cabeçalho Range combinando com ele. Recomeçar é
    // mais simples e mais seguro do que adivinhar se o pedaço serve.
    let _ = std::fs::remove_file(&parcial);

    let (total, soma_anunciada) = cabecalhos(prog, endereco);
    if let Some(total) = total {
        andamento.lock().unwrap_or_else(|e| e.into_inner()).total = total;
        sinal.mudou();
    }
    // A tabela ganha do cabeçalho: ela é nossa e foi conferida antes; o
    // cabeçalho é do servidor de agora. A reserva existe para o modelo que não
    // está na tabela — e é melhor conferir contra o que o próprio servidor
    // declarou do que não conferir nada.
    let soma_esperada = soma_esperada.or(soma_anunciada.as_deref());

    let mut filho = match prog {
        // Os prazos importam: sem eles, uma conexão que abre e para de entregar
        // bytes deixa a barra congelada até o keepalive do kernel desistir —
        // dez minutos ou mais —, e nesse meio-tempo o programa recusa começar
        // outro download porque este ainda está "andando".
        "curl" => Command::new("curl")
            .args([
                "-L",
                "--fail",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "20",
                // Menos de 1 kB/s por 60 s seguidos é conexão morta.
                "--speed-limit",
                "1024",
                "--speed-time",
                "60",
                "--retry",
                "2",
                "-o",
            ])
            .arg(&parcial)
            .arg(endereco)
            .stderr(Stdio::piped())
            .spawn()?,
        _ => Command::new("wget")
            .args([
                "--quiet",
                "--timeout=20",
                "--read-timeout=60",
                "--tries=3",
                "-O",
            ])
            .arg(&parcial)
            .arg(endereco)
            .stderr(Stdio::piped())
            .spawn()?,
    };

    // O progresso sai do tamanho do arquivo, não da saída do programa: as duas
    // ferramentas relatam de jeitos diferentes, e o arquivo não mente.
    loop {
        match filho.try_wait()? {
            Some(status) => {
                if !status.success() {
                    let _ = std::fs::remove_file(&parcial);
                    let mut erro = String::new();
                    if let Some(mut saida) = filho.stderr.take() {
                        use std::io::Read as _;
                        let _ = saida.read_to_string(&mut erro);
                    }
                    let erro = erro.trim();
                    anyhow::bail!(if erro.is_empty() {
                        "o download falhou; verifique a conexão".to_string()
                    } else {
                        erro.to_string()
                    });
                }
                break;
            }
            None => {
                if andamento
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cancelado
                {
                    let _ = filho.kill();
                    let _ = filho.wait();
                    let _ = std::fs::remove_file(&parcial);
                    log::info!("download do modelo cancelado");
                    anyhow::bail!("Download cancelado.");
                }
                if let Ok(meta) = std::fs::metadata(&parcial) {
                    let mut p = andamento.lock().unwrap_or_else(|e| e.into_inner());
                    if p.baixados != meta.len() {
                        p.baixados = meta.len();
                        drop(p);
                        sinal.mudou();
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }

    conferir(&parcial, total, soma_esperada).inspect_err(|_| {
        let _ = std::fs::remove_file(&parcial);
    })?;
    std::fs::rename(&parcial, destino)?;
    Ok(destino.to_path_buf())
}

/// O arquivo baixado é mesmo um modelo GGML inteiro?
///
/// Sem esta conferência, qualquer arquivo ruim que chegasse ao destino trancava
/// a instalação inteira: `modelo_faltando` decide só por `exists()`, então o
/// botão de baixar nunca mais aparecia; `--baixar-modelo` e o script respondiam
/// "já existe"; e o único botão restante recarregava eternamente o mesmo
/// arquivo, enquanto a tela de configurações dizia "Arquivo encontrado".
fn conferir(parcial: &Path, total: Option<u64>, soma: Option<&str>) -> anyhow::Result<()> {
    use std::io::Read as _;

    let tamanho_real = std::fs::metadata(parcial)?.len();
    if let Some(total) = total
        && total != tamanho_real
    {
        anyhow::bail!(
            "o download veio incompleto ({} de {}); tente de novo",
            tamanho_legivel(tamanho_real),
            tamanho_legivel(total)
        );
    }

    // Todo modelo do whisper.cpp começa com a assinatura GGML, o inteiro
    // 0x67676d6c. Pega o caso em que um proxy ou portal cativo entregou uma
    // página HTML com o status 200 que o `--fail` do curl não recusa.
    //
    // **A leitura é como inteiro little-endian, e não como os quatro caracteres
    // "ggml".** O whisper.cpp grava o número na ordem nativa da máquina, que em
    // x86 e ARM é little-endian: no disco os bytes saem invertidos, `6c 6d 67
    // 67`, que lidos como texto dão "lmgg".
    //
    // Esta conferência já esteve escrita como `assinatura != b"ggml"` e **nunca
    // podia passar** — nem no Linux. Ninguém percebeu porque ela só roda no fim
    // de um download, e quem programou já tinha o modelo no disco: o
    // `--baixar-modelo` responde "já está aqui" antes de chegar até aqui. O erro
    // apareceu na primeira máquina que baixou o modelo do zero, que por acaso
    // foi a do Windows — e a mensagem acusava a rede de ter devolvido uma página
    // depois de 573 MB baixados corretamente.
    const ASSINATURA_GGML: u32 = 0x6767_6d6c;

    let mut assinatura = [0u8; 4];
    std::fs::File::open(parcial)?.read_exact(&mut assinatura)?;
    if u32::from_le_bytes(assinatura) != ASSINATURA_GGML {
        anyhow::bail!(
            "o arquivo baixado não é um modelo do Whisper — \
             a rede pode ter devolvido uma página no lugar dele"
        );
    }

    // A soma por último: ela lê os 574 MB inteiros, e não faz sentido pagar
    // isso por um arquivo que as duas conferências acima já reprovaram.
    let Some(esperada) = soma else {
        log::info!(
            "soma de verificação desconhecida para este modelo; \
             conferidos só o tamanho e a assinatura"
        );
        return Ok(());
    };
    let calculada = somar(parcial)?;
    if !calculada.eq_ignore_ascii_case(esperada) {
        anyhow::bail!(
            "o arquivo baixado não confere com a soma de verificação \
             (esperava {}…, veio {}…). O download veio corrompido; tente de novo",
            &esperada[..esperada.len().min(12)],
            &calculada[..calculada.len().min(12)]
        );
    }
    log::info!("soma de verificação confere: {}…", &calculada[..12]);
    Ok(())
}

/// O SHA-256 do arquivo, em hexadecimal minúsculo.
///
/// Em blocos, e não com o arquivo inteiro na memória: são 574 MB no modelo
/// padrão e 3,1 GB no `large-v3`, e ler tudo de uma vez para depois somar
/// dobraria o pico de memória do programa no pior momento possível — logo
/// depois de um download, com o contexto do Whisper prestes a ser carregado.
fn somar(caminho: &Path) -> anyhow::Result<String> {
    use std::io::Read as _;

    let mut arquivo = std::io::BufReader::new(std::fs::File::open(caminho)?);
    let mut hash = Sha256::new();
    let mut bloco = vec![0u8; 1 << 20];
    loop {
        let lidos = arquivo.read(&mut bloco)?;
        if lidos == 0 {
            break;
        }
        hash.update(&bloco[..lidos]);
    }
    Ok(hash
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        }))
}

/// Para onde mandar a saída que não interessa.
#[cfg(target_os = "windows")]
const DESCARTE: &str = "NUL";
#[cfg(not(target_os = "windows"))]
const DESCARTE: &str = "/dev/null";

/// O que os cabeçalhos da resposta contam antes de o download começar: o
/// tamanho, para a barra ter escala, e a soma de verificação que o servidor
/// declara. Sem nenhum dos dois o download ainda funciona.
fn cabecalhos(prog: &str, endereco: &str) -> (Option<u64>, Option<String>) {
    let saida = match prog {
        "curl" => Command::new("curl")
            .args([
                "-sIL",
                "-o",
                // O buraco negro do sistema. No Windows não existe `/dev/null`:
                // o curl trataria isso como um caminho relativo e tentaria criar
                // uma pasta `dev` no diretório de trabalho — que pode ser o
                // `C:\Windows\System32` de onde o Explorer lançou o programa.
                DESCARTE,
                "-w",
                "%{size_download}\n%{header_json}",
            ])
            .arg(endereco)
            .output(),
        _ => Command::new("wget")
            .args(["--spider", "--server-response", "-q"])
            .arg(endereco)
            .output(),
    };
    let Ok(saida) = saida else {
        return (None, None);
    };
    let texto = String::from_utf8_lossy(&saida.stdout) + String::from_utf8_lossy(&saida.stderr);
    (content_length(&texto), linked_etag(&texto))
}

/// O `x-linked-etag` da resposta, que num arquivo do Git LFS **é** o SHA-256.
///
/// A Hugging Face o expõe por `Access-Control-Expose-Headers` e o manda em toda
/// resposta de `resolve/`. Não confundir com o `etag` comum: aquele, na
/// resposta final do CDN, é outro valor (o hash do Xet), e conferir contra ele
/// reprovaria todo download bom.
///
/// O primeiro, e não o último: ele vem no redirecionamento, e a resposta final
/// do CDN não o repete.
fn linked_etag(texto: &str) -> Option<String> {
    const CHAVE: &str = "x-linked-etag";
    let baixo = texto.to_lowercase();
    let i = baixo.find(CHAVE)?;
    // O valor vem entre aspas, no formato `"abc123…"`. Fora de aspas, não é o
    // que esperamos e é melhor não conferir nada do que conferir contra lixo.
    let resto = &texto[i + CHAVE.len()..];
    let fim_da_linha = resto.find('\n').unwrap_or(resto.len());
    let linha = &resto[..fim_da_linha];
    let inicio = linha.find('"')? + 1;
    let fim = linha[inicio..].find('"')? + inicio;
    let valor = &linha[inicio..fim];

    // Um SHA-256 em hexadecimal tem 64 caracteres. A Hugging Face também usa
    // este cabeçalho para o hash de arquivos que **não** são LFS, onde o valor é
    // um SHA-1 do Git (40 caracteres) — e conferir um arquivo contra o hash
    // errado reprovaria todo download bom.
    (valor.len() == 64 && valor.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| valor.to_ascii_lowercase())
}

/// Último `content-length` do texto — o último porque o endereço redireciona, e
/// só o destino final anuncia o tamanho de verdade.
fn content_length(texto: &str) -> Option<u64> {
    let baixo = texto.to_lowercase();
    let mut achado = None;
    let mut resto = baixo.as_str();
    while let Some(i) = resto.find("content-length") {
        resto = &resto[i + "content-length".len()..];
        let numero: String = resto
            .chars()
            .skip_while(|c| !c.is_ascii_digit() && *c != '\n')
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = numero.parse::<u64>()
            && n > 0
        {
            achado = Some(n);
        }
    }
    achado
}

/// "574 MB" — em unidades decimais, que é como o tamanho de um download é
/// anunciado em todo lugar (e como o `baixar-modelo.sh` e o README contam).
///
/// A faixa dos quilobytes existe por causa de dois usos que não são o modelo: o
/// começo de um download, onde os primeiros segundos mostravam "0 MB" enquanto os
/// bytes chegavam, e o tamanho do histórico na tela das transcrições — que com
/// quarenta e oito mil bytes guardados anunciava "0 MB", ou seja, nada.
pub fn tamanho_legivel(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0).replace('.', ",")
    } else if mb >= 1.0 {
        format!("{mb:.0} MB")
    } else {
        // Arredondando para cima a partir de meio quilobyte: um histórico com
        // uma linha não pode dizer "0 kB".
        format!(
            "{:.0} kB",
            (bytes as f64 / 1e3).max(if bytes > 0 { 1.0 } else { 0.0 })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Os bytes que um modelo do Whisper tem mesmo no começo do arquivo.
    ///
    /// Escritos aqui na ordem em que aparecem no disco — conferidos numa
    /// requisição `Range: 0-15` ao próprio servidor da Hugging Face, e não
    /// deduzidos da constante. É a diferença entre este teste e o bug que ele
    /// existe para impedir: a versão anterior de `conferir` comparava com
    /// `b"ggml"`, que é o mesmo número na ordem contrária, e recusava **todo**
    /// download bem-sucedido.
    const COMECO_DE_UM_MODELO: [u8; 8] = [0x6c, 0x6d, 0x67, 0x67, 0x9a, 0xca, 0x00, 0x00];

    fn arquivo_de_teste(nome: &str, conteudo: &[u8]) -> std::path::PathBuf {
        let caminho =
            std::env::temp_dir().join(format!("ditador-modelo-{}-{nome}", std::process::id()));
        std::fs::write(&caminho, conteudo).expect("gravando o arquivo do teste");
        caminho
    }

    #[test]
    fn aceita_um_modelo_de_verdade_e_recusa_uma_pagina_html() {
        let bom = arquivo_de_teste("bom.bin", &COMECO_DE_UM_MODELO);
        assert!(
            conferir(&bom, Some(COMECO_DE_UM_MODELO.len() as u64), None).is_ok(),
            "recusou um arquivo que começa exatamente como todo modelo do Whisper"
        );

        // O caso que a conferência existe para pegar: um portal cativo ou proxy
        // devolvendo uma página com status 200, que o `--fail` do curl não recusa.
        let pagina = arquivo_de_teste("pagina.html", b"<!DOCTYPE html><html>");
        assert!(conferir(&pagina, None, None).is_err());

        // E o download que veio pela metade, que tem a assinatura certa mas não
        // o tamanho anunciado.
        assert!(
            conferir(&bom, Some(999_999), None).is_err(),
            "aceitou um download incompleto"
        );

        for caminho in [bom, pagina] {
            let _ = std::fs::remove_file(caminho);
        }
    }

    #[test]
    fn a_soma_de_verificacao_separa_o_arquivo_bom_do_corrompido() {
        // O caso que só a soma pega: tamanho certo, assinatura certa, bytes
        // trocados no meio. Sem esta conferência ele chegava ao destino, e o
        // whisper.cpp recusava carregá-lo com uma mensagem sobre formato
        // inválido — com a instalação travada e nada explicando o porquê.
        let bom = arquivo_de_teste("soma-bom.bin", &COMECO_DE_UM_MODELO);
        let soma = somar(&bom).expect("somando");
        assert_eq!(soma.len(), 64, "o hexadecimal saiu do tamanho errado");
        assert!(soma.chars().all(|c| c.is_ascii_hexdigit()));

        assert!(
            conferir(&bom, None, Some(&soma)).is_ok(),
            "recusou um arquivo cuja soma é exatamente a esperada"
        );
        // Maiúsculas e minúsculas dão na mesma: a tabela é minúscula, mas um
        // cabeçalho pode vir de outro jeito.
        assert!(conferir(&bom, None, Some(&soma.to_uppercase())).is_ok());

        // Um byte trocado no meio, com o mesmo tamanho e a mesma assinatura.
        let mut estragado = COMECO_DE_UM_MODELO;
        estragado[6] ^= 0xFF;
        let ruim = arquivo_de_teste("soma-ruim.bin", &estragado);
        let erro = conferir(&ruim, Some(estragado.len() as u64), Some(&soma))
            .expect_err("aceitou um arquivo corrompido");
        let texto = format!("{erro:#}");
        assert!(
            texto.contains("soma de verificação"),
            "a mensagem não diz o que houve: {texto}"
        );

        for caminho in [bom, ruim] {
            let _ = std::fs::remove_file(caminho);
        }
    }

    #[test]
    fn sem_soma_conhecida_o_download_ainda_acontece() {
        // Recusar um modelo por não saber a soma dele deixaria sem saída quem
        // baixa um modelo afinado por conta própria.
        let bom = arquivo_de_teste("sem-soma.bin", &COMECO_DE_UM_MODELO);
        assert!(conferir(&bom, None, None).is_ok());
        let _ = std::fs::remove_file(bom);
    }

    #[test]
    fn a_soma_e_a_do_sha256_de_verdade() {
        // Contra um vetor conhecido, e não contra nós mesmos: uma implementação
        // que devolvesse sempre o mesmo valor passaria em todos os testes
        // acima.
        let vazio = arquivo_de_teste("vazio.bin", b"");
        assert_eq!(
            somar(&vazio).expect("somando"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = arquivo_de_teste("abc.bin", b"abc");
        assert_eq!(
            somar(&abc).expect("somando"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        for caminho in [vazio, abc] {
            let _ = std::fs::remove_file(caminho);
        }
    }

    #[test]
    fn a_soma_em_blocos_bate_com_a_do_arquivo_inteiro() {
        // O modelo tem 574 MB e é lido em blocos de 1 MB. Um erro na costura
        // entre blocos só apareceria num arquivo maior que um bloco — que é o
        // que este teste faz, sem chegar perto de meio giga.
        let bytes: Vec<u8> = (0..(3 * (1 << 20) + 12345))
            .map(|i| (i % 251) as u8)
            .collect();
        let grande = arquivo_de_teste("grande.bin", &bytes);
        let em_blocos = somar(&grande).expect("somando");

        let mut de_uma_vez = Sha256::new();
        de_uma_vez.update(&bytes);
        let esperado: String = de_uma_vez
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(em_blocos, esperado);
        let _ = std::fs::remove_file(grande);
    }

    #[test]
    fn as_somas_conhecidas_tem_a_forma_de_um_sha256() {
        // Um caractere a mais, um a menos ou um "g" copiado errado da API só
        // apareceria no fim de um download de 574 MB — e como uma acusação de
        // corrupção contra um arquivo perfeito.
        for (nome, soma) in SOMAS {
            assert_eq!(soma.len(), 64, "a soma de {nome} não tem 64 caracteres");
            assert!(
                soma.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                "a soma de {nome} não é hexadecimal minúsculo: {soma}"
            );
        }
        // E nenhum nome repetido, que faria a segunda entrada nunca ser usada.
        let mut nomes: Vec<&str> = SOMAS.iter().map(|(n, _)| *n).collect();
        nomes.sort_unstable();
        let antes = nomes.len();
        nomes.dedup();
        assert_eq!(antes, nomes.len(), "há nome repetido na tabela de somas");
    }

    #[test]
    fn o_modelo_padrao_tem_soma_conhecida() {
        // O que a esmagadora maioria das instalações baixa. Se algum dia ele
        // mudar de nome, é aqui que se descobre.
        assert!(
            soma_conhecida(PADRAO).is_some(),
            "o modelo padrão ({PADRAO}) ficou sem soma de verificação"
        );
        assert!(soma_conhecida("um-modelo-que-ninguem-tem").is_none());
    }

    #[test]
    fn os_modelos_do_script_estao_todos_na_tabela() {
        // O `baixar-modelo.sh --lista` é a lista que o README manda consultar.
        // Um modelo oferecido lá e ausente daqui baixaria sem conferência de
        // soma, em silêncio.
        let script = include_str!("../baixar-modelo.sh");
        let oferecidos: Vec<&str> = script
            .lines()
            // As linhas da lista têm o nome recuado e o tamanho em seguida:
            // `  large-v3-turbo-q5_0   ~574 MB   …`
            .filter_map(|linha| {
                let recuado = linha.strip_prefix("  ")?;
                let nome = recuado.split_whitespace().next()?;
                recuado
                    .contains("MB")
                    .then_some(nome)
                    .or_else(|| recuado.contains("GB").then_some(nome))
            })
            .collect();
        assert!(
            !oferecidos.is_empty(),
            "não achei a lista de modelos no baixar-modelo.sh; \
             o formato dela mudou e este teste precisa acompanhar"
        );
        for nome in oferecidos {
            assert!(
                soma_conhecida(nome).is_some(),
                "o modelo {nome} é oferecido pelo baixar-modelo.sh e não tem \
                 soma de verificação na tabela SOMAS de src/modelo.rs"
            );
        }
    }

    #[test]
    fn o_linked_etag_e_lido_e_o_etag_comum_e_ignorado() {
        // Os dois cabeçalhos existem na mesma resposta e têm valores
        // diferentes: o `etag` final é o hash do Xet, e conferir contra ele
        // reprovaria todo download bom.
        let resposta = "HTTP/2 302\r\n\
             x-linked-etag: \"394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2\"\r\n\
             \r\n\
             HTTP/2 200\r\n\
             etag: \"9c7b9c6bf60cf555f34fe7d81e8643764ff03d2f60b6fa550f5630be52eef830\"\r\n\
             content-length: 574041195\r\n\r\n";
        assert_eq!(
            linked_etag(resposta).as_deref(),
            Some("394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2")
        );
        assert_eq!(content_length(resposta), Some(574_041_195));

        // Sem o cabeçalho, nada — e o download segue com as outras conferências.
        assert_eq!(linked_etag("HTTP/2 200\r\netag: \"abc\"\r\n"), None);
        // Um SHA-1 do Git (40 caracteres) não serve, e aceitá-lo reprovaria um
        // arquivo bom.
        assert_eq!(
            linked_etag("x-linked-etag: \"0e2474d5ec0361bb1726829aa83317ed4cbc3f18\"\r\n"),
            None
        );
        // Valor fora de aspas, ou com lixo dentro, também não.
        assert_eq!(linked_etag("x-linked-etag: abc\r\n"), None);
        assert_eq!(
            linked_etag(&format!("x-linked-etag: \"{}\"\r\n", "z".repeat(64))),
            None
        );
    }

    #[test]
    fn o_descarte_e_o_do_sistema() {
        // `/dev/null` não existe no Windows: o curl o trataria como caminho
        // relativo e tentaria criar uma pasta `dev` no diretório de trabalho —
        // que, lançado pelo Explorer, pode ser o System32.
        #[cfg(target_os = "windows")]
        assert_eq!(DESCARTE, "NUL");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(DESCARTE, "/dev/null");
    }

    #[test]
    fn pega_o_content_length_do_destino_final() {
        // A resposta do redirecionamento vem primeiro e anuncia outro tamanho;
        // quem vale é a última.
        let texto = "HTTP/1.1 302 Found\r\nContent-Length: 271\r\n\r\n\
                     HTTP/1.1 200 OK\r\ncontent-length: 601397440\r\n\r\n";
        assert_eq!(content_length(texto), Some(601_397_440));
        assert_eq!(content_length("HTTP/1.1 200 OK\r\n"), None);
    }

    #[test]
    fn a_fracao_so_existe_com_tamanho_conhecido() {
        let mut p = Progresso::default();
        assert_eq!(p.fracao(), None);
        assert!(p.andando());
        p.total = 200;
        p.baixados = 50;
        assert_eq!(p.fracao(), Some(0.25));
        // Um servidor que anuncie menos do que entrega não estoura a barra.
        p.baixados = 400;
        assert_eq!(p.fracao(), Some(1.0));
    }

    #[test]
    fn o_tamanho_sai_legivel() {
        assert_eq!(tamanho_legivel(574_041_195), "574 MB");
        assert_eq!(tamanho_legivel(3_300_000_000), "3,3 GB");
        // Abaixo de um megabyte a resposta era "0 MB", que é o mesmo que não
        // responder — e ela aparecia no começo de todo download e na tela das
        // transcrições de quem tem poucas.
        assert_eq!(tamanho_legivel(48_320), "48 kB");
        assert_eq!(tamanho_legivel(1_500_000), "2 MB");
        assert_eq!(tamanho_legivel(0), "0 kB");
        // Uma linha de histórico não pode dizer "0 kB".
        assert_eq!(tamanho_legivel(120), "1 kB");
    }

    #[test]
    fn o_endereco_aponta_para_o_arquivo_ggml() {
        assert!(url(PADRAO).ends_with("/ggml-large-v3-turbo-q5_0.bin"));
        assert!(caminho(PADRAO).ends_with("ggml-large-v3-turbo-q5_0.bin"));
    }
}
