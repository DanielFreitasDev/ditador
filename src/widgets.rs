//! Controles feitos de vidro.
//!
//! O egui desenha botões e caixas de seleção como retângulos chapados; aqui
//! cada controle é uma peça de `glass.rs` — mesma silhueta de squircle, mesma
//! borda especular, mesma faixa de refração do painel. Além da aparência, todos
//! reagem ao cursor com uma animação curta: no vidro líquido a luz responde ao
//! toque, e é isso que separa "um retângulo translúcido" de "vidro".

use crate::glass::{self, Vidro};
use egui::{
    Color32, Id, Pos2, Rect, Response, Sense, Stroke, TextStyle, TextWrapMode, Ui, Vec2, WidgetText,
};

/// Cores compartilhadas com a interface.
pub const TEXT: Color32 = Color32::from_rgb(242, 243, 248);
pub const MUTED: Color32 = Color32::from_rgb(156, 159, 176);
pub const REC: Color32 = Color32::from_rgb(255, 96, 108);
pub const ACCENT: Color32 = Color32::from_rgb(122, 173, 255);
pub const OK: Color32 = Color32::from_rgb(112, 224, 158);

/// Tempo das animações de realce. Curto o bastante para parecer resposta física.
const ANIM: f32 = 0.14;

// ---------------------------------------------------------------------- botão

/// Botão em cápsula de vidro.
pub struct Botao {
    texto: WidgetText,
    cor: Color32,
    /// Botão principal da tela: recebe tinta em vez de só clarear.
    destaque: bool,
    largura_minima: f32,
}

impl Botao {
    pub fn new(texto: impl Into<WidgetText>) -> Self {
        Self {
            texto: texto.into(),
            cor: TEXT,
            destaque: false,
            largura_minima: 0.0,
        }
    }

    /// Botão principal: fundo tingido com `cor`.
    pub fn destaque(mut self, cor: Color32) -> Self {
        self.cor = cor;
        self.destaque = true;
        self
    }

    /// Botão comum, com o texto colorido (avisos, ações destrutivas).
    pub fn cor(mut self, cor: Color32) -> Self {
        self.cor = cor;
        self
    }

    pub fn largura_minima(mut self, largura: f32) -> Self {
        self.largura_minima = largura;
        self
    }
}

impl egui::Widget for Botao {
    fn ui(self, ui: &mut Ui) -> Response {
        let galley = self.texto.into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );

        let tamanho = Vec2::new(
            (galley.size().x + 36.0).max(self.largura_minima),
            (galley.size().y + 18.0).max(36.0),
        );
        let (rect, resposta) = ui.allocate_at_least(tamanho, Sense::click());
        if !ui.is_rect_visible(rect) {
            return resposta;
        }

        let ativo = ui.is_enabled();
        let sob_cursor = realce(ui, &resposta, ativo);
        let pressionado = ui.ctx().animate_bool_with_time(
            resposta.id.with("pressao"),
            ativo && resposta.is_pointer_button_down_on(),
            0.06,
        );

        // Afunda um fio ao ser pressionado: o vidro cede.
        let rect = rect.shrink(pressionado * 1.5);
        let raio = rect.height() / 2.0;
        let energia = (0.45 * sob_cursor + 0.55 * pressionado).min(1.0);

        let mut vidro = Vidro::controle(energia);
        let mut cor_texto = self.cor;
        if self.destaque {
            let alfa = (76.0 + 54.0 * energia) as u8;
            vidro = vidro.com_corpo(glass::tint(self.cor.r(), self.cor.g(), self.cor.b(), alfa));
            vidro.borda += 0.25;
            cor_texto = TEXT;
        }

        let painter = ui.painter();
        if sob_cursor > 0.0 {
            let halo = if self.destaque { self.cor } else { TEXT };
            painter.add(glass::glow(
                rect.expand(3.0),
                raio,
                halo.gamma_multiply(0.10 * sob_cursor),
                16.0,
            ));
        }
        painter.add(glass::peca(rect, raio, apagar(vidro, ativo)));

        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos.round(), galley, cor(cor_texto, ativo));

        resposta
    }
}

/// Atalho para `ui.add(Botao::new(..))`.
pub fn botao(ui: &mut Ui, texto: impl Into<WidgetText>) -> Response {
    ui.add(Botao::new(texto))
}

// -------------------------------------------------------------- botão de ícone

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icone {
    Fechar,
    Ajustes,
}

/// Botão redondo de vidro com um ícone desenhado a vetor — nada de glifos de
/// fonte, que em tamanho pequeno saem borrados e desalinhados.
pub fn botao_icone(ui: &mut Ui, icone: Icone, dica: &str) -> Response {
    let (rect, resposta) = ui.allocate_at_least(Vec2::splat(30.0), Sense::click());
    let resposta = resposta.on_hover_text(dica);
    if !ui.is_rect_visible(rect) {
        return resposta;
    }

    let ativo = ui.is_enabled();
    let sob_cursor = realce(ui, &resposta, ativo);
    let pressionado = ui.ctx().animate_bool_with_time(
        resposta.id.with("pressao"),
        ativo && resposta.is_pointer_button_down_on(),
        0.06,
    );
    let energia = (0.5 * sob_cursor + 0.5 * pressionado).min(1.0);

    let painter = ui.painter();
    painter.add(glass::peca(
        rect,
        rect.height() / 2.0,
        apagar(Vidro::controle(energia), ativo),
    ));

    let c = rect.center();
    let cor_traco = cor(glass::mix(MUTED, TEXT, sob_cursor), ativo);
    match icone {
        Icone::Fechar => {
            let d = 4.2;
            let traco = Stroke::new(1.5, cor_traco);
            painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], traco);
            painter.line_segment([c + Vec2::new(d, -d), c + Vec2::new(-d, d)], traco);
        }
        Icone::Ajustes => {
            // Três cursores deslizantes, como o ícone de ajustes do iOS.
            let traco = Stroke::new(1.4, cor_traco);
            for (i, x) in [1.5f32, -1.5, 3.0].into_iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * 4.6;
                painter.line_segment([Pos2::new(c.x - 6.0, y), Pos2::new(c.x + 6.0, y)], traco);
                painter.circle_filled(Pos2::new(c.x + x, y), 2.1, cor_traco);
            }
        }
    }

    resposta
}

// ---------------------------------------------------------------- interruptor

/// Linha inteira com rótulo à esquerda e um interruptor de vidro à direita.
/// Clicar em qualquer ponto da linha alterna o valor.
pub fn interruptor(ui: &mut Ui, ligado: &mut bool, rotulo: impl Into<WidgetText>) -> Response {
    const TRILHO: Vec2 = Vec2::new(46.0, 27.0);

    let largura = ui.available_width();
    let rotulo: WidgetText = rotulo.into();
    let galley = rotulo.into_galley(
        ui,
        Some(TextWrapMode::Wrap),
        (largura - TRILHO.x - 16.0).max(40.0),
        TextStyle::Body,
    );
    let altura = galley.size().y.max(TRILHO.y) + 8.0;

    let (rect, mut resposta) = ui.allocate_at_least(Vec2::new(largura, altura), Sense::click());
    if resposta.clicked() {
        *ligado = !*ligado;
        resposta.mark_changed();
    }
    if !ui.is_rect_visible(rect) {
        return resposta;
    }

    let ativo = ui.is_enabled();
    let sob_cursor = realce(ui, &resposta, ativo);
    let t = ui
        .ctx()
        .animate_bool_with_time(resposta.id.with("ligado"), *ligado, 0.18);

    let painter = ui.painter();
    painter.galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0).round(),
        galley,
        cor(TEXT, ativo),
    );

    let trilho = Rect::from_center_size(
        Pos2::new(rect.right() - TRILHO.x / 2.0, rect.center().y),
        TRILHO,
    );
    let raio = trilho.height() / 2.0;

    if t > 0.0 {
        painter.add(glass::glow(
            trilho.expand(2.0),
            raio,
            ACCENT.gamma_multiply(0.16 * t * if ativo { 1.0 } else { 0.4 }),
            14.0,
        ));
    }
    let corpo = glass::mix(
        glass::white((22.0 + 16.0 * sob_cursor) as u8),
        glass::tint(ACCENT.r(), ACCENT.g(), ACCENT.b(), 205),
        t,
    );
    let mut vidro = Vidro::controle(0.25 * sob_cursor).com_corpo(corpo);
    vidro.borda += 0.3 * t;
    painter.add(glass::peca(trilho, raio, apagar(vidro, ativo)));

    // O botão desliza e cresce um fio ao ligar, como se a luz o inflasse.
    let curso = trilho.width() - trilho.height();
    let centro = Pos2::new(trilho.left() + raio + curso * t, trilho.center().y);
    let botao_raio = raio - 3.5 + 0.6 * t;
    let botao = Rect::from_center_size(centro, Vec2::splat(botao_raio * 2.0));
    painter.add(glass::glow(
        botao.translate(Vec2::new(0.0, 1.5)),
        botao_raio,
        Color32::from_black_alpha(if ativo { 70 } else { 30 }),
        6.0,
    ));
    painter.add(glass::peca(
        botao,
        botao_raio,
        apagar(
            Vidro {
                corpo: glass::white(if *ligado { 250 } else { 226 }),
                brilho: 0.9,
                borda: 0.8,
                lente: 2.0,
                base: 0.0,
                foco: None,
            },
            ativo,
        ),
    ));

    resposta
}

// -------------------------------------------------------------------- cartões

/// Cartão de vidro por trás de um grupo de controles.
///
/// O conteúdo é disposto primeiro e a peça é inserida atrás depois (`set`),
/// porque só ao final se sabe a altura que ele ocupou.
pub fn cartao<R>(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui) -> R) -> R {
    const MARGEM: f32 = 14.0;

    let largura = ui.available_width();
    let lugar = ui.painter().add(egui::Shape::Noop);
    let resposta = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(MARGEM as i8, (MARGEM - 2.0) as i8))
        .show(ui, |ui| {
            ui.set_min_width(largura - 2.0 * MARGEM);
            conteudo(ui)
        });

    ui.painter().set(
        lugar,
        glass::peca(resposta.response.rect, glass::RAIO_CARTAO, Vidro::cartao()),
    );
    resposta.inner
}

/// Título de seção, acima de um cartão.
pub fn secao(ui: &mut Ui, titulo: &str) {
    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(titulo.to_uppercase())
            .size(10.5)
            .color(MUTED)
            .family(egui::FontFamily::Name("forte".into())),
    );
    ui.add_space(5.0);
}

/// Tecla de teclado desenhada como uma peça de vidro — usada para mostrar o
/// atalho no meio de uma frase.
pub fn keycap(ui: &mut Ui, texto: &str) -> Response {
    let galley = WidgetText::from(texto).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Small,
    );
    let tamanho = Vec2::new(galley.size().x + 14.0, galley.size().y + 8.0);
    let (rect, resposta) = ui.allocate_at_least(tamanho, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.add(glass::peca(
            rect,
            7.0,
            Vidro {
                corpo: glass::white(30),
                brilho: 0.7,
                borda: 0.55,
                lente: 2.5,
                base: 0.2,
                foco: None,
            },
        ));
        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos.round(), galley, TEXT);
    }
    resposta
}

// ---------------------------------------------------------------------- apoio

/// Animação de "sob o cursor", de 0 a 1.
fn realce(ui: &Ui, resposta: &Response, ativo: bool) -> f32 {
    ui.ctx()
        .animate_bool_with_time(id_de(resposta), ativo && resposta.hovered(), ANIM)
}

fn id_de(resposta: &Response) -> Id {
    resposta.id.with("realce")
}

/// Apaga uma peça inteira quando o controle está desabilitado.
fn apagar(mut vidro: Vidro, ativo: bool) -> Vidro {
    if !ativo {
        vidro.corpo = vidro.corpo.gamma_multiply(0.45);
        vidro.brilho *= 0.4;
        vidro.borda *= 0.35;
        vidro.lente *= 0.5;
        vidro.base *= 0.3;
    }
    vidro
}

fn cor(c: Color32, ativo: bool) -> Color32 {
    if ativo { c } else { c.gamma_multiply(0.45) }
}
