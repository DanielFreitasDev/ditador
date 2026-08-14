//! Vocabulário visual "vidro líquido".
//!
//! Nenhum compositor do Linux expõe desfoque de fundo para um aplicativo comum,
//! então o vidro não vem de borrar o que está atrás — vem das outras pistas que
//! o olho usa para reconhecer vidro grosso:
//!
//!   * tinta escura translúcida, deixando o fundo aparecer só como um tom;
//!   * gradiente de brilho descendo do topo, como luz entrando pela quina;
//!   * borda especular que acende em cima e apaga embaixo;
//!   * halos desfocados atrás dos elementos vivos, como luz refratada.

use egui::epaint::{PathShape, PathStroke, RectShape, Shadow};
use egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Vec2};

/// Raio dos cantos do painel principal.
pub const RADIUS: f32 = 24.0;
/// Espaço reservado em volta do painel para a sombra projetada.
pub const SHADOW_PAD: f32 = 12.0;

/// Branco com alfa (já pré-multiplicado, que é o formato interno do egui).
pub const fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
}

/// Cor com alfa, informada em componentes normais (não pré-multiplicados).
pub fn tint(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Painel de vidro: sombra, corpo translúcido, brilho no topo e borda especular.
pub fn panel(painter: &Painter, rect: Rect, radius: f32) {
    painter.add(
        Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(130),
        }
        .as_shape(rect, radius),
    );

    // Corpo. Translúcido o bastante para o fundo virar um tom, opaco o bastante
    // para o texto continuar legível — sem desfoque disponível, um vidro
    // transparente demais deixaria a leitura sobre qualquer coisa.
    painter.add(RectShape::filled(rect, radius, tint(16, 17, 24, 190)));

    // Luz entrando pela quina de cima. Concentrada: um brilho espalhado por
    // metade do painel lê como "gradiente", não como reflexo.
    let inner = rect.shrink(0.5);
    vertical_gradient(painter, inner, radius, 0.42, |t| {
        let fade = (1.0 - t).powf(2.0);
        white((78.0 * fade) as u8)
    });

    // Devolução fraca de luz na base, como a espessura do vidro acendendo.
    vertical_gradient_from_bottom(painter, inner, radius, 0.34, |t| {
        let fade = (1.0 - t).powf(2.2);
        tint(140, 176, 255, (34.0 * fade) as u8)
    });

    // Duas bordas a 2 pt de distância dão a impressão de espessura: a de fora é
    // a face frontal do vidro, a de dentro é a luz atravessando até o fundo.
    inner_edge(painter, rect.shrink(2.0), (radius - 2.0).max(1.0));
    specular_edge(painter, rect, radius);
}

/// Linha interna, visível só perto do topo.
fn inner_edge(painter: &Painter, rect: Rect, radius: f32) {
    painter.add(Shape::Path(PathShape {
        points: rounded_outline(rect, radius, 6),
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new_uv(1.0, |bounds, pos| {
            let t = ((pos.y - bounds.top()) / bounds.height().max(1.0)).clamp(0.0, 1.0);
            white((62.0 * (1.0 - t).powf(3.0)) as u8)
        }),
    }));
}

/// Borda que acende em cima e some embaixo — a pista mais forte de "vidro".
pub fn specular_edge(painter: &Painter, rect: Rect, radius: f32) {
    let points = rounded_outline(rect, radius, 7);
    painter.add(Shape::Path(PathShape {
        points,
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new_uv(1.2, |bounds, pos| {
            let t = ((pos.y - bounds.top()) / bounds.height().max(1.0)).clamp(0.0, 1.0);
            let alpha = 165.0 * (1.0 - t).powf(1.5) + 34.0 * t.powf(2.2);
            white(alpha as u8)
        }),
    }));
}

/// Halo desfocado, para o que estiver "vivo" na tela.
pub fn glow(painter: &Painter, rect: Rect, radius: f32, color: Color32, blur: f32) {
    painter.add(RectShape::filled(rect, radius, color).with_blur_width(blur));
}

/// Halo redondo em volta de um ponto.
pub fn glow_dot(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    glow(
        painter,
        Rect::from_center_size(center, Vec2::splat(radius * 2.0)),
        radius,
        color,
        radius * 1.5,
    );
}

/// Pílula translúcida com brilho no topo — barras do medidor, marcadores.
pub fn pill(painter: &Painter, rect: Rect, color: Color32) {
    let radius = (rect.width().min(rect.height()) / 2.0).max(1.0);
    painter.add(RectShape::filled(rect, radius, color));
    if rect.height() > 6.0 {
        vertical_gradient(painter, rect.shrink(0.5), radius, 0.5, |t| {
            white((70.0 * (1.0 - t).powf(1.5)) as u8)
        });
    }
}

// ------------------------------------------------------------------ desenho

/// Gradiente vertical exato dentro de um retângulo arredondado.
///
/// Faixas horizontais com cor por vértice: a silhueta acompanha o arredondamento
/// dos cantos, e a interpolação é a mesma em toda a largura (um leque a partir do
/// centro, alternativa mais óbvia, borraria a cor do meio para os lados).
///
/// `span` é a fração da altura em que o gradiente se esgota; `color_at` recebe
/// 0.0 no topo dessa faixa e 1.0 no fim dela.
pub fn vertical_gradient(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    span: f32,
    color_at: impl Fn(f32) -> Color32,
) {
    let height = (rect.height() * span).max(1.0);
    strips(painter, rect, radius, rect.top(), height, color_at);
}

/// O mesmo, subindo a partir da base.
pub fn vertical_gradient_from_bottom(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    span: f32,
    color_at: impl Fn(f32) -> Color32,
) {
    let height = (rect.height() * span).max(1.0);
    strips(painter, rect, radius, rect.bottom(), -height, color_at);
}

fn strips(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    start_y: f32,
    height: f32,
    color_at: impl Fn(f32) -> Color32,
) {
    const STEPS: usize = 40;

    let mut mesh = Mesh::default();
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let y = start_y + t * height;
        let inset = corner_inset(rect, radius, y);
        let color = color_at(t);
        mesh.colored_vertex(Pos2::new(rect.left() + inset, y), color);
        mesh.colored_vertex(Pos2::new(rect.right() - inset, y), color);
    }
    for i in 0..STEPS as u32 {
        let base = i * 2;
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 3, base + 2);
    }
    painter.add(Shape::mesh(mesh));
}

/// Quanto a borda arredondada avança para dentro na altura `y`.
fn corner_inset(rect: Rect, radius: f32, y: f32) -> f32 {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    if r <= 0.0 {
        return 0.0;
    }
    let dy = if y < rect.top() + r {
        (rect.top() + r) - y
    } else if y > rect.bottom() - r {
        y - (rect.bottom() - r)
    } else {
        return 0.0;
    };
    let dy = dy.clamp(0.0, r);
    r - (r * r - dy * dy).sqrt()
}

/// Contorno de um retângulo arredondado, em sentido horário a partir da
/// esquerda do topo.
pub fn rounded_outline(rect: Rect, radius: f32, per_corner: usize) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0f32),
        (Pos2::new(rect.right() - r, rect.top() + r), 270.0),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
    ];

    let mut points = Vec::with_capacity(per_corner * 4 + 4);
    for (center, start) in corners {
        for i in 0..=per_corner {
            let angle = (start + 90.0 * i as f32 / per_corner as f32).to_radians();
            points.push(Pos2::new(
                center.x + r * angle.cos(),
                center.y + r * angle.sin(),
            ));
        }
    }
    points
}

/// Mistura duas cores já pré-multiplicadas.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_premultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contorno_fecha_o_retangulo() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 60.0));
        let points = rounded_outline(rect, 12.0, 6);
        assert_eq!(points.len(), 28);
        for p in &points {
            assert!(rect.expand(0.01).contains(*p), "ponto fora: {p:?}");
        }
    }

    #[test]
    fn recuo_zero_no_meio_e_maximo_na_ponta() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 60.0));
        assert_eq!(corner_inset(rect, 12.0, 30.0), 0.0);
        // No topo exato, o recuo é o raio inteiro.
        assert!((corner_inset(rect, 12.0, 0.0) - 12.0).abs() < 0.01);
    }
}
