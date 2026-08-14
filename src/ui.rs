//! Interface: sobreposição de gravação, caixa de resultado e configurações.
//!
//! O visual é de vidro escuro — ver `glass.rs` para como o efeito é construído e
//! `widgets.rs` para os controles feitos com ele.

use crate::audio::Levels;
use crate::glass::{self, Vidro};
use crate::state::{ModelState, SharedState, Sinal, UiAction, View, lock};
use crate::stt;
use crate::widgets::{self, ACCENT, Botao, Icone, MUTED, OK, REC, TEXT};
use crate::{clipboard, keys};
use crossbeam_channel::Sender;
use egui::{
    Color32, CornerRadius, FontFamily, LayerId, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2,
    ViewportCommand,
};
use std::time::Duration;

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
    /// Medição de quadros por segundo (ver `Medidor`).
    medidor: Option<Medidor>,
    /// Quando a tela atual começou a aparecer, para a animação de mola.
    abertura: Option<std::time::Instant>,
}

/// Diagnóstico opcional: com `DITADOR_QUADROS=1` a janela repinta sem parar e
/// sem sincronia vertical, e a cada dois segundos relata quantos quadros por
/// segundo saíram. Serve para comparar mudanças no desenho — com a sincronia
/// ligada todo mundo empata em 60.
struct Medidor {
    desde: std::time::Instant,
    quadros: u32,
}

impl Medidor {
    fn novo() -> Option<Self> {
        std::env::var_os("DITADOR_QUADROS").map(|_| Self {
            desde: std::time::Instant::now(),
            quadros: 0,
        })
    }

    fn quadro(&mut self, view: View) {
        self.quadros += 1;
        let decorrido = self.desde.elapsed().as_secs_f32();
        if decorrido >= 2.0 {
            log::info!(
                "{view:?}: {:.0} quadros/s ({:.2} ms por quadro)",
                self.quadros as f32 / decorrido,
                1000.0 * decorrido / self.quadros as f32
            );
            self.desde = std::time::Instant::now();
            self.quadros = 0;
        }
    }
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
        if let Some(gl) = cc.gl.clone() {
            crate::glass_gpu::iniciar(gl);
        }
        carregar_fontes(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.all_styles_mut(estilo_de_vidro);

        Self {
            shared,
            actions,
            levels,
            applied: None,
            bars: vec![0.0; crate::audio::LEVEL_HISTORY],
            captura: Captura::default(),
            medidor: Medidor::novo(),
            abertura: None,
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

/// Tipografia: uma sans humanista do sistema, com um corte mais encorpado para
/// títulos e botões. O egui traz só um peso embutido, e texto claro sobre vidro
/// escuro fica anêmico quando tudo tem a mesma espessura.
///
/// Se nada disso existir na máquina, a fonte embutida continua valendo — daí a
/// família `forte` ser sempre registrada, mesmo que aponte para o padrão.
fn carregar_fontes(ctx: &egui::Context) {
    const CORPO: &[&str] = &[
        "/usr/share/fonts/truetype/lato/Lato-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    const FORTE: &[&str] = &[
        "/usr/share/fonts/truetype/lato/Lato-Semibold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ];

    fn instalar(fontes: &mut egui::FontDefinitions, nome: &str, opcoes: &[&str]) -> bool {
        for caminho in opcoes {
            if let Ok(bytes) = std::fs::read(caminho) {
                fontes.font_data.insert(
                    nome.to_string(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                return true;
            }
        }
        false
    }

    let mut fontes = egui::FontDefinitions::default();
    let corpo = instalar(&mut fontes, "ditador-corpo", CORPO);
    let forte = instalar(&mut fontes, "ditador-forte", FORTE);

    if corpo {
        fontes
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ditador-corpo".to_string());
    }
    // A família "forte" herda os mesmos reservas (emoji, símbolos) da normal.
    let mut lista = fontes
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if forte {
        lista.insert(0, "ditador-forte".to_string());
    }
    fontes
        .families
        .insert(FontFamily::Name("forte".into()), lista);

    ctx.set_fonts(fontes);
}

/// Controles translúcidos, para que fiquem sobre o vidro em vez de tapá-lo.
fn estilo_de_vidro(style: &mut egui::Style) {
    style.text_styles = [
        (egui::TextStyle::Heading, fonte_forte(19.0)),
        (egui::TextStyle::Body, egui::FontId::proportional(14.5)),
        (egui::TextStyle::Button, fonte_forte(14.0)),
        (egui::TextStyle::Small, egui::FontId::proportional(11.5)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
    ]
    .into();

    let v = &mut style.visuals;
    v.override_text_color = Some(TEXT);
    v.panel_fill = Color32::TRANSPARENT;
    // Listas suspensas e menus saem numa camada própria, fora do vidro do
    // painel: precisam de fundo próprio ou ficariam ilegíveis sobre o desktop.
    v.window_fill = glass::tint(19, 20, 28, 244);
    v.window_stroke = Stroke::new(1.0, glass::white(40));
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(16);
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = v.window_shadow;
    v.faint_bg_color = glass::white(10);
    // Fundo dos campos de texto.
    v.extreme_bg_color = glass::white(14);
    v.selection.bg_fill = glass::tint(122, 173, 255, 92);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    let vidro = |w: &mut egui::style::WidgetVisuals, fill: u8, borda: u8| {
        w.bg_fill = glass::white(fill);
        w.weak_bg_fill = glass::white(fill);
        w.bg_stroke = Stroke::new(1.0, glass::white(borda));
        w.fg_stroke = Stroke::new(1.0, TEXT);
        w.corner_radius = CornerRadius::same(13);
        w.expansion = 0.0;
    };
    vidro(&mut v.widgets.inactive, 28, 54);
    vidro(&mut v.widgets.hovered, 48, 92);
    vidro(&mut v.widgets.active, 64, 124);
    vidro(&mut v.widgets.open, 30, 56);
    vidro(&mut v.widgets.noninteractive, 0, 24);

    style.spacing.item_spacing = Vec2::new(9.0, 9.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.spacing.slider_width = 190.0;
    style.spacing.combo_height = 260.0;
    // Barra de rolagem flutuante: some quase por completo quando ninguém a
    // está usando, para não cortar o vidro com um trilho opaco. A margem à
    // direita é o corredor onde ela aparece, longe da borda dos cartões.
    style.spacing.scroll = egui::style::ScrollStyle::floating();
    let barra = &mut style.spacing.scroll;
    barra.bar_width = 8.0;
    barra.floating_width = 4.0;
    barra.handle_min_length = 28.0;
    barra.content_margin = Margin {
        right: 10,
        ..Margin::ZERO
    };
    barra.dormant_handle_opacity = 0.22;
    barra.active_handle_opacity = 0.45;
    barra.interact_handle_opacity = 0.75;
}

fn fonte_forte(tamanho: f32) -> egui::FontId {
    egui::FontId::new(tamanho, FontFamily::Name("forte".into()))
}

/// Texto de título/rótulo com o corte mais encorpado.
fn forte(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto)
        .size(tamanho)
        .family(FontFamily::Name("forte".into()))
}

/// Texto de apoio: pequeno e apagado.
fn nota(texto: impl Into<String>) -> RichText {
    RichText::new(texto).size(11.5).color(MUTED)
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
        // Ao abrir as configurações, o interruptor de início automático precisa
        // mostrar o que o sistema realmente tem armado, não o que ficou gravado
        // da última vez — o usuário pode ter mexido nisso por fora.
        if view == View::Settings && self.applied != Some(View::Settings) {
            state.draft.start_with_session = crate::autostart::ligado();
            state.config.start_with_session = state.draft.start_with_session;
        }
        drop(state);

        if self.applied != Some(view) {
            apply_window(ctx, view);
            self.applied = Some(view);
            // Cada tela entra com a sua própria mola. Trocar de tela também
            // redimensiona a janela, e a animação é o que costura as duas
            // coisas em um movimento só.
            self.abertura = (view != View::Hidden).then(std::time::Instant::now);
        }

        match view {
            // Animação da gravação e do spinner.
            View::Recording | View::Processing => ctx.request_repaint(),
            // Mantém o aviso de "copiado" e o tempo limite em dia.
            View::Result => ctx.request_repaint_after(Duration::from_millis(250)),
            _ => {}
        }

        if let Some(medidor) = &mut self.medidor
            && view != View::Hidden
        {
            medidor.quadro(view);
            ctx.request_repaint();
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

        // O vidro por GPU precisa saber onde a janela caiu na tela, para
        // recortar o papel de parede que vai por baixo dele.
        // Com as configurações abertas vale o rascunho, não o que está salvo:
        // assim o controle mostra o que faz enquanto está sendo arrastado.
        let mut aparencia = if view == View::Settings {
            state.draft.appearance
        } else {
            state.config.appearance
        };
        aparencia.sanear();
        crate::glass_gpu::aplicar_aparencia(aparencia);
        crate::glass_gpu::atualizar_tela(ui.ctx());
        self.animar_abertura(ui, aparencia);

        // O painel vai na camada de fundo, antes de qualquer widget. A posição
        // do cursor vai junto: é ela que faz a beirada acender por onde a mão
        // passa (`None` quando o ponteiro está fora da janela).
        let card = ui.max_rect().shrink(glass::SHADOW_PAD);
        let foco = ui.ctx().input(|i| i.pointer.hover_pos());
        ui.ctx()
            .layer_painter(LayerId::background())
            .add(glass::painel(card, glass::RADIUS, foco));

        let margem = glass::SHADOW_PAD as i8 + 16;
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

impl App {
    /// A tela entra crescendo de dentro do próprio centro, com uma mola curta.
    ///
    /// A escala vai numa transformação da camada de fundo, então ela pega tudo
    /// de uma vez — vidro, texto e controles — em vez de cada peça se animar por
    /// conta. Já a opacidade precisa de dois caminhos: o do egui não alcança os
    /// callbacks de desenho, que é o que o vidro é.
    fn animar_abertura(&mut self, ui: &mut egui::Ui, ap: crate::config::Appearance) {
        let camada = LayerId::background();
        let ctx = ui.ctx().clone();

        let x = match self.abertura {
            Some(inicio) if ap.animation && ap.animation_ms > 0 => {
                inicio.elapsed().as_secs_f32() / (ap.animation_ms as f32 / 1000.0)
            }
            _ => 1.0,
        };
        if x >= 1.0 {
            self.abertura = None;
            ctx.set_transform_layer(camada, egui::emath::TSTransform::IDENTITY);
            crate::glass_gpu::definir_opacidade(1.0);
            return;
        }

        let t = glass::mola(x, ap.animation_bounce);
        let escala = 1.0 - (1.0 - ap.animation_scale) * (1.0 - t);
        // A opacidade fecha antes do movimento: o painel já está inteiro quando
        // a mola ainda está assentando, e o que se vê é só o assentar.
        let opacidade = (t * 1.6).clamp(0.02, 1.0);

        // A âncora é o centro da janela, que é onde o painel está.
        let centro = ui.max_rect().center();
        ctx.set_transform_layer(
            camada,
            egui::emath::TSTransform::from_translation(centro.to_vec2())
                * egui::emath::TSTransform::from_scaling(escala)
                * egui::emath::TSTransform::from_translation(-centro.to_vec2()),
        );
        ui.multiply_opacity(opacidade);
        crate::glass_gpu::definir_opacidade(opacidade);
        ctx.request_repaint();
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
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            let pulso = 0.5 + 0.5 * (tempo * 3.4).sin();
            let painter = ui.painter();
            painter.add(glass::glow_dot(
                rect.center(),
                9.5 + 4.5 * pulso,
                REC.gamma_multiply(0.20 + 0.32 * pulso),
            ));
            painter.circle_filled(rect.center(), 5.5, REC);
            // Reflexo no alto da bolinha: até ela é uma conta de vidro.
            painter.circle_filled(rect.center() - Vec2::new(1.4, 1.8), 1.9, glass::white(120));

            ui.add_space(3.0);
            ui.label(forte("Ouvindo", 17.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}:{:02.0}", decorrido / 60.0, decorrido % 60.0))
                        .size(14.0)
                        .color(MUTED)
                        .monospace(),
                );
            });
        });

        ui.add_space(10.0);
        self.waveform(ui, tempo);
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(nota("Solte"));
            widgets::keycap(ui, &keys::combo_label(&state.config.hotkey));
            ui.label(nota("para transcrever"));
        });
    }

    fn waveform(&mut self, ui: &mut egui::Ui, tempo: f32) {
        let altura = 54.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), altura), Sense::hover());

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
            painter.add(glass::glow(
                rect.shrink2(Vec2::new(rect.width() * 0.12, altura * 0.28)),
                altura,
                REC.gamma_multiply(0.10 + 0.16 * pico),
                48.0,
            ));
        }

        let vao = 4.5;
        let largura = ((rect.width() - vao * (total as f32 - 1.0)) / total as f32).max(1.0);
        let meio = rect.center().y;

        for (i, valor) in self.bars.iter().enumerate() {
            let x = rect.left() + i as f32 * (largura + vao);
            // Onda lenta atravessando as barras: mesmo em silêncio o painel
            // respira, deixando claro que está ouvindo.
            let onda = 0.5 + 0.5 * (tempo * 1.7 + i as f32 * 0.42).sin();
            let repouso = 4.0 + 11.0 * onda;
            let h = (valor * altura * 0.94).max(repouso);
            let barra = Rect::from_min_size(Pos2::new(x, meio - h / 2.0), Vec2::new(largura, h));

            // Frio no silêncio, quente na voz.
            let cor = glass::mix(glass::tint(150, 178, 235, 122), REC, valor.min(1.0));
            painter.add(glass::pastilha(barra, cor));
        }
    }

    // ----------------------------------------------------------- processando

    fn processing(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        let tempo = ui.input(|i| i.time) as f32;
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
            let painter = ui.painter();
            painter.add(glass::glow_dot(
                rect.center(),
                21.0,
                ACCENT.gamma_multiply(0.20),
            ));

            // Contas de luz girando: cada uma acende e apaga com um atraso, o
            // que dá a impressão de uma única gota correndo pelo anel.
            const CONTAS: usize = 10;
            for i in 0..CONTAS {
                let fase = (tempo * 1.15 - i as f32 / CONTAS as f32).rem_euclid(1.0);
                let brilho = (1.0 - fase).powf(2.2);
                let angulo =
                    std::f32::consts::TAU * i as f32 / CONTAS as f32 - std::f32::consts::FRAC_PI_2;
                let centro = rect.center() + Vec2::angled(angulo) * 14.5;
                painter.circle_filled(
                    centro,
                    1.8 + 1.5 * brilho,
                    ACCENT.gamma_multiply(0.16 + 0.84 * brilho),
                );
            }

            ui.add_space(10.0);
            ui.label(forte("Transcrevendo…", 15.0));
            if !state.status.is_empty() {
                ui.label(nota(&state.status));
            }
        });
    }

    // -------------------------------------------------------------- resultado

    fn result(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "resultado");

        ui.horizontal(|ui| {
            ui.label(forte("Texto transcrito", 16.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::botao_icone(ui, Icone::Fechar, "Fechar").clicked() {
                    self.act(UiAction::Hide);
                }
                if widgets::botao_icone(ui, Icone::Ajustes, "Configurações").clicked() {
                    self.act(UiAction::OpenSettings);
                }
                if !state.status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(nota(&state.status));
                }
            });
        });

        ui.add_space(10.0);

        let altura_texto = ui.available_height() - 60.0;
        widgets::cartao(ui, |ui| {
            ui.set_min_height(altura_texto - 24.0);
            if state.config.editable_result {
                ui.add_sized(
                    [ui.available_width(), altura_texto - 24.0],
                    egui::TextEdit::multiline(&mut state.text)
                        .desired_width(f32::INFINITY)
                        // O cartão já é a moldura; o campo entra sem a dele.
                        .frame(egui::Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(egui::TextStyle::Body),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(altura_texto - 24.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(&state.text).size(14.5));
                    });
            }
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let copiado = state
                .copied_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(3));

            let botao = if copiado {
                Botao::new("✔  Copiado").destaque(OK)
            } else {
                Botao::new("Copiar").destaque(ACCENT)
            };
            if ui.add(botao.largura_minima(126.0)).clicked() {
                self.act(UiAction::Copy);
            }

            if clipboard::paste_available() && widgets::botao(ui, "Copiar e colar").clicked() {
                self.act(UiAction::Paste);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.message.is_empty() {
                    ui.label(RichText::new(&state.message).size(11.5).color(REC));
                } else if state.config.auto_copy {
                    ui.label(nota("cópia automática ligada"));
                }
            });
        });
    }

    // ------------------------------------------------------------------ erro

    fn error(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        drag_area(ui, "erro");

        let carregando = state.model == ModelState::Loading;
        let cor = if carregando { ACCENT } else { REC };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            let painter = ui.painter();
            painter.add(glass::glow_dot(
                rect.center(),
                16.0,
                cor.gamma_multiply(0.22),
            ));
            painter.add(glass::peca(
                rect,
                13.0,
                Vidro::controle(0.0).com_corpo(glass::tint(cor.r(), cor.g(), cor.b(), 60)),
            ));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if carregando { "⏳" } else { "!" },
                fonte_forte(14.0),
                cor,
            );
            ui.add_space(4.0);
            ui.label(forte("Ditador", 16.5));
        });

        ui.add_space(12.0);
        ui.label(RichText::new(&state.message).size(13.5));
        ui.add_space(14.0);

        if self.modelo_faltando(ui, state) {
            return;
        }

        ui.horizontal(|ui| {
            if state.model == ModelState::Failed
                && ui
                    .add(Botao::new("Tentar de novo").destaque(ACCENT))
                    .clicked()
            {
                self.act(UiAction::ReloadModel);
            }
            if widgets::botao(ui, "Configurações").clicked() {
                self.act(UiAction::OpenSettings);
            }
            if widgets::botao(ui, "Fechar").clicked() {
                self.act(UiAction::Hide);
            }
        });
    }

    /// Instalação nova: o programa está inteiro, mas o modelo — que tem
    /// centenas de megabytes e não cabe num pacote — ainda não foi baixado.
    /// Em vez de mandar o usuário para o terminal, o botão resolve aqui.
    ///
    /// Devolve `true` quando assumiu a tela (aí o resto dos botões não sai).
    fn modelo_faltando(&self, ui: &mut egui::Ui, state: &crate::state::Shared) -> bool {
        if let Some(andamento) = &state.download {
            let p = andamento.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if p.andando() {
                let quanto = match (p.fracao(), p.total) {
                    (Some(f), total) => format!(
                        "{:.0} % de {}",
                        f * 100.0,
                        crate::modelo::tamanho_legivel(total)
                    ),
                    _ => format!("{} até agora", crate::modelo::tamanho_legivel(p.baixados)),
                };
                widgets::progresso(ui, p.fracao(), &format!("Baixando o modelo — {quanto}"));
                return true;
            }
            if let Some(Err(erro)) = &p.fim {
                ui.label(RichText::new(erro).size(11.5).color(REC));
                ui.add_space(8.0);
            }
        }

        if state.config.model_path.exists() {
            return false;
        }

        ui.horizontal(|ui| {
            let baixavel = crate::modelo::disponivel();
            ui.add_enabled_ui(baixavel, |ui| {
                if ui
                    .add(Botao::new("Baixar o modelo (574 MB)").destaque(ACCENT))
                    .on_hover_text(format!(
                        "Baixa {} de huggingface.co para {}",
                        crate::modelo::PADRAO,
                        crate::modelo::caminho(crate::modelo::PADRAO).display()
                    ))
                    .clicked()
                {
                    self.act(UiAction::DownloadModel);
                }
            });
            if widgets::botao(ui, "Configurações").clicked() {
                self.act(UiAction::OpenSettings);
            }
            if widgets::botao(ui, "Fechar").clicked() {
                self.act(UiAction::Hide);
            }
        });
        ui.add_space(6.0);
        ui.label(nota(if crate::modelo::disponivel() {
            "É a única coisa que falta. Depois disso tudo roda na sua máquina, \
             sem internet."
        } else {
            "Preciso do curl ou do wget para baixar: sudo apt install curl"
        }));
        true
    }

    // --------------------------------------------------------- configurações

    fn settings(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "config");

        ui.horizontal(|ui| {
            ui.label(forte("Configurações", 19.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::keycap(
                    ui,
                    &format!("v{} · {}", env!("CARGO_PKG_VERSION"), stt::BACKEND),
                );
            });
        });

        let rodape = 56.0;
        let area = egui::ScrollArea::vertical()
            .max_height(ui.available_height() - rodape)
            .show(ui, |ui| {
                self.settings_atalho(ui, state);
                self.settings_sistema(ui, state);
                self.settings_transcricao(ui, state);
                self.settings_area_transferencia(ui, state);
                self.settings_desempenho(ui, state);
                self.settings_aparencia(ui, state);
                self.settings_avancado(ui, state);
                ui.add_space(6.0);
            });

        // O conteúdo não termina no corte: ele some por baixo do vidro.
        let faixa = Rect::from_min_max(
            Pos2::new(area.inner_rect.left(), area.inner_rect.bottom() - 26.0),
            area.inner_rect.max,
        );
        ui.painter()
            .add(glass::gradiente_da_base(faixa, 0.0, 1.0, |t| {
                glass::tint(15, 16, 23, (196.0 * (1.0 - t).powf(1.5)) as u8)
            }));

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui
                .add(Botao::new("Salvar").destaque(ACCENT).largura_minima(112.0))
                .clicked()
            {
                self.act(UiAction::ApplyDraft);
            }
            if widgets::botao(ui, "Cancelar").clicked() {
                self.act(UiAction::CloseSettings);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(Botao::new("Encerrar o Ditador").cor(REC))
                    .on_hover_text("Fecha o aplicativo por completo")
                    .clicked()
                {
                    self.act(UiAction::Quit);
                }
            });
        });
    }

    fn settings_atalho(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Atalho");
        widgets::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Segure para falar:");

                if state.capturing_hotkey {
                    ui.label(forte("pressione a combinação…", 14.0).color(ACCENT));
                    if widgets::botao(ui, "Cancelar").clicked() {
                        self.act(UiAction::CancelHotkeyCapture);
                    }
                } else {
                    let atual = keys::combo_label(&state.draft.hotkey);
                    if ui
                        .add(Botao::new(RichText::new(atual).monospace()))
                        .on_hover_text("Clique e pressione a nova tecla ou combinação")
                        .clicked()
                    {
                        self.act(UiAction::StartHotkeyCapture);
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(nota(
                "A leitura é passiva: a tecla continua funcionando normalmente nos \
                 outros programas. Prefira teclas sem função própria (Pause, F13…). \
                 Esc cancela a captura.",
            ));
        });
    }

    /// Início automático. É a única chave da tela que vale na hora, sem passar
    /// pelo Salvar: quem guarda o estado é o systemd (ou o autostart do XDG), e
    /// não teria sentido a tela discordar do sistema até alguém salvar.
    fn settings_sistema(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Sistema");
        widgets::cartao(ui, |ui| {
            let mut ligado = state.draft.start_with_session;
            if widgets::interruptor(ui, &mut ligado, "Iniciar junto com a sessão").changed() {
                match crate::autostart::definir(ligado) {
                    Ok(()) => {
                        state.draft.start_with_session = ligado;
                        state.config.start_with_session = ligado;
                        state.message.clear();
                    }
                    Err(e) => {
                        state.message = format!("Não consegui mudar o início automático: {e:#}");
                    }
                }
            }
            ui.label(nota(match crate::autostart::metodo() {
                crate::autostart::Metodo::Systemd => {
                    "Pelo serviço de usuário do systemd. Vale na hora, sem precisar salvar. \
                     Para ver o que está acontecendo: journalctl --user -u ditador -f"
                }
                crate::autostart::Metodo::Xdg => {
                    "Por um atalho em ~/.config/autostart. Vale na hora, sem precisar salvar. \
                     Instalando pelo pacote, passa a usar o serviço do systemd."
                }
            }));
        });
    }

    fn settings_transcricao(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Transcrição");
        widgets::cartao(ui, |ui| {
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

            widgets::interruptor(ui, &mut state.draft.translate, "Traduzir para inglês");

            ui.horizontal(|ui| {
                ui.label("Microfone:");
                let atual = state
                    .draft
                    .input_device
                    .clone()
                    .unwrap_or_else(|| "Padrão do sistema".to_string());
                let dispositivos = state.devices.clone();
                egui::ComboBox::from_id_salt("microfone")
                    .selected_text(encurtar(&atual, 34))
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.draft.input_device,
                            None,
                            "Padrão do sistema",
                        );
                        for nome in &dispositivos {
                            ui.selectable_value(
                                &mut state.draft.input_device,
                                Some(nome.clone()),
                                encurtar(nome, 46),
                            );
                        }
                    });
            });
        });
    }

    fn settings_area_transferencia(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Área de transferência");
        widgets::cartao(ui, |ui| {
            widgets::interruptor(
                ui,
                &mut state.draft.auto_copy,
                "Copiar o texto automaticamente ao terminar",
            );

            // Só faz sentido se o texto for parar na área de transferência
            // sozinho; sem isso a janela é o único jeito de pegá-lo.
            let copia_sozinho = state.draft.auto_copy || state.draft.auto_paste;
            ui.add_enabled_ui(copia_sozinho, |ui| {
                widgets::interruptor(
                    ui,
                    &mut state.draft.show_result,
                    "Mostrar a janela com o texto transcrito",
                );
            });
            if !copia_sozinho {
                ui.label(nota(
                    "Sem a cópia automática a janela sempre aparece — é por ela \
                     que o texto é pego.",
                ));
            } else if !state.draft.show_result {
                ui.label(nota(
                    "Nada vai aparecer na tela: solte a tecla, espere o ícone da \
                     barra voltar ao normal e cole. Se algo der errado, a janela \
                     aparece assim mesmo, para o texto não se perder.",
                ));
            }

            let ydotool = clipboard::paste_available();
            ui.add_enabled_ui(ydotool, |ui| {
                widgets::interruptor(
                    ui,
                    &mut state.draft.auto_paste,
                    "Colar na janela em foco (Ctrl+V)",
                );
            });
            if !ydotool {
                ui.label(nota(
                    "Colagem automática requer o ydotool: sudo apt install ydotool",
                ));
            } else if state.draft.auto_paste {
                ui.label(nota(
                    "Com a colagem automática a janela de resultado não aparece — \
                     o texto vai direto para onde você estava escrevendo.",
                ));
            }

            if !clipboard::wl_copy_available() {
                ui.label(nota(
                    "wl-copy não encontrado; usando a área de transferência do X11.",
                ));
            }
        });
    }

    fn settings_desempenho(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Desempenho");
        widgets::cartao(ui, |ui| {
            ui.add_enabled_ui(stt::GPU_CAPABLE, |ui| {
                widgets::interruptor(
                    ui,
                    &mut state.draft.use_gpu,
                    format!("Usar a GPU ({})", stt::BACKEND),
                );
            });
            if !stt::GPU_CAPABLE {
                ui.label(nota("Este binário foi compilado só para CPU."));
            }

            ui.add(egui::Slider::new(&mut state.draft.threads, 1..=16).text("Threads de CPU"));

            ui.add_space(2.0);
            ui.label("Modelo:");
            let mut caminho = state.draft.model_path.display().to_string();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut caminho)
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(10, 7)),
                )
                .changed()
            {
                state.draft.model_path = caminho.into();
            }

            let existe = state.draft.model_path.exists();
            ui.label(
                RichText::new(if existe {
                    "Arquivo encontrado."
                } else {
                    "Arquivo não encontrado — a tela inicial oferece baixá-lo"
                })
                .size(11.5)
                .color(if existe { MUTED } else { REC }),
            );
        });
    }

    /// Os controles do vidro. É um recorte: o `config.json` tem todos, e a
    /// mudança aqui vale no quadro seguinte, então dá para ver o efeito
    /// enquanto se arrasta o controle.
    fn settings_aparencia(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Aparência");
        widgets::cartao(ui, |ui| {
            let ap = &mut state.draft.appearance;

            widgets::interruptor(ui, &mut ap.wallpaper, "Papel de parede por baixo do vidro");
            ui.add_enabled_ui(ap.wallpaper, |ui| {
                porcentagem(ui, &mut ap.wallpaper_opacity, 0.0..=1.0, "Quanto aparece");
                let mut nitidez = ap.wallpaper_detail as i32;
                if ui
                    .add(
                        egui::Slider::new(&mut nitidez, 60..=1200)
                            .text("Detalhe (menor = mais borrado)"),
                    )
                    .changed()
                {
                    ap.wallpaper_detail = nitidez as u32;
                }
            });
            ui.label(nota(
                "O vidro precisa de algo para refratar, e nenhum compositor do \
                 Linux entrega o que está atrás da janela. Desligue para deixar \
                 o painel só com a tinta escura.",
            ));

            ui.add_space(4.0);
            ui.add(
                egui::Slider::new(&mut ap.refraction, 1.0..=2.0)
                    .text("Refração")
                    .fixed_decimals(2),
            );
            porcentagem(ui, &mut ap.edge, 0.0..=2.0, "Brilho das bordas");
            porcentagem(ui, &mut ap.sheen, 0.0..=2.0, "Véu da superfície");
            porcentagem(ui, &mut ap.shadow, 0.0..=1.0, "Sombra projetada");

            ui.add_space(4.0);
            widgets::interruptor(ui, &mut ap.animation, "Animação de mola ao abrir");
            ui.add_enabled_ui(ap.animation, |ui| {
                let mut ms = ap.animation_ms as i32;
                if ui
                    .add(egui::Slider::new(&mut ms, 0..=800).text("Duração (ms)"))
                    .changed()
                {
                    ap.animation_ms = ms as u64;
                }
                porcentagem(ui, &mut ap.animation_bounce, 0.0..=1.0, "Ultrapassagem");
            });

            ui.add_space(6.0);
            if widgets::botao(ui, "Voltar ao padrão").clicked() {
                *ap = crate::config::Appearance::PADRAO;
            }
        });
    }

    fn settings_avancado(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Avançado");
        widgets::cartao(ui, |ui| {
            ui.label(nota(
                "Contexto passado ao modelo (jargão, nomes próprios, estilo de pontuação):",
            ));
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.initial_prompt)
                    .desired_rows(2)
                    .margin(Margin::symmetric(10, 7))
                    .desired_width(f32::INFINITY),
            );

            widgets::interruptor(
                ui,
                &mut state.draft.normalize_audio,
                "Normalizar o volume antes de transcrever",
            );
            widgets::interruptor(
                ui,
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

            widgets::interruptor(
                ui,
                &mut state.draft.force_x11,
                "Desenhar a janela via XWayland (recomendado no GNOME)",
            );
            ui.label(nota(
                "Sem isso o GNOME decide onde a janela aparece e ela pode ficar \
                 atrás das outras. Mudança exige reiniciar o Ditador.",
            ));
        });
    }
}

// --------------------------------------------------------------------- apoio

/// Faixa invisível no topo que permite arrastar a janela sem decoração.
fn drag_area(ui: &mut egui::Ui, id: &str) {
    let rect = egui::Rect::from_min_size(
        ui.max_rect().left_top(),
        Vec2::new(ui.available_width(), 28.0),
    );
    let response = ui.interact(rect, egui::Id::new(id), Sense::drag());
    if response.dragged() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

/// Controle deslizante de um fator, mostrado em porcentagem — que é como se lê
/// "quanto disto", bem melhor do que 0,55.
fn porcentagem(
    ui: &mut egui::Ui,
    valor: &mut f32,
    faixa: std::ops::RangeInclusive<f32>,
    rotulo: &str,
) {
    let mut pct = (*valor * 100.0).round();
    let limites = (*faixa.start() * 100.0).round()..=(*faixa.end() * 100.0).round();
    if ui
        .add(
            egui::Slider::new(&mut pct, limites)
                .suffix(" %")
                .text(rotulo),
        )
        .changed()
    {
        *valor = pct / 100.0;
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
