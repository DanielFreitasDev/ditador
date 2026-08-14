//! Vidro líquido desenhado pela GPU.
//!
//! O `glass.rs` reproduz o vidro empilhando formas vetoriais — é o que dá para
//! fazer com o tesselador do egui, e funciona, mas cada pista óptica precisa
//! virar uma camada separada. A extensão de GNOME que serviu de referência
//! (ryohsuke1231/liquid-glass) faz o contrário: um único shader por peça, que
//! calcula, pixel a pixel, a altura da superfície, a normal em 3D, para onde a
//! luz entorta ao atravessar o vidro (lei de Snell) e como ela volta pela
//! beirada. É assim que a Apple faz, e é o que este módulo porta para cá.
//!
//! A diferença de contexto é uma só, e é a que manda em todo o resto: a
//! extensão roda *dentro* do compositor, então ela tem o quadro da tela inteira
//! para refratar. Um aplicativo comum não tem — nenhum compositor do Linux
//! entrega o que está atrás da janela. Então o fundo que este shader refrata é
//! montado de duas fontes:
//!
//!   * para o painel: o papel de parede da área de trabalho, lido do GNOME,
//!     borrado e recortado na posição da janela. Não é o que está atrás de
//!     verdade (uma janela por baixo não aparece), mas é a cor real do desktop
//!     naquele ponto, e entra por baixo com alfa parcial — o que estiver atrás
//!     continua aparecendo pelo canal alfa da janela;
//!   * para tudo que fica *dentro* do painel (cartões, botões, interruptores):
//!     uma cópia do próprio framebuffer feita logo antes de desenhar a peça.
//!     Esse fundo é exato: a beirada de um botão realmente entorta o texto e o
//!     vidro que estão embaixo dele.
//!
//! Cada peça é um quadrilátero que cobre o retângulo do `Shape::Callback`. Fora
//! da silhueta o shader devolve o pixel do fundo sem tocar, então a peça pode
//! ser desenhada com a mistura desligada e ainda assim não deixar rastro.

use crate::config::Appearance;
use crossbeam_channel::Receiver;
use eframe::egui_glow::CallbackFn;
use eframe::glow::{self, HasContext as _};
use egui::{Pos2, Rect};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Receita de uma peça de vidro, em pontos lógicos (o shader converte para
/// pixels). Os nomes seguem os de `glass::Vidro`, com o que só o shader usa.
#[derive(Clone, Copy)]
pub struct Peca {
    pub rect: Rect,
    /// Raio dos cantos.
    pub raio: f32,
    /// Expoente da superelipse: 2 = arco de círculo, ~4,2 = squircle.
    pub n: f32,
    /// Largura da faixa em que a superfície sobe da borda até a altura cheia.
    pub bisel: f32,
    /// Altura máxima do relevo. É ela que dita o quanto a refração desloca.
    pub espessura: f32,
    /// Índice de refração do "material".
    pub ior: f32,
    /// Separação das componentes de cor na refração, em pontos.
    pub cromatica: f32,
    /// Tinta do corpo, RGBA não pré-multiplicado, 0..1.
    pub tinta: [f32; 4],
    /// Escurecimento junto da borda, por dentro (oclusão de ambiente).
    pub ao: f32,
    pub ao_raio: f32,
    /// Intensidade e largura da borda especular.
    pub rim: f32,
    pub rim_larg: f32,
    /// Brilho especular concentrado e véu amplo da superfície.
    pub espec: f32,
    pub sheen: f32,
    /// Cor do retorno frio pelo lado oposto ao da luz, já com a intensidade.
    pub frio: [f32; 3],
    /// Cursor: a beirada mais próxima dele acende.
    pub foco: Option<Pos2>,
    pub foco_alcance: f32,
    /// Quanto do papel de parede entra por baixo (0 = só o framebuffer).
    pub parede: f32,
    /// Sombra projetada: raio e intensidade (0 desliga).
    pub sombra: [f32; 2],
    /// Opacidade da peça inteira, sombra incluída — é por aqui que a animação
    /// de abertura faz o vidro surgir.
    pub opacidade: f32,
}

impl Default for Peca {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            raio: 12.0,
            n: 4.2,
            bisel: 8.0,
            espessura: 5.0,
            ior: 1.42,
            cromatica: 0.8,
            tinta: [1.0, 1.0, 1.0, 0.1],
            ao: 0.14,
            ao_raio: 8.0,
            rim: 1.0,
            rim_larg: 1.6,
            espec: 0.4,
            sheen: 0.14,
            frio: [0.0, 0.0, 0.0],
            foco: None,
            foco_alcance: 95.0,
            parede: 0.0,
            sombra: [0.0, 0.0],
            opacidade: 1.0,
        }
    }
}

/// Folga entre a peça e o retângulo do callback: cabe a sombra projetada e o
/// quanto a refração amostra para fora da silhueta.
fn folga(peca: &Peca) -> f32 {
    peca.sombra[0].max(2.0) + 2.0
}

/// Devolve a peça como um `Shape` do egui, ou `None` se a GPU não estiver
/// disponível — nesse caso quem chamou usa o desenho vetorial de `glass.rs`.
pub fn shape(peca: Peca) -> Option<egui::Shape> {
    let gpu = GPU.get()?;
    if !peca.rect.is_positive() || peca.opacidade <= 0.001 {
        return None;
    }

    let rect = peca.rect.expand(folga(&peca));
    let gpu = gpu.clone();
    Some(egui::Shape::Callback(egui::PaintCallback {
        rect,
        callback: Arc::new(CallbackFn::new(move |info, _painter| {
            if let Ok(mut gpu) = gpu.lock() {
                gpu.desenhar(&info, &peca);
            }
        })),
    }))
}

/// Prepara a GPU. Sem isto (ou se algo falhar) tudo cai no caminho vetorial.
/// `DITADOR_SEM_GPU=1` força o caminho vetorial — útil para comparar os dois e
/// para escapar de um driver que engasgue com o shader.
pub fn iniciar(gl: Arc<glow::Context>) {
    if GPU.get().is_some() || std::env::var_os("DITADOR_SEM_GPU").is_some() {
        return;
    }
    match Gpu::new(gl) {
        Ok(gpu) => {
            let _ = GPU.set(Arc::new(Mutex::new(gpu)));
            log::info!("vidro por GPU ligado");
        }
        Err(e) => log::warn!("vidro por GPU indisponível ({e}); usando o desenho vetorial"),
    }
}

/// Informa onde a janela está na tela, para recortar o papel de parede. Precisa
/// ser chamado a cada quadro: a janela se move e muda de tamanho.
pub fn atualizar_tela(ctx: &egui::Context) {
    let Some(gpu) = GPU.get() else { return };
    let (janela, monitor, ppp) = ctx.input(|i| {
        (
            i.viewport().outer_rect,
            i.viewport().monitor_size,
            i.pixels_per_point(),
        )
    });
    if let Ok(mut gpu) = gpu.lock() {
        gpu.posicionar(janela, monitor, ppp);
    }
}

/// Aparência em vigor. Vale a partir do quadro seguinte, então mexer num
/// controle das configurações se vê na hora; só o papel de parede, quando os
/// parâmetros de leitura dele mudam, precisa ser lido do disco de novo.
pub fn aplicar_aparencia(nova: Appearance) {
    let mut atual = APARENCIA.write().unwrap_or_else(|e| e.into_inner());
    if *atual == nova {
        return;
    }
    let releitura = atual.wallpaper_detail != nova.wallpaper_detail
        || atual.wallpaper_brightness != nova.wallpaper_brightness
        || atual.wallpaper_saturation != nova.wallpaper_saturation;
    *atual = nova;
    if releitura {
        RELER_PAREDE.store(true, Ordering::Relaxed);
    }
}

/// A aparência em vigor, para quem monta as receitas das peças.
pub fn aparencia() -> Appearance {
    *APARENCIA.read().unwrap_or_else(|e| e.into_inner())
}

/// Opacidade de todas as peças no quadro que vem. O egui aplica a dele nas
/// formas que ele mesmo desenha (`Ui::multiply_opacity`), mas não alcança os
/// callbacks — daí o vidro precisar do próprio caminho.
pub fn definir_opacidade(o: f32) {
    OPACIDADE.store(o.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn opacidade() -> f32 {
    f32::from_bits(OPACIDADE.load(Ordering::Relaxed))
}

static GPU: OnceLock<Arc<Mutex<Gpu>>> = OnceLock::new();
static APARENCIA: RwLock<Appearance> = RwLock::new(Appearance::PADRAO);
static RELER_PAREDE: AtomicBool = AtomicBool::new(false);
static OPACIDADE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1.0f32.to_bits());

// ------------------------------------------------------------------ o renderer

struct Gpu {
    gl: Arc<glow::Context>,
    programa: glow::Program,
    vao: glow::VertexArray,
    loc: Locais,
    /// Cópia do framebuffer: o que está atrás da peça que vai ser desenhada.
    fundo: glow::Texture,
    fundo_tam: [i32; 2],
    /// Papel de parede já borrado e escurecido.
    parede: Option<glow::Texture>,
    parede_tam: [f32; 2],
    parede_chegando: Option<Receiver<Imagem>>,
    /// Mapeamento pixel-da-tela → uv do papel de parede.
    parede_uv: [f32; 4],
    parede_pronta: bool,
}

struct Locais {
    fb: Option<glow::UniformLocation>,
    rect: Option<glow::UniformLocation>,
    geo: Option<glow::UniformLocation>,
    opt: Option<glow::UniformLocation>,
    tinta: Option<glow::UniformLocation>,
    luzes: Option<glow::UniformLocation>,
    extra: Option<glow::UniformLocation>,
    luz: Option<glow::UniformLocation>,
    frio: Option<glow::UniformLocation>,
    foco: Option<glow::UniformLocation>,
    pared: Option<glow::UniformLocation>,
    opacidade: Option<glow::UniformLocation>,
    tex_fundo: Option<glow::UniformLocation>,
    tex_parede: Option<glow::UniformLocation>,
}

struct Imagem {
    pixels: Vec<u8>,
    largura: u32,
    altura: u32,
}

impl Gpu {
    fn new(gl: Arc<glow::Context>) -> Result<Self, String> {
        let versao = eframe::egui_glow::ShaderVersion::get(&gl);
        let cabecalho = if versao.is_embedded() {
            "#version 300 es\nprecision highp float;\nprecision highp sampler2D;\n"
        } else {
            "#version 140\n"
        };

        let programa = unsafe { compilar(&gl, cabecalho)? };
        let vao = unsafe { gl.create_vertex_array()? };
        let fundo = unsafe { criar_textura(&gl)? };

        let loc = unsafe {
            Locais {
                fb: gl.get_uniform_location(programa, "u_fb"),
                rect: gl.get_uniform_location(programa, "u_rect"),
                geo: gl.get_uniform_location(programa, "u_geo"),
                opt: gl.get_uniform_location(programa, "u_opt"),
                tinta: gl.get_uniform_location(programa, "u_tinta"),
                luzes: gl.get_uniform_location(programa, "u_luzes"),
                extra: gl.get_uniform_location(programa, "u_extra"),
                luz: gl.get_uniform_location(programa, "u_luz"),
                frio: gl.get_uniform_location(programa, "u_frio"),
                foco: gl.get_uniform_location(programa, "u_foco"),
                pared: gl.get_uniform_location(programa, "u_pared"),
                opacidade: gl.get_uniform_location(programa, "u_opacidade"),
                tex_fundo: gl.get_uniform_location(programa, "u_fundo"),
                tex_parede: gl.get_uniform_location(programa, "u_parede"),
            }
        };

        Ok(Self {
            gl,
            programa,
            vao,
            loc,
            fundo,
            fundo_tam: [0, 0],
            parede: None,
            parede_tam: [1.0, 1.0],
            parede_chegando: Some(parede::carregar_em_segundo_plano(aparencia())),
            parede_uv: [0.0, 0.0, 0.0, 0.0],
            parede_pronta: false,
        })
    }

    /// Recalcula o recorte do papel de parede para a posição atual da janela.
    fn posicionar(&mut self, janela: Option<Rect>, monitor: Option<egui::Vec2>, ppp: f32) {
        let (Some(janela), Some(monitor)) = (janela, monitor) else {
            self.parede_pronta = false;
            return;
        };
        let [iw, ih] = self.parede_tam;
        let (mx, my) = (monitor.x * ppp, monitor.y * ppp);
        if iw < 1.0 || ih < 1.0 || mx < 1.0 || my < 1.0 {
            self.parede_pronta = false;
            return;
        }

        // "Cover": a imagem cobre o monitor inteiro, sobrando dos dois lados do
        // eixo mais folgado — é o que o GNOME faz ao pintar a área de trabalho.
        let escala = (mx / iw).max(my / ih);
        let (larg, alt) = (iw * escala, ih * escala);
        let (ox, oy) = ((mx - larg) / 2.0, (my - alt) / 2.0);

        self.parede_uv = [
            (janela.left() * ppp - ox) / larg,
            (janela.top() * ppp - oy) / alt,
            1.0 / larg,
            1.0 / alt,
        ];
        self.parede_pronta = true;
    }

    fn desenhar(&mut self, info: &egui::PaintCallbackInfo, peca: &Peca) {
        self.receber_parede();

        let gl = self.gl.clone();
        let ppp = info.pixels_per_point;
        let [fbw, fbh] = info.screen_size_px;
        let (fbw, fbh) = (fbw as i32, fbh as i32);
        if fbw <= 0 || fbh <= 0 {
            return;
        }

        let alvo = info.viewport_in_pixels();
        if alvo.width_px <= 0 || alvo.height_px <= 0 {
            return;
        }

        // A camada pode estar sob uma transformação (a animação de abertura), e
        // o egui já a aplicou no retângulo do callback. Comparando o retângulo
        // que chegou com o que a peça pediu sai a mesma transformação, que
        // precisa valer para a silhueta e para todos os comprimentos — senão a
        // peça encolhe mas o vidro dela fica do tamanho de antes.
        let pedido = peca.rect.expand(folga(peca));
        let escala = if pedido.width() > 0.5 {
            info.viewport.width() / pedido.width()
        } else {
            1.0
        };
        let desloc = info.viewport.min.to_vec2() - pedido.min.to_vec2() * escala;
        let mapear = |p: Pos2| (p.to_vec2() * escala + desloc).to_pos2();
        let r = Rect::from_min_max(mapear(peca.rect.min), mapear(peca.rect.max));
        // Escala combinada com a densidade de pixels: leva pontos da peça para
        // pixels do framebuffer.
        let esc = escala * ppp;

        // A cópia só precisa cobrir o que a refração vai amostrar para fora do
        // quadrilátero: o deslocamento máximo mais a separação das cores.
        let margem = ((peca.bisel * 1.5 + peca.cromatica + 2.0) * esc).ceil() as i32;

        unsafe {
            self.copiar_fundo(&gl, [fbw, fbh], &alvo, margem);

            // A mistura fica desligada porque o shader devolve o composto
            // inteiro, fundo incluído; o recorte do egui (o scissor) continua
            // valendo, senão uma peça dentro de uma área rolável vazaria.
            gl.disable(glow::BLEND);
            gl.bind_vertex_array(Some(self.vao));
            gl.use_program(Some(self.programa));

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.fundo));
            gl.uniform_1_i32(self.loc.tex_fundo.as_ref(), 0);
            gl.active_texture(glow::TEXTURE1);
            // Sem papel de parede, a unidade 1 recebe a textura de fundo só
            // para não ficar apontando para uma textura incompleta.
            gl.bind_texture(glow::TEXTURE_2D, Some(self.parede.unwrap_or(self.fundo)));
            gl.uniform_1_i32(self.loc.tex_parede.as_ref(), 1);

            // Retângulo da peça em pixels do framebuffer, com o y crescendo para
            // cima (que é como o gl_FragCoord conta).
            gl.uniform_4_f32(
                self.loc.rect.as_ref(),
                r.left() * ppp,
                fbh as f32 - r.bottom() * ppp,
                r.right() * ppp,
                fbh as f32 - r.top() * ppp,
            );
            gl.uniform_2_f32(self.loc.fb.as_ref(), fbw as f32, fbh as f32);

            let lado = (r.width().min(r.height()) * ppp * 0.5).max(1.0);
            let raio = (peca.raio * esc).min(lado);
            let bisel = (peca.bisel * esc).min(lado);
            gl.uniform_4_f32(
                self.loc.geo.as_ref(),
                raio,
                peca.n,
                bisel.max(1.0),
                peca.espessura * esc,
            );

            let parede = if self.parede.is_some() && self.parede_pronta {
                peca.parede
            } else {
                0.0
            };
            gl.uniform_4_f32(
                self.loc.opt.as_ref(),
                peca.ior,
                peca.cromatica * esc,
                0.75 * ppp,
                parede,
            );
            gl.uniform_4_f32(
                self.loc.tinta.as_ref(),
                peca.tinta[0],
                peca.tinta[1],
                peca.tinta[2],
                peca.tinta[3],
            );
            gl.uniform_4_f32(
                self.loc.luzes.as_ref(),
                peca.rim,
                (peca.rim_larg * esc).max(0.75),
                peca.espec,
                peca.sheen,
            );
            gl.uniform_4_f32(
                self.loc.extra.as_ref(),
                peca.ao,
                (peca.ao_raio * esc).max(1.0),
                peca.sombra[0] * esc,
                peca.sombra[1] * peca.opacidade,
            );

            // A luz vem de cima e um pouco da esquerda, na mesma direção que o
            // caminho vetorial usa — as duas versões precisam combinar.
            gl.uniform_3_f32(self.loc.luz.as_ref(), -0.34, 0.94, 0.55);
            gl.uniform_3_f32(
                self.loc.frio.as_ref(),
                peca.frio[0],
                peca.frio[1],
                peca.frio[2],
            );

            match peca.foco.map(mapear) {
                Some(f) => gl.uniform_3_f32(
                    self.loc.foco.as_ref(),
                    f.x * ppp,
                    fbh as f32 - f.y * ppp,
                    (peca.foco_alcance * esc).max(1.0),
                ),
                None => gl.uniform_3_f32(self.loc.foco.as_ref(), 0.0, 0.0, -1.0),
            }

            let uv = self.parede_uv;
            gl.uniform_4_f32(self.loc.pared.as_ref(), uv[0], uv[1], uv[2], uv[3]);
            gl.uniform_1_f32(self.loc.opacidade.as_ref(), peca.opacidade.clamp(0.0, 1.0));

            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);

            // O egui restaura o resto do estado sozinho depois do callback.
            gl.bind_vertex_array(None);
            gl.active_texture(glow::TEXTURE0);
            gl.enable(glow::BLEND);
        }
    }

    /// Copia para a textura de fundo a região do framebuffer que a peça vai
    /// cobrir (mais uma folga, porque a refração amostra para os lados).
    unsafe fn copiar_fundo(
        &mut self,
        gl: &glow::Context,
        [fbw, fbh]: [i32; 2],
        alvo: &egui::epaint::ViewportInPixels,
        folga: i32,
    ) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.fundo));
            if self.fundo_tam != [fbw, fbh] {
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    fbw,
                    fbh,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.fundo_tam = [fbw, fbh];
            }

            let x = (alvo.left_px - folga).clamp(0, fbw);
            let y = (alvo.from_bottom_px - folga).clamp(0, fbh);
            let w = (alvo.left_px + alvo.width_px + folga).clamp(0, fbw) - x;
            let h = (alvo.from_bottom_px + alvo.height_px + folga).clamp(0, fbh) - y;
            if w > 0 && h > 0 {
                gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, x, y, x, y, w, h);
            }
        }
    }

    /// Sobe o papel de parede para a GPU assim que a thread de leitura termina,
    /// e manda ler de novo quando o desfoque ou a cor dele mudam nas
    /// configurações (aí a imagem tem que ser reprocessada do arquivo).
    fn receber_parede(&mut self) {
        if RELER_PAREDE.swap(false, Ordering::Relaxed) {
            self.parede_chegando = Some(parede::carregar_em_segundo_plano(aparencia()));
        }
        let Some(rx) = &self.parede_chegando else {
            return;
        };
        let Ok(img) = rx.try_recv() else {
            return;
        };
        self.parede_chegando = None;

        unsafe {
            if let Some(antiga) = self.parede.take() {
                self.gl.delete_texture(antiga);
            }
            match criar_textura(&self.gl) {
                Ok(tex) => {
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    self.gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        img.largura as i32,
                        img.altura as i32,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&img.pixels)),
                    );
                    self.parede = Some(tex);
                    self.parede_tam = [img.largura as f32, img.altura as f32];
                    log::info!(
                        "papel de parede em {}×{} pronto para o vidro",
                        img.largura,
                        img.altura
                    );
                }
                Err(e) => log::warn!("não consegui criar a textura do papel de parede: {e}"),
            }
        }
    }
}

unsafe fn criar_textura(gl: &glow::Context) -> Result<glow::Texture, String> {
    unsafe {
        let tex = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        for (chave, valor) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, chave, valor as i32);
        }
        Ok(tex)
    }
}

unsafe fn compilar(gl: &glow::Context, cabecalho: &str) -> Result<glow::Program, String> {
    unsafe {
        let programa = gl.create_program()?;
        let mut shaders = Vec::new();
        for (tipo, fonte) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, FRAGMENT),
        ] {
            let shader = gl.create_shader(tipo)?;
            gl.shader_source(shader, &format!("{cabecalho}{fonte}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(programa, shader);
            shaders.push(shader);
        }
        gl.link_program(programa);
        if !gl.get_program_link_status(programa) {
            return Err(gl.get_program_info_log(programa));
        }
        for shader in shaders {
            gl.detach_shader(programa, shader);
            gl.delete_shader(shader);
        }
        Ok(programa)
    }
}

// ----------------------------------------------------------------- os shaders

/// Não há malha nenhuma: o quadrilátero sai do índice do vértice. O egui já
/// deixou o `glViewport` no retângulo do callback, então o quadrado de -1 a 1
/// cai exatamente em cima dele.
const VERTEX: &str = r#"
void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"
uniform vec2 u_fb;      // tamanho do framebuffer, px
uniform vec4 u_rect;    // x0, y0, x1, y1 da peça, px (y crescendo para cima)
uniform vec4 u_geo;     // raio, expoente da superelipse, bisel, espessura
uniform vec4 u_opt;     // ior, aberração cromática, suavização, papel de parede
uniform vec4 u_tinta;   // cor do corpo, alfa não pré-multiplicado
uniform vec4 u_luzes;   // borda, largura da borda, especular, véu
uniform vec4 u_extra;   // oclusão, raio da oclusão, raio da sombra, intensidade
uniform vec3 u_luz;     // direção da luz, em 3D
uniform vec3 u_frio;    // cor do retorno pelo lado oposto ao da luz
uniform vec3 u_foco;    // cursor em px e alcance (alcance < 0 = sem cursor)
uniform vec4 u_pared;   // px da tela -> uv do papel de parede: (a.xy, b.zw)
uniform float u_opacidade; // a peça inteira surgindo (animação de abertura)
uniform sampler2D u_fundo;
uniform sampler2D u_parede;

out vec4 cor_saida;

/// Perfil da superfície: quanto mais alto o expoente, mais a peça fica com cara
/// de almofada — sobe depressa junto da borda e achata cedo no meio.
const float PERFIL = 2.4;

/// Distância com sinal até a silhueta. Com expoente 2 é o retângulo de cantos
/// redondos de sempre; acima disso os cantos viram superelipse (o "squircle" da
/// Apple), em que a curvatura entra cedo e se estica ao longo das retas.
float sdf(vec2 p, vec2 b, float r, float n) {
    vec2 q = abs(p) - b + vec2(r);
    vec2 m = max(q, vec2(0.0));
    float fora = (n <= 2.05)
        ? length(m)
        : pow(pow(m.x, n) + pow(m.y, n), 1.0 / n);
    return min(max(q.x, q.y), 0.0) + fora - r;
}

/// Altura da superfície no ponto. Fora da peça é zero, mas com uma descida
/// suave: um degrau aqui viraria um pico no gradiente, e o pico apareceria como
/// uma serrilha na refração.
float altura(vec2 p, vec2 b) {
    float d = sdf(p, b, u_geo.x, u_geo.y);
    float zona = max(u_opt.z, 1.0);
    if (d > zona) return 0.0;

    float t = clamp(-d / u_geo.z, 0.0, 1.0);
    float inv = clamp(1.0 - t, 0.0, 1.0);
    float h = pow(max(1.0 - pow(inv, PERFIL), 0.0), 1.0 / PERFIL);
    return h * (1.0 - smoothstep(-zona, zona, d)) * u_geo.w;
}

/// Normal em 3D, tirada por diferenças finitas do campo de altura.
vec3 normal_em(vec2 p, vec2 b) {
    float e = 0.7;
    float gx = altura(p + vec2(e, 0.0), b) - altura(p - vec2(e, 0.0), b);
    float gy = altura(p + vec2(0.0, e), b) - altura(p - vec2(0.0, e), b);
    return normalize(vec3(-gx / (2.0 * e), -gy / (2.0 * e), 1.0));
}

/// Põe o papel de parede por baixo do que já foi desenhado no framebuffer.
/// Recebe a amostra pronta para poder ser reaproveitada sem custo.
vec4 com_parede(vec4 fb, vec2 px) {
    if (u_opt.w <= 0.0) return fb;
    // A tela conta o y de cima para baixo; o framebuffer, de baixo para cima.
    vec2 tela = vec2(px.x, u_fb.y - px.y);
    vec2 uv = clamp(u_pared.xy + tela * u_pared.zw, vec2(0.0), vec2(1.0));
    vec3 papel = texture(u_parede, uv).rgb;
    float a = u_opt.w * (1.0 - fb.a);
    return vec4(fb.rgb + papel * a, fb.a + a);
}

/// O que está atrás da peça, em pixels do framebuffer. Devolve cor
/// pré-multiplicada.
vec4 fundo_em(vec2 px) {
    return com_parede(texture(u_fundo, px / u_fb), px);
}

/// Sombra projetada, só do lado de fora: um núcleo curto e escuro (a umbra,
/// onde o vidro tapa a luz por inteiro) e um halo largo e macio (a penumbra).
float sombra(float d, vec2 para_fora) {
    float raio = u_extra.z;
    float forca = u_extra.w;
    if (raio <= 0.01 || forca <= 0.0 || d <= 0.0) return 0.0;

    // A sombra se estica um pouco mais do lado oposto ao da luz.
    float alinhamento = max(dot(normalize(para_fora), -normalize(u_luz.xy)), 0.0);
    float r = raio * (0.85 + 0.15 * alinhamento);
    float intensidade = forca * (0.85 + 0.15 * alinhamento);

    float umbra = (1.0 - clamp(d / max(r * 0.40, 0.5), 0.0, 1.0)) * 0.80;
    float t = 1.0 - clamp(d / max(r, 0.001), 0.0, 1.0);
    // Quíntica de Perlin: chega a zero com a inclinação e a curvatura zeradas
    // também, então o fim do halo não deixa um anel visível.
    float penumbra = t * t * t * (t * (t * 6.0 - 15.0) + 10.0) * 0.55;

    return clamp((umbra + penumbra) * intensidade, 0.0, 1.0);
}

void main() {
    vec2 px = gl_FragCoord.xy;
    vec2 centro = (u_rect.xy + u_rect.zw) * 0.5;
    vec2 b = (u_rect.zw - u_rect.xy) * 0.5;
    vec2 p = px - centro;

    float d = sdf(p, b, u_geo.x, u_geo.y);
    float pena = max(u_opt.z, 0.5);
    float de_fora = smoothstep(-pena, pena, d);
    // A opacidade entra na cobertura: a peça inteira — vidro, luz e sombra —
    // aparece por cima do que já estava, sem precisar de uma passada à parte.
    float cobertura = (1.0 - de_fora) * u_opacidade;

    // O que já estava no framebuffer, sem o papel de parede: fora da peça a
    // saída tem que ser idêntica a ele, ou a peça deixaria um rastro.
    vec4 dest = texture(u_fundo, px / u_fb);

    // Sombra por baixo de tudo.
    float sa = sombra(d, p + vec2(1e-4)) * de_fora;
    vec3 cor_sombra = vec3(0.03, 0.04, 0.08);
    vec4 sob = vec4(cor_sombra * sa + dest.rgb * (1.0 - sa), sa + dest.a * (1.0 - sa));

    if (cobertura <= 0.002) {
        cor_saida = sob;
        return;
    }

    // Passado o bisel a superfície é plana: a normal é reta, a refração é nula
    // e o campo de altura não precisa nem ser amostrado. Num painel grande esse
    // é o caso da maioria absoluta dos pixels, e é o que segura o custo. O
    // resultado é o mesmo do caminho longo — ali o gradiente já seria zero.
    bool plano = -d > u_geo.z + 2.0;

    vec3 n = plano ? vec3(0.0, 0.0, 1.0) : normal_em(p, b);

    // ------------------------------------------------------------- refração
    //
    // Lei de Snell: o raio que entra de frente sai torto dentro do vidro, e o
    // quanto ele anda de lado até chegar ao fundo é o deslocamento da amostra.
    vec2 desl = vec2(0.0);
    if (!plano) {
        vec3 raio = refract(vec3(0.0, 0.0, -1.0), n, 1.0 / max(u_opt.x, 1.001));
        if (dot(raio, raio) > 1e-8) {
            desl = raio.xy / max(-raio.z, 0.15) * u_geo.w;
            float limite = max(u_geo.z * 1.5, 2.0);
            if (length(desl) > limite) desl = normalize(desl) * limite;
        }
        // Junto da borda o gradiente é abrupto; amansar ali evita serrilha.
        desl *= smoothstep(0.0, pena * 3.0, -d);
    }

    float anda = length(desl);
    vec4 atras;
    if (anda < 0.25) {
        // Sem deslocamento que se veja, a amostra é o pixel de baixo, que já
        // está na mão — nem uma busca a mais na textura.
        atras = com_parede(dest, px);
    } else if (u_opt.y < 0.05) {
        atras = fundo_em(px + desl);
    } else {
        // Aberração cromática: o vidro entorta cada cor um pouco diferente.
        vec2 ca = (desl / anda) * u_opt.y;
        vec4 amostra_g = fundo_em(px + desl);
        atras = vec4(
            fundo_em(px + desl + ca).r,
            amostra_g.g,
            fundo_em(px + desl - ca).b,
            amostra_g.a
        );
    }

    vec3 cor_fundo = atras.a > 0.004 ? atras.rgb / atras.a : vec3(0.0);
    float alfa_fundo = atras.a;

    // ----------------------------------------------------------- corpo e luz
    float am = u_tinta.a;
    vec3 cor = u_tinta.rgb * am + cor_fundo * (1.0 - am);
    float alfa = am + alfa_fundo * (1.0 - am);

    // Oclusão: o próprio corpo do vidro tapa a luz junto da borda, por dentro.
    float ao = 1.0 - smoothstep(0.0, u_extra.y, -d);
    cor *= 1.0 - ao * u_extra.x;

    vec3 luz = normalize(u_luz);
    vec3 vista = vec3(0.0, 0.0, 1.0);

    // A beirada tem duas escalas, e as duas importam: um fio quase sem largura
    // bem em cima do contorno (o reflexo especular da quina, que é o que dá
    // nitidez à peça) e um realce mais largo descendo pelo bisel (a espessura
    // do vidro acendendo). Só o largo deixa a peça borrada; só o fino a deixa
    // com cara de retângulo com contorno de 1 pixel.
    float fina = 1.0 - smoothstep(0.0, max(1.25, pena * 1.6), abs(d));
    float larga = 1.0 - smoothstep(0.0, u_luzes.y, abs(d));
    float banda = max(fina, larga);

    // Borda especular: acende onde a normal encara a luz e, do lado oposto,
    // devolve só o resto frio de quem atravessou a peça inteira.
    float fresnel = pow(1.0 - clamp(n.z, 0.0, 1.0), 3.0);
    float forma = 0.72 * fina + 0.62 * mix(pow(larga, 0.85), fresnel, 0.55) * larga;
    float frente = max(dot(n, luz), 0.0);
    float tras = max(-dot(n, luz), 0.0);
    vec3 borda = (pow(frente, 1.2) * vec3(1.0, 0.99, 0.96)
                + pow(tras, 1.6) * u_frio) * forma * u_luzes.x;

    // Reflexo concentrado. Pelo vetor médio (Blinn) em vez do raio refletido:
    // com uma superfície tão pouco inclinada quanto esta, o raio refletido
    // quase nunca cai no olho e o brilho simplesmente não aparece.
    vec3 meio = normalize(luz + vista);
    float espec = pow(max(dot(n, meio), 0.0), 18.0)
                * u_luzes.z * (0.30 + 0.70 * banda);

    // Véu amplo: a face inteira devolvendo um pouco de luz. Como cai com o
    // ângulo, na prática ele acende o bisel de cima — a luz entrando pela quina.
    float veu = pow(frente, 1.65) * u_luzes.w;

    // A beirada mais perto do cursor acende, como vidro polido sob a mão.
    float perto = 0.0;
    if (u_foco.z > 0.0) {
        float q = distance(px, u_foco.xy) / u_foco.z;
        perto = exp(-q * q) * banda;
    }

    vec3 acrescimo = vec3(espec + veu + perto * 0.85) + borda;

    // Mistura "screen" em vez de soma: a luz satura em 1 em vez de estourar.
    vec3 acesa = cor + acrescimo - cor * acrescimo;
    float pico = max(acesa.r, max(acesa.g, acesa.b));
    if (pico > 1.0) acesa /= pico;   // estoura mantendo o matiz
    acesa = max(acesa, vec3(0.0));

    cor_saida = vec4(
        acesa * alfa * cobertura + sob.rgb * (1.0 - cobertura),
        alfa * cobertura + sob.a * (1.0 - cobertura)
    );
}
"#;

// -------------------------------------------------------- papel de parede

mod parede {
    use super::Imagem;
    use crate::config::Appearance;
    use crossbeam_channel::Receiver;

    /// Desfoque extra, em pixels da imagem já reduzida. A maior parte do borrão
    /// vem da redução em si (`wallpaper_detail`), que é um passa-baixa de
    /// verdade; isto só tira o quadriculado que sobra.
    const DESFOQUE: f32 = 2.5;

    pub fn carregar_em_segundo_plano(a: Appearance) -> Receiver<Imagem> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let _ = std::thread::Builder::new()
            .name("papel-de-parede".into())
            .spawn(move || {
                if let Some(img) = carregar(a) {
                    let _ = tx.send(img);
                }
            });
        rx
    }

    fn carregar(a: Appearance) -> Option<Imagem> {
        let caminho = caminho()?;
        let bruta = match image::ImageReader::open(&caminho) {
            Ok(leitor) => match leitor.with_guessed_format().ok()?.decode() {
                Ok(img) => img,
                Err(e) => {
                    log::info!("papel de parede {caminho} não pôde ser lido ({e})");
                    return None;
                }
            },
            Err(e) => {
                log::info!("papel de parede {caminho} não pôde ser aberto ({e})");
                return None;
            }
        };

        let (lo, ao) = (bruta.width().max(1), bruta.height().max(1));
        let largura = a.wallpaper_detail.min(lo);
        let altura = ((largura as f32 / lo as f32) * ao as f32).round().max(1.0) as u32;

        let mut pequena = image::imageops::resize(
            &bruta.to_rgba8(),
            largura,
            altura,
            image::imageops::FilterType::Triangle,
        );
        pequena = image::imageops::blur(&pequena, DESFOQUE);

        for p in pequena.pixels_mut() {
            let [r, g, b, _] = p.0;
            let (r, g, b) = (r as f32, g as f32, b as f32);
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            let ajusta = |c: f32| {
                ((luma + (c - luma) * a.wallpaper_saturation) * a.wallpaper_brightness)
                    .clamp(0.0, 255.0) as u8
            };
            p.0 = [ajusta(r), ajusta(g), ajusta(b), 255];
        }

        Some(Imagem {
            largura,
            altura,
            pixels: pequena.into_raw(),
        })
    }

    /// Caminho do papel de parede atual, pelo GNOME. Segue o tema claro/escuro,
    /// que é o que o usuário está de fato vendo.
    fn caminho() -> Option<String> {
        let escuro =
            ler("org.gnome.desktop.interface", "color-scheme").is_some_and(|v| v.contains("dark"));
        let chaves: &[&str] = if escuro {
            &["picture-uri-dark", "picture-uri"]
        } else {
            &["picture-uri", "picture-uri-dark"]
        };
        for chave in chaves {
            if let Some(uri) = ler("org.gnome.desktop.background", chave)
                && let Some(caminho) = do_uri(&uri)
                && std::path::Path::new(&caminho).is_file()
            {
                return Some(caminho);
            }
        }
        None
    }

    fn ler(esquema: &str, chave: &str) -> Option<String> {
        let saida = std::process::Command::new("gsettings")
            .args(["get", esquema, chave])
            .output()
            .ok()?;
        if !saida.status.success() {
            return None;
        }
        Some(
            String::from_utf8_lossy(&saida.stdout)
                .trim()
                .trim_matches('\'')
                .to_string(),
        )
    }

    /// `file:///caminho%20com%20espaço` → `/caminho com espaço`.
    fn do_uri(uri: &str) -> Option<String> {
        let bruto = uri.strip_prefix("file://")?;
        let bytes = bruto.as_bytes();
        let mut saida = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    saida.push(byte);
                    i += 3;
                    continue;
                }
            }
            saida.push(bytes[i]);
            i += 1;
        }
        String::from_utf8(saida).ok()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn uri_vira_caminho() {
            assert_eq!(
                super::do_uri("file:///usr/share/Grand%20Triangle.jpg").as_deref(),
                Some("/usr/share/Grand Triangle.jpg")
            );
            assert_eq!(super::do_uri("/j\u{e1}/e/caminho"), None);
        }
    }
}
