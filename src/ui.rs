//! Interface: sobreposição de gravação, caixa de resultado e configurações.
//!
//! O visual é de vidro escuro — ver `glass.rs` para como o efeito é construído.

use crate::audio::Levels;
use crate::glass;
use crate::state::{ModelState, Sinal, SharedState, UiAction, View, lock};
use crate::stt;
use crate::{clipboard, keys};
use crossbeam_channel::Sender;
use egui::{
    Color32, CornerRadius, LayerId, Margin, Pos2, Rect, RichText, Stroke, Vec2, ViewportCommand,
};
use std::time::Duration;

const TEXT: Color32 = Color32::from_rgb(240, 241, 246);
const MUTED: Color32 = Color32::from_rgb(154, 156, 172);
const REC: Color32 = Color32::from_rgb(255, 92, 104);
const ACCENT: Color32 = Color32::from_rgb(126, 176, 255);
const OK: Color32 = Color32::from_rgb(118, 222, 158);

const IDIOMAS: &[(&str, &str)] = &[
    ("pt", "Português"),
    ("en", "Inglês"),
    ("es", "Espanhol"),
    ("fr", "Francês"),
    ("de", "Alemão"),
    ("it", "Italiano"),
    ("auto", "Detectar automaticamente"),
];

pub struct App {
    shared: SharedState,
    actions: Sender<UiAction>,
    levels: Levels,
    /// Última tela para a qual já enviamos comandos de janela.
    applied: Option<View>,
    /// Alturas suavizadas das barras da animação.
    bars: Vec<f32>,
    /// Estado da captura de diagnóstico (ver `diagnostico`).
    captura: Captura,
}

/// Diagnóstico opcional: com `DITADOR_CAPTURA=<pasta>`, grava um PNG de cada
/// tela assim que ela estabiliza. Existe porque o GNOME nega a API de captura
/// de tela a aplicativos comuns, e sem isso não há como conferir o desenho.
#[derive(Default)]
struct Captura {
    tela: Option<View>,
    frames_restantes: u32,
    arquivo: Option<String>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: SharedState,
        actions: Sender<UiAction>,
        levels: Levels,
        sinal: Sinal,
    ) -> Self {
        sinal.ligar_interface(cc.egui_ctx.clone());
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.all_styles_mut(estilo_de_vidro);

        Self {
            shared,
            actions,
            levels,
            applied: None,
            bars: vec![0.0; crate::audio::LEVEL_HISTORY],
            captura: Captura::default(),
        }
    }

    fn diagnostico(&mut self, ctx: &egui::Context, view: View) {
        let Some(pasta) = std::env::var_os("DITADOR_CAPTURA") else {
            return;
        };
        let pasta = std::path::PathBuf::from(pasta);

        // Imagens já prontas chegam como eventos de entrada.
        let recebidas: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        for imagem in recebidas {
            let Some(nome) = self.captura.arquivo.take() else {
                continue;
            };
            let [largura, altura] = imagem.size;
            let caminho = pasta.join(format!("{nome}.png"));
            match image::RgbaImage::from_raw(
                largura as u32,
                altura as u32,
                imagem.as_raw().to_vec(),
            ) {
                Some(buffer) => match buffer.save(&caminho) {
                    Ok(()) => log::info!("captura gravada em {}", caminho.display()),
                    Err(e) => log::warn!("não consegui gravar a captura: {e}"),
                },
                None => log::warn!("captura com tamanho inesperado"),
            }
        }

        // Espera a tela assentar (animação, layout) antes de fotografar.
        if self.captura.tela != Some(view) {
            self.captura.tela = Some(view);
            self.captura.frames_restantes = 12;
        } else if self.captura.frames_restantes > 0 {
            self.captura.frames_restantes -= 1;
            ctx.request_repaint();
            if self.captura.frames_restantes == 0 && view != View::Hidden {
                self.captura.arquivo = Some(format!("{view:?}").to_lowercase());
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }

    fn act(&self, action: UiAction) {
        let _ = self.actions.send(action);
    }
}

/// Controles translúcidos, para que fiquem sobre o vidro em vez de tapá-lo.
fn estilo_de_vidro(style: &mut egui::Style) {
    let v = &mut style.visuals;
    v.override_text_color = Some(TEXT);
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = Color32::TRANSPARENT;
    v.faint_bg_color = glass::white(10);
    // Fundo dos campos de texto.
    v.extreme_bg_color = glass::white(16);
    v.selection.bg_fill = glass::tint(126, 176, 255, 90);
    v.selection.stroke = Stroke::new(1.0, TEXT);

    let vidro = |w: &mut egui::style::WidgetVisuals, fill: u8, borda: u8| {
        w.bg_fill = glass::white(fill);
        w.weak_bg_fill = glass::white(fill);
        w.bg_stroke = Stroke::new(1.0, glass::white(borda));
        w.fg_stroke = Stroke::new(1.0, TEXT);
        w.corner_radius = CornerRadius::same(11);
        w.expansion = 0.0;
    };
    vidro(&mut v.widgets.inactive, 32, 58);
    vidro(&mut v.widgets.hovered, 52, 92);
    vidro(&mut v.widgets.active, 66, 120);
    vidro(&mut v.widgets.open, 30, 56);
    vidro(&mut v.widgets.noninteractive, 0, 26);

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 7.0);
    style.spacing.slider_width = 190.0;
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Roda a cada `request_repaint`, inclusive com a janela escondida — é aqui
    /// que decidimos mostrá-la, redimensioná-la e posicioná-la.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut state = lock(&self.shared);

        if state.quitting {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        // Fecha o resultado sozinho, se configurado.
        if state.view == View::Result && state.config.result_timeout_secs > 0 {
            let limite = Duration::from_secs(state.config.result_timeout_secs);
            if state.result_shown_at.is_some_and(|t| t.elapsed() >= limite) {
                state.view = View::Hidden;
            }
        }

        let view = state.view;
        drop(state);

        if self.applied != Some(view) {
            apply_window(ctx, view);
            self.applied = Some(view);
        }

        match view {
            // Animação da gravação e do spinner.
            View::Recording | View::Processing => ctx.request_repaint(),
            // Mantém o aviso de "copiado" e o tempo limite em dia.
            View::Result => ctx.request_repaint_after(Duration::from_millis(250)),
            _ => {}
        }

        self.diagnostico(ctx, view);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // O Arc é clonado para que o guard não fique emprestando `self` — o
        // fechamento abaixo precisa de acesso exclusivo a `self`.
        let shared = self.shared.clone();
        let mut state = lock(&shared);
        let view = state.view;
        if view == View::Hidden {
            return;
        }

        // O painel vai na camada de fundo, antes de qualquer widget.
        let card = ui.max_rect().shrink(glass::SHADOW_PAD);
        glass::panel(
            &ui.ctx().layer_painter(LayerId::background()),
            card,
            glass::RADIUS,
        );

        let margem = glass::SHADOW_PAD as i8 + 14;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(Margin::same(margem)))
            .show(ui, |ui| match view {
                View::Recording => self.recording(ui, &state),
                View::Processing => self.processing(ui, &state),
                View::Result => self.result(ui, &mut state),
                View::Settings => self.settings(ui, &mut state),
                View::Error => self.error(ui, &state),
                View::Hidden => {}
            });
    }
}

/// Redimensiona, centraliza na parte de baixo da tela e mostra/esconde.
fn apply_window(ctx: &egui::Context, view: View) {
    if view == View::Hidden {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        return;
    }

    let [w, h] = view.size();
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(w, h)));

    if let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) {
        let x = ((monitor.x - w) / 2.0).max(0.0);
        let y = if view == View::Settings {
            ((monitor.y - h) / 2.0).max(0.0)
        } else {
            (monitor.y - h - 130.0).max(0.0)
        };
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(x, y)));
    }

    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
}

impl App {
    // ------------------------------------------------------------- gravando

    fn recording(&mut self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        let decorrido = state
            .recording_since
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let tempo = ui.input(|i| i.time) as f32;

        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
            let pulso = 0.5 + 0.5 * (tempo * 3.4).sin();
            glass::glow_dot(
                ui.painter(),
                rect.center(),
                9.0 + 4.0 * pulso,
                REC.gamma_multiply(0.20 + 0.30 * pulso),
            );
            ui.painter().circle_filled(rect.center(), 5.0, REC);

            ui.add_space(2.0);
            ui.label(RichText::new("Ouvindo").size(17.0).strong());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}:{:02.0}", decorrido / 60.0, decorrido % 60.0))
                        .size(15.0)
                        .color(MUTED)
                        .monospace(),
                );
            });
        });

        ui.add_space(8.0);
        self.waveform(ui, tempo);
        ui.add_space(8.0);

        ui.label(
            RichText::new(format!(
                "Solte {} para transcrever",
                keys::combo_label(&state.config.hotkey)
            ))
            .size(12.0)
            .color(MUTED),
        );
    }

    fn waveform(&mut self, ui: &mut egui::Ui, tempo: f32) {
        let altura = 48.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), altura), egui::Sense::hover());

        let leituras: Vec<f32> = {
            let guard = self.levels.lock().unwrap_or_else(|e| e.into_inner());
            guard.iter().copied().collect()
        };

        let total = self.bars.len();
        // Alinha as leituras à direita: o som mais recente fica na ponta.
        for i in 0..total {
            let alvo = if leituras.len() >= total {
                leituras[leituras.len() - total + i]
            } else if i + leituras.len() >= total {
                leituras[i + leituras.len() - total]
            } else {
                0.0
            };
            // Raiz quadrada dá mais presença visual aos sons baixos.
            let alvo = alvo.clamp(0.0, 1.0).sqrt();
            let suavizacao = if alvo > self.bars[i] { 0.5 } else { 0.14 };
            self.bars[i] += (alvo - self.bars[i]) * suavizacao;
        }

        let pico = self.bars.last().copied().unwrap_or(0.0);
        let painter = ui.painter();

        // Halo geral acompanhando o volume: o vidro "acende" quando você fala.
        if pico > 0.04 {
            glass::glow(
                painter,
                rect.shrink2(Vec2::new(rect.width() * 0.12, altura * 0.28)),
                altura,
                REC.gamma_multiply(0.10 + 0.16 * pico),
                46.0,
            );
        }

        let vao = 4.0;
        let largura = ((rect.width() - vao * (total as f32 - 1.0)) / total as f32).max(1.0);
        let meio = rect.center().y;

        for (i, valor) in self.bars.iter().enumerate() {
            let x = rect.left() + i as f32 * (largura + vao);
            // Onda lenta atravessando as barras: mesmo em silêncio o painel
            // respira, deixando claro que está ouvindo.
            let onda = 0.5 + 0.5 * (tempo * 1.7 + i as f32 * 0.42).sin();
            let repouso = 4.0 + 11.0 * onda;
            let h = (valor * altura * 0.94).max(repouso);
            let barra =
                Rect::from_min_size(Pos2::new(x, meio - h / 2.0), Vec2::new(largura, h));

            // Frio no silêncio, quente na voz.
            let cor = glass::mix(glass::tint(150, 178, 235, 120), REC, valor.min(1.0));
            glass::pill(painter, barra, cor);
        }
    }

    // ----------------------------------------------------------- processando

    fn processing(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
            glass::glow_dot(ui.painter(), rect.center(), 24.0, ACCENT.gamma_multiply(0.22));
            ui.put(rect, egui::Spinner::new().size(28.0).color(ACCENT));

            ui.add_space(8.0);
            ui.label(RichText::new("Transcrevendo…").size(15.0));
            if !state.status.is_empty() {
                ui.label(RichText::new(&state.status).size(12.0).color(MUTED));
            }
        });
    }

    // -------------------------------------------------------------- resultado

    fn result(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "resultado");

        ui.horizontal(|ui| {
            ui.label(RichText::new("Texto transcrito").size(16.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text("Fechar").clicked() {
                    self.act(UiAction::Hide);
                }
                if ui.button("⚙").on_hover_text("Configurações").clicked() {
                    self.act(UiAction::OpenSettings);
                }
                if !state.status.is_empty() {
                    ui.label(RichText::new(&state.status).size(11.0).color(MUTED));
                }
            });
        });

        ui.add_space(8.0);

        let altura_texto = ui.available_height() - 50.0;
        if state.config.editable_result {
            ui.add_sized(
                [ui.available_width(), altura_texto],
                egui::TextEdit::multiline(&mut state.text)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::same(10))
                    .font(egui::TextStyle::Body),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(altura_texto)
                .show(ui, |ui| {
                    ui.label(RichText::new(&state.text).size(14.0));
                });
        }

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let copiado = state
                .copied_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(3));

            let rotulo = if copiado {
                RichText::new("✔ Copiado").color(OK)
            } else {
                RichText::new("Copiar")
            };
            if ui.button(rotulo).clicked() {
                self.act(UiAction::Copy);
            }

            if clipboard::paste_available() && ui.button("Copiar e colar").clicked() {
                self.act(UiAction::Paste);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.message.is_empty() {
                    ui.label(RichText::new(&state.message).size(11.0).color(REC));
                } else if state.config.auto_copy {
                    ui.label(
                        RichText::new("cópia automática ligada")
                            .size(11.0)
                            .color(MUTED),
                    );
                }
            });
        });
    }

    // ------------------------------------------------------------------ erro

    fn error(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        drag_area(ui, "erro");

        let carregando = state.model == ModelState::Loading;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
            let cor = if carregando { ACCENT } else { REC };
            glass::glow_dot(ui.painter(), rect.center(), 15.0, cor.gamma_multiply(0.22));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if carregando { "⏳" } else { "⚠" },
                egui::FontId::proportional(15.0),
                cor,
            );
            ui.add_space(2.0);
            ui.label(RichText::new("Ditador").size(16.0).strong());
        });

        ui.add_space(10.0);
        ui.label(RichText::new(&state.message).size(13.0));
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.button("Fechar").clicked() {
                self.act(UiAction::Hide);
            }
            if state.model == ModelState::Failed && ui.button("Tentar de novo").clicked() {
                self.act(UiAction::ReloadModel);
            }
            if ui.button("Configurações").clicked() {
                self.act(UiAction::OpenSettings);
            }
        });
    }

    // --------------------------------------------------------- configurações

    fn settings(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "config");

        ui.horizontal(|ui| {
            ui.label(RichText::new("Configurações").size(17.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{} · {}", env!("CARGO_PKG_VERSION"), stt::BACKEND))
                        .size(11.0)
                        .color(MUTED),
                );
            });
        });

        ui.add_space(6.0);
        ui.separator();

        let rodape = 48.0;
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - rodape)
            .show(ui, |ui| {
                self.settings_atalho(ui, state);
                self.settings_transcricao(ui, state);
                self.settings_area_transferencia(ui, state);
                self.settings_desempenho(ui, state);
                self.settings_avancado(ui, state);
            });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Salvar").clicked() {
                self.act(UiAction::ApplyDraft);
            }
            if ui.button("Cancelar").clicked() {
                self.act(UiAction::CloseSettings);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Encerrar o Ditador").color(REC))
                    .on_hover_text("Fecha o aplicativo por completo")
                    .clicked()
                {
                    self.act(UiAction::Quit);
                }
            });
        });
    }

    fn settings_atalho(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        secao(ui, "Atalho");
        ui.horizontal(|ui| {
            ui.label("Segure para falar:");

            if state.capturing_hotkey {
                ui.label(
                    RichText::new("pressione a combinação…")
                        .color(ACCENT)
                        .strong(),
                );
                if ui.button("Cancelar").clicked() {
                    self.act(UiAction::CancelHotkeyCapture);
                }
            } else {
                let atual = keys::combo_label(&state.draft.hotkey);
                if ui
                    .button(RichText::new(atual).monospace())
                    .on_hover_text("Clique e pressione a nova tecla ou combinação")
                    .clicked()
                {
                    self.act(UiAction::StartHotkeyCapture);
                }
            }
        });
        ui.label(
            RichText::new(
                "A leitura é passiva: a tecla continua funcionando normalmente nos \
                 outros programas. Prefira teclas sem função própria (Pause, F13…). \
                 Esc cancela a captura.",
            )
            .size(11.0)
            .color(MUTED),
        );
    }

    fn settings_transcricao(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        secao(ui, "Transcrição");

        ui.horizontal(|ui| {
            ui.label("Idioma:");
            let atual = IDIOMAS
                .iter()
                .find(|(code, _)| *code == state.draft.language)
                .map(|(_, nome)| *nome)
                .unwrap_or("Personalizado");
            egui::ComboBox::from_id_salt("idioma")
                .selected_text(atual)
                .show_ui(ui, |ui| {
                    for (code, nome) in IDIOMAS {
                        ui.selectable_value(&mut state.draft.language, code.to_string(), *nome);
                    }
                });
        });

        ui.checkbox(&mut state.draft.translate, "Traduzir para inglês");

        ui.horizontal(|ui| {
            ui.label("Microfone:");
            let atual = state
                .draft
                .input_device
                .clone()
                .unwrap_or_else(|| "Padrão do sistema".to_string());
            let dispositivos = state.devices.clone();
            egui::ComboBox::from_id_salt("microfone")
                .selected_text(encurtar(&atual, 40))
                .width(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.draft.input_device, None, "Padrão do sistema");
                    for nome in &dispositivos {
                        ui.selectable_value(
                            &mut state.draft.input_device,
                            Some(nome.clone()),
                            encurtar(nome, 46),
                        );
                    }
                });
        });
    }

    fn settings_area_transferencia(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        secao(ui, "Área de transferência");

        ui.checkbox(
            &mut state.draft.auto_copy,
            "Copiar o texto automaticamente ao terminar",
        );

        let ydotool = clipboard::paste_available();
        ui.add_enabled_ui(ydotool, |ui| {
            ui.checkbox(
                &mut state.draft.auto_paste,
                "Colar automaticamente na janela em foco (Ctrl+V)",
            );
        });
        if !ydotool {
            ui.label(
                RichText::new("Colagem automática requer o ydotool: sudo apt install ydotool")
                    .size(11.0)
                    .color(MUTED),
            );
        } else if state.draft.auto_paste {
            ui.label(
                RichText::new(
                    "Com a colagem automática a janela de resultado não aparece — \
                     o texto vai direto para onde você estava escrevendo.",
                )
                .size(11.0)
                .color(MUTED),
            );
        }

        if !clipboard::wl_copy_available() {
            ui.label(
                RichText::new("wl-copy não encontrado; usando a área de transferência do X11.")
                    .size(11.0)
                    .color(MUTED),
            );
        }
    }

    fn settings_desempenho(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        secao(ui, "Desempenho");

        ui.add_enabled_ui(stt::GPU_CAPABLE, |ui| {
            ui.checkbox(
                &mut state.draft.use_gpu,
                format!("Usar a GPU ({})", stt::BACKEND),
            );
        });
        if !stt::GPU_CAPABLE {
            ui.label(
                RichText::new("Este binário foi compilado só para CPU.")
                    .size(11.0)
                    .color(MUTED),
            );
        }

        ui.add(egui::Slider::new(&mut state.draft.threads, 1..=16).text("Threads de CPU"));

        ui.horizontal(|ui| {
            ui.label("Modelo:");
            let mut caminho = state.draft.model_path.display().to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut caminho).desired_width(350.0))
                .changed()
            {
                state.draft.model_path = caminho.into();
            }
        });

        let existe = state.draft.model_path.exists();
        ui.label(
            RichText::new(if existe {
                "Arquivo encontrado."
            } else {
                "Arquivo não encontrado — rode ./baixar-modelo.sh"
            })
            .size(11.0)
            .color(if existe { MUTED } else { REC }),
        );
    }

    fn settings_avancado(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        egui::CollapsingHeader::new("Avançado")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Contexto passado ao modelo (jargão, nomes próprios, estilo de pontuação):",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut state.draft.initial_prompt)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );

                ui.checkbox(
                    &mut state.draft.normalize_audio,
                    "Normalizar o volume antes de transcrever",
                );
                ui.checkbox(
                    &mut state.draft.editable_result,
                    "Permitir editar o texto no resultado",
                );

                let mut minimo = state.draft.min_recording_ms as i32;
                if ui
                    .add(egui::Slider::new(&mut minimo, 0..=2000).text("Gravação mínima (ms)"))
                    .changed()
                {
                    state.draft.min_recording_ms = minimo as u64;
                }

                let mut maximo = state.draft.max_recording_secs as i32;
                if ui
                    .add(egui::Slider::new(&mut maximo, 10..=600).text("Gravação máxima (s)"))
                    .changed()
                {
                    state.draft.max_recording_secs = maximo as u64;
                }

                let mut fechar = state.draft.result_timeout_secs as i32;
                if ui
                    .add(
                        egui::Slider::new(&mut fechar, 0..=120)
                            .text("Fechar o resultado após (s, 0 = nunca)"),
                    )
                    .changed()
                {
                    state.draft.result_timeout_secs = fechar as u64;
                }

                ui.checkbox(
                    &mut state.draft.force_x11,
                    "Desenhar a janela via XWayland (recomendado no GNOME)",
                );
                ui.label(
                    RichText::new(
                        "Sem isso o GNOME decide onde a janela aparece e ela pode ficar \
                         atrás das outras. Mudança exige reiniciar o Ditador.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
            });
    }
}

// --------------------------------------------------------------------- apoio

fn secao(ui: &mut egui::Ui, titulo: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(titulo).size(12.0).strong().color(ACCENT));
    ui.add_space(2.0);
}

/// Faixa invisível no topo que permite arrastar a janela sem decoração.
fn drag_area(ui: &mut egui::Ui, id: &str) {
    let rect = egui::Rect::from_min_size(
        ui.max_rect().left_top(),
        Vec2::new(ui.available_width(), 28.0),
    );
    let response = ui.interact(rect, egui::Id::new(id), egui::Sense::drag());
    if response.dragged() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

fn encurtar(texto: &str, max: usize) -> String {
    if texto.chars().count() <= max {
        texto.to_string()
    } else {
        let inicio: String = texto.chars().take(max - 1).collect();
        format!("{inicio}…")
    }
}
