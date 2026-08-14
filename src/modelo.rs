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
    ["curl", "wget"].into_iter().find(|p| {
        Command::new("which")
            .arg(p)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
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

    let _ = std::thread::Builder::new()
        .name("baixar-modelo".into())
        .spawn(move || {
            let resultado = executar(&endereco, &destino, &saida, &sinal);
            if let Ok(pronto) = &resultado {
                let _ = tx.send(pronto.clone());
            }
            saida
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .fim
                .replace(resultado.map_err(|e| format!("{e:#}")));
            sinal.mudou();
        });

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
    let parcial = destino.with_extension("parcial");
    // Um download interrompido antes deixaria um arquivo pela metade, e a
    // retomada precisaria de um cabeçalho Range combinando com ele. Recomeçar é
    // mais simples e mais seguro do que adivinhar se o pedaço serve.
    let _ = std::fs::remove_file(&parcial);

    if let Some(total) = tamanho(prog, endereco) {
        andamento.lock().unwrap_or_else(|e| e.into_inner()).total = total;
        sinal.mudou();
    }

    let mut filho = match prog {
        "curl" => Command::new("curl")
            .args(["-L", "--fail", "--silent", "--show-error", "-o"])
            .arg(&parcial)
            .arg(endereco)
            .stderr(Stdio::piped())
            .spawn()?,
        _ => Command::new("wget")
            .args(["--quiet", "-O"])
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

    std::fs::rename(&parcial, destino)?;
    Ok(destino.to_path_buf())
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
