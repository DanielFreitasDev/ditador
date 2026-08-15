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

mod audio;
mod autostart;
mod clipboard;
mod config;
mod controller;
mod hotkey;
mod icones;
mod ipc;
mod keys;
mod modelo;
mod plataforma;
mod programas;
mod resample;
mod state;
mod stt;
mod tema;
mod tray;
mod ui;
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
    // Precisa vir antes de qualquer mexida em variáveis de ambiente.
    clipboard::remember_environment();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(FILTRO_PADRAO))
        .init();
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
            let nome = args.get(1).map_or(modelo::PADRAO, String::as_str);
            return baixar_modelo(nome);
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
        Some("--configuracoes" | "--settings") => {
            if let Some(resposta) = ipc::send("settings") {
                println!("{resposta}");
                return Ok(());
            }
            ao_iniciar = Some(IpcCommand::Settings);
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

    let hotkey = HotkeyListener::start(&config.hotkey, hotkey_tx);
    let audio = audio::spawn(
        AudioSettings {
            device: config.input_device.clone(),
            max_secs: config.max_recording_secs,
        },
        audio_tx,
    );
    let levels = audio.levels.clone();
    let stt_cmd_tx = stt::spawn(stt_tx);

    // Socket de controle: ícone do aplicativo, atalho do GNOME, terminal.
    if let Some(listener) = listener {
        let ipc_tx = ipc_tx.clone();
        let shared = shared.clone();
        ipc::serve(listener, move |linha| match linha {
            "toggle" => {
                let _ = ipc_tx.send(IpcCommand::Toggle);
                "ok".to_string()
            }
            "settings" => {
                let _ = ipc_tx.send(IpcCommand::Settings);
                "ok".to_string()
            }
            "quit" => {
                let _ = ipc_tx.send(IpcCommand::Quit);
                "encerrando".to_string()
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
                    match &estado.aviso_atalho {
                        Some(aviso) => format!(" · atenção: {aviso}"),
                        None => String::new(),
                    }
                )
            }
            outro => format!("comando desconhecido: {outro}"),
        });
    }

    let controlador = Controller {
        shared: shared.clone(),
        sinal: sinal.clone(),
        audio,
        stt: stt_cmd_tx,
        hotkey,
    };
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
            .with_transparent(true)
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
fn baixar_modelo(nome: &str) -> Result<()> {
    use std::io::Write as _;

    let destino = modelo::caminho(nome);
    if destino.exists() {
        println!("O modelo já está aqui: {}", destino.display());
        return Ok(());
    }

    println!("Baixando ggml-{nome}.bin para {}", destino.display());
    let (andamento, _pronto) = modelo::baixar(nome, state::Sinal::default());
    loop {
        let p = andamento.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match &p.fim {
            Some(Ok(caminho)) => {
                println!("\rPronto: {}                    ", caminho.display());
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
    /// colagem automática no Windows não existe por decisão de projeto, e a
    /// leitura do teclado não é mensurável de dentro deste processo. Marcá-las
    /// com `!!` faria o comando terminar dizendo "há o que resolver" numa
    /// máquina onde não há, que é justamente o erro que ele existe para evitar.
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
    linha(
        // Informativo quando não há: no Windows a colagem automática não existe
        // por decisão de projeto, e no Linux ela é um extra que o usuário
        // escolhe instalar. Nos dois casos, "não tem" não é defeito.
        clipboard::paste_available().then_some(true),
        "Colagem automática",
        if clipboard::paste_available() {
            "disponível. Ela também precisa do serviço: systemctl --user status ydotool"
        } else {
            clipboard::COMO_HABILITAR_A_COLAGEM
        },
    );

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
            match (integracoes.gnome, integracoes.plasma) {
                (true, true) => "extensão do GNOME e widget do Plasma, os dois no ar. \
                     O ícone da bandeja fica recolhido."
                    .to_string(),
                (true, false) => "extensão do GNOME Shell no ar. O ícone da bandeja fica \
                     recolhido e o aviso de gravação é o OSD do Shell."
                    .to_string(),
                (false, true) => "widget do Plasma no ar. O ícone da bandeja fica recolhido; \
                     o aviso de gravação continua sendo a janela do Ditador."
                    .to_string(),
                (false, false) => plataforma::integracoes::sem_nenhuma(),
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
        println!("Há o que resolver acima antes de o ditado funcionar.");
        std::process::exit(1);
    }
}

fn ajuda() {
    println!(
        r#"Ditador — ditado por voz offline com Whisper

USO
  ditador                    inicia em segundo plano
  ditador --alternar         começa/para a gravação (para ícone e atalhos)
  ditador --configuracoes    abre as configurações
  ditador --status           mostra o estado da instância em execução
  ditador --encerrar         fecha o aplicativo
  ditador --baixar-modelo    baixa o modelo de transcrição (~574 MB)
  ditador --microfones       lista os microfones disponíveis
  ditador --diagnostico      confere tudo de que o Ditador depende
  ditador --versao           versão e backend
  ditador --ajuda            esta mensagem

USO NORMAL
  Segure a tecla de atalho, fale, solte. O texto aparece numa caixinha.

ARQUIVOS
  ~/.config/ditador/config.json
  ~/.local/share/ditador/models/"#
    );
}
