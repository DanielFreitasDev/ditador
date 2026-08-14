//! Ditador — ditado por voz offline com Whisper.
//!
//! Roda em segundo plano. Segure a tecla de atalho, fale, solte: o texto
//! aparece (e, se você quiser, já vai para a área de transferência).

mod audio;
mod clipboard;
mod config;
mod controller;
mod glass;
mod hotkey;
mod ipc;
mod keys;
mod resample;
mod state;
mod stt;
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

fn main() -> Result<()> {
    // Precisa vir antes de qualquer mexida em variáveis de ambiente.
    clipboard::remember_environment();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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
        Some("--microfones") => {
            for nome in audio::list_input_devices() {
                println!("{nome}");
            }
            return Ok(());
        }
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
    let listener = match ipc::bind()? {
        ipc::Bind::Ready(listener) => listener,
        ipc::Bind::AlreadyRunning => {
            println!("O Ditador já está rodando.");
            return Ok(());
        }
    };
    let config = Config::load();

    // No GNOME/Wayland um aplicativo comum não escolhe onde sua janela aparece
    // nem consegue ficar por cima das outras. Pelo XWayland isso funciona.
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
            normalize: config.normalize_audio,
        },
        audio_tx,
    );
    let levels = audio.levels.clone();
    let stt_cmd_tx = stt::spawn(stt_tx);

    // Socket de controle: ícone do aplicativo, atalho do GNOME, terminal.
    {
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
                format!(
                    "modelo: {} · atalho: {} · backend: {}",
                    match estado.model {
                        ModelState::Loading => "carregando",
                        ModelState::Ready => "pronto",
                        ModelState::Failed => "falhou",
                    },
                    keys::combo_label(&estado.config.hotkey),
                    stt::BACKEND
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

    tray::start(shared.clone(), &sinal, ipc_tx.clone());

    if let Some(comando) = ao_iniciar {
        let _ = ipc_tx.send(comando);
    }

    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("ditador")
            .with_title("Ditador")
            .with_inner_size([440.0, 152.0])
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
        // alfa o painel de vidro vira um retângulo preto. O glow entrega a
        // janela ARGB que o efeito precisa.
        renderer: eframe::Renderer::Glow,
        // Sem multiamostragem: nenhuma configuração do glutin nesta máquina
        // junta canal alfa com MSAA, e o pedido derruba a criação da janela. As
        // silhuetas de vidro saem suaves mesmo assim — o egui aplica a própria
        // suavização (feathering) em tudo que ele tesselia, e os degradês em
        // malha ficam recuados meio ponto, sob a borda especular.
        multisampling: 0,
        persist_window: false,
        centered: false,
        ..Default::default()
    };

    let resultado = eframe::run_native(
        "Ditador",
        opcoes,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shared, ui_tx, levels, sinal)))),
    );

    ipc::cleanup();
    resultado.map_err(|e| anyhow::anyhow!("falha na interface: {e}"))?;

    // Encerra sem rodar os destrutores globais. Os do ggml/Vulkan desmontam a
    // GPU enquanto o driver ainda está sendo usado, e o preço de um encerramento
    // "arrumado" seria um SIGSEGV — que o systemd leria como falha e reiniciaria
    // o serviço em seguida.
    sair_sem_desmontar()
}

/// Sai imediatamente, pulando `atexit` e destrutores estáticos.
fn sair_sem_desmontar() -> ! {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    unsafe extern "C" {
        fn _exit(status: i32) -> !;
    }
    unsafe { _exit(0) }
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
  ditador --microfones       lista os microfones disponíveis
  ditador --versao           versão e backend
  ditador --ajuda            esta mensagem

USO NORMAL
  Segure a tecla de atalho, fale, solte. O texto aparece numa caixinha.

ARQUIVOS
  ~/.config/ditador/config.json
  ~/.local/share/ditador/models/"#
    );
}
