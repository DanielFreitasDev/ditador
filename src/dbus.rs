//! Interface D-Bus de sessão: a porta pela qual a extensão do GNOME Shell fala
//! com o Ditador.
//!
//! O socket Unix (`ipc.rs`) continua sendo o caminho da linha de comando, e não
//! sai daqui. Os dois existem porque servem a públicos diferentes: o socket
//! atende um processo que nasce, pergunta e morre — `ditador --alternar` —, e o
//! D-Bus atende um observador que fica, quer saber de mudanças sem perguntar
//! toda hora e já fala essa língua nativamente. Fazer a extensão do Shell rodar
//! um subprocesso a cada clique seria pôr um `fork` dentro do processo que
//! desenha a sua área de trabalho.
//!
//! A interface é fina de propósito: cada método manda um `IpcCommand` pelo mesmo
//! canal que o socket e a bandeja usam, e quem decide o que fazer continua sendo
//! o `controller.rs`. Nada de regra de negócio aqui.
//!
//! ## O nome no barramento
//!
//! `io.github.danielfreitasdev.Ditador`, com o objeto em
//! `/io/github/danielfreitasdev/Ditador`.
//!
//! É o domínio do projeto ao contrário. O repositório é
//! `github.com/DanielFreitasDev/ditador`, e o domínio que o mantenedor controla
//! de fato é a página de projeto, `danielfreitasdev.github.io` — invertida, dá
//! `io.github.danielfreitasdev`. A parte do domínio vai em minúsculas porque um
//! nome de domínio não distingue maiúsculas, e é assim que o Flathub, o
//! AppStream e as convenções de D-Bus o escrevem (`io.github.usuario.Aplicativo`);
//! só o último elemento, que é o nome do aplicativo e não parte do domínio,
//! leva maiúscula. Escrever `io.github.DanielFreitasDev.Ditador` funcionaria no
//! barramento, mas brigaria com o identificador que qualquer outra ferramenta do
//! ecossistema geraria para este mesmo projeto — e um dia haveria dois.
//!
//! O mesmo texto serve de nome da interface: com um objeto só e uma interface
//! só, um segundo nome seria só mais uma coisa para digitar errado.

use crate::config::Config;
use crate::controller::IpcCommand;
use crate::state::{EstadoPublico, SharedState, Sinal, lock};
use crossbeam_channel::Sender;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Nome bem-conhecido no barramento de sessão, e também o nome da interface.
pub const NOME: &str = "io.github.danielfreitasdev.Ditador";

/// Onde o objeto mora.
pub const CAMINHO: &str = "/io/github/danielfreitasdev/Ditador";

/// O nome que a extensão do GNOME segura enquanto está habilitada.
///
/// É a única coisa que ela precisa publicar, e é o que faz o ícone da bandeja
/// sair de cena sem que ninguém precise avisar nada a ninguém: quem segura um
/// nome no barramento é uma *conexão*, e o barramento o solta sozinho quando ela
/// cai. Shell reiniciado, extensão desabilitada, GJS que morreu no meio de um
/// `disable()` — os três terminam em conexão fechada, e nos três o ícone volta.
pub const NOME_DA_EXTENSAO: &str = "io.github.danielfreitasdev.Ditador.GnomeExtension";

/// O que a interface publica, tirado do estado compartilhado.
///
/// É um retrato, e não uma leitura ao vivo, pelo mesmo motivo do `tray.rs`: os
/// métodos do D-Bus rodam na thread da conexão, e travar ali o mutex principal
/// seria deixar o barramento decidir quando o controlador anda.
#[derive(Clone, PartialEq)]
struct Retrato {
    estado: EstadoPublico,
    mensagem: String,
    /// O instante em que a gravação começou, guardado só para reconhecer se a
    /// que está correndo agora é a mesma de antes. Não viaja pelo barramento.
    inicio: Option<Instant>,
    /// O mesmo instante em milissegundos desde a época, que é o que viaja.
    gravando_desde: u64,
    modelo: String,
    idioma: String,
    atalho: String,
}

impl Retrato {
    /// Tira o retrato de agora. O anterior entra porque o
    /// `gravando_desde` é derivado, e derivá-lo de novo daria um número
    /// ligeiramente diferente a cada vez (veja `epoca_ms`) — o que faria a
    /// interface do GNOME receber um "a gravação começou em outro instante" a
    /// cada mudança de estado, e reiniciar o cronômetro no meio da frase.
    fn tirar(shared: &SharedState, anterior: Option<&Retrato>) -> Self {
        let estado = lock(shared);
        let inicio = estado.recording_since;
        Self {
            estado: estado.estado_publico(),
            mensagem: estado.message.clone(),
            gravando_desde: match (inicio, anterior) {
                (None, _) => 0,
                // A mesma gravação continua: o valor publicado não muda.
                (Some(i), Some(a)) if a.inicio == Some(i) => a.gravando_desde,
                (Some(i), _) => epoca_ms(i),
            },
            inicio,
            modelo: nome_do_modelo(&estado.config),
            idioma: crate::config::nome_do_idioma(&estado.config.language).to_string(),
            atalho: crate::keys::combo_label(&estado.config.hotkey),
        }
    }
}

/// Um `Instant` em milissegundos desde a época.
///
/// O `Instant` do Rust é monotônico e não tem origem conhecida — ele não
/// atravessa o barramento. A conversão é "a hora de agora menos o quanto já se
/// passou", e erra pelos microssegundos entre as duas leituras do relógio: para
/// um cronômetro que conta segundos, é exato.
fn epoca_ms(inicio: Instant) -> u64 {
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    agora.saturating_sub(inicio.elapsed()).as_millis() as u64
}

/// O nome curto do modelo: `ggml-large-v3-turbo-q5_0.bin` vira
/// `large-v3-turbo-q5_0`. O prefixo e a extensão são iguais em todos eles, e o
/// que sobra é o que cabe numa linha de menu.
fn nome_do_modelo(config: &Config) -> String {
    let Some(nome) = config.model_path.file_stem() else {
        return String::new();
    };
    let nome = nome.to_string_lossy();
    nome.strip_prefix("ggml-").unwrap_or(&nome).to_string()
}

struct Servico {
    comandos: Sender<IpcCommand>,
    retrato: Retrato,
}

impl Servico {
    fn enviar(&self, comando: IpcCommand) {
        log::debug!("D-Bus pediu {comando:?}");
        let _ = self.comandos.send(comando);
    }
}

/// Os nomes em PascalCase que aparecem no barramento — `Alternar`,
/// `IniciarGravacao`, `Estado`, `GravandoDesde` — são o que o zbus produz a
/// partir destes, em português e em snake_case como o resto do projeto.
#[zbus::interface(name = "io.github.danielfreitasdev.Ditador")]
impl Servico {
    /// Grava se estiver parado, para se estiver gravando.
    fn alternar(&self) {
        self.enviar(IpcCommand::Toggle);
    }

    /// Começa a gravar. Não faz nada se o microfone já estiver aberto.
    fn iniciar_gravacao(&self) {
        self.enviar(IpcCommand::Start);
    }

    /// Para de gravar e manda transcrever. Não faz nada se não estiver gravando.
    fn parar_gravacao(&self) {
        self.enviar(IpcCommand::Stop);
    }

    /// Abre a janela de configurações do próprio Ditador.
    fn abrir_configuracoes(&self) {
        self.enviar(IpcCommand::Settings);
    }

    /// Encerra o Ditador.
    fn encerrar(&self) {
        self.enviar(IpcCommand::Quit);
    }

    /// `carregando`, `pronto`, `gravando`, `transcrevendo` ou `erro`.
    ///
    /// Não existe `indisponivel` aqui: essa é a ausência deste nome no
    /// barramento, e quem a percebe é quem pergunta.
    #[zbus(property)]
    fn estado(&self) -> &str {
        self.retrato.estado.nome()
    }

    /// A última mensagem de erro ou aviso, vazia quando não há nenhuma.
    #[zbus(property)]
    fn mensagem(&self) -> &str {
        &self.retrato.mensagem
    }

    /// Quando a gravação em curso começou, em milissegundos desde a época; zero
    /// quando não há gravação. É a fonte da verdade do cronômetro.
    #[zbus(property)]
    fn gravando_desde(&self) -> u64 {
        self.retrato.gravando_desde
    }

    /// O modelo em uso, pelo nome curto (`large-v3-turbo-q5_0`).
    #[zbus(property)]
    fn modelo(&self) -> &str {
        &self.retrato.modelo
    }

    /// O idioma configurado, por extenso (`Português`).
    #[zbus(property)]
    fn idioma(&self) -> &str {
        &self.retrato.idioma
    }

    /// O atalho global, como se escreve numa frase (`Pause`).
    #[zbus(property)]
    fn atalho(&self) -> &str {
        &self.retrato.atalho
    }
}

/// Publica a interface e a mantém em dia. Como a bandeja, não devolve erro:
/// ficar sem D-Bus custa a integração com o GNOME, não o ditado.
pub fn start(shared: SharedState, sinal: &Sinal, comandos: Sender<IpcCommand>) {
    let mudancas = sinal.observar();
    let retrato = Retrato::tirar(&shared, None);

    let conexao = match zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(NOME))
        .and_then(|b| {
            b.serve_at(
                CAMINHO,
                Servico {
                    comandos,
                    retrato: retrato.clone(),
                },
            )
        })
        .and_then(|b| b.build())
    {
        Ok(conexao) => conexao,
        Err(e) => {
            log::warn!(
                "sem a interface D-Bus ({e}); a extensão do GNOME não vai enxergar o Ditador"
            );
            return;
        }
    };

    vigiar_a_extensao(&conexao, shared.clone(), sinal.clone());

    std::thread::Builder::new()
        .name("dbus".into())
        .spawn(move || {
            let mut atual = retrato;
            while mudancas.recv().is_ok() {
                let novo = Retrato::tirar(&shared, Some(&atual));
                if novo == atual {
                    continue;
                }
                if let Err(e) = publicar(&conexao, &atual, &novo) {
                    log::debug!("não consegui publicar a mudança de estado: {e}");
                }
                atual = novo;
            }
        })
        .expect("spawn dbus thread");

    log::info!("interface D-Bus no ar em {NOME}");
}

/// Grava o retrato novo e avisa quem observa: **um** `PropertiesChanged` com
/// tudo o que mudou junto, e só o que mudou de verdade.
///
/// Numa mensagem só porque as propriedades não são independentes. Começar a
/// gravar muda `Estado` e `GravandoDesde` ao mesmo tempo, e quem desenha o
/// cronômetro lê os dois para desenhar uma coisa. Mandados em mensagens
/// separadas — que foi como isto nasceu —, existe um instante em que já se sabe
/// que a gravação começou e ainda não se sabe quando: o `Estado` chega primeiro,
/// e quem reagir a ele lê um `GravandoDesde` velho.
///
/// Não é hipótese. No binário de depuração as duas mensagens chegavam juntas o
/// bastante para o defeito não aparecer; no de release, não — e o cronômetro
/// nascia contando a partir de zero, ou do começo da gravação anterior.
///
/// Só o que mudou porque a alternativa, mandar as seis sempre, faria quem
/// escuta redesenhar tudo porque uma mensagem de erro trocou de texto.
fn publicar(
    conexao: &zbus::blocking::Connection,
    antes: &Retrato,
    agora: &Retrato,
) -> zbus::Result<()> {
    use zbus::zvariant::Value;

    let referencia = conexao.object_server().interface::<_, Servico>(CAMINHO)?;
    // O guarda de escrita cai antes de emitir: o que vem depois espera pelo
    // barramento, e segurar a interface travada nesse meio-tempo deixaria uma
    // chamada de método vinda de fora esperando por nada.
    {
        let mut servico = referencia.get_mut();
        servico.retrato = agora.clone();
    }

    let mut mudou: std::collections::HashMap<&str, Value<'_>> = std::collections::HashMap::new();
    if antes.estado != agora.estado {
        mudou.insert("Estado", Value::from(agora.estado.nome()));
    }
    if antes.mensagem != agora.mensagem {
        mudou.insert("Mensagem", Value::from(agora.mensagem.as_str()));
    }
    if antes.gravando_desde != agora.gravando_desde {
        mudou.insert("GravandoDesde", Value::from(agora.gravando_desde));
    }
    if antes.modelo != agora.modelo {
        mudou.insert("Modelo", Value::from(agora.modelo.as_str()));
    }
    if antes.idioma != agora.idioma {
        mudou.insert("Idioma", Value::from(agora.idioma.as_str()));
    }
    if antes.atalho != agora.atalho {
        mudou.insert("Atalho", Value::from(agora.atalho.as_str()));
    }
    if mudou.is_empty() {
        return Ok(());
    }

    zbus::block_on(zbus::fdo::Properties::properties_changed(
        referencia.signal_emitter(),
        zbus::names::InterfaceName::from_static_str(NOME)?,
        mudou,
        std::borrow::Cow::Borrowed(&[]),
    ))
}

/// Fica de olho no nome da extensão do GNOME e anota no estado compartilhado se
/// ela está no ar.
///
/// Quem lê essa anotação é o `tray.rs` (que recolhe o ícone) e o `ui.rs` (que
/// deixa a sobreposição de gravação para o OSD do Shell).
fn vigiar_a_extensao(conexao: &zbus::blocking::Connection, shared: SharedState, sinal: Sinal) {
    let proxy = match zbus::blocking::fdo::DBusProxy::new(conexao) {
        Ok(proxy) => proxy,
        Err(e) => {
            log::warn!("não consigo vigiar a extensão do GNOME ({e}); o ícone da bandeja fica");
            return;
        }
    };

    // A assinatura vem antes da pergunta, e não depois: entre "ela está no ar?"
    // e "me avise quando mudar" cabe a extensão inteira ser habilitada, e a
    // mudança que caísse nessa fresta não chegaria nunca. O filtro por argumento
    // é do próprio barramento — não recebemos o vaivém de nomes alheios.
    let avisos = match proxy.receive_name_owner_changed_with_args(&[(0, NOME_DA_EXTENSAO)]) {
        Ok(avisos) => avisos,
        Err(e) => {
            log::warn!("não consigo vigiar a extensão do GNOME ({e}); o ícone da bandeja fica");
            return;
        }
    };

    // A extensão pode já estar de pé quando o Ditador sobe: é o caso normal de
    // quem entra na sessão com as duas coisas habilitadas.
    let presente = proxy
        .name_has_owner(NOME_DA_EXTENSAO.try_into().expect("nome válido"))
        .unwrap_or(false);
    anotar(&shared, &sinal, presente);

    std::thread::Builder::new()
        .name("dbus-extensao".into())
        .spawn(move || {
            for aviso in avisos {
                let Ok(args) = aviso.args() else { continue };
                anotar(&shared, &sinal, args.new_owner().is_some());
            }
        })
        .expect("spawn dbus-extensao thread");
}

fn anotar(shared: &SharedState, sinal: &Sinal, presente: bool) {
    {
        let mut estado = lock(shared);
        if estado.extensao_gnome == presente {
            return;
        }
        estado.extensao_gnome = presente;
    }
    if presente {
        log::info!("extensão do GNOME no ar; recolhendo o ícone da bandeja e a sobreposição");
    } else {
        log::info!("extensão do GNOME saiu; o ícone da bandeja e a sobreposição voltam");
    }
    sinal.mudou();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ModelState, Shared, View};
    use std::sync::{Arc, Mutex};

    fn bancada() -> SharedState {
        Arc::new(Mutex::new(Shared::new(Config::default(), Vec::new())))
    }

    #[test]
    fn o_nome_do_modelo_perde_o_ggml_e_a_extensao() {
        let config = Config {
            model_path: "/casa/modelos/ggml-large-v3-turbo-q5_0.bin".into(),
            ..Config::default()
        };
        assert_eq!(nome_do_modelo(&config), "large-v3-turbo-q5_0");

        // Um arquivo que não segue a convenção continua sendo dito por inteiro:
        // o caminho é escolhido pelo usuário e não temos o que prometer sobre ele.
        let outro = Config {
            model_path: "/casa/modelos/meu-modelo.bin".into(),
            ..Config::default()
        };
        assert_eq!(nome_do_modelo(&outro), "meu-modelo");
    }

    #[test]
    fn o_inicio_da_gravacao_nao_dança_enquanto_a_gravacao_e_a_mesma() {
        // O cronômetro da extensão é desenhado a partir deste número. Se ele
        // mudar no meio da frase, o contador na tela volta para zero.
        let shared = bancada();
        {
            let mut estado = lock(&shared);
            estado.model = ModelState::Ready;
            estado.recording_since = Some(Instant::now());
            estado.view = View::Recording;
        }

        let primeiro = Retrato::tirar(&shared, None);
        assert_eq!(primeiro.estado, EstadoPublico::Gravando);
        assert_ne!(primeiro.gravando_desde, 0);

        // Outra coisa qualquer muda, e o retrato é tirado de novo.
        lock(&shared).message = "algo aconteceu".to_string();
        let segundo = Retrato::tirar(&shared, Some(&primeiro));
        assert_eq!(
            segundo.gravando_desde, primeiro.gravando_desde,
            "o começo da gravação foi recalculado no meio dela"
        );

        // Uma gravação nova é outro começo, e aí o número é recalculado. Os
        // cinco segundos para trás são só para o relógio de parede ter como
        // separar as duas: `Instant::now()` duas vezes seguidas cabe no mesmo
        // milissegundo, e o teste passaria por acidente em vez de por mérito.
        let cinco_segundos_atras = Instant::now() - std::time::Duration::from_secs(5);
        lock(&shared).recording_since = Some(cinco_segundos_atras);
        let terceiro = Retrato::tirar(&shared, Some(&segundo));
        let recuou = segundo.gravando_desde - terceiro.gravando_desde;
        assert!(
            (4_900..=5_100).contains(&recuou),
            "o começo devia ter recuado uns 5 s, e recuou {recuou} ms"
        );

        // E parar zera.
        lock(&shared).recording_since = None;
        let quarto = Retrato::tirar(&shared, Some(&terceiro));
        assert_eq!(quarto.gravando_desde, 0);
        assert_eq!(quarto.estado, EstadoPublico::Pronto);
    }

    #[test]
    fn os_estados_publicados_tem_nomes_estaveis() {
        // São estes textos que a extensão do GNOME compara. Mudar um é mudar o
        // protocolo, e este teste existe para que isso não passe despercebido.
        assert_eq!(EstadoPublico::Carregando.nome(), "carregando");
        assert_eq!(EstadoPublico::Pronto.nome(), "pronto");
        assert_eq!(EstadoPublico::Gravando.nome(), "gravando");
        assert_eq!(EstadoPublico::Transcrevendo.nome(), "transcrevendo");
        assert_eq!(EstadoPublico::Erro.nome(), "erro");
    }
}
