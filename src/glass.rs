//! Vocabulário visual "vidro líquido" (Liquid Glass).
//!
//! Aqui ficam a **geometria** das peças (a silhueta de squircle, as normais, os
//! gradientes) e a **receita** de cada uma — o quanto ela é grossa, o quanto
//! acende, que tinta tem. Quem desenha é `glass_gpu.rs`, um shader que faz a
//! óptica pixel a pixel.
//!
//! Os números do padrão são os da extensão de GNOME `ryohsuke1231/liquid-glass`
//! (ver `config::Appearance`): vidro claro e quase transparente, refração forte
//! e borda acesa em volta inteira. O que dá forma à peça ali não é a tinta — é
//! o fundo refratado por ela. Por isso, quando a GPU não está disponível, o
//! desenho vetorial de reserva volta ao vidro escuro e denso de antes do
//! shader: sem óptica, a tinta clara sozinha não desenharia nada. Ele empilha
//! as pistas em camadas:
//!
//!   * silhueta de **squircle**: os cantos são superelipses, não arcos de
//!     círculo, então a curvatura entra cedo e se estica — a forma da Apple;
//!   * tinta translúcida, deixando o fundo aparecer só como um tom;
//!   * gradiente de brilho descendo do topo, como luz entrando pela quina;
//!   * **faixa de refração**: junto da borda o vidro funciona como lente e
//!     concentra luz, então há um degradê claro que morre para dentro;
//!   * **borda especular com direção**: a linha da beirada acende onde a normal
//!     encara a luz (topo e esquerda) e escurece do lado oposto, onde sobra
//!     apenas o retorno frio da luz que atravessou a peça;
//!   * halos desfocados atrás dos elementos vivos, como luz refratada.
//!
//! Todas as funções devolvem `Shape` em vez de desenhar direto: assim a mesma
//! peça serve tanto para o painel de fundo quanto para ser inserida atrás de um
//! conteúdo já disposto (`Painter::set`), que é como os cartões funcionam.

use crate::glass_gpu;
use egui::epaint::{PathShape, PathStroke, RectShape, Shadow};
use egui::{Color32, Mesh, Pos2, Rect, Shape, Vec2};

/// Espaço reservado em volta do painel para a sombra projetada. Acompanha o
/// raio da sombra configurado, senão um raio grande sairia cortado no limite da
/// janela.
pub fn shadow_pad() -> f32 {
    glass_gpu::aparencia().shadow_radius.max(8.0)
}
/// Raio dos cartões que agrupam controles dentro do painel.
pub const RAIO_CARTAO: f32 = 18.0;

/// Expoente da superelipse dos cantos. 2,0 devolveria o arco de círculo; quanto
/// maior, mais a curva se estica ao longo das retas — 4,2 fica na mesma família
/// do "squircle" que a Apple usa em ícones e cápsulas.
const SQUIRCLE: f32 = 4.2;

/// Direção em que a luz viaja: de cima, ligeiramente pela esquerda.
const LUZ: Vec2 = Vec2::new(0.34, 0.94);

/// Branco com alfa (já pré-multiplicado, que é o formato interno do egui).
pub const fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
}

/// Cor com alfa, informada em componentes normais (não pré-multiplicados).
pub fn tint(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Receita de uma peça de vidro. Muda a espessura aparente e o quanto ela
/// "acende"; a geometria é sempre a mesma.
#[derive(Clone, Copy)]
pub struct Vidro {
    /// Tinta do corpo, translúcida.
    pub corpo: Color32,
    /// Intensidade do brilho que desce do topo.
    pub brilho: f32,
    /// Intensidade da borda especular.
    pub borda: f32,
    /// Largura, em pontos, da faixa de refração junto da borda.
    pub lente: f32,
    /// Intensidade do retorno frio de luz pela base.
    pub base: f32,
    /// Ponto onde a luz se junta — na prática, o cursor. A beirada mais próxima
    /// dele acende, como acontece ao passar a mão sobre vidro polido.
    pub foco: Option<Pos2>,
}

impl Vidro {
    /// O painel da janela. É ele que usa a tinta configurada: no padrão, branco
    /// a 12% — o vidro claro e quase transparente da extensão, em que quem
    /// desenha a peça é o fundo refratado, não a tinta.
    pub fn painel() -> Self {
        let ap = glass_gpu::aparencia();
        let [r, g, b] = ap.tint;
        Self {
            corpo: tint(r, g, b, (ap.tint_strength * 255.0).round() as u8),
            brilho: 1.0,
            borda: 1.0,
            lente: 11.0,
            base: 1.0,
            foco: None,
        }
    }

    /// Cartão interno: uma lâmina fina apoiada sobre o painel. Um pouco mais
    /// densa que ele, senão some — o que está por baixo já é vidro.
    pub fn cartao() -> Self {
        Self {
            corpo: white(28),
            brilho: 0.5,
            borda: 0.55,
            lente: 5.0,
            base: 0.35,
            foco: None,
        }
    }

    /// Controle (botão, interruptor, cápsula). `realce` vai de 0 (em repouso) a
    /// 1 (pressionado), passando por ~0,45 sob o cursor.
    pub fn controle(realce: f32) -> Self {
        Self {
            corpo: white((38.0 + 52.0 * realce) as u8),
            brilho: 0.62 + 0.35 * realce,
            borda: 0.8 + 0.5 * realce,
            lente: 3.5,
            base: 0.3,
            foco: None,
        }
    }

    /// Mesma peça, com a tinta trocada — usado por botões coloridos.
    pub fn com_corpo(mut self, corpo: Color32) -> Self {
        self.corpo = corpo;
        self
    }

    /// Acende a beirada mais próxima de um ponto (o cursor).
    pub fn com_foco(mut self, foco: Option<Pos2>) -> Self {
        self.foco = foco;
        self
    }
}

/// Painel da janela: sombras projetadas e o vidro por cima. `foco` é onde está
/// o cursor, se ele estiver sobre a janela.
pub fn painel(rect: Rect, radius: f32, foco: Option<Pos2>) -> Shape {
    let a = glass_gpu::aparencia();
    let vidro = Vidro::painel().com_foco(foco);
    let receita = glass_gpu::Peca {
        // O papel de parede não é o que está atrás de verdade — é a cor da área
        // de trabalho naquele ponto —, então entra com alfa parcial: o que
        // estiver mesmo atrás continua aparecendo pelo canal alfa da janela, e o
        // vidro ganha uma cor de ambiente para refratar.
        parede: if a.wallpaper {
            a.wallpaper_opacity
        } else {
            0.0
        },
        sombra: [a.shadow_radius, a.shadow_intensity],
        ..receita(rect, radius, &vidro)
    };
    if let Some(shape) = glass_gpu::shape(receita) {
        return shape;
    }

    Shape::Vec(vec![
        // Duas sombras: uma ampla e difusa (a luz do ambiente contornando a
        // peça) e uma curta e mais forte logo abaixo (o contato com o fundo).
        Shape::Rect(
            Shadow {
                offset: [0, 18],
                blur: 44,
                spread: 0,
                color: Color32::from_black_alpha(96),
            }
            .as_shape(rect, radius),
        ),
        Shape::Rect(
            Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(86),
            }
            .as_shape(rect, radius),
        ),
        // Sem GPU não há fundo refratado, e é ele que dá forma ao painel no
        // padrão da extensão — a tinta sozinha, a 12% de branco, deixaria a
        // janela quase invisível. Aqui o corpo volta a ser o vidro escuro e
        // denso de antes do shader: não é o padrão, é o que sobra sem óptica.
        peca_vetorial(rect, radius, vidro.com_corpo(tint(15, 16, 23, 182))),
    ])
}

/// Uma peça de vidro. Sai pela GPU quando ela estiver disponível.
pub fn peca(rect: Rect, radius: f32, v: Vidro) -> Shape {
    match glass_gpu::shape(receita(rect, radius, &v)) {
        Some(shape) => shape,
        None => peca_vetorial(rect, radius, v),
    }
}

/// Traduz a receita de `Vidro` para os parâmetros que o shader entende.
///
/// A ideia é a mesma dos dois lados; o que muda é que o shader parte de uma
/// superfície com altura de verdade, então "espessura" e "bisel" (a faixa em
/// que a superfície sobe da borda) substituem as camadas empilhadas à mão.
fn receita(rect: Rect, radius: f32, v: &Vidro) -> glass_gpu::Peca {
    let ap = glass_gpu::aparencia();
    let raio = radius_util(rect, radius);
    let lado = (rect.width().min(rect.height()) / 2.0).max(0.5);
    // Na extensão a altura da superfície é normalizada pelo próprio raio do
    // canto — a faixa em que o vidro sobe da borda *é* o raio. Aqui vale o
    // mesmo, com o teto do meio-lado para peças achatadas não virarem cúpula.
    let bisel = raio.clamp(2.5, lado);
    // Toda medida óptica em pixels (deslocamento, altura, desfoque) foi afinada
    // para uma peça do tamanho do painel. Numa peça menor ela entra reduzida na
    // mesma proporção, senão um botão de 24 pt receberia o deslocamento de 78
    // pt do painel inteiro e o fundo dele viraria um borrão esticado.
    let escala = (raio / ap.corner_radius.max(1.0)).clamp(0.12, 1.0);
    let [r, g, b, a] = v.corpo.to_srgba_unmultiplied();

    glass_gpu::Peca {
        rect,
        raio,
        n: expoente(rect, raio),
        bisel,
        perfil: ap.profile_n,
        espessura: ap.max_z * escala,
        desloc: ap.displacement * escala,
        desfoque: ap.blur_radius,
        ior: ap.ior,
        cromatica: ap.chroma,
        tinta: [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ],
        ao: ap.ao,
        ao_raio: (ap.ao_radius * escala).max(1.0),
        // O ganho de cor da borda é um multiplicador à parte na extensão; aqui
        // ele já entra junto com a intensidade, que é como o shader usa.
        rim: ap.rim_intensity * ap.rim_color_intensity * v.borda,
        rim_larg: (ap.rim_width * escala).max(1.0),
        rim_pot: ap.rim_power,
        rim_dir_pot: ap.rim_directional_power,
        espec: ap.specular * v.brilho,
        espec_pot: ap.shininess,
        sheen: ap.sheen * v.brilho,
        foco: v.foco,
        foco_alcance: 95.0,
        parede: 0.0,
        sombra: [0.0, 0.0],
        opacidade: glass_gpu::opacidade(),
    }
}

// ------------------------------------------------------------------- animação

/// Mola amortecida, de 0 (fechado) a 1 (assentado), com uma ultrapassagem curta
/// no caminho — é o que separa "apareceu" de "chegou".
///
/// `x` vai de 0 a 1 ao longo da animação e `salto` de 0 (sem ultrapassar, uma
/// desaceleração exponencial limpa) a 1 (a mola completa). A curva é
/// `1 − e^(−5x)·cos(ωx)`: vale exatamente 0 no início e, com o decaimento
/// escolhido, já chegou em 1 no fim.
pub fn mola(x: f32, salto: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let omega = 4.6 * salto.clamp(0.0, 1.0);
    1.0 - (-5.0 * x).exp() * (omega * x).cos()
}

/// Uma peça de vidro completa em vetores, na ordem em que a luz a constrói.
/// É a reserva de quando não há GPU.
fn peca_vetorial(rect: Rect, radius: f32, v: Vidro) -> Shape {
    let radius = radius_util(rect, radius);
    let mut camadas: Vec<Shape> = Vec::with_capacity(12);

    camadas.push(preencher(rect, radius, v.corpo));

    // Luz entrando pela quina de cima. A faixa tem altura quase fixa, em vez de
    // uma fração do corpo: é luz de beirada, e numa peça alta a fração viraria
    // um degradê de cima a baixo — parece papel pintado, não vidro.
    let interno = rect.shrink(0.5);
    let raio_interno = (radius - 0.5).max(0.0);
    let altura = rect.height().max(1.0);
    if v.brilho > 0.0 {
        camadas.push(gradiente_do_topo(
            interno,
            raio_interno,
            (74.0 / altura).min(0.46),
            {
                let forca = 76.0 * v.brilho;
                move |t| white((forca * (1.0 - t).powf(2.1)) as u8)
            },
        ));
    }

    // Devolução fraca de luz na base, como a espessura do vidro acendendo.
    if v.base > 0.0 {
        camadas.push(gradiente_da_base(
            interno,
            raio_interno,
            (58.0 / altura).min(0.36),
            {
                let forca = 38.0 * v.base;
                move |t| tint(140, 176, 255, (forca * (1.0 - t).powf(2.2)) as u8)
            },
        ));
    }

    if v.lente > 0.0 {
        camadas.extend(refracao(rect, radius, v.lente));
    }
    if v.borda > 0.0 {
        camadas.push(borda(rect, radius, v.borda, v.foco));
        // Segunda linha, 2 pt para dentro: a face de trás do vidro vista
        // através dele. É o que dá a impressão de espessura.
        let dentro = rect.shrink(2.2);
        if dentro.width() > 2.0 && dentro.height() > 2.0 {
            camadas.push(borda_interna(
                dentro,
                (radius - 2.2).max(0.0),
                0.5 * v.borda,
            ));
        }
    }

    Shape::Vec(camadas)
}

/// Silhueta preenchida, sem nenhum efeito.
pub fn preencher(rect: Rect, radius: f32, cor: Color32) -> Shape {
    Shape::convex_polygon(contorno(rect, radius), cor, PathStroke::NONE)
}

/// Borda especular: acende onde a superfície encara a luz e escurece do lado
/// oposto, onde só resta o retorno frio de quem atravessou o vidro. Perto do
/// `foco` — o cursor — a beirada ganha um brilho extra que o acompanha.
pub fn borda(rect: Rect, radius: f32, forca: f32, foco: Option<Pos2>) -> Shape {
    /// Alcance do brilho que segue o cursor, em pontos.
    const ALCANCE: f32 = 95.0;

    let radius = radius_util(rect, radius);
    Shape::Path(PathShape {
        points: contorno(rect, radius),
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new_uv(1.3, move |_bounds, p| {
            let (direta, retorno) = incidencia(normal(rect, radius, p));
            let perto = foco.map_or(0.0, |f| {
                let d = p.distance(f) / ALCANCE;
                (-(d * d)).exp()
            });
            let alfa = forca
                * (198.0 * direta.powf(1.35) + 52.0 * retorno.powf(1.7) + 14.0 + 120.0 * perto);
            if direta >= retorno || perto > 0.3 {
                tint(255, 253, 248, alfa.min(255.0) as u8)
            } else {
                tint(166, 196, 255, alfa.min(255.0) as u8)
            }
        }),
    })
}

/// Linha fina no interior, visível só onde a luz bate — a face de trás.
fn borda_interna(rect: Rect, radius: f32, forca: f32) -> Shape {
    Shape::Path(PathShape {
        points: contorno(rect, radius),
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new_uv(1.0, move |_bounds, p| {
            let (direta, _) = incidencia(normal(rect, radius, p));
            white((forca * 96.0 * direta.powf(2.6)) as u8)
        }),
    })
}

/// Faixa de refração: perto da borda o vidro concentra luz. São várias linhas
/// concêntricas, cada uma mais fraca, porque o degradê precisa acompanhar a
/// silhueta — um retângulo desfocado escaparia dos cantos.
fn refracao(rect: Rect, radius: f32, largura: f32) -> Vec<Shape> {
    const CAMADAS: usize = 4;
    let passo = largura / CAMADAS as f32;

    let mut linhas = Vec::with_capacity(CAMADAS);
    for k in 0..CAMADAS {
        let recuo = (k as f32 + 0.5) * passo;
        let faixa = rect.shrink(recuo);
        if faixa.width() <= passo || faixa.height() <= passo {
            break;
        }
        let raio = radius_util(faixa, (radius - recuo).max(0.0));
        // Quadrática: quase toda a luz fica no primeiro terço da faixa.
        let queda = (1.0 - recuo / largura).powf(2.0);
        linhas.push(Shape::Path(PathShape {
            points: contorno(faixa, raio),
            closed: true,
            fill: Color32::TRANSPARENT,
            stroke: PathStroke::new_uv(passo * 1.2, move |_bounds, p| {
                let (direta, retorno) = incidencia(normal(faixa, raio, p));
                white((queda * (30.0 * direta + 17.0 * retorno + 7.0)) as u8)
            }),
        }));
    }
    linhas
}

/// Halo desfocado, para o que estiver "vivo" na tela.
pub fn glow(rect: Rect, radius: f32, color: Color32, blur: f32) -> Shape {
    Shape::Rect(RectShape::filled(rect, radius, color).with_blur_width(blur))
}

/// Halo redondo em volta de um ponto.
pub fn glow_dot(center: Pos2, radius: f32, color: Color32) -> Shape {
    glow(
        Rect::from_center_size(center, Vec2::splat(radius * 2.0)),
        radius,
        color,
        radius * 1.5,
    )
}

/// Cápsula translúcida com brilho no topo — barras do medidor, marcadores.
pub fn pastilha(rect: Rect, color: Color32) -> Shape {
    let radius = (rect.width().min(rect.height()) / 2.0).max(0.5);
    let mut camadas = vec![preencher(rect, radius, color)];
    if rect.height() > 6.0 {
        camadas.push(gradiente_do_topo(rect.shrink(0.5), radius, 0.5, |t| {
            white((76.0 * (1.0 - t).powf(1.5)) as u8)
        }));
    }
    Shape::Vec(camadas)
}

// ------------------------------------------------------------------ geometria

/// Contorno da peça, em sentido horário a partir da esquerda do topo.
///
/// Cada canto é um quarto de superelipse `|x/r|^n + |y/r|^n = 1`. Com `n = 2`
/// isso é o arco de círculo de sempre; o expoente maior é o que transforma o
/// canto em squircle. Cápsulas e círculos (raio no limite) voltam ao arco: ali
/// o squircle deixaria a ponta visivelmente achatada.
pub fn contorno(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius_util(rect, radius);
    let k = 2.0 / expoente(rect, r);
    let lados = ((r * 0.9) as usize).clamp(6, 22);

    let (l, t, d, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let mut pontos = Vec::with_capacity(lados * 4 + 4);
    // (centro do canto, sinal em x, sinal em y, trocar cosseno e seno)
    let cantos = [
        (Pos2::new(l + r, t + r), -1.0, -1.0, false),
        (Pos2::new(d - r, t + r), 1.0, -1.0, true),
        (Pos2::new(d - r, b - r), 1.0, 1.0, false),
        (Pos2::new(l + r, b - r), -1.0, 1.0, true),
    ];
    for (centro, sx, sy, trocar) in cantos {
        for i in 0..=lados {
            let a = std::f32::consts::FRAC_PI_2 * i as f32 / lados as f32;
            // O `max` protege o `powf`: no fim do quarto de volta o cosseno sai
            // negativo por erro de arredondamento, e `(-1e-8)^0,48` é NaN.
            let (mut u, mut w) = (a.cos().max(0.0).powf(k), a.sin().max(0.0).powf(k));
            if trocar {
                std::mem::swap(&mut u, &mut w);
            }
            pontos.push(Pos2::new(centro.x + sx * r * u, centro.y + sy * r * w));
        }
    }
    pontos
}

/// Normal (apontando para fora) da silhueta no ponto `p`.
///
/// É o gradiente da mesma superelipse do contorno, o que faz a normal girar
/// suavemente ao longo do canto — nas retas ela é exatamente perpendicular.
pub fn normal(rect: Rect, radius: f32, p: Pos2) -> Vec2 {
    let r = radius_util(rect, radius);
    if r <= 0.0 {
        return Vec2::new(0.0, -1.0);
    }
    // Distância até a região reta: zero em cima das retas, ±r nas pontas.
    let dx = p.x - p.x.clamp(rect.left() + r, rect.right() - r);
    let dy = p.y - p.y.clamp(rect.top() + r, rect.bottom() - r);

    let n = expoente(rect, r) - 1.0;
    let g = Vec2::new(
        dx.signum() * (dx.abs() / r).powf(n),
        dy.signum() * (dy.abs() / r).powf(n),
    );
    if g.length() < 1e-4 {
        Vec2::new(0.0, -1.0)
    } else {
        g.normalized()
    }
}

/// Quanto a luz bate de frente e quanto volta pelo lado oposto.
fn incidencia(n: Vec2) -> (f32, f32) {
    let d = n.x * LUZ.x + n.y * LUZ.y;
    ((-d).max(0.0), d.max(0.0))
}

fn expoente(rect: Rect, r: f32) -> f32 {
    let limite = rect.width().min(rect.height()) / 2.0;
    if r >= limite - 0.25 { 2.0 } else { SQUIRCLE }
}

fn radius_util(rect: Rect, radius: f32) -> f32 {
    radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0)
}

// ------------------------------------------------------------------ gradientes

/// Gradiente vertical exato dentro da silhueta, descendo do topo.
///
/// Faixas horizontais com cor por vértice: a silhueta acompanha o arredondamento
/// dos cantos, e a interpolação é a mesma em toda a largura (um leque a partir do
/// centro, alternativa mais óbvia, borraria a cor do meio para os lados).
///
/// `span` é a fração da altura em que o gradiente se esgota; `color_at` recebe
/// 0.0 no topo dessa faixa e 1.0 no fim dela.
pub fn gradiente_do_topo(
    rect: Rect,
    radius: f32,
    span: f32,
    color_at: impl Fn(f32) -> Color32,
) -> Shape {
    let altura = (rect.height() * span).max(1.0);
    faixas(rect, radius, rect.top(), altura, color_at)
}

/// O mesmo, subindo a partir da base.
pub fn gradiente_da_base(
    rect: Rect,
    radius: f32,
    span: f32,
    color_at: impl Fn(f32) -> Color32,
) -> Shape {
    let altura = (rect.height() * span).max(1.0);
    faixas(rect, radius, rect.bottom(), -altura, color_at)
}

fn faixas(
    rect: Rect,
    radius: f32,
    inicio_y: f32,
    altura: f32,
    color_at: impl Fn(f32) -> Color32,
) -> Shape {
    const PASSOS: usize = 40;
    const CANTO: usize = 20;

    // Alturas onde a malha é cortada, em fração da faixa. Uma grade uniforme não
    // basta: na altura dos cantos a silhueta some depressa para dentro, e a reta
    // entre duas linhas afastadas passaria por fora dela — o canto ficaria
    // faltando um pedaço. Por isso as pontas ganham linhas próprias.
    let banda = (radius_util(rect, radius) / altura.abs().max(1.0)).min(1.0);
    let mut cortes: Vec<f32> = (0..=PASSOS).map(|i| i as f32 / PASSOS as f32).collect();
    for i in 1..CANTO {
        let passo = banda * i as f32 / CANTO as f32;
        cortes.push(passo);
        cortes.push(1.0 - passo);
    }
    cortes.sort_by(|a, b| a.total_cmp(b));
    cortes.dedup_by(|a, b| (*a - *b).abs() < 1e-4);

    let mut mesh = Mesh::default();
    for &t in &cortes {
        let y = inicio_y + t * altura;
        let recuo = recuo_do_canto(rect, radius, y);
        let color = color_at(t);
        mesh.colored_vertex(Pos2::new(rect.left() + recuo, y), color);
        mesh.colored_vertex(Pos2::new(rect.right() - recuo, y), color);
    }
    for i in 0..cortes.len() as u32 - 1 {
        let base = i * 2;
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 3, base + 2);
    }
    Shape::mesh(mesh)
}

/// Quanto a silhueta avança para dentro na altura `y` — a mesma superelipse do
/// contorno, resolvida para x.
fn recuo_do_canto(rect: Rect, radius: f32, y: f32) -> f32 {
    let r = radius_util(rect, radius);
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
    let u = (dy / r).clamp(0.0, 1.0);
    let n = expoente(rect, r);
    r - r * (1.0 - u.powf(n)).max(0.0).powf(1.0 / n)
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

    fn ret() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 60.0))
    }

    #[test]
    fn contorno_fecha_o_retangulo() {
        let pontos = contorno(ret(), 12.0);
        assert!(pontos.len() >= 28);
        for p in &pontos {
            assert!(ret().expand(0.01).contains(*p), "ponto fora: {p:?}");
        }
    }

    #[test]
    fn squircle_enche_mais_o_canto_que_o_circulo() {
        // A 45° o squircle fica mais perto da quina do que o arco de círculo:
        // é exatamente isso que suaviza a transição para a reta.
        let r = 20.0;
        let quina = Pos2::new(r, r);
        let mais_longe = |exp: f32| {
            let k = 2.0 / exp;
            let a = std::f32::consts::FRAC_PI_4;
            Pos2::new(r - r * a.cos().powf(k), r - r * a.sin().powf(k)).distance(quina)
        };
        assert!(mais_longe(SQUIRCLE) > mais_longe(2.0));
    }

    #[test]
    fn recuo_zero_no_meio_e_maximo_na_ponta() {
        assert_eq!(recuo_do_canto(ret(), 12.0, 30.0), 0.0);
        // No topo exato, o recuo é o raio inteiro.
        assert!((recuo_do_canto(ret(), 12.0, 0.0) - 12.0).abs() < 0.01);
    }

    #[test]
    fn normais_apontam_para_fora() {
        let rect = ret();
        let n_topo = normal(rect, 12.0, Pos2::new(50.0, 0.0));
        assert!(n_topo.y < -0.99, "{n_topo:?}");
        let n_esq = normal(rect, 12.0, Pos2::new(0.0, 30.0));
        assert!(n_esq.x < -0.99, "{n_esq:?}");
        // No canto, a normal aponta na diagonal.
        let n_canto = normal(rect, 12.0, Pos2::new(3.5, 3.5));
        assert!(n_canto.x < -0.2 && n_canto.y < -0.2, "{n_canto:?}");
    }

    #[test]
    fn a_receita_devolve_a_tinta_sem_premultiplicar() {
        // O Color32 guarda a cor já multiplicada pelo alfa; o shader mistura em
        // alfa reto, então a tinta precisa voltar à cor original.
        let v = Vidro::painel();
        let r = receita(ret(), 30.0, &v);
        let padrao = crate::config::Appearance::PADRAO;
        let [_, _, _, a] = r.tinta;
        assert!((a - padrao.tint_strength).abs() < 0.01, "alfa {a}");
        // A cor sai como foi configurada, e não já multiplicada pelo alfa.
        let azul = padrao.tint[2] as f32 / 255.0;
        assert!((r.tinta[2] - azul).abs() < 0.01, "{:?}", r.tinta);
    }

    #[test]
    fn o_bisel_nunca_passa_da_metade_da_peca() {
        // Numa cápsula baixa, a faixa de relevo (que é o raio do canto) é maior
        // que a peça inteira; deixá-la passar viraria uma normal para fora.
        let capsula = Rect::from_min_size(Pos2::ZERO, Vec2::new(120.0, 8.0));
        let r = receita(capsula, 4.0, &Vidro::painel());
        assert!(r.bisel <= 4.0 + 1e-3, "bisel {}", r.bisel);
        // E o canto totalmente redondo volta ao arco de círculo.
        assert_eq!(r.n, 2.0);
    }

    #[test]
    fn a_mola_sai_do_zero_e_assenta_em_um() {
        for salto in [0.0, 0.5, 1.0] {
            assert_eq!(mola(0.0, salto), 0.0);
            assert_eq!(mola(1.0, salto), 1.0);
            // Sem degrau na emenda com o fim da animação.
            assert!((mola(0.999, salto) - 1.0).abs() < 0.01, "salto {salto}");
        }
    }

    #[test]
    fn a_mola_ultrapassa_o_alvo_so_quando_pedida() {
        let maximo = |salto: f32| {
            (0..=200)
                .map(|i| mola(i as f32 / 200.0, salto))
                .fold(f32::MIN, f32::max)
        };
        // Sem salto é uma desaceleração limpa: nunca passa de 1.
        assert!(maximo(0.0) <= 1.0 + 1e-4);
        // Com salto cheio ela passa, mas pouco — nada de gelatina.
        let pico = maximo(1.0);
        assert!(pico > 1.01 && pico < 1.10, "pico {pico}");
    }

    #[test]
    fn a_luz_bate_no_topo_e_volta_pela_base() {
        let (direta, retorno) = incidencia(Vec2::new(0.0, -1.0));
        assert!(direta > 0.9 && retorno == 0.0);
        let (direta, retorno) = incidencia(Vec2::new(0.0, 1.0));
        assert!(direta == 0.0 && retorno > 0.9);
    }
}
