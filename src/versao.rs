//! Saber que saiu uma versão nova.
//!
//! ## Por que isto existe
//!
//! Este programa não se atualiza sozinho e não deveria: quem instalou pelo
//! `.deb` espera que quem mande no arquivo seja o `dpkg`, e um aplicativo que
//! troca o próprio binário por baixo do gerenciador de pacotes é a receita de
//! uma instalação que ninguém mais consegue explicar. Só que a alternativa que
//! havia até aqui era pior: **nenhuma**. Quem instalou a 0.5 continua na 0.5
//! para sempre, sem nunca saber que o atalho que não pegava foi consertado três
//! versões atrás.
//!
//! O meio-termo é este: o programa **avisa**, e quem atualiza é a pessoa, pelo
//! caminho que ela escolheu para instalar.
//!
//! ## O que sai daqui para a rede, exatamente
//!
//! Um `GET` por dia em `api.github.com`, pedindo o último *release* publicado
//! deste repositório. Nada é enviado: nem qual versão está instalada, nem qual
//! sistema, nem identificador nenhum — a resposta é a mesma para todo mundo, e
//! a comparação acontece aqui dentro. O GitHub vê o que qualquer servidor vê de
//! qualquer visita: um endereço IP e o `User-Agent` do `curl`.
//!
//! Isso é dito com todas as letras porque o README deste programa promete que
//! o áudio nunca sai da máquina, e essa promessa continua inteira — o
//! microfone, a transcrição e o histórico não têm nada a ver com esta conexão.
//! Ainda assim, **quem não quiser nenhuma conexão tem um interruptor** em
//! *Configurações → Sistema*, e desligado ele é desligado de verdade: a thread
//! desta conferência nem chega a ser criada.
//!
//! ## Por que o `curl`, e não uma pilha de HTTP
//!
//! Pelo mesmo motivo do `src/modelo.rs`, que já baixa centenas de megabytes
//! assim: trazer `reqwest` e o `rustls` inteiro para dentro de um programa de
//! dezesseis dependências, a fim de buscar uma linha de JSON por dia, seria
//! multiplicar a árvore de dependências pelo recurso mais dispensável que
//! existe aqui.

use crate::state::{SharedState, Sinal, lock};
use serde::Deserialize;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// De quanto em quanto tempo se pergunta de novo.
///
/// Um dia. O programa fica meses de pé sob o serviço de usuário do systemd, e
/// perguntar a cada arranque só serviria a quem reinicia — que é justamente
/// quem menos precisa, porque acabou de mexer na máquina.
const INTERVALO: Duration = Duration::from_secs(24 * 60 * 60);

/// Quanto tempo se espera antes da primeira conferência.
///
/// O arranque deste programa já tem o que fazer: carregar 574 MB de modelo,
/// abrir o microfone, subir a janela e pegar o nome no barramento. Uma conexão
/// de rede competindo com isso atrasaria o que a pessoa está esperando para
/// resolver o que ninguém pediu. Trinta segundos depois, a casa está em ordem.
const ESPERA_INICIAL: Duration = Duration::from_secs(30);

/// O que se lê da resposta do GitHub. Todo o resto do JSON é ignorado.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    /// A página do release, que é o que a pessoa quer abrir para baixar.
    html_url: String,
}

/// Uma versão semântica, do jeito que este projeto as publica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Versao {
    maior: u64,
    menor: u64,
    correcao: u64,
}

impl Versao {
    /// Lê `0.7.3`, `v0.7.3` e `0.7.3-rc1` — o sufixo é ignorado.
    ///
    /// A ordem dos campos na struct **é** a ordem da comparação: o `Ord`
    /// derivado compara maior, depois menor, depois correção, que é exatamente
    /// a regra do versionamento semântico. Escrever um `cmp` à mão aqui seria
    /// reimplementar o que o derive já faz certo.
    pub fn ler(texto: &str) -> Option<Self> {
        let texto = texto.trim().trim_start_matches(['v', 'V']);
        // Um sufixo de pré-lançamento não entra na conta. Comparar "0.8.0-rc1"
        // com "0.8.0" pelo número daria empate, e é a resposta certa para o que
        // se quer aqui: não avisar de novo sobre a versão que já se tem.
        let numero = texto
            .split(['-', '+'])
            .next()
            .unwrap_or(texto)
            .trim_end_matches(|c: char| !c.is_ascii_digit());
        let mut partes = numero.split('.');
        let mut proxima = || partes.next()?.parse::<u64>().ok();
        Some(Self {
            maior: proxima()?,
            menor: proxima()?,
            correcao: proxima().unwrap_or(0),
        })
    }

    /// A versão deste binário.
    pub fn desta_compilacao() -> Self {
        // O `CARGO_PKG_VERSION` é gerado pelo Cargo a partir do `Cargo.toml` e
        // sempre tem os três números. Um `expect` aqui seria pânico em código
        // que roda no arranque; o zero é o valor que nunca acusa versão nova, e
        // portanto o que erra para o lado seguro.
        Self::ler(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            maior: 0,
            menor: 0,
            correcao: 0,
        })
    }
}

/// A novidade que a interface mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Novidade {
    /// Como o número aparece na tela: `0.8.0`, sem o `v` da tag.
    pub versao: String,
    /// A página do release, para copiar.
    pub endereco: String,
}

/// Já existe uma vigília de pé neste processo?
///
/// Existe porque `vigiar` é chamado de dois lugares — o arranque, quando a opção
/// já está ligada, e o Salvar das configurações, quando ela **acaba** de ser
/// ligada. Sem esta trava, desligar e religar a opção duas vezes numa tarde
/// deixaria três threads perguntando a mesma coisa ao GitHub: a que estava lá
/// não morre na hora em que a opção é desligada, e sim na volta seguinte dela,
/// que pode estar a um dia de distância.
static VIGIANDO: AtomicBool = AtomicBool::new(false);

/// Acompanha os lançamentos enquanto o programa estiver de pé.
///
/// Não devolve nada e não bloqueia: escreve em `state.versao_nova` quando
/// houver o que dizer e acorda a interface. Chamada só quando a configuração
/// permite — veja o `//!` do módulo.
pub fn vigiar(shared: SharedState, sinal: Sinal) {
    if !reservar_a_vigilia() {
        log::debug!("já há uma vigília de versão de pé");
        return;
    }
    let criada = std::thread::Builder::new()
        .name("versao".into())
        .spawn(move || {
            std::thread::sleep(ESPERA_INICIAL);
            loop {
                // A configuração é relida a cada volta, e não capturada uma vez:
                // quem desligar o aviso com o programa de pé não deve continuar
                // sendo consultado todo dia até reiniciar.
                if !lock(&shared).config.aviso_de_versao {
                    log::debug!("aviso de versão desligado; a vigília encerra");
                    liberar_a_vigilia();
                    return;
                }
                if let Some(nova) = conferir() {
                    log::info!("versão nova disponível: {}", nova.versao);
                    lock(&shared).versao_nova = Some(nova);
                    sinal.mudou();
                }
                std::thread::sleep(INTERVALO);
            }
        });
    if let Err(e) = criada {
        // Sem esta thread ninguém fica sabendo de versão nova, e é só isso: o
        // programa transcreve igual. Uma linha no log e a vida segue.
        log::warn!("não consegui acompanhar os lançamentos: {e}");
        liberar_a_vigilia();
    }
}

/// Toma o lugar da vigília para este chamador. Falso quer dizer "já tem uma".
fn reservar_a_vigilia() -> bool {
    !VIGIANDO.swap(true, Ordering::SeqCst)
}

/// Devolve o lugar, para que ligar a opção de novo volte a criar a thread.
fn liberar_a_vigilia() {
    VIGIANDO.store(false, Ordering::SeqCst);
}

/// Pergunta uma vez. `None` quando não há novidade, quando não há rede ou
/// quando a resposta não faz sentido.
fn conferir() -> Option<Novidade> {
    novidade_de(buscar(&endereco_da_api())?, Versao::desta_compilacao())
}

/// O que esta release representa para quem está na versão `atual`.
///
/// Separada do `conferir` para poder ser testada sem rede: a regra que decide se
/// alguém vê um aviso é justamente a parte que não pode errar — avisar de menos
/// deixa a pessoa parada numa versão velha, e avisar de mais (a cada arranque,
/// sobre a versão que ela já tem) é o tipo de coisa que faz gente desligar o
/// recurso e nunca mais ligar.
fn novidade_de(release: Release, atual: Versao) -> Option<Novidade> {
    let publicada = Versao::ler(&release.tag_name)?;
    if publicada <= atual {
        return None;
    }
    Some(Novidade {
        versao: release
            .tag_name
            .trim_start_matches(['v', 'V'])
            .trim()
            .to_string(),
        endereco: release.html_url,
    })
}

/// De onde se pergunta, derivado do `repository` do `Cargo.toml`.
///
/// Derivado, e não escrito à mão: um fork que troque o `repository` passa a
/// conferir os lançamentos *dele* sem precisar descobrir que existe uma segunda
/// cópia do endereço escondida neste arquivo.
fn endereco_da_api() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY")
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let dono_e_nome = repo
        .rsplit_once("github.com/")
        .map_or("DanielFreitasDev/ditador", |(_, resto)| resto);
    format!("https://api.github.com/repos/{dono_e_nome}/releases/latest")
}

fn buscar(endereco: &str) -> Option<Release> {
    let programa = crate::programas::primeiro(&["curl", "wget"])?;
    let saida = match programa {
        "curl" => crate::programas::sem_janela(&mut Command::new("curl"))
            .args([
                "--location",
                "--fail",
                "--silent",
                "--show-error",
                // Prazos curtos: isto é um recado, não uma tarefa. Uma rede
                // ruim não pode deixar uma thread pendurada por dez minutos.
                "--connect-timeout",
                "10",
                "--max-time",
                "20",
                // A API do GitHub recusa quem não se apresenta.
                "--user-agent",
                CLIENTE,
                "--header",
                "Accept: application/vnd.github+json",
            ])
            .arg(endereco)
            .output(),
        _ => crate::programas::sem_janela(&mut Command::new("wget"))
            .args([
                "--quiet",
                "--timeout=10",
                "--tries=1",
                "--user-agent",
                CLIENTE,
                "--header",
                "Accept: application/vnd.github+json",
                "-O",
                "-",
            ])
            .arg(endereco)
            .output(),
    };

    let saida = match saida {
        Ok(saida) if saida.status.success() => saida,
        Ok(saida) => {
            // Rede caída, GitHub fora do ar, limite de requisições estourado:
            // nada disso é problema do usuário e nada disso vai para a tela.
            log::debug!(
                "não consegui conferir a versão ({}): {}",
                saida.status,
                String::from_utf8_lossy(&saida.stderr).trim()
            );
            return None;
        }
        Err(e) => {
            log::debug!("não consegui conferir a versão: {e}");
            return None;
        }
    };

    match serde_json::from_slice::<Release>(&saida.stdout) {
        Ok(release) => Some(release),
        Err(e) => {
            log::debug!("a resposta do GitHub não tinha o que esperávamos: {e}");
            None
        }
    }
}

/// Como este programa se apresenta ao GitHub.
const CLIENTE: &str = concat!("ditador/", env!("CARGO_PKG_VERSION"));

#[cfg(test)]
mod tests {
    use super::*;

    fn v(texto: &str) -> Versao {
        Versao::ler(texto).expect("versão válida")
    }

    #[test]
    fn a_tag_do_github_e_lida_com_e_sem_o_v() {
        assert_eq!(v("v0.7.3"), v("0.7.3"));
        assert_eq!(v("V1.2.3"), v("1.2.3"));
        assert_eq!(v(" 0.7.3 "), v("0.7.3"));
    }

    #[test]
    fn a_comparacao_segue_o_versionamento_semantico() {
        assert!(v("0.8.0") > v("0.7.3"));
        assert!(v("0.7.4") > v("0.7.3"));
        assert!(v("1.0.0") > v("0.99.99"));
        // O número maior de uma casa não ganha da casa anterior: 0.10 vem
        // depois de 0.9, e comparar como texto diria o contrário.
        assert!(v("0.10.0") > v("0.9.0"));
        assert!(v("0.7.3") == v("0.7.3"));
    }

    #[test]
    fn o_sufixo_de_pre_lancamento_nao_conta() {
        assert_eq!(v("0.8.0-rc1"), v("0.8.0"));
        assert_eq!(v("0.8.0+build7"), v("0.8.0"));
        // E, por isso, um `rc` da versão que já se tem não vira aviso.
        assert!(v("0.7.3-rc2") <= v("0.7.3"));
    }

    #[test]
    fn a_versao_de_duas_casas_completa_a_terceira() {
        assert_eq!(v("1.2"), v("1.2.0"));
    }

    #[test]
    fn o_que_nao_e_versao_nao_vira_versao() {
        assert_eq!(Versao::ler(""), None);
        assert_eq!(Versao::ler("v"), None);
        assert_eq!(Versao::ler("latest"), None);
        assert_eq!(Versao::ler("0"), None);
        assert_eq!(Versao::ler("uma.coisa.qualquer"), None);
    }

    #[test]
    fn a_versao_desta_compilacao_e_a_do_cargo_toml() {
        assert_eq!(Versao::desta_compilacao(), v(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn o_endereco_da_api_sai_do_repositorio_do_cargo_toml() {
        let endereco = endereco_da_api();
        assert!(
            endereco.starts_with("https://api.github.com/repos/"),
            "endereço inesperado: {endereco}"
        );
        assert!(
            endereco.ends_with("/releases/latest"),
            "endereço inesperado: {endereco}"
        );
        // O dono e o nome saem do `repository`, e não de uma segunda cópia
        // escrita aqui dentro.
        assert!(
            endereco.contains("ditador"),
            "endereço inesperado: {endereco}"
        );
    }

    #[test]
    fn a_resposta_do_github_e_lida_pelos_dois_campos_que_importam() {
        // Um recorte da resposta real, com os campos que ignoramos no meio.
        let json = br#"{
            "url": "https://api.github.com/repos/x/y/releases/1",
            "tag_name": "v0.9.1",
            "name": "Ditador 0.9.1",
            "draft": false,
            "html_url": "https://github.com/x/y/releases/tag/v0.9.1",
            "assets": [{"name": "ditador.deb"}]
        }"#;
        let release: Release = serde_json::from_slice(json).expect("resposta válida");
        assert_eq!(release.tag_name, "v0.9.1");
        assert_eq!(
            release.html_url,
            "https://github.com/x/y/releases/tag/v0.9.1"
        );
    }

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            html_url: format!("https://github.com/x/y/releases/tag/{tag}"),
        }
    }

    #[test]
    fn so_a_versao_mais_nova_vira_aviso() {
        let atual = v("0.7.3");

        // A que já se tem, e as anteriores, não avisam nada.
        assert_eq!(novidade_de(release("v0.7.3"), atual), None);
        assert_eq!(novidade_de(release("v0.7.2"), atual), None);
        assert_eq!(novidade_de(release("v0.6.9"), atual), None);

        // A mais nova, sim — e o `v` da tag não vai para a tela.
        let nova = novidade_de(release("v0.8.0"), atual).expect("0.8.0 é mais nova que 0.7.3");
        assert_eq!(nova.versao, "0.8.0");
        assert!(nova.endereco.starts_with("https://github.com/"));
    }

    #[test]
    fn uma_tag_que_nao_e_versao_nao_vira_aviso() {
        // O GitHub aceita qualquer texto como tag. Uma que não seja versão não
        // pode virar um aviso dizendo "versão nightly disponível" nem, pior,
        // ser comparada como se fosse maior que tudo.
        let atual = v("0.7.3");
        assert_eq!(novidade_de(release("nightly"), atual), None);
        assert_eq!(novidade_de(release("latest"), atual), None);
        assert_eq!(novidade_de(release(""), atual), None);
    }

    #[test]
    #[ignore = "fala com a rede; rode com --ignored"]
    fn o_github_ainda_responde_o_que_esperamos() {
        // O contrato com a API. Se a Microsoft renomear `tag_name` ou parar de
        // mandar `html_url`, o programa fica em silêncio para sempre — e
        // silêncio é justamente o estado normal deste recurso, então ninguém
        // repara. Este teste é o que repara.
        let release = buscar(&endereco_da_api())
            .expect("o GitHub não respondeu, ou respondeu o que não esperávamos");
        assert!(
            Versao::ler(&release.tag_name).is_some(),
            "a tag publicada não parece uma versão: {:?}",
            release.tag_name
        );
        assert!(
            release.html_url.starts_with("https://github.com/"),
            "o endereço da release mudou de forma: {:?}",
            release.html_url
        );
        println!(
            "última publicada: {} — {}",
            release.tag_name, release.html_url
        );
    }

    #[test]
    fn a_vigilia_nao_nasce_duas_vezes() {
        // A opção pode ser desligada e religada quantas vezes alguém quiser, e
        // cada "religar" chama o `vigiar`. Sem a trava, cada uma dessas vezes
        // deixaria mais uma thread perguntando a mesma coisa ao GitHub — a
        // anterior não morre no instante em que a opção é desligada, e sim na
        // volta seguinte dela, que pode estar a um dia de distância.
        //
        // Só a reserva é exercitada aqui, e não o `vigiar` inteiro: aquele cria
        // thread e, meio minuto depois, fala com a rede. Teste não faz nem uma
        // coisa nem outra.
        assert!(reservar_a_vigilia(), "a primeira vigília precisa nascer");
        assert!(!reservar_a_vigilia(), "a segunda não pode nascer junto");
        assert!(!reservar_a_vigilia());

        liberar_a_vigilia();
        assert!(
            reservar_a_vigilia(),
            "depois de a vigília encerrar, religar a opção precisa criar outra"
        );
        liberar_a_vigilia();
    }

    #[test]
    fn o_cliente_se_apresenta_com_nome_e_versao() {
        // A API do GitHub responde 403 a quem não manda `User-Agent`.
        assert!(CLIENTE.starts_with("ditador/"));
        assert!(CLIENTE.len() > "ditador/".len());
    }
}
