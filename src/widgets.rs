//! Os controles do Ditador, em cores sólidas.
//!
//! Cada um é um retângulo arredondado preenchido com uma cor da paleta (ver
//! `tema.rs`) e, quando precisa se separar do fundo, uma borda de um pixel. Sob
//! o cursor a superfície troca de tom numa animação curta — é a única coisa que
//! se move, e custa uma interpolação de cor por quadro.
//!
//! A hierarquia é a mesma em toda tela: **um** botão principal, em cor cheia e
//! invertida em relação ao fundo; o resto em superfície discreta.

use crate::tema::{self, paleta};
use egui::{
    Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, TextStyle,
    TextWrapMode, Ui, Vec2, WidgetText,
};

/// Tempo das animações de realce. Curto o bastante para parecer resposta, não
/// transição.
const ANIM: f32 = 0.12;

/// Altura dos botões e dos campos de uma linha.
const ALTURA: f32 = 36.0;

// ---------------------------------------------------------------------- botão

/// O peso de um botão dentro da tela.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Peso {
    /// A ação da tela: cor cheia, invertida em relação ao fundo.
    Principal,
    /// Todo o resto: superfície discreta com borda.
    Comum,
    /// Ação destrutiva: sem preenchimento, texto vermelho.
    Perigo,
}

pub struct Botao {
    texto: WidgetText,
    peso: Peso,
    largura_minima: f32,
}

impl Botao {
    pub fn new(texto: impl Into<WidgetText>) -> Self {
        Self {
            texto: texto.into(),
            peso: Peso::Comum,
            largura_minima: 0.0,
        }
    }

    pub fn principal(mut self) -> Self {
        self.peso = Peso::Principal;
        self
    }

    pub fn perigo(mut self) -> Self {
        self.peso = Peso::Perigo;
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
            (galley.size().x + 32.0).max(self.largura_minima),
            ALTURA.max(galley.size().y + 16.0),
        );
        let (rect, resposta) = ui.allocate_at_least(tamanho, Sense::click());
        if !ui.is_rect_visible(rect) {
            return resposta;
        }

        let ativo = ui.is_enabled();
        let realce = animar(ui, &resposta, ativo && resposta.hovered());
        let pressao = animar_com(
            ui,
            resposta.id.with("pressao"),
            ativo && resposta.is_pointer_button_down_on(),
            0.05,
        );
        let energia = (realce + pressao).min(1.0);

        let p = paleta();
        let (fundo, borda, cor_texto) = match self.peso {
            Peso::Principal => (
                // O botão principal já é a cor mais forte da tela: sob o cursor
                // ele recua na direção do fundo em vez de acender mais.
                mistura(p.primario, p.fundo, 0.10 * energia),
                Color32::TRANSPARENT,
                p.sobre_primario,
            ),
            Peso::Comum => (
                mistura(p.superficie, p.superficie_forte, energia),
                mistura(p.borda, p.borda_forte, energia),
                p.texto,
            ),
            Peso::Perigo => (
                mistura(Color32::TRANSPARENT, p.erro.gamma_multiply(0.14), energia),
                mistura(p.borda, p.erro.gamma_multiply(0.5), energia),
                p.erro,
            ),
        };

        // Afunda um fio ao ser pressionado.
        let rect = rect.shrink(pressao);
        let raio = CornerRadius::same((rect.height() / 2.0) as u8);
        let painter = ui.painter();
        painter.rect_filled(rect, raio, esmaecer(fundo, ativo));
        if borda != Color32::TRANSPARENT {
            painter.rect_stroke(
                rect,
                raio,
                Stroke::new(1.0, esmaecer(borda, ativo)),
                StrokeKind::Inside,
            );
        }

        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos.round(), galley, esmaecer(cor_texto, ativo));
        resposta
    }
}

/// Atalho para um botão comum.
pub fn botao(ui: &mut Ui, texto: impl Into<WidgetText>) -> Response {
    ui.add(Botao::new(texto))
}

// ------------------------------------------------------------ botão de ícone

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icone {
    Fechar,
    Ajustes,
}

/// Botão redondo com o ícone desenhado a vetor — nada de glifos de fonte, que
/// em tamanho pequeno saem borrados e desalinhados.
pub fn botao_icone(ui: &mut Ui, icone: Icone, dica: &str) -> Response {
    let (rect, resposta) = ui.allocate_at_least(Vec2::splat(32.0), Sense::click());
    let resposta = resposta.on_hover_text(dica);
    if !ui.is_rect_visible(rect) {
        return resposta;
    }

    let ativo = ui.is_enabled();
    let realce = animar(ui, &resposta, ativo && resposta.hovered());
    let p = paleta();

    let painter = ui.painter();
    if realce > 0.0 {
        painter.circle_filled(
            rect.center(),
            rect.height() / 2.0,
            p.superficie_forte.gamma_multiply(realce),
        );
    }

    let c = rect.center();
    let cor = esmaecer(mistura(p.texto_fraco, p.texto, realce), ativo);
    match icone {
        Icone::Fechar => {
            let d = 4.5;
            let traco = Stroke::new(1.6, cor);
            painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], traco);
            painter.line_segment([c + Vec2::new(d, -d), c + Vec2::new(-d, d)], traco);
        }
        Icone::Ajustes => {
            // Três cursores deslizantes, como qualquer ícone de ajustes.
            let traco = Stroke::new(1.5, cor);
            for (i, x) in [1.5f32, -1.5, 3.0].into_iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * 4.8;
                painter.line_segment([Pos2::new(c.x - 6.0, y), Pos2::new(c.x + 6.0, y)], traco);
                painter.circle_filled(Pos2::new(c.x + x, y), 2.2, cor);
            }
        }
    }

    resposta
}

// ------------------------------------------------------------------ progresso

/// Barra de progresso. Com `fracao` em `None` fica indeterminada: uma faixa
/// indo e voltando pelo trilho, para o caso de o servidor não dizer o tamanho
/// do arquivo.
pub fn progresso(ui: &mut Ui, fracao: Option<f32>, rotulo: &str) {
    const ALTURA_BARRA: f32 = 8.0;

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ALTURA_BARRA),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }

    let p = paleta();
    let raio = CornerRadius::same((ALTURA_BARRA / 2.0) as u8);
    let painter = ui.painter();
    painter.rect_filled(rect, raio, p.superficie_forte);

    let cheio = match fracao {
        Some(f) => {
            let largura = (rect.width() * f.clamp(0.0, 1.0)).max(ALTURA_BARRA);
            Rect::from_min_size(rect.min, Vec2::new(largura, ALTURA_BARRA))
        }
        None => {
            let t = ui.input(|i| i.time) as f32 * 0.8;
            let largura = rect.width() * 0.3;
            let curso = (rect.width() - largura).max(0.0);
            let x = rect.left() + curso * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos());
            ui.ctx().request_repaint();
            Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(largura, ALTURA_BARRA))
        }
    };
    painter.rect_filled(cheio, raio, p.primario);

    if !rotulo.is_empty() {
        ui.add_space(6.0);
        ui.label(tema::nota(rotulo));
    }
}

// ---------------------------------------------------------------- interruptor

/// Linha inteira com o rótulo à esquerda e o interruptor à direita. Clicar em
/// qualquer ponto da linha alterna o valor.
pub fn interruptor(ui: &mut Ui, ligado: &mut bool, rotulo: impl Into<WidgetText>) -> Response {
    const TRILHO: Vec2 = Vec2::new(42.0, 24.0);

    let largura = ui.available_width();
    let galley = rotulo.into().into_galley(
        ui,
        Some(TextWrapMode::Wrap),
        (largura - TRILHO.x - 16.0).max(40.0),
        TextStyle::Body,
    );
    let altura = galley.size().y.max(TRILHO.y) + 6.0;

    let (rect, mut resposta) = ui.allocate_at_least(Vec2::new(largura, altura), Sense::click());
    if resposta.clicked() {
        *ligado = !*ligado;
        resposta.mark_changed();
    }
    if !ui.is_rect_visible(rect) {
        return resposta;
    }

    let ativo = ui.is_enabled();
    let realce = animar(ui, &resposta, ativo && resposta.hovered());
    let t = animar_com(ui, resposta.id.with("ligado"), *ligado, 0.15);

    let p = paleta();
    let painter = ui.painter();
    painter.galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0).round(),
        galley,
        esmaecer(p.texto, ativo),
    );

    let trilho = Rect::from_center_size(
        Pos2::new(rect.right() - TRILHO.x / 2.0, rect.center().y),
        TRILHO,
    );
    let raio = trilho.height() / 2.0;
    let desligado = mistura(p.superficie_forte, p.borda_forte, 0.4 * realce);
    painter.rect_filled(
        trilho,
        CornerRadius::same(raio as u8),
        esmaecer(mistura(desligado, p.primario, t), ativo),
    );

    // O botão desliza de uma ponta à outra; a cor dele é a do fundo quando
    // desligado e a do texto sobre o principal quando ligado.
    let curso = trilho.width() - trilho.height();
    let centro = Pos2::new(trilho.left() + raio + curso * t, trilho.center().y);
    let bolinha = mistura(p.fundo, p.sobre_primario, t);
    painter.circle_filled(centro, raio - 3.0, esmaecer(bolinha, ativo));

    resposta
}

// ------------------------------------------------------------------ segmentado

/// Escolha entre poucas opções, lado a lado numa cápsula. Para conjuntos de
/// duas ou três opções curtas é melhor que uma lista suspensa: mostra todas as
/// alternativas de uma vez e resolve em um clique.
pub fn segmentado<T: PartialEq + Copy>(
    ui: &mut Ui,
    valor: &mut T,
    opcoes: &[(T, &str)],
) -> Response {
    const ALTURA_SEG: f32 = 32.0;

    let largura = ui.available_width();
    let (rect, mut resposta) = ui.allocate_at_least(Vec2::new(largura, ALTURA_SEG), Sense::hover());
    if !ui.is_rect_visible(rect) || opcoes.is_empty() {
        return resposta;
    }

    let p = paleta();
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie);
    painter.rect_stroke(
        rect,
        CornerRadius::same(tema::RAIO_CONTROLE),
        Stroke::new(1.0, p.borda),
        StrokeKind::Inside,
    );

    let passo = rect.width() / opcoes.len() as f32;
    for (i, (opcao, rotulo)) in opcoes.iter().enumerate() {
        let celula = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * passo, rect.top()),
            Vec2::new(passo, rect.height()),
        )
        .shrink(3.0);
        let clique = ui.interact(celula, resposta.id.with(i), Sense::click());
        if clique.clicked() && *valor != *opcao {
            *valor = *opcao;
            resposta.mark_changed();
        }

        let escolhida = *valor == *opcao;
        let realce = animar(ui, &clique, ui.is_enabled() && clique.hovered());
        if escolhida {
            painter.rect_filled(celula, CornerRadius::same(tema::RAIO_CONTROLE - 3), p.fundo);
            painter.rect_stroke(
                celula,
                CornerRadius::same(tema::RAIO_CONTROLE - 3),
                Stroke::new(1.0, p.borda),
                StrokeKind::Inside,
            );
        }
        let cor = if escolhida {
            p.texto
        } else {
            mistura(p.texto_fraco, p.texto, realce)
        };
        painter.text(
            celula.center(),
            egui::Align2::CENTER_CENTER,
            rotulo,
            tema::fonte_media(13.5),
            esmaecer(cor, ui.is_enabled()),
        );
    }

    resposta
}

// -------------------------------------------------------------------- cartões

/// Cartão por trás de um grupo de controles: superfície, borda de um pixel.
pub fn cartao<R>(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui) -> R) -> R {
    let p = paleta();
    egui::Frame::NONE
        .fill(p.superficie)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(tema::RAIO_CARTAO))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width() - 28.0);
            conteudo(ui)
        })
        .inner
}

/// Título de seção, acima de um cartão.
pub fn secao(ui: &mut Ui, titulo: &str) {
    ui.add_space(12.0);
    ui.label(
        tema::medio(titulo.to_uppercase(), 11.0)
            .color(paleta().texto_fraco)
            .extra_letter_spacing(0.6),
    );
    ui.add_space(4.0);
}

/// Tecla de teclado, para mostrar o atalho no meio de uma frase.
pub fn keycap(ui: &mut Ui, texto: &str) -> Response {
    etiqueta(ui, texto, paleta().texto)
}

/// Etiqueta pequena: mesma caixa da tecla, para versão e estado.
pub fn etiqueta(ui: &mut Ui, texto: &str, cor: Color32) -> Response {
    let galley = WidgetText::from(egui::RichText::new(texto).monospace().size(11.5)).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Small,
    );
    let tamanho = Vec2::new(galley.size().x + 14.0, galley.size().y + 8.0);
    let (rect, resposta) = ui.allocate_at_least(tamanho, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = paleta();
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(7), p.superficie);
        painter.rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0, p.borda),
            StrokeKind::Inside,
        );
        painter.galley((rect.center() - galley.size() / 2.0).round(), galley, cor);
    }
    resposta
}

// ---------------------------------------------------------------------- apoio

/// Interpola duas cores. `t` vai de 0 (a primeira) a 1 (a segunda).
pub fn mistura(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let canal = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        canal(a.r(), b.r()),
        canal(a.g(), b.g()),
        canal(a.b(), b.b()),
        canal(a.a(), b.a()),
    )
}

/// Apaga uma cor quando o controle está desabilitado.
fn esmaecer(cor: Color32, ativo: bool) -> Color32 {
    if ativo {
        cor
    } else {
        mistura(cor, paleta().fundo, 0.55)
    }
}

fn animar(ui: &Ui, resposta: &Response, condicao: bool) -> f32 {
    animar_com(ui, resposta.id.with("realce"), condicao, ANIM)
}

fn animar_com(ui: &Ui, id: egui::Id, condicao: bool, tempo: f32) -> f32 {
    ui.ctx().animate_bool_with_time(id, condicao, tempo)
}
