//! O sistema visual do Ditador: cores sólidas, dois temas.
//!
//! Nada de transparência, refração ou desfoque — a janela é uma superfície
//! opaca, com uma borda de um pixel e uma sombra por baixo. Cada tela custa um
//! punhado de retângulos arredondados, e é só isso que a GPU precisa desenhar.
//!
//! A referência é a interface do ChatGPT: o preto é preto, o branco é branco, e
//! a hierarquia se faz com três tons de cinza e uma borda, não com camadas
//! translúcidas. Daí também vir o botão principal em cor cheia e invertida —
//! preto sobre claro, branco sobre escuro —, que é o único elemento de contraste
//! máximo em qualquer tela.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

/// Raio dos cantos da janela.
pub const RAIO_JANELA: u8 = 16;
/// Raio dos cartões que agrupam controles.
pub const RAIO_CARTAO: u8 = 12;
/// Raio dos campos, listas e caixas de texto.
pub const RAIO_CONTROLE: u8 = 10;
/// Folga reservada em volta da janela para a sombra. A janela em si é
/// transparente; o que se vê é o retângulo desenhado dentro desta margem.
///
/// **Zero no Windows, e não por economia.** Lá a janela não é transparente: o
/// glutin não entrega canal alfa por pixel numa janela OpenGL, então a folga não
/// some — ela aparece como uma moldura opaca de 22 px em volta do cartão, com o
/// canto arredondado e a borda que o Windows 11 desenha por fora. O efeito é
/// exatamente o de "uma caixa atrás da janela", que foi como quem viu primeiro o
/// descreveu.
///
/// Sem a folga, a janela **é** o cartão. A sombra e o canto arredondado passam a
/// ser do sistema, que já os desenha em toda janela de nível superior — e é o que
/// faz o Ditador parecer nativo lá em vez de trazer a sombra do GNOME junto.
#[cfg(target_os = "windows")]
pub const FOLGA_SOMBRA: f32 = 0.0;
#[cfg(not(target_os = "windows"))]
pub const FOLGA_SOMBRA: f32 = 22.0;

/// As cores de um tema. Poucas de propósito: fundo, duas superfícies, uma
/// borda, dois níveis de texto e as três cores de estado.
pub struct Paleta {
    /// Fundo da janela.
    pub fundo: Color32,
    /// Cartões, campos e botões comuns.
    pub superficie: Color32,
    /// A mesma superfície sob o cursor.
    pub superficie_forte: Color32,
    /// Linha de um pixel que separa superfícies de mesmo peso.
    pub borda: Color32,
    /// A mesma linha quando precisa ser vista (campo em foco, botão pressionado).
    pub borda_forte: Color32,
    pub texto: Color32,
    /// Rótulos de apoio, notas, unidades.
    pub texto_fraco: Color32,
    /// Botão principal: cor cheia e invertida em relação ao fundo.
    pub primario: Color32,
    pub sobre_primario: Color32,
    /// Gravando.
    pub gravando: Color32,
    /// Concluído.
    pub ok: Color32,
    /// Erro.
    pub erro: Color32,
    /// Sombra projetada da janela.
    pub sombra: Color32,
}

pub const ESCURA: Paleta = Paleta {
    fundo: Color32::from_rgb(0x21, 0x21, 0x21),
    superficie: Color32::from_rgb(0x2F, 0x2F, 0x2F),
    superficie_forte: Color32::from_rgb(0x3C, 0x3C, 0x3C),
    borda: Color32::from_rgb(0x3A, 0x3A, 0x3A),
    borda_forte: Color32::from_rgb(0x56, 0x56, 0x56),
    texto: Color32::from_rgb(0xEC, 0xEC, 0xEC),
    texto_fraco: Color32::from_rgb(0x9B, 0x9B, 0x9B),
    primario: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    sobre_primario: Color32::from_rgb(0x0D, 0x0D, 0x0D),
    gravando: Color32::from_rgb(0xF8, 0x71, 0x71),
    ok: Color32::from_rgb(0x2F, 0xBB, 0x81),
    erro: Color32::from_rgb(0xF8, 0x71, 0x71),
    sombra: Color32::from_black_alpha(150),
};

pub const CLARA: Paleta = Paleta {
    fundo: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    superficie: Color32::from_rgb(0xF7, 0xF7, 0xF8),
    superficie_forte: Color32::from_rgb(0xEC, 0xEC, 0xEE),
    borda: Color32::from_rgb(0xE3, 0xE3, 0xE6),
    borda_forte: Color32::from_rgb(0xC4, 0xC4, 0xCA),
    texto: Color32::from_rgb(0x0D, 0x0D, 0x0D),
    texto_fraco: Color32::from_rgb(0x6E, 0x6E, 0x80),
    primario: Color32::from_rgb(0x0D, 0x0D, 0x0D),
    sobre_primario: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    gravando: Color32::from_rgb(0xD9, 0x37, 0x3D),
    ok: Color32::from_rgb(0x0D, 0x8F, 0x6B),
    erro: Color32::from_rgb(0xD9, 0x37, 0x3D),
    sombra: Color32::from_black_alpha(46),
};

static ESCURO: AtomicBool = AtomicBool::new(true);

pub fn escuro() -> bool {
    ESCURO.load(Ordering::Relaxed)
}

/// A paleta em uso. É lida em todo canto do desenho, então mora aqui em vez de
/// ser passada de mão em mão.
pub fn paleta() -> &'static Paleta {
    if escuro() { &ESCURA } else { &CLARA }
}

/// Troca o tema. Devolve `true` quando ele de fato mudou — aí quem chamou
/// precisa reaplicar o estilo do egui, que guarda as cores por cópia.
pub fn definir_escuro(novo: bool) -> bool {
    ESCURO.swap(novo, Ordering::Relaxed) != novo
}

// ------------------------------------------------------------------ tipografia

/// **Plus Jakarta Sans**, embutida no binário.
///
/// É uma grotesca geométrica do Tokotype, do Google Fonts (SIL OFL 1.1).
/// Escolhida por ter personalidade sem atrapalhar: o `a` e o `g` de andar único,
/// o corte diagonal do `t` e do `l` e a altura de x alta dão cara própria a uma
/// tela que é quase toda rótulo curto, e ela segura bem os acentos do português,
/// que numa fonte de interface costumam ser a primeira coisa a apertar.
///
/// Vai embutida em vez de procurada no sistema porque nenhuma máquina a tem
/// instalada por padrão, e o visual não pode depender disso. As instâncias são
/// estáticas, e não a fonte variável: o rasterizador do egui não interpola
/// eixos, então uma variável renderizaria tudo no peso padrão.
///
/// Três pesos, cada um com um trabalho: **Regular** no texto corrido, **Medium**
/// nos rótulos e botões, **SemiBold** nos títulos.
///
/// A monoespaçada é a **JetBrains Mono**, também do Google Fonts, e aparece só
/// onde largura fixa é a informação: o cronômetro (que senão dança a cada
/// segundo), o valor de um controle deslizante, a tecla do atalho e a versão.
pub fn instalar_fontes(ctx: &egui::Context) {
    const CORPO: &[u8] = include_bytes!("../assets/fontes/PlusJakartaSans-Regular.ttf");
    const MEDIO: &[u8] = include_bytes!("../assets/fontes/PlusJakartaSans-Medium.ttf");
    const FORTE: &[u8] = include_bytes!("../assets/fontes/PlusJakartaSans-SemiBold.ttf");
    const MONO: &[u8] = include_bytes!("../assets/fontes/JetBrainsMono-Medium.ttf");

    let mut fontes = egui::FontDefinitions::default();
    for (nome, bytes) in [
        ("corpo", CORPO),
        ("medio", MEDIO),
        ("forte", FORTE),
        ("mono", MONO),
    ] {
        fontes.font_data.insert(
            nome.to_string(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
    }

    // As embutidas do egui continuam atrás como reserva, para o que a Plus
    // Jakarta Sans não cobrir (emoji, símbolos, alfabetos não latinos).
    fontes
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "corpo".to_string());
    fontes
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "mono".to_string());

    // Os outros pesos herdam essas mesmas reservas.
    let reservas = fontes
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (familia, peso) in [(MEDIO_NOME, "medio"), (FORTE_NOME, "forte")] {
        let mut lista = reservas.clone();
        lista.insert(0, peso.to_string());
        fontes
            .families
            .insert(FontFamily::Name(familia.into()), lista);
    }

    ctx.set_fonts(fontes);
}

const MEDIO_NOME: &str = "medio";
const FORTE_NOME: &str = "forte";

pub fn fonte_media(tamanho: f32) -> FontId {
    FontId::new(tamanho, FontFamily::Name(MEDIO_NOME.into()))
}

pub fn fonte_forte(tamanho: f32) -> FontId {
    FontId::new(tamanho, FontFamily::Name(FORTE_NOME.into()))
}

/// Corpo dos rótulos de controle. É o mesmo do texto corrido: o que separa um
/// do outro é o peso, não o tamanho.
pub const CORPO: f32 = 14.5;

/// Texto de título. Sai com a letra um fio mais junta — a Plus Jakarta Sans é
/// larga, e em corpo grande o espaçamento normal abre demais.
pub fn titulo(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto)
        .size(tamanho)
        .family(FontFamily::Name(FORTE_NOME.into()))
        .extra_letter_spacing(-0.2)
}

/// Rótulo de controle: mesmo corpo do texto, um peso acima. É o que faz a
/// pergunta ("Gravação mínima") se destacar da explicação abaixo dela sem
/// precisar de outro tamanho nem de outra cor.
pub fn rotulo(texto: impl Into<String>) -> RichText {
    medio(texto, CORPO)
}

/// Texto no peso do meio, em qualquer corpo.
pub fn medio(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto)
        .size(tamanho)
        .family(FontFamily::Name(MEDIO_NOME.into()))
}

/// Texto de apoio: pequeno e apagado.
pub fn nota(texto: impl Into<String>) -> RichText {
    RichText::new(texto).size(12.5).color(paleta().texto_fraco)
}

/// Números e teclas: monoespaçado, para largura fixa onde ela é a informação.
pub fn tecnico(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto).size(tamanho).monospace()
}

// ---------------------------------------------------------------------- estilo

/// Escreve a paleta atual no estilo do egui — o que pega as listas suspensas,
/// os controles deslizantes e as caixas de texto, que o egui desenha sozinho.
pub fn estilo(style: &mut egui::Style) {
    let p = paleta();

    // Uma escala curta, e o peso fazendo o trabalho que o tamanho faria: título
    // em SemiBold, rótulo em Medium, explicação em Regular e apagada. Só três
    // corpos aparecem na tela — 20, 14,5 e 12,5.
    style.text_styles = [
        (egui::TextStyle::Heading, fonte_forte(20.0)),
        (egui::TextStyle::Body, FontId::proportional(CORPO)),
        (egui::TextStyle::Button, fonte_forte(13.5)),
        (egui::TextStyle::Small, FontId::proportional(12.5)),
        (egui::TextStyle::Monospace, FontId::monospace(12.5)),
    ]
    .into();

    let v = &mut style.visuals;
    v.dark_mode = escuro();
    // Nada de `override_text_color`: com ele o egui carimba a cor no texto já
    // no traçado, e um botão que pinta o próprio rótulo — o principal, que é
    // claro sobre escuro ou escuro sobre claro — perderia a cor dele. A cor
    // normal do texto vem do `fg_stroke` de cada estado, logo abaixo.
    v.override_text_color = None;
    v.weak_text_color = Some(p.texto_fraco);
    v.panel_fill = p.fundo;
    v.faint_bg_color = p.superficie;
    // Fundo das caixas de texto e do trilho dos controles deslizantes.
    v.extreme_bg_color = p.superficie;
    v.code_bg_color = p.superficie;
    v.warn_fg_color = p.gravando;
    v.error_fg_color = p.erro;
    v.hyperlink_color = p.texto;

    // Menus e listas suspensas saem numa camada por cima da janela.
    v.window_fill = p.fundo;
    v.window_stroke = Stroke::new(1.0, p.borda);
    v.window_corner_radius = CornerRadius::same(RAIO_CARTAO);
    v.menu_corner_radius = CornerRadius::same(RAIO_CARTAO);
    v.window_shadow = sombra_menu();
    v.popup_shadow = v.window_shadow;

    // `selection` faz três trabalhos ao mesmo tempo: o texto marcado numa caixa
    // de texto, a parte cheia do controle deslizante e a linha escolhida de uma
    // lista suspensa — nesta última ele também dita a cor do texto por cima
    // (`selection.stroke`). Por isso é um tom médio, e não a cor do texto com
    // transparência: aquilo deixava a linha escolhida da lista quase ilegível.
    v.selection.bg_fill = p.borda_forte;
    v.selection.stroke = Stroke::new(1.0, p.texto);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;
    v.button_frame = true;
    v.indent_has_left_vline = false;

    let controle = |w: &mut egui::style::WidgetVisuals, fundo: Color32, borda: Color32| {
        w.bg_fill = fundo;
        w.weak_bg_fill = fundo;
        w.bg_stroke = Stroke::new(1.0, borda);
        w.fg_stroke = Stroke::new(1.5, p.texto);
        w.corner_radius = CornerRadius::same(RAIO_CONTROLE);
        w.expansion = 0.0;
    };
    controle(&mut v.widgets.noninteractive, p.superficie, p.borda);
    controle(&mut v.widgets.inactive, p.superficie, p.borda);
    controle(&mut v.widgets.hovered, p.superficie_forte, p.borda_forte);
    controle(&mut v.widgets.active, p.superficie_forte, p.borda_forte);
    controle(&mut v.widgets.open, p.superficie, p.borda_forte);

    // `weak_bg_fill` é o fundo dos botões e das listas suspensas; `bg_fill`, o
    // trilho e o cursor dos controles deslizantes. Precisam ser diferentes: o
    // trilho fica dentro de um cartão da cor da superfície, e da cor dela
    // sumia — o controle virava um círculo solto no meio do nada.
    v.widgets.inactive.bg_fill = p.superficie_forte;
    v.widgets.hovered.bg_fill = p.borda_forte;
    v.widgets.active.bg_fill = p.borda_forte;

    style.spacing.item_spacing = Vec2::new(8.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.spacing.slider_width = 180.0;
    style.spacing.slider_rail_height = 6.0;
    style.spacing.combo_height = 280.0;
    style.spacing.interact_size = Vec2::new(40.0, 20.0);
    style.spacing.menu_margin = Margin::same(6);

    // Barra de rolagem fininha, encostada na borda direita.
    style.spacing.scroll = egui::style::ScrollStyle::floating();
    let barra = &mut style.spacing.scroll;
    barra.bar_width = 8.0;
    barra.floating_width = 5.0;
    barra.handle_min_length = 32.0;
    barra.content_margin = Margin {
        right: 8,
        ..Margin::ZERO
    };
    barra.dormant_handle_opacity = 0.0;
    barra.active_handle_opacity = 0.35;
    barra.interact_handle_opacity = 0.6;
}

/// Sombra da janela. Larga e fraca — o suficiente para a superfície descolar do
/// que estiver atrás sem virar uma mancha.
pub fn sombra_janela() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: paleta().sombra,
    }
}

/// A mesma ideia, menor, para menus e listas suspensas.
pub fn sombra_menu() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: paleta().sombra,
    }
}

// ------------------------------------------------------------ tema do sistema

/// 0 = ainda não perguntamos, 1 = claro, 2 = escuro.
static SISTEMA: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// O que o sistema prefere, com a resposta guardada.
///
/// Perguntar custa um processo, e isto é lido a cada quadro — daí a memória.
/// Quem quiser a resposta fresca chama `reler_o_sistema` (as configurações
/// fazem isso ao abrir).
pub fn sistema_escuro() -> bool {
    match SISTEMA.load(Ordering::Relaxed) {
        0 => reler_o_sistema(),
        1 => false,
        _ => true,
    }
}

/// Pergunta de novo ao sistema e devolve a resposta.
pub fn reler_o_sistema() -> bool {
    let escuro = sistema_prefere_escuro();
    SISTEMA.store(1 + escuro as u8, Ordering::Relaxed);
    escuro
}

/// O que o GNOME diz preferir em *Configurações → Aparência*.
///
/// Vem do `gsettings`, que é o que o próprio GNOME lê. Sem ele — outra área de
/// trabalho, sessão sem D-Bus —, fica no escuro, que é o padrão do Ditador.
fn sistema_prefere_escuro() -> bool {
    let saida = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output();
    match saida {
        Ok(saida) if saida.status.success() => {
            // 'default', 'prefer-dark' ou 'prefer-light', entre aspas. O
            // 'default' do GNOME é o tema claro.
            String::from_utf8_lossy(&saida.stdout).contains("prefer-dark")
        }
        _ => true,
    }
}
