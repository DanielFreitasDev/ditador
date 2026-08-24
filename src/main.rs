//! Ditador — ditado por voz offline com Whisper.
//!
//! Roda em segundo plano. Segure a tecla de atalho, fale, solte: o texto
//! aparece (e, se você quiser, já vai para a área de transferência).

// O whisper.cpp é compilado com um back-end só, então as três features são
// mutuamente exclusivas. Sem estes guardas o Cargo somava as features como
// sempre faz: quem esquecesse o `--no-default-features` levava o Vulkan junto
// em silêncio — `--features cpu` produzia um binário ligado à libvulkan que
// ainda se anunciava como "CPU", e `--features cuda` mandava compilar os dois.
#[cfg(all(feature = "vulkan", feature = "cuda"))]
compile_error!(
    "vulkan e cuda não convivem — o whisper.cpp aceita um back-end só. \
     Use: cargo build --release --no-default-features --features cuda"
);
#[cfg(all(feature = "vulkan", feature = "cpu"))]
compile_error!(
    "vulkan e cpu não convivem — o vulkan vem do default. \
     Use: cargo build --release --no-default-features --features cpu"
);
#[cfg(all(feature = "cuda", feature = "cpu"))]
compile_error!(
    "cuda e cpu não convivem — escolha um: \
     cargo build --release --no-default-features --features cuda (ou cpu)"
);
#[cfg(not(any(feature = "vulkan", feature = "cuda", feature = "cpu")))]
compile_error!(
    "nenhum back-end escolhido. Use --features vulkan (o padrão), \
     ou --no-default-features --features cpu|cuda"
);

mod assinatura;
mod audio;
mod autostart;
mod clipboard;
mod config;
mod controller;
mod dicionario;
mod historico;
mod hotkey;
mod icones;
mod ipc;
mod keys;
mod memoria;
mod modelo;
mod plataforma;
mod portatil;
mod programas;
mod resample;
mod retrato;
mod sons;
mod state;
mod stt;
mod tema;
mod tray;
mod ui;
mod vad;
mod versao;
mod widgets;

use crate::audio::AudioSettings;
use crate::config::Config;
use crate::controller::{Channels, Controller, IpcCommand};
use crate::hotkey::HotkeyListener;
use crate::state::{ModelState, Shared, SharedState, Sinal};
use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Nível padrão dos registros.
///
/// O `info` é o nosso; o resto da linha é contenção de barulho alheio. Num dia
/// de uso normal o journal tinha 3 190 linhas, das quais 1 530 eram o aperto de
/// mão do zbus (que o ksni usa para falar com a barra superior) despejando
/// arrays de bytes crus e 780 eram o whisper.cpp e o ggml escrevendo em C —
/// sobravam umas setecentas nossas no meio. Como o journal é a única superfície
/// de diagnóstico deste programa, deixá-la três quartos ocupada por biblioteca
/// é o mesmo que não ter nenhuma.
///
/// Para depurar, `RUST_LOG=ditador=debug` continua valendo e não traz o barulho
/// de volta; `RUST_LOG=debug` seco traz, e é isso mesmo que se quer quando o
/// problema está numa delas.
const FILTRO_PADRAO: &str = "info,\
     zbus=warn,tracing=warn,\
     whisper_rs::whisper_logging_hook=warn,whisper_rs::ggml_logging_hook=warn";

fn main() -> Result<()> {
    // A primeira de todas: no Windows o destino do arquivo de log sai de
    // `data_dir()`, e `data_dir()` depende de o modo portátil estar decidido.
    // Ela não escreve no log justamente por isso — quem conta o que ela
    // descobriu é o `portatil::relatar()`, logo abaixo.
    portatil::init();
    // Precisa vir antes de qualquer mexida em variáveis de ambiente.
    clipboard::remember_environment();
    // E esta, antes de o programa alocar qualquer coisa de grande: ela desliga
    // a heurística da glibc que faz os buffers de cada ditado ficarem presos nas
    // arenas do malloc. Medido nesta máquina, são 29 MB de RSS que o processo
    // segurava para sempre — veja `src/memoria.rs`, que tem a medida e o
    // teste que a defende.
    memoria::pinar_o_alocador();
    let mut registro =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(FILTRO_PADRAO));
    // No Linux isto é `None` e o log continua indo para a saída de erro, que o
    // systemd recolhe. No Windows não há quem recolha, e o destino é um arquivo
    // em `LocalAppData` — sem ele, o Ditador instalado ficava sem log nenhum,
    // porque quem o sobe é o frontend, com `CreateNoWindow`.
    if let Some(destino) = plataforma::registro::destino() {
        registro.target(env_logger::Target::Pipe(destino));
    }
    registro.init();
    // Sem isto o whisper.cpp e o ggml escrevem direto no stderr, por fora do
    // `log`: o filtro acima não os alcança e as linhas chegam ao journal sem
    // nível, sem alvo e fora de ordem. É a feature `log_backend` do whisper-rs,
    // que já pagávamos no Cargo.toml sem nunca ligar.
    whisper_rs::install_logging_hooks();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut ao_iniciar: Option<IpcCommand> = None;

    match args.first().map(String::as_str) {
        Some("--ajuda" | "-h" | "--help") => {
            ajuda();
            return Ok(());
        }
        Some("--versao" | "--version" | "-V") => {
            println!(
                "ditador {} · backend {} · whisper.cpp {}",
                env!("CARGO_PKG_VERSION"),
                stt::BACKEND,
                whisper_rs::WHISPER_CPP_VERSION
            );
            return Ok(());
        }
        Some("--baixar-modelo") => {
            if matches!(args.get(1).map(String::as_str), Some("--lista" | "-l")) {
                listar_modelos();
                return Ok(());
            }
            // Sem nome, o sugerido para **esta** instalação, e não o padrão
            // sempre. Num binário compilado só para CPU, ou com a GPU desligada
            // na configuração, mandar 574 MB de modelo de GPU é entregar a
            // pessoa a uma transcrição mais lenta do que a própria fala — que é
            // o número que o `Cargo.toml` mede e o `modelo::PADRAO_CPU` existe
            // para evitar.
            let config = config::Config::load();
            let padrao = modelo::sugerido(config.use_gpu && stt::GPU_CAPABLE);
            let nome = args.get(1).map_or(padrao, String::as_str);
            return baixar_modelo(nome, &config);
        }
        Some("--microfones") => {
            for nome in audio::list_input_devices() {
                println!("{nome}");
            }
            return Ok(());
        }
        Some("--diagnostico" | "--doctor") => return diagnostico(),
        Some("--alternar" | "--toggle") => {
            if let Some(resposta) = ipc::send("toggle") {
                println!("{resposta}");
                return Ok(());
            }
            // Não havia ninguém rodando: sobe o serviço.
            eprintln!("Ditador não estava rodando; iniciando em segundo plano.");
        }
        Some("--cancelar" | "--cancel") => {
            return match ipc::send("cancelar") {
                Some(resposta) => {
                    println!("{resposta}");
                    Ok(())
                }
                None => {
                    eprintln!("O Ditador não está rodando.");
                    std::process::exit(1);
                }
            };
        }
        Some("--configuracoes" | "--settings") => {
            if let Some(resposta) = ipc::send("settings") {
                println!("{resposta}");
                return Ok(());
            }
            ao_iniciar = Some(IpcCommand::Settings);
        }
        // Com `--janela` abre a lista na instância que estiver rodando; sem
        // ela, imprime as últimas no terminal — que funciona sem sessão gráfica
        // e sem o Ditador estar de pé, porque o histórico é um arquivo.
        Some("--historico" | "--history") => {
            if args.get(1).map(String::as_str) == Some("--janela") {
                return match ipc::send("historico") {
                    Some(resposta) => {
                        println!("{resposta}");
                        Ok(())
                    }
                    None => {
                        eprintln!("O Ditador não está rodando.");
                        std::process::exit(1);
                    }
                };
            }
            let quantas = args
                .get(1)
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(20);
            return imprimir_historico(quantas);
        }
        Some("--encerrar" | "--quit") => {
            return match ipc::send("quit") {
                Some(resposta) => {
                    println!("{resposta}");
                    Ok(())
                }
                None => {
                    eprintln!("O Ditador não está rodando.");
                    std::process::exit(1);
                }
            };
        }
        Some("--status") => {
            return match ipc::send("status") {
                Some(resposta) => {
                    println!("{resposta}");
                    Ok(())
                }
                None => {
                    println!("parado");
                    std::process::exit(1);
                }
            };
        }
        Some(outro) if outro.starts_with('-') => {
            eprintln!("Opção desconhecida: {outro}");
            ajuda();
            std::process::exit(2);
        }
        _ => {}
    }

    executar(ao_iniciar)
}

fn executar(ao_iniciar: Option<IpcCommand>) -> Result<()> {
    // Aqui, e não no `main`: o que a detecção do modo portátil descobriu
    // interessa ao journal de quem está rodando o programa, e não a quem digitou
    // `ditador --status` num terminal — ali a linha é só barulho antes da
    // resposta. Quem responde "em que modo estou?" pela linha de comando é o
    // `--diagnostico`, que tem uma linha própria para isso.
    portatil::relatar();

    let listener = match ipc::bind() {
        ipc::Bind::Escutando(listener) => Some(listener),
        ipc::Bind::JaRodando => {
            println!("O Ditador já está rodando.");
            return Ok(());
        }
        // Sem socket dá para ditar; o que se perde é o controle por linha de
        // comando. Derrubar a inicialização por causa de um acessório seria
        // trocar o programa inteiro pela parte dele que faltou.
        ipc::Bind::SemSocket(motivo) => {
            log::warn!(
                "sem o socket de controle ({motivo}); `ditador --alternar` não vai funcionar"
            );
            None
        }
    };
    let config = Config::load();

    // Primeira linha de todo journal: sem ela, `journalctl --user -u ditador`
    // depois de uma atualização não diz qual binário está de pé.
    log::info!(
        "ditador {} · backend {} · whisper.cpp {}",
        env!("CARGO_PKG_VERSION"),
        stt::BACKEND,
        whisper_rs::WHISPER_CPP_VERSION
    );

    // No GNOME/Wayland um aplicativo comum não escolhe onde sua janela aparece
    // nem consegue ficar por cima das outras. Pelo XWayland isso funciona.
    //
    // No Windows a pergunta não existe: `WS_EX_TOPMOST` e o posicionamento
    // sempre funcionaram, e não há dois protocolos de janela disputando. O campo
    // `force_x11` continua na configuração — apagá-lo quebraria os arquivos já
    // gravados, e o `CLAUDE.md` proíbe — mas aqui ele simplesmente não é lido.
    #[cfg(target_os = "linux")]
    if config.force_x11 && std::env::var_os("DISPLAY").is_some() {
        unsafe {
            std::env::set_var("WINIT_UNIX_BACKEND", "x11");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        log::info!("desenhando a janela via XWayland");
    }

    let shared: SharedState = Arc::new(Mutex::new(Shared::new(
        config.clone(),
        audio::list_input_devices(),
    )));
    let sinal = Sinal::default();

    let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();
    let (audio_tx, audio_rx) = crossbeam_channel::unbounded();
    let (stt_tx, stt_rx) = crossbeam_channel::unbounded();
    let (ui_tx, ui_rx) = crossbeam_channel::unbounded();
    let (ipc_tx, ipc_rx) = crossbeam_channel::unbounded();

    let hotkey = HotkeyListener::start(&config.hotkey, &config.atalho_de_cancelar, hotkey_tx);
    let audio = audio::spawn(
        AudioSettings {
            device: config.input_device.clone(),
            max_secs: config.max_recording_secs,
            sempre_aberto: config.microfone_sempre_aberto,
            canal: config.canal_do_microfone,
        },
        audio_tx,
    );
    let levels = audio.levels.clone();
    let stt_cmd_tx = stt::spawn(stt_tx);

    // Canal de controle: ícone do aplicativo, atalho do GNOME, terminal e — no
    // Windows — o `Ditador.Windows`, que assina e fica ouvindo.
    //
    // A regra de evolução é a do contrato D-Bus, e vale palavra por palavra:
    // **acrescentar, nunca renomear**. Um comando novo é invisível para quem não
    // o conhece; um renomeado quebra o atalho que alguém configurou no painel do
    // sistema para chamar `ditador --alternar`.
    if let Some(listener) = listener {
        let ipc_tx = ipc_tx.clone();
        let shared = shared.clone();
        let sinal_do_ipc = sinal.clone();
        let niveis_do_ipc = levels.clone();
        ipc::serve(listener, move |linha| match linha {
            // Só no Windows. Assinar liga `Integracoes::frontend`, que recolhe o
            // ícone da bandeja e passa o aviso de gravação para quem assinou —
            // e no Linux quem faz esse papel é o D-Bus, com um nome no barramento
            // que o `dbus.rs` vigia. Aberto aqui também, bastaria mandar a
            // palavra pelo socket à mão para o Ditador ficar sem ícone e sem
            // aviso numa área de trabalho que não tem nada para substituí-los.
            "assinar" if cfg!(target_os = "windows") => {
                ipc::Resposta::Fluxo(assinatura::abrir(&shared, &sinal_do_ipc, &niveis_do_ipc))
            }
            outro => ipc::Resposta::Linha(match outro {
                "toggle" => {
                    let _ = ipc_tx.send(IpcCommand::Toggle);
                    "ok".to_string()
                }
                // `iniciar` e `parar` existem para o mesmo que o `IniciarGravacao` e
                // o `PararGravacao` do D-Bus: quem desenha um botão de "gravar" quer
                // dizer o que quer, e não "inverta o que estiver valendo" — que dá
                // resultado errado quando dois cliques se cruzam.
                "iniciar" => {
                    let _ = ipc_tx.send(IpcCommand::Start);
                    "ok".to_string()
                }
                "parar" => {
                    let _ = ipc_tx.send(IpcCommand::Stop);
                    "ok".to_string()
                }
                // Descarta o ditado em curso. Como o `iniciar` e o `parar`, ele
                // diz o que quer em vez de inverter um estado — e como eles,
                // desiste sozinho quando não há gravação nenhuma.
                "cancelar" => {
                    let _ = ipc_tx.send(IpcCommand::Cancel);
                    "ok".to_string()
                }
                "settings" => {
                    let _ = ipc_tx.send(IpcCommand::Settings);
                    "ok".to_string()
                }
                "historico" => {
                    let _ = ipc_tx.send(IpcCommand::Historico);
                    "ok".to_string()
                }
                "quit" => {
                    let _ = ipc_tx.send(IpcCommand::Quit);
                    "encerrando".to_string()
                }
                // Quem pergunta isto é o `ditador --diagnostico`, que roda em outro
                // processo e por isso não tem como ver o estado compartilhado daqui.
                // É o equivalente da consulta que o Linux faz ao barramento.
                "integracoes" => {
                    if state::lock(&shared).integracoes.frontend {
                        "frontend".to_string()
                    } else {
                        "nenhuma".to_string()
                    }
                }
                "status" => {
                    let estado = state::lock(&shared);
                    // O aviso do atalho entra aqui porque é a falha mais comum e a
                    // mais silenciosa deste programa (usuário fora do grupo
                    // `input`): quem for descobrir por que "não acontece nada ao
                    // segurar a tecla" digita `ditador --status` antes de qualquer
                    // outra coisa.
                    format!(
                        "modelo: {} · atalho: {} · microfone: {} · backend: {}{}",
                        match estado.model {
                            ModelState::Loading => "carregando",
                            ModelState::Ready => "pronto",
                            ModelState::Failed => "falhou",
                        },
                        keys::combo_label(&estado.config.hotkey),
                        if estado.gravando() {
                            "gravando"
                        } else {
                            "parado"
                        },
                        stt::BACKEND,
                        // Numa linha só: a resposta trafega pelo socket terminada
                        // por `\n`, e o cliente lê exatamente uma linha.
                        match estado
                            .aviso_atalho
                            .as_deref()
                            .or(estado.aviso_desempenho.as_deref())
                        {
                            Some(aviso) => format!(" · atenção: {aviso}"),
                            None => String::new(),
                        }
                    )
                }
                desconhecido => format!("comando desconhecido: {desconhecido}"),
            }),
        });
    }

    let controlador = Controller::novo(shared.clone(), sinal.clone(), audio, stt_cmd_tx, hotkey);
    std::thread::Builder::new()
        .name("controller".into())
        .spawn(move || {
            controlador.run(Channels {
                hotkey: hotkey_rx,
                audio: audio_rx,
                stt: stt_rx,
                ui: ui_rx,
                ipc: ipc_rx,
            })
        })
        .expect("spawn controller");

    // As integrações vêm antes da bandeja porque são elas que descobrem se
    // alguém já está mostrando o Ditador na barra. Descobrindo primeiro, a
    // bandeja nasce sabendo, e o ícone não chega a piscar no login de quem usa
    // as duas coisas. É uma das armadilhas registradas no `CLAUDE.md`, e a
    // ordem continua valendo — no Windows por não haver nada a inverter, no
    // Linux pelo motivo medido lá.
    plataforma::integracoes::start(shared.clone(), &sinal, ipc_tx.clone(), levels.clone());
    tray::start(shared.clone(), &sinal, ipc_tx.clone());

    // A conferência de versão nova é a única coisa aqui que fala com a rede sem
    // ninguém ter pedido, e é por isso que a pergunta é feita **antes** de criar
    // a thread: desligada, ela não existe — não há conexão adiada, thread
    // dormindo nem chamada de sistema esperando um interruptor mudar de ideia.
    if state::lock(&shared).config.aviso_de_versao {
        versao::vigiar(shared.clone(), sinal.clone());
    }

    if let Some(comando) = ao_iniciar {
        let _ = ipc_tx.send(comando);
    }

    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("ditador")
            .with_title("Ditador")
            .with_icon(icones::janela())
            .with_inner_size(state::View::Recording.size())
            .with_min_inner_size([320.0, 110.0])
            .with_decorations(false)
            // Transparência só onde ela existe de verdade. No Windows o glutin
            // não entrega alfa por pixel numa janela OpenGL: pedi-la não a torna
            // transparente, só faz a folga da sombra virar uma moldura opaca —
            // veja `tema::FOLGA_SOMBRA`, que é zero lá pelo mesmo motivo.
            .with_transparent(cfg!(not(target_os = "windows")))
            .with_resizable(false)
            .with_always_on_top()
            // Começa escondido e sem roubar o foco de quem está digitando.
            .with_visible(false)
            .with_active(false),
        // O renderizador wgpu recusou a transparência nesta máquina ("surface
        // does not support a CompositeAlphaMode with transparency"), e sem canal
        // alfa os cantos arredondados da janela saem em cima de um retângulo
        // preto. O glow entrega a janela ARGB que eles precisam.
        renderer: eframe::Renderer::Glow,
        // Sem multiamostragem: nenhuma configuração do glutin nesta máquina
        // junta canal alfa com MSAA, e o pedido derruba a criação da janela. Os
        // cantos saem suaves mesmo assim — o egui aplica a própria suavização
        // (feathering) em tudo que ele tesselia.
        multisampling: 0,
        persist_window: false,
        centered: false,
        // Com `DITADOR_QUADROS` a interface roda solta, sem esperar o monitor, e
        // relata quantos quadros por segundo consegue — é assim que se mede o
        // custo real do vidro, que a sincronia vertical esconderia.
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: std::env::var_os("DITADOR_QUADROS").is_none(),
            ..Default::default()
        },
        ..Default::default()
    };

    let resultado = eframe::run_native(
        "Ditador",
        opcoes,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shared, ui_tx, levels, sinal)))),
    );

    ipc::cleanup();

    // O erro é registrado e não propagado. Devolvê-lo com `?` faria o processo
    // terminar pelo runtime do Rust, com `atexit` e destrutores estáticos — o
    // caminho exato que o `_exit(0)` logo abaixo existe para evitar. E a thread
    // do whisper nasce antes da janela, então neste ponto o contexto do ggml
    // provavelmente já está montado na GPU: um erro de interface viraria core
    // dump, e o `Restart=on-failure` reiniciaria por cima dele.
    let codigo = match resultado {
        Ok(()) => 0,
        Err(e) => {
            log::error!("falha na interface: {e}");
            1
        }
    };
    sair_sem_desmontar(codigo)
}

/// Sai imediatamente, pulando `atexit` e destrutores estáticos.
///
/// Não é otimização nem descuido: desmontar os buffers do ggml/Vulkan dá SIGSEGV
/// no driver da NVIDIA, e o systemd trataria isso como falha e reiniciaria o
/// aplicativo em laço. Todo caminho de saída passa por aqui, inclusive o de erro
/// da interface — devolver o erro com `?` faria o processo terminar pelo runtime
/// do Rust, que é exatamente o que se quer evitar.
///
/// A mesma decisão vale no Windows pelo mesmo motivo — o driver é o mesmo, o
/// ggml é o mesmo —, com a chamada de lá: `ExitProcess`. O `_exit` da libc não
/// serve aqui, porque com o MSVC ele é um detalhe interno do runtime C e não um
/// símbolo estável para se ligar.
fn sair_sem_desmontar(codigo: i32) -> ! {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    #[cfg(target_os = "linux")]
    unsafe {
        unsafe extern "C" {
            fn _exit(status: i32) -> !;
        }
        _exit(codigo)
    }

    #[cfg(target_os = "windows")]
    unsafe {
        windows_sys::Win32::System::Threading::ExitProcess(codigo as u32)
    }
}

/// Baixa o modelo pelo terminal, com a mesma máquina que a interface usa.
/// Existe para instalações sem tela (um servidor, uma sessão por SSH) e para
/// quem prefere resolver tudo de uma vez antes de usar o programa.
/// O catálogo, como o `--baixar-modelo --lista` o imprime.
///
/// Existe porque o `baixar-modelo.sh` não é instalado com o programa: quem
/// recebeu o `.deb` tem o binário e não tem o script, e até aqui a única forma
/// de descobrir que existe um modelo leve era ler o README no GitHub.
fn listar_modelos() {
    println!("Modelos disponíveis (o ✓ já está baixado, o ★ é o sugerido):\n");
    let sugerido_daqui = modelo::sugerido(config::Config::load().use_gpu && stt::GPU_CAPABLE);
    for m in modelo::CATALOGO {
        let marca = if m.baixado() {
            '✓'
        } else if m.nome == sugerido_daqui {
            '★'
        } else {
            ' '
        };
        println!(
            "  {marca} {:<22} {:>8}  {}",
            m.nome,
            modelo::tamanho_legivel(m.tamanho),
            m.nota
        );
    }
    println!("\nUso: ditador --baixar-modelo [nome]");
}

fn baixar_modelo(nome: &str, config: &config::Config) -> Result<()> {
    use std::io::Write as _;

    let destino = modelo::caminho(nome);
    // `exists()` não basta, e a diferença é o caso em que este comando mais
    // importa. Um arquivo ruim no destino — a página de um portal cativo, um
    // download interrompido pelo disco cheio, uma cópia truncada — trancava a
    // instalação inteira: a janela já oferece "Baixar o modelo de novo" quando o
    // Whisper recusa o arquivo, mas o terminal, que é o caminho de quem está
    // numa sessão por SSH, respondia "já está aqui" e não dava saída nenhuma.
    // Quem estivesse nessa situação tinha de descobrir sozinho que precisava
    // apagar o arquivo à mão.
    if destino.exists() {
        if modelo::parece_um_modelo(&destino) {
            println!("O modelo já está aqui: {}", destino.display());
            return Ok(());
        }
        println!(
            "O arquivo em {} existe mas não é um modelo do Whisper; baixando de novo por cima.",
            destino.display()
        );
    }

    println!("Baixando ggml-{nome}.bin para {}", destino.display());
    let (andamento, _pronto) = modelo::baixar(nome, state::Sinal::default());
    loop {
        let p = andamento.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match &p.fim {
            Some(Ok(caminho)) => {
                println!("\rPronto: {}                    ", caminho.display());
                // Baixar não é escolher. Quem pediu um modelo diferente do que
                // está em uso precisa ouvir isso agora, e não descobrir depois
                // que o Ditador continua transcrevendo com o de antes — a
                // configuração é da instância que está rodando, e não deste
                // processo de linha de comando.
                if *caminho != config.model_path {
                    println!(
                        "Para passar a usá-lo: Configurações → Desempenho → Modelo \
                         (o atual é {}).",
                        config::caminho_curto(&config.model_path)
                    );
                }
                return Ok(());
            }
            Some(Err(e)) => anyhow::bail!("{e}"),
            None => {
                match p.fracao() {
                    Some(f) => print!(
                        "\r{:>3.0} % de {}   ",
                        f * 100.0,
                        modelo::tamanho_legivel(p.total)
                    ),
                    None => print!("\r{}   ", modelo::tamanho_legivel(p.baixados)),
                }
                let _ = std::io::stdout().flush();
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
}

/// Imprime as últimas transcrições no terminal.
///
/// Existe porque o histórico é um arquivo e não um banco: dá para lê-lo sem
/// sessão gráfica, sem o Ditador de pé e sem ferramenta nenhuma. É o caminho de
/// quem está numa sessão por SSH, de quem quer canalizar o texto para outro
/// programa, e de quem só precisa recuperar a frase que acabou de se perder.
fn imprimir_historico(quantas: usize) -> Result<()> {
    let entradas = historico::ler_recentes(quantas);
    if entradas.is_empty() {
        println!(
            "Nada guardado ainda. O histórico fica em {}",
            config::caminho_curto(&historico::arquivo())
        );
        return Ok(());
    }

    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for entrada in &entradas {
        // O tempo relativo primeiro, numa coluna de largura fixa, e o texto
        // depois: assim a saída continua legível quando alguém a passa por um
        // `grep` ou por um `head`.
        println!("{:>10}  {}", entrada.ha_quanto_tempo(agora), entrada.texto);
    }
    Ok(())
}

/// Confere, uma a uma, as coisas de que o Ditador depende e que ele não
/// controla.
///
/// Existe porque a falha mais comum deste programa é também a mais silenciosa:
/// sem o usuário no grupo `input` o atalho global simplesmente não acontece, e
/// nada na tela explica o porquê. As outras — modelo faltando, `wl-copy`
/// ausente, `ydotool` sem serviço, bandeja sem extensão — degradam de formas
/// parecidas. Uma tela que responde "o que está faltando aqui?" custa menos que
/// a soma das perguntas que ela evita.
fn diagnostico() -> Result<()> {
    /// Uma linha do relatório.
    ///
    /// `None` é informativo: a linha aparece, com `--` na frente, e **não** pesa
    /// no veredito. Existe porque nem tudo que não está lá é problema — a
    /// colagem automática no Linux é um extra opcional, e a leitura do teclado
    /// não é mensurável de dentro deste processo. Marcá-las com `!!` faria o
    /// comando terminar dizendo "há o que resolver" numa máquina onde não há, que
    /// é justamente o erro que ele existe para evitar.
    fn linha(situacao: Option<bool>, titulo: &str, detalhe: &str) -> bool {
        println!(
            "{} {titulo}\n    {detalhe}",
            match situacao {
                Some(true) => "ok  ",
                Some(false) => "!!  ",
                None => "--  ",
            }
        );
        situacao.unwrap_or(true)
    }

    println!(
        "Ditador {} · backend {} · whisper.cpp {}\n",
        env!("CARGO_PKG_VERSION"),
        stt::BACKEND,
        whisper_rs::WHISPER_CPP_VERSION
    );

    let mut tudo_bem = true;

    // 1. O atalho global. A pergunta é a mesma nos dois sistemas — "dá para ler
    // o teclado daqui?" —, mas o motivo de falhar e o conselho para resolver não
    // têm nada em comum, então quem monta a linha inteira é a plataforma.
    let (teclado_ok, teclado_titulo, teclado_detalhe) = plataforma::teclado::diagnostico();
    tudo_bem &= linha(teclado_ok, teclado_titulo, &teclado_detalhe);

    // 2. O modelo.
    let config = Config::load();
    let modelo_ok = config.model_path.exists();
    tudo_bem &= linha(
        Some(modelo_ok),
        "Modelo de transcrição",
        &if modelo_ok {
            format!(
                "{} ({})",
                config::caminho_curto(&config.model_path),
                std::fs::metadata(&config.model_path)
                    .map(|m| modelo::tamanho_legivel(m.len()))
                    .unwrap_or_else(|_| "tamanho desconhecido".into())
            )
        } else {
            format!(
                "não está em {}. Rode: ditador --baixar-modelo",
                config::caminho_curto(&config.model_path)
            )
        },
    );

    // 3. O microfone.
    let microfones = audio::list_input_devices();
    tudo_bem &= linha(
        Some(!microfones.is_empty()),
        "Microfone",
        &match config.input_device.as_deref() {
            _ if microfones.is_empty() => "nenhum encontrado.".to_string(),
            Some(escolhido) if !microfones.iter().any(|m| m == escolhido) => format!(
                "o escolhido (\"{escolhido}\") não está mais aqui; \
                 {} disponível(is). Escolha outro nas configurações.",
                microfones.len()
            ),
            Some(escolhido) => format!("\"{escolhido}\"."),
            None => format!(
                "padrão do sistema, entre {} disponível(is).",
                microfones.len()
            ),
        },
    );

    // 4. Área de transferência e colagem — degradam, mas com aviso. Nenhuma das
    // duas entra no veredito: dá para ditar sem elas.
    linha(
        Some(clipboard::aviso_da_copia().is_none()),
        "Área de transferência",
        clipboard::aviso_da_copia()
            .unwrap_or("funcionando pelo caminho nativo desta área de trabalho."),
    );
    // Três respostas, e não duas. "Instalado" não quer dizer "funciona": no
    // Linux o `ydotool` precisa de um serviço à parte, e sem ele a colagem
    // aborta **depois** de a transcrição ficar pronta e ser copiada — hora
    // péssima para descobrir que faltava um pacote.
    let aviso_da_colagem = clipboard::aviso_da_colagem();
    let colagem_ok = linha(
        match (clipboard::paste_available(), aviso_da_colagem.is_some()) {
            // Informativo quando não há: no Linux a colagem é um extra que o
            // usuário escolhe instalar, e "não tem" não é defeito — dá para
            // ditar sem ela.
            (false, _) => None,
            (true, true) => Some(false),
            (true, false) => Some(true),
        },
        "Colagem automática",
        &match (clipboard::paste_available(), aviso_da_colagem) {
            (false, _) => clipboard::COMO_HABILITAR_A_COLAGEM.to_string(),
            (true, Some(aviso)) => format!("{aviso}{}", clipboard::COMO_LIGAR_A_COLAGEM),
            (true, None) => format!("disponível. {}", clipboard::SOBRE_A_COLAGEM),
        },
    );
    // Entra no veredito só de quem ligou a chave. Sem ela, não ter colagem não é
    // problema nenhum — dá para ditar a semana inteira sem ela; com ela, há um
    // recurso ligado nas configurações que não vai acontecer, e terminar o
    // relatório dizendo "tudo o que o Ditador precisa está no lugar" logo abaixo
    // de um `!!` é a contradição que faz alguém parar de ler o relatório.
    if config.auto_paste {
        tudo_bem &= colagem_ok;
    }

    // 5. Download do modelo.
    linha(
        Some(modelo::disponivel()),
        "Download do modelo (curl ou wget)",
        if modelo::disponivel() {
            "disponível."
        } else {
            "nenhum dos dois. Para instalar: sudo apt install curl"
        },
    );

    // 5a. Onde as coisas ficam. Informativo, e é a resposta a duas perguntas
    // que já custaram caro por escrito: "onde está o meu histórico?" e "por que
    // as configurações que eu salvei sumiram?" — esta última, quando alguém põe
    // um marcador de modo portátil ao lado do executável sem saber o que ele
    // faz, ou quando o marcador está lá e a pasta não aceita escrita.
    linha(
        None,
        if portatil::ativo() {
            "Onde ficam os dados (modo portátil)"
        } else {
            "Onde ficam os dados"
        },
        &format!(
            "configuração em {}\n    modelos em {}\n    histórico em {} ({} guardadas)",
            config::caminho_curto(&config::config_path()),
            config::caminho_curto(&config::models_dir()),
            config::caminho_curto(&historico::arquivo()),
            historico::ler().len()
        ),
    );

    // 5b. Onde está o log, quando ele é um arquivo nosso. No Linux quem guarda é
    // o journal e esta linha não aparece — dizer "use o journalctl" a quem já
    // está no journal não ajuda ninguém.
    if let Some(caminho) = plataforma::registro::caminho() {
        linha(
            None,
            "Log do backend",
            &format!("{} (o anterior fica em .log.1)", caminho.display()),
        );
    }

    // 6. A integração da área de trabalho. Não entra no veredito: ditar
    // funciona sem nenhuma delas, e a bandeja é a reserva de todo mundo. Está
    // aqui porque a pergunta que ela responde — "por que o ícone do Ditador
    // sumiu da barra?" — não tem outro lugar onde ser respondida.
    match plataforma::integracoes::integracoes_no_ar() {
        Some(integracoes) => println!(
            "{}   Integração da área de trabalho\n    {}",
            if integracoes.mostram_o_icone() {
                "ok"
            } else {
                "--"
            },
            match (integracoes.frontend, integracoes.gnome, integracoes.plasma) {
                (true, _, _) => "Ditador.Windows no ar. O ícone na área de notificação e o \
                     aviso de gravação na tela são dele."
                    .to_string(),
                (_, true, true) => "extensão do GNOME e widget do Plasma, os dois no ar. \
                     O ícone da bandeja fica recolhido."
                    .to_string(),
                (_, true, false) => "extensão do GNOME Shell no ar. O ícone da bandeja fica \
                     recolhido e o aviso de gravação é o OSD do Shell."
                    .to_string(),
                (_, false, true) => "widget do Plasma no ar. O ícone da bandeja fica recolhido; \
                     o aviso de gravação continua sendo a janela do Ditador."
                    .to_string(),
                (false, false, false) => plataforma::integracoes::sem_nenhuma(),
            }
        ),
        None => println!(
            "--   Integração da área de trabalho\n    \
             sem barramento de sessão; não há como perguntar daqui."
        ),
    }

    // 7. A instância em execução, se houver.
    match ipc::send("status") {
        Some(resposta) => println!("ok   Instância em execução\n    {resposta}"),
        None => println!(
            "--   Instância em execução\n    {}",
            plataforma::teclado::COMO_SUBIR_O_SERVICO
        ),
    }

    println!();
    if tudo_bem {
        println!("Tudo o que o Ditador precisa está no lugar.");
        Ok(())
    } else {
        // Não "antes de o ditado funcionar": desde que a colagem automática
        // entrou no veredito, um relatório inteiro em `ok` pode terminar aqui só
        // porque falta o serviço do ydotool — e aí o ditado funciona, o que não
        // funciona é a entrega do texto. Quem diz o que é cada coisa é a linha
        // marcada, não o fecho.
        println!("Há o que resolver acima: o que está marcado com !! não está pronto.");
        std::process::exit(1);
    }
}

fn ajuda() {
    println!(
        r#"Ditador — ditado por voz offline com Whisper

USO
  ditador                    inicia em segundo plano
  ditador --alternar         começa/para a gravação (para ícone e atalhos)
  ditador --cancelar         descarta a gravação em curso, sem transcrever
  ditador --configuracoes    abre as configurações
  ditador --historico [n]    imprime as últimas n transcrições (padrão: 20)
  ditador --historico --janela   abre a lista na janela do Ditador
  ditador --status           mostra o estado da instância em execução
  ditador --encerrar         fecha o aplicativo
  ditador --baixar-modelo    baixa o modelo sugerido para esta máquina
  ditador --baixar-modelo --lista   mostra todos os modelos e os tamanhos
  ditador --baixar-modelo <nome>    baixa um modelo específico da lista
  ditador --microfones       lista os microfones disponíveis
  ditador --diagnostico      confere tudo de que o Ditador depende
  ditador --versao           versão e backend
  ditador --ajuda            esta mensagem

USO NORMAL
  Segure a tecla de atalho, fale, solte. O texto aparece numa caixinha.

ARQUIVOS
  ~/.config/ditador/config.json
  ~/.local/share/ditador/models/
  ~/.local/share/ditador/historico/historico.jsonl

MODO PORTÁTIL
  Um arquivo chamado "portatil" ao lado do executável faz tudo isso morar
  numa pasta "Dados/" vizinha a ele — para pendrive e máquina emprestada."#
    );
}
