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

use crate::config::models_dir;
use crate::state::Sinal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Modelo sugerido: rápido o bastante para ditado e preciso em português.
pub const PADRAO: &str = "large-v3-turbo-q5_0";

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
    let saida = andamento.clone();

    let thread = std::thread::Builder::new()
        .name("baixar-modelo".into())
        .spawn({
            let saida = saida.clone();
            move || {
                let resultado = executar(&endereco, &destino, &saida, &sinal);
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

    let total = tamanho(prog, endereco);
    if let Some(total) = total {
        andamento.lock().unwrap_or_else(|e| e.into_inner()).total = total;
        sinal.mudou();
    }

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

    conferir(&parcial, total).inspect_err(|_| {
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
fn conferir(parcial: &Path, total: Option<u64>) -> anyhow::Result<()> {
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
    Ok(())
}

/// Para onde mandar a saída que não interessa.
#[cfg(target_os = "windows")]
const DESCARTE: &str = "NUL";
#[cfg(not(target_os = "windows"))]
const DESCARTE: &str = "/dev/null";

/// Tamanho anunciado pelo servidor, para a barra ter escala. Sem isto o
/// download ainda funciona — a barra é que fica indeterminada.
fn tamanho(prog: &str, endereco: &str) -> Option<u64> {
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
            .output()
            .ok()?,
        _ => Command::new("wget")
            .args(["--spider", "--server-response", "-q"])
            .arg(endereco)
            .output()
            .ok()?,
    };
    let texto = String::from_utf8_lossy(&saida.stdout) + String::from_utf8_lossy(&saida.stderr);
    content_length(&texto)
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

/// "574 MB" — em megabytes decimais, que é como o tamanho de um download é
/// anunciado em todo lugar (e como o `baixar-modelo.sh` e o README contam).
pub fn tamanho_legivel(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0).replace('.', ",")
    } else {
        format!("{mb:.0} MB")
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
            conferir(&bom, Some(COMECO_DE_UM_MODELO.len() as u64)).is_ok(),
            "recusou um arquivo que começa exatamente como todo modelo do Whisper"
        );

        // O caso que a conferência existe para pegar: um portal cativo ou proxy
        // devolvendo uma página com status 200, que o `--fail` do curl não recusa.
        let pagina = arquivo_de_teste("pagina.html", b"<!DOCTYPE html><html>");
        assert!(conferir(&pagina, None).is_err());

        // E o download que veio pela metade, que tem a assinatura certa mas não
        // o tamanho anunciado.
        assert!(
            conferir(&bom, Some(999_999)).is_err(),
            "aceitou um download incompleto"
        );

        for caminho in [bom, pagina] {
            let _ = std::fs::remove_file(caminho);
        }
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
    }

    #[test]
    fn o_endereco_aponta_para_o_arquivo_ggml() {
        assert!(url(PADRAO).ends_with("/ggml-large-v3-turbo-q5_0.bin"));
        assert!(caminho(PADRAO).ends_with("ggml-large-v3-turbo-q5_0.bin"));
    }
}
