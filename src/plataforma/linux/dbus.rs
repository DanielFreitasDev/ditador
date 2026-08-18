//! Interface D-Bus de sessão: a porta pela qual as integrações de área de
//! trabalho falam com o Ditador — a extensão do GNOME Shell e o widget do
//! Plasma.
//!
//! Uma porta só para as duas, e é regra: a cópia canônica do contrato está em
//! `dbus/contrato.xml`, o lado do Plasma é *gerado* dela em tempo de compilação,
//! e um teste aqui embaixo confere que os três lados continuam dizendo a mesma
//! coisa. Duas APIs paralelas seriam duas chances de o mesmo estado ser
//! publicado de dois jeitos.
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

use crate::audio::Levels;
use crate::controller::IpcCommand;
use crate::retrato::Retrato;
use crate::state::{SharedState, Sinal, lock};
use crossbeam_channel::Sender;

/// Nome bem-conhecido no barramento de sessão, e também o nome da interface.
pub const NOME: &str = "io.github.danielfreitasdev.Ditador";

/// Onde o objeto mora.
pub const CAMINHO: &str = "/io/github/danielfreitasdev/Ditador";

/// Outra conexão pode nos tomar o nome? **Não.**
///
/// Este e o [`SUBSTITUIMOS_QUEM_JA_TEM`] existem porque o padrão do zbus é o
/// contrário dos dois, e o preço disso foi medido numa máquina de verdade.
/// `BitFlags::<RequestNameFlags>::default()` — que é o que o `Builder` usa
/// quando ninguém diz nada — vale
/// `AllowReplacement | ReplaceExisting | DoNotQueue`. Quer dizer que todo
/// Ditador pedia o nome oferecendo-o de bandeja a quem viesse depois, e ao
/// mesmo tempo tomando-o de quem já estava lá.
///
/// O estrago é silencioso e não tem volta. Um segundo Ditador que suba por
/// qualquer motivo — a instância única só barra quem consegue tomar o socket de
/// controle, e o caminho `SemSocket` existe justamente para quando ele não dá —
/// **rouba** o nome do que está rodando, e os dois escrevem no journal a mesma
/// linha dizendo que a interface subiu. Quando o intruso morre, o nome fica sem
/// dono nenhum: `DoNotQueue` impede o legítimo de voltar para a fila, e ele
/// segue de pé sem barramento, sem uma linha de log a respeito e sem nada que o
/// faça tentar de novo.
///
/// Para quem usa, o sintoma é este: a extensão do GNOME (ou o widget do Plasma)
/// passa a dizer "Indisponível", **e o ícone da bandeja não volta** — o nome da
/// extensão continua no barramento, então `Integracoes::gnome` continua
/// verdadeiro e o `tray.rs` continua achando que alguém já mostra o Ditador na
/// barra. Fica-se sem ícone, sem OSD e sem a janela de gravação, com o programa
/// ditando normalmente. Foi exatamente assim que este defeito foi encontrado.
///
/// Com os dois em `false` o barramento faz o que se espera de um programa de
/// instância única: quem chegou primeiro fica com o nome, e o segundo recebe
/// `NameTaken` e segue sem interface D-Bus — que é o degrau abaixo certo, e o
/// que a linha de log ao lado do `build` já dizia.
const PODE_SER_SUBSTITUIDO: bool = false;

/// E nós tomamos o nome de quem chegou antes? **Também não.** Veja
/// [`PODE_SER_SUBSTITUIDO`].
const SUBSTITUIMOS_QUEM_JA_TEM: bool = false;

/// O nome que a extensão do GNOME segura enquanto está habilitada.
///
/// É a única coisa que ela precisa publicar, e é o que faz o ícone da bandeja
/// sair de cena sem que ninguém precise avisar nada a ninguém: quem segura um
/// nome no barramento é uma *conexão*, e o barramento o solta sozinho quando ela
/// cai. Shell reiniciado, extensão desabilitada, GJS que morreu no meio de um
/// `disable()` — os três terminam em conexão fechada, e nos três o ícone volta.
pub const NOME_DA_EXTENSAO_GNOME: &str = "io.github.danielfreitasdev.Ditador.GnomeExtension";

/// O mesmo, para o widget do Plasma (`kde-plasma/`).
///
/// Quem o segura é o plugin C++ do widget, e a mesma promessa vale palavra por
/// palavra: `plasmashell` que caiu, widget removido do painel, plugin que não
/// carregou, sessão encerrada — em todos a conexão morre e o nome se solta.
///
/// Um nome só, mesmo que o usuário ponha dois widgets no painel: quem o detém é
/// a primeira instância a chegar, e as outras seguem funcionando sem ele. É por
/// isso que este lado pergunta "alguém detém o nome?", e nunca "quantos são".
pub const NOME_DA_INTEGRACAO_PLASMA: &str = "io.github.danielfreitasdev.Ditador.PlasmaIntegration";

// O `Retrato` que esta interface publica mora em `src/retrato.rs`, e não aqui.
// Ele nasceu neste arquivo, quando o D-Bus era o único jeito de o mundo de fora
// enxergar o Ditador; hoje o named pipe do Windows publica o mesmo estado para o
// `Ditador.Windows`, e duas cópias da mesma tabela é o que o `CLAUDE.md` proíbe
// em tantas palavras. O que continua sendo daqui é *como* ele viaja: propriedade
// por propriedade, com `PropertiesChanged` — veja `publicar`.

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

    /// O pico do microfone agora, de 0 a 1.
    ///
    /// É um sinal, e não uma propriedade, porque não é um estado: é um fio de
    /// água passando. Uma propriedade guardaria o último valor para sempre —
    /// inclusive depois que o microfone fechou — e faria o barramento anunciar
    /// `PropertiesChanged` quinze vezes por segundo, que é o oposto do que
    /// `PropertiesChanged` existe para dizer.
    ///
    /// Sai cru, sem correção nenhuma. A raiz quadrada que dá presença visual aos
    /// sons baixos é escolha de quem desenha, e cada superfície faz a sua — a
    /// janela do egui já fazia antes de existir barramento aqui.
    ///
    /// Só é emitido enquanto se grava, e nunca fora disso.
    #[zbus(signal)]
    async fn nivel(
        emissor: &zbus::object_server::SignalEmitter<'_>,
        valor: f64,
    ) -> zbus::Result<()>;
}

/// Pede o nome bem-conhecido, com as duas ressalvas que o padrão do zbus não
/// tem — veja [`PODE_SER_SUBSTITUIDO`].
///
/// É uma função, e não duas linhas soltas dentro do `start`, para que o teste
/// que prova o roubo peça o nome exatamente como o programa pede. Copiadas lá,
/// o teste passaria a conferir a si mesmo.
fn pedir_o_nome(
    builder: zbus::blocking::connection::Builder<'_>,
) -> zbus::Result<zbus::blocking::connection::Builder<'_>> {
    builder
        .allow_name_replacements(PODE_SER_SUBSTITUIDO)
        .replace_existing_names(SUBSTITUIMOS_QUEM_JA_TEM)
        .name(NOME)
}

/// Publica a interface e a mantém em dia. Como a bandeja, não devolve erro:
/// ficar sem D-Bus custa as integrações de desktop, não o ditado.
pub fn start(shared: SharedState, sinal: &Sinal, comandos: Sender<IpcCommand>, niveis: Levels) {
    let mudancas = sinal.observar();
    let retrato = Retrato::tirar(&shared, None);

    let conexao = match zbus::blocking::connection::Builder::session()
        .and_then(pedir_o_nome)
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
                "sem a interface D-Bus ({e}); as integrações do desktop não vão enxergar o Ditador"
            );
            return;
        }
    };

    vigiar_as_integracoes(&conexao, shared.clone(), sinal.clone());
    emitir_niveis(&conexao, shared.clone(), sinal, niveis);

    std::thread::Builder::new()
        .name("dbus".into())
        .spawn(move || {
            let mut atual = retrato;
            // Uma publicação que falha é quase sempre a conexão que caiu, e daí
            // em diante *todas* falham. A primeira sai como aviso, porque uma
            // integração que parou de receber estado é coisa que se quer ler no
            // journal; as seguintes voltam a ser depuração, senão cada ditado
            // encheria o log com a mesma linha.
            let mut ja_avisei = false;
            while mudancas.recv().is_ok() {
                let novo = Retrato::tirar(&shared, Some(&atual));
                if novo == atual {
                    continue;
                }
                match publicar(&conexao, &atual, &novo) {
                    Ok(()) => ja_avisei = false,
                    Err(e) if ja_avisei => {
                        log::debug!("não consegui publicar a mudança de estado: {e}");
                    }
                    Err(e) => {
                        log::warn!(
                            "não consegui publicar a mudança de estado no barramento ({e}); \
                             as integrações do desktop param de acompanhar o Ditador"
                        );
                        ja_avisei = true;
                    }
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

/// De quanto em quanto tempo o nível do microfone vai para o barramento.
///
/// Quinze por segundo: o bastante para as barras parecerem acompanhar a voz, e
/// pouco o suficiente para não ser um assunto. A janela do egui desenha a
/// sessenta porque ela já está desenhando de qualquer jeito; aqui cada quadro é
/// uma mensagem atravessando o barramento e entrando no laço do GNOME Shell ou
/// do `plasmashell` — o processo que desenha a área de trabalho inteira.
const INTERVALO_DO_NIVEL: std::time::Duration = std::time::Duration::from_millis(66);

/// Publica o nível do microfone enquanto ele estiver aberto.
///
/// Fora da gravação esta thread fica parada num `recv`, sem custo nenhum — não
/// há laço acordando para perguntar se já é hora. Quem a acorda é o mesmo sinal
/// de "o estado mudou" que move a bandeja e a interface.
fn emitir_niveis(
    conexao: &zbus::blocking::Connection,
    shared: SharedState,
    sinal: &Sinal,
    niveis: Levels,
) {
    let mudancas = sinal.observar();
    let emissor = match zbus::object_server::SignalEmitter::new(conexao.inner(), CAMINHO) {
        Ok(emissor) => emissor,
        Err(e) => {
            log::warn!("sem o nível do microfone no barramento ({e})");
            return;
        }
    };

    std::thread::Builder::new()
        .name("dbus-nivel".into())
        .spawn(move || {
            loop {
                if !lock(&shared).gravando() {
                    // Dorme até alguém mexer no estado. Se o programa está
                    // encerrando, o canal fecha e a thread sai com ele.
                    if mudancas.recv().is_err() {
                        return;
                    }
                    continue;
                }

                let valor = niveis
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .back()
                    .copied()
                    .unwrap_or(0.0);

                if zbus::block_on(Servico::nivel(&emissor, f64::from(valor.clamp(0.0, 1.0))))
                    .is_err()
                {
                    // A conexão caiu; não há a quem contar mais nada.
                    return;
                }
                std::thread::sleep(INTERVALO_DO_NIVEL);
            }
        })
        .expect("spawn dbus-nivel thread");
}

/// Pergunta ao barramento quais integrações estão no ar agora, sem depender do
/// Ditador estar rodando. É o que o `--diagnostico` conta.
///
/// Devolve `None` quando não há barramento de sessão nenhum — numa sessão por
/// SSH, por exemplo, onde a resposta certa não é "não há integração" e sim "não
/// há como saber".
pub fn integracoes_no_ar() -> Option<crate::state::Integracoes> {
    let conexao = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::fdo::DBusProxy::new(&conexao).ok()?;
    let tem = |nome: &str| {
        nome.try_into()
            .ok()
            .and_then(|nome| proxy.name_has_owner(nome).ok())
            .unwrap_or(false)
    };
    Some(crate::state::Integracoes {
        gnome: tem(NOME_DA_EXTENSAO_GNOME),
        plasma: tem(NOME_DA_INTEGRACAO_PLASMA),
        // O `frontend` é a assinatura do canal de controle, que no Linux ninguém
        // usa (quem observa o Ditador aqui fala D-Bus). E, mesmo que usasse,
        // este comando roda em **outro processo**: quem sabe a resposta é a
        // instância viva, e perguntar a ela daqui exigiria abrir o socket — que
        // é justamente o que a linha seguinte do `--diagnostico` já faz.
        ..Default::default()
    })
}

/// Fica de olho nos nomes das integrações nativas e anota no estado
/// compartilhado quais estão no ar.
///
/// Quem lê essa anotação é o `tray.rs` (que recolhe o ícone) e o `state.rs` (que
/// decide, em `tela_visivel`, se a sobreposição de gravação ainda é nossa).
///
/// São duas vigílias e não uma porque uma regra de correspondência do barramento
/// filtra por *um* valor de argumento: não existe "me avise sobre este nome ou
/// aquele". Duas assinaturas custam duas threads paradas num `recv`, que é o
/// preço de não receber o vaivém de nomes alheios da sessão inteira.
fn vigiar_as_integracoes(conexao: &zbus::blocking::Connection, shared: SharedState, sinal: Sinal) {
    let proxy = match zbus::blocking::fdo::DBusProxy::new(conexao) {
        Ok(proxy) => proxy,
        Err(e) => {
            log::warn!(
                "não consigo vigiar as integrações do desktop ({e}); o ícone da bandeja fica"
            );
            return;
        }
    };

    vigiar(
        &proxy,
        Qual::Gnome,
        shared.clone(),
        sinal.clone(),
        "dbus-gnome",
    );
    vigiar(&proxy, Qual::Plasma, shared, sinal, "dbus-plasma");
}

/// Qual integração, para os dois lugares em que isso importa: o nome que ela
/// segura no barramento e o campo em que a presença dela é anotada.
#[derive(Clone, Copy)]
enum Qual {
    Gnome,
    Plasma,
}

impl Qual {
    fn nome_no_barramento(self) -> &'static str {
        match self {
            Self::Gnome => NOME_DA_EXTENSAO_GNOME,
            Self::Plasma => NOME_DA_INTEGRACAO_PLASMA,
        }
    }

    fn como_se_chama(self) -> &'static str {
        match self {
            Self::Gnome => "extensão do GNOME",
            Self::Plasma => "widget do Plasma",
        }
    }
}

fn vigiar(
    proxy: &zbus::blocking::fdo::DBusProxy<'_>,
    qual: Qual,
    shared: SharedState,
    sinal: Sinal,
    thread: &str,
) {
    let nome = qual.nome_no_barramento();

    // A assinatura vem antes da pergunta, e não depois: entre "ela está no ar?"
    // e "me avise quando mudar" cabe a integração inteira ser habilitada, e a
    // mudança que caísse nessa fresta não chegaria nunca. O filtro por argumento
    // é do próprio barramento — não recebemos o vaivém de nomes alheios.
    let avisos = match proxy.receive_name_owner_changed_with_args(&[(0, nome)]) {
        Ok(avisos) => avisos,
        Err(e) => {
            log::warn!(
                "não consigo vigiar a {} ({e}); o ícone da bandeja fica",
                qual.como_se_chama()
            );
            return;
        }
    };

    // Ela pode já estar de pé quando o Ditador sobe: é o caso normal de quem
    // entra na sessão com as duas coisas habilitadas.
    let presente = proxy
        .name_has_owner(nome.try_into().expect("nome válido"))
        .unwrap_or(false);
    anotar(&shared, &sinal, qual, presente);

    std::thread::Builder::new()
        .name(thread.into())
        .spawn(move || {
            for aviso in avisos {
                let Ok(args) = aviso.args() else { continue };
                anotar(&shared, &sinal, qual, args.new_owner().is_some());
            }
            desistir_de_vigiar(&shared, &sinal, qual);
        })
        .expect("spawn thread de vigília");
}

/// O fluxo de avisos acabou: a conexão com o barramento morreu, e daqui em
/// diante esta vigília não sabe mais de nada.
///
/// A resposta certa é "não há integração nenhuma", e não o silêncio. Parando com
/// a última anotação de pé, o `tray.rs` continuaria achando que alguém já mostra
/// o Ditador na barra e não republicaria o ícone, e o `tela_visivel` continuaria
/// escondendo a tela de gravação por causa de um OSD que ninguém mais desenha —
/// o programa seguiria ditando sem nada na tela dizendo isso, que é o pior
/// desfecho possível para uma conexão que caiu.
///
/// Assumir que a integração saiu erra no lado barato: se ela ainda estiver lá,
/// aparecem dois ícones. Errando para o outro lado, não aparece nenhum.
fn desistir_de_vigiar(shared: &SharedState, sinal: &Sinal, qual: Qual) {
    log::warn!(
        "perdi o barramento de sessão; deixo de vigiar a {} e assumo que ela saiu",
        qual.como_se_chama()
    );
    anotar(shared, sinal, qual, false);
}

fn anotar(shared: &SharedState, sinal: &Sinal, qual: Qual, presente: bool) {
    {
        let mut estado = lock(shared);
        let campo = match qual {
            Qual::Gnome => &mut estado.integracoes.gnome,
            Qual::Plasma => &mut estado.integracoes.plasma,
        };
        if *campo == presente {
            return;
        }
        *campo = presente;
    }
    if presente {
        log::info!(
            "{} no ar; recolhendo o ícone da bandeja",
            qual.como_se_chama()
        );
    } else {
        log::info!("{} saiu; o ícone da bandeja volta", qual.como_se_chama());
    }
    sinal.mudou();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::{EstadoPublico, Shared};
    use std::sync::{Arc, Mutex};

    fn bancada() -> SharedState {
        Arc::new(Mutex::new(Shared::new(Config::default(), Vec::new())))
    }

    // O que era testado aqui sobre o `Retrato` — o nome curto do modelo e o
    // começo da gravação que não pode ser recalculado no meio da frase — mudou
    // de casa junto com ele, para `src/retrato.rs`.

    /// Os membros de um XML de introspecção, em texto e em ordem estável.
    ///
    /// Um analisador de três linhas em vez de uma dependência de XML: o que se
    /// compara aqui são três arquivos deste próprio repositório, todos escritos
    /// à mão em cima do mesmo molde. Os argumentos de um sinal grudam no membro
    /// anterior, que é o dono deles, para a ordenação não separar `Nivel` do `d`
    /// que ele carrega.
    fn membros(xml: &str) -> Vec<String> {
        fn atributo(tag: &str, qual: &str) -> String {
            let procurado = format!("{qual}=\"");
            match tag.find(&procurado) {
                Some(i) => {
                    let resto = &tag[i + procurado.len()..];
                    resto[..resto.find('"').unwrap_or(resto.len())].to_string()
                }
                None => String::new(),
            }
        }

        let mut fora: Vec<String> = Vec::new();
        for pedaco in xml.split('<').skip(1) {
            let tag = &pedaco[..pedaco.find('>').unwrap_or(pedaco.len())];
            let Some(especie) = tag.split_whitespace().next() else {
                continue;
            };
            match especie {
                "method" | "signal" => {
                    fora.push(format!("{especie} {}", atributo(tag, "name")));
                }
                "property" => fora.push(format!(
                    "property {} {} {}",
                    atributo(tag, "name"),
                    atributo(tag, "type"),
                    atributo(tag, "access")
                )),
                "arg" => {
                    if let Some(dono) = fora.last_mut() {
                        dono.push_str(&format!(
                            "({}: {})",
                            atributo(tag, "name"),
                            atributo(tag, "type")
                        ));
                    }
                }
                _ => {}
            }
        }
        fora.sort();
        fora
    }

    #[test]
    fn o_contrato_canonico_bate_com_os_tres_lados() {
        // Há três cópias desta interface no repositório, e elas não têm como
        // ser uma só: o Rust a monta a partir do código, a extensão do GNOME a
        // leva embutida no ZIP que o Shell carrega, e o Plasma a gera do XML em
        // tempo de compilação. O que dá para garantir é que ninguém mexa numa
        // sem mexer nas outras — e é isso que este teste faz.
        //
        // O lado do Rust não é uma lista escrita à mão: sai do próprio
        // `#[zbus::interface]`, pelo `introspect_to_writer`, que não precisa de
        // barramento nenhum para responder. Acrescentar um método lá e esquecer
        // o resto falha aqui.
        use zbus::object_server::Interface as _;

        let (comandos, _) = crossbeam_channel::unbounded();
        let servico = Servico {
            comandos,
            retrato: Retrato::tirar(&bancada(), None),
        };
        let mut publicado = String::new();
        servico.introspect_to_writer(&mut publicado, 0);

        let canonico = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/dbus/contrato.xml"));
        let extensao = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/gnome-extension/src/backend.js"
        ));
        // O XML da extensão mora numa string de template do JavaScript.
        let embutido = extensao
            .split_once("const INTERFACE = `")
            .expect("a extensão do GNOME perdeu a constante INTERFACE")
            .1;
        let embutido = &embutido[..embutido.find('`').expect("template sem fim")];

        let publicado = membros(&publicado);
        assert!(
            !publicado.is_empty(),
            "não consegui ler o que o zbus publica"
        );
        assert_eq!(
            membros(canonico),
            publicado,
            "dbus/contrato.xml e src/plataforma/linux/dbus.rs discordam"
        );
        assert_eq!(
            membros(embutido),
            publicado,
            "gnome-extension/src/backend.js e src/plataforma/linux/dbus.rs discordam"
        );
    }

    #[test]
    fn os_nomes_de_presenca_das_integracoes_sao_estaveis() {
        // Cada integração segura o seu enquanto está carregada, e é a ausência
        // dele que traz o ícone da bandeja de volta. Quem os escreve do outro
        // lado é `gnome-extension/src/backend.js` e
        // `kde-plasma/plugin/presenca.cpp`; errar uma letra não daria erro
        // nenhum — daria dois ícones do Ditador na barra, para sempre.
        assert_eq!(
            NOME_DA_EXTENSAO_GNOME,
            "io.github.danielfreitasdev.Ditador.GnomeExtension"
        );
        assert_eq!(
            NOME_DA_INTEGRACAO_PLASMA,
            "io.github.danielfreitasdev.Ditador.PlasmaIntegration"
        );

        // Os dois pendem do nome do serviço: são ele mais um sufixo. Escrito
        // assim, um `NOME` renomeado leva os dois junto em vez de deixá-los
        // apontando para um serviço que não existe mais.
        for nome in [NOME_DA_EXTENSAO_GNOME, NOME_DA_INTEGRACAO_PLASMA] {
            assert!(
                nome.starts_with(&format!("{NOME}.")),
                "{nome} não pende de {NOME}"
            );
        }
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

    #[test]
    fn o_barramento_que_cai_devolve_o_icone_e_a_tela_de_gravacao() {
        use crate::state::View;

        // A outra metade do mesmo defeito. A vigília das integrações vive de um
        // fluxo de avisos do barramento, e esse fluxo acaba quando a conexão
        // morre. Antes, a thread simplesmente terminava — e a última anotação
        // ficava valendo para sempre: com a extensão do GNOME marcada como
        // presente, o `tray.rs` não republicava o ícone e o `tela_visivel`
        // continuava escondendo a gravação por causa de um OSD que ninguém mais
        // desenhava. O Ditador seguia ditando sem ícone, sem aviso e sem janela.
        let shared = bancada();
        let sinal = Sinal::default();

        anotar(&shared, &sinal, Qual::Gnome, true);
        anotar(&shared, &sinal, Qual::Plasma, true);
        {
            let mut estado = lock(&shared);
            estado.view = View::Recording;
            assert!(estado.integracoes.mostram_o_icone());
            assert_eq!(estado.tela_visivel(), View::Hidden);
        }

        desistir_de_vigiar(&shared, &sinal, Qual::Gnome);
        desistir_de_vigiar(&shared, &sinal, Qual::Plasma);

        let estado = lock(&shared);
        assert!(
            !estado.integracoes.mostram_o_icone(),
            "o ícone da bandeja não voltou depois de o barramento cair"
        );
        assert_eq!(
            estado.tela_visivel(),
            View::Recording,
            "ninguém mais avisa que o microfone está aberto, e a nossa tela \
             continuou escondida"
        );
    }

    /// Sobe um barramento só deste teste e devolve o endereço dele, junto com o
    /// processo para ser encerrado no fim.
    ///
    /// Um barramento próprio, e não o da sessão: o teste pede um nome
    /// bem-conhecido e mede quem fica com ele, e fazer isso no barramento de
    /// quem roda `cargo test` derrubaria a interface D-Bus do Ditador que essa
    /// pessoa tem aberto — que é, palavra por palavra, o defeito que este teste
    /// existe para impedir.
    fn barramento_de_teste() -> Option<(String, std::process::Child)> {
        use std::io::BufRead as _;

        // A mesma pergunta que o `--diagnostico` faz sobre o curl: é uma
        // ferramenta do sistema, e não estar lá é um fato sobre a máquina.
        let programa = crate::programas::primeiro(&["dbus-daemon"])?;
        let mut bus = std::process::Command::new(programa)
            .args(["--session", "--print-address", "--nofork", "--nopidfile"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;

        let mut endereco = String::new();
        let saida = bus.stdout.take().expect("pedimos o stdout por cano");
        if std::io::BufReader::new(saida)
            .read_line(&mut endereco)
            .is_err()
            || endereco.is_empty()
        {
            let _ = bus.kill();
            let _ = bus.wait();
            return None;
        }
        Some((endereco.trim().to_string(), bus))
    }

    /// Pede o nome como o programa pede, num barramento qualquer.
    fn conectar(endereco: &str) -> zbus::Result<zbus::blocking::Connection> {
        zbus::blocking::connection::Builder::address(endereco)
            .and_then(pedir_o_nome)
            .and_then(|b| b.build())
    }

    #[test]
    fn uma_segunda_instancia_nao_rouba_o_nome_da_que_ja_esta_no_ar() {
        // O defeito, medido numa máquina de verdade: o padrão do zbus para as
        // bandeiras do `RequestName` é
        // `AllowReplacement | ReplaceExisting | DoNotQueue`, e com ele o segundo
        // Ditador tomava o nome do primeiro. Os dois escreviam no journal a
        // mesma linha dizendo que a interface tinha subido; quando o segundo
        // saía, o nome ficava **sem dono nenhum**, e o primeiro seguia de pé,
        // vivo e ditando, sem barramento e sem ícone na barra.
        //
        // É um teste de ponta a ponta porque o que se quer provar é o que o
        // barramento faz, e não o que o nosso código acha que pediu.
        let Some((endereco, mut bus)) = barramento_de_teste() else {
            eprintln!(
                "AVISO: `uma_segunda_instancia_nao_rouba_o_nome_da_que_ja_esta_no_ar` \
                 não conferiu nada — o dbus-daemon não está nesta máquina."
            );
            return;
        };

        let primeira = conectar(&endereco).expect("a primeira precisa ficar com o nome");
        let dona = primeira
            .inner()
            .unique_name()
            .expect("conexão de barramento tem nome único")
            .to_string();

        let segunda = conectar(&endereco);
        assert!(
            segunda.is_err(),
            "a segunda instância ficou com o nome do barramento — ela roubou o \
             da primeira, que continua rodando"
        );

        // E a primeira continua dona: não basta a segunda ter recebido um erro,
        // o que importa é de quem o barramento diz que o nome é.
        let proxy = zbus::blocking::fdo::DBusProxy::new(&primeira).expect("proxy do barramento");
        let agora = proxy
            .get_name_owner(NOME.try_into().expect("nome válido"))
            .expect("o nome precisa continuar tendo dono");
        assert_eq!(
            agora.as_str(),
            dona,
            "o nome mudou de dono com a primeira instância ainda de pé"
        );

        drop(primeira);
        let _ = bus.kill();
        let _ = bus.wait();
    }
}
