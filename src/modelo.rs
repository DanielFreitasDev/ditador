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

    // Todo modelo do whisper.cpp começa com a assinatura "ggml" (0x67676d6c).
    // Pega o caso em que um proxy ou portal cativo entregou uma página HTML com
    // o status 200 que o `--fail` do curl não recusa.
    let mut assinatura = [0u8; 4];
    std::fs::File::open(parcial)?.read_exact(&mut assinatura)?;
    if &assinatura != b"ggml" {
        anyhow::bail!(
            "o arquivo baixado não é um modelo do Whisper — \
             a rede pode ter devolvido uma página no lugar dele"
        );
    }
    Ok(())
}

/// Tamanho anunciado pelo servidor, para a barra ter escala. Sem isto o
/// download ainda funciona — a barra é que fica indeterminada.
fn tamanho(prog: &str, endereco: &str) -> Option<u64> {
    let saida = match prog {
        "curl" => Command::new("curl")
            .args([
                "-sIL",
                "-o",
                "/dev/null",
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
