//! Interface: sobreposição de gravação, caixa de resultado e configurações.
//!
//! O visual é sólido — ver `tema.rs` para a paleta e `widgets.rs` para os
//! controles. Cada tela é uma janela sem decoração: um retângulo arredondado
//! preenchido com a cor de fundo do tema, uma borda de um pixel e uma sombra por
//! baixo. O resto da janela é transparente, e é só por isso que ela precisa de
//! canal alfa.
//!
//! **No Windows não é assim, e não é limitação nossa.** O glutin não entrega alfa
//! por pixel numa janela OpenGL lá, então a folga em volta do retângulo não some:
//! ela vira uma moldura opaca de 22 px com o canto e a borda que o Windows 11
//! desenha por fora — "uma caixa atrás da janela". Lá a janela **é** o cartão
//! (`tema::FOLGA_SOMBRA` é zero), e a sombra e os cantos arredondados são os do
//! sistema, que já os aplica a toda janela de nível superior. O resultado parece
//! mais nativo do que a nossa sombra pareceria.

use crate::audio::Levels;
use crate::config::{IDIOMAS, MetodoDeColagem, TeclaDeEnvio, Tema};
use crate::state::{ModelState, QualAtalho, SharedState, Sinal, UiAction, View, lock};
use crate::stt;
use crate::tema::{self, medio, nota, paleta, titulo};
use crate::widgets::{self, Botao, Icone};
use crate::{clipboard, keys};
use crossbeam_channel::Sender;
use egui::{
    CornerRadius, LayerId, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
    ViewportCommand,
};
use std::time::Duration;

/// Folga a mais embaixo da fileira de botões que fecha uma tela.
///
/// A margem da janela é a mesma dos quatro lados, mas embaixo ela não basta: o
/// canto arredondado come parte do espaço, e um botão que é a última coisa da
/// tela pede mais ar do que um rótulo encostado na lateral. Somada à margem, dá
/// cerca do dobro do que sobra dos lados.
const RESPIRO: f32 = 16.0;

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
    /// Passeio automático pelas telas (ver `Demo`).
    demo: Option<Demo>,
    /// Quando a tela atual começou a aparecer, para a animação de entrada.
    abertura: Option<std::time::Instant>,
}

/// Diagnóstico opcional: com `DITADOR_DEMO=1` o programa passa sozinho pelas
/// quatro telas que ilustram o README — gravando, resultado, transcrições e
/// configurações —, com conteúdo de exemplo, e sai. Junto com `DITADOR_CAPTURA` é o que gera as
/// imagens do README a partir de um clone qualquer, sem precisar de microfone,
/// de modelo baixado nem de alguém falando na hora certa.
struct Demo {
    /// Marcado no primeiro quadro do passeio, e não na construção do `App`.
    ///
    /// Entre uma coisa e outra passam segundos que ninguém controla: o eframe
    /// criando a janela, o glow escolhendo o contexto, o driver Vulkan
    /// acordando. Contando da construção, esse tempo saía do orçamento da
    /// primeira fase — foi assim que o passeio chegou a gravar `result` e
    /// `settings` no mesmo segundo e a não gravar `recording` nenhuma vez.
    /// Contando daqui, cada tela tem os seus segundos inteiros.
    desde: Option<std::time::Instant>,
}

impl Demo {
    fn novo() -> Option<Self> {
        std::env::var_os("DITADOR_DEMO").map(|_| Self { desde: None })
    }
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

/// O convite para baixar o modelo, quando ele falta ou não abriu.
struct Oferta {
    /// Há `curl` ou `wget` para fazer o download.
    baixavel: bool,
    /// Qual modelo o botão baixa. Era o padrão, sempre; passou a ser o que a
    /// configuração aponta, que é o que quem escolheu um modelo de CPU na tela
    /// de configurações espera ver oferecido aqui.
    modelo: &'static crate::modelo::Modelo,
    rotulo: String,
    nota: &'static str,
}

/// Diagnóstico opcional: com `DITADOR_CAPTURA=<pasta>`, grava um PNG de cada
/// tela assim que ela estabiliza. Existe porque o GNOME nega a API de captura
/// de tela a aplicativos comuns, e sem isso não há como conferir o desenho — é
/// com ele que saem as imagens do README.
#[derive(Default)]
struct Captura {
    tela: Option<View>,
    /// Quando esta tela apareceu — a foto sai `ASSENTAR` depois (ver o uso).
    assentou_em: Option<std::time::Instant>,
    arquivo: Option<String>,
}

/// Quanto se espera uma tela nova parar de se mexer antes de fotografá-la.
///
/// Acima do teto da animação de entrada — `Appearance::animation_ms` é limitado
/// a 1000 ms em `config.rs` —, que é a única coisa que ainda muda depois do
/// primeiro quadro. Vale a folga: a fase mais curta do passeio dura 4 s, e uma
/// captura tirada cedo demais sai com a tela no meio do caminho.
const ASSENTAR: Duration = Duration::from_millis(1200);

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: SharedState,
        actions: Sender<UiAction>,
        levels: Levels,
        sinal: Sinal,
    ) -> Self {
        sinal.ligar_interface(cc.egui_ctx.clone());
        // Diagnóstico opcional: `DITADOR_ZOOM=1.5` desenha tudo maior, o que
        // serve tanto para conferir a interface numa tela densa quanto para as
        // imagens do README saírem com resolução de sobra.
        if std::env::var_os("DITADOR_ZOOM").is_some() {
            // `is_finite` antes do `clamp`: o `clamp` do Rust devolve NaN
            // quando o valor é NaN, e um fator NaN contamina o layout inteiro —
            // a janela sai vazia e nada na tela diz por quê.
            let zoom = std::env::var("DITADOR_ZOOM")
                .ok()
                .and_then(|z| z.parse::<f32>().ok())
                .filter(|z| z.is_finite())
                .unwrap_or(1.0);
            cc.egui_ctx.set_zoom_factor(zoom.clamp(0.5, 3.0));
        }
        tema::instalar_fontes(&cc.egui_ctx);
        tema::definir_escuro(escuro_agora(&lock(&shared).config.appearance));
        cc.egui_ctx.all_styles_mut(tema::estilo);

        Self {
            shared,
            actions,
            levels,
            applied: None,
            bars: vec![0.0; crate::audio::LEVEL_HISTORY],
            captura: Captura::default(),
            medidor: Medidor::novo(),
            demo: Demo::novo(),
            abertura: None,
        }
    }

    /// Passa sozinho pelas telas do README, com conteúdo de exemplo.
    ///
    /// Devolve `true` enquanto estiver conduzindo. Ver `Demo`.
    fn demonstrar(&mut self, ctx: &egui::Context, state: &mut crate::state::Shared) -> bool {
        const TEXTO: &str = "Confirmei com o time da manhã: o relatório de agosto sai na \
             sexta, com os números de julho já fechados. Se aparecer alguma pendência do \
             financeiro até quarta, me avisa que eu remanejo a revisão para a segunda. \
             Já pedi ao Marcelo que adiante a parte de compras, que é a que sempre atrasa, \
             e deixei a apresentação de terça marcada só depois do almoço.";

        let Some(demo) = &mut self.demo else {
            return false;
        };
        let inicio = *demo.desde.get_or_insert_with(std::time::Instant::now);
        let t = inicio.elapsed().as_secs_f32();

        state.model = ModelState::Ready;
        // O passeio promete telas limpas numa máquina qualquer, e as duas
        // queixas que ele não controla chegam por vias próprias: a thread do
        // Whisper escreve em `message` que o modelo não está lá, e quem não
        // estiver no grupo `input` ganha o aviso do atalho. As duas saíram na
        // captura do README — a do modelo por cima dos botões da tela de
        // resultado, porque o passeio rodou no minuto em que o download ainda
        // não tinha terminado. Forçar `Ready` sem limpá-las é dizer meia
        // verdade para a tela.
        state.message.clear();
        state.aviso_atalho = None;
        // Pelo mesmo motivo, o passeio ignora as integrações da máquina em que
        // roda: com a extensão do GNOME no ar, `tela_visivel` esconde as telas
        // de gravação e de transcrição — que é o certo no uso normal, e é
        // justamente a primeira imagem do README. O script parava com "o
        // passeio não gravou recording.png", sem dizer que a causa era a
        // extensão instalada de quem estava gerando as imagens.
        state.integracoes = Default::default();
        state.view = match t {
            _ if t < 4.0 => {
                // Alguns segundos para trás, para o cronômetro da imagem não
                // ficar parado em zero.
                state.recording_since = inicio.checked_sub(Duration::from_secs(7));
                View::Recording
            }
            _ if t < 9.0 => {
                state.text = TEXTO.to_string();
                state.status = "3,4 s · Vulkan".to_string();
                View::Result
            }
            _ if t < 14.0 => {
                // O histórico do passeio é inventado aqui, e não lido do disco:
                // a imagem do README precisa sair igual em qualquer clone, e o
                // histórico de quem gera as imagens é dele.
                state.historico = historico_de_exemplo();
                state.historico_em_disco = 48_320;
                View::Historico
            }
            _ if t < 20.0 => View::Settings,
            _ => {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                View::Hidden
            }
        };
        ctx.request_repaint();
        true
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

        // Espera a tela assentar (animação, layout) antes de fotografar — em
        // tempo, e não em quadros contados.
        //
        // Contava doze quadros, o que é a mesma coisa só enquanto a janela
        // recebe os 60 por segundo que se supõe. Ela não recebe: recém-criada e
        // com sincronia vertical, esta aqui roda a cerca de dois quadros por
        // segundo sob XWayland — com `DITADOR_QUADROS=1`, que desliga a
        // sincronia, a mesma interface faz 1800. Os doze quadros viraram cinco
        // segundos, mais do que os quatro da primeira tela do passeio, e a
        // captura da gravação simplesmente não saía; as outras duas saíam
        // atrasadas, no mesmo segundo. E as animações que esta espera existe
        // para deixar terminar são cronometradas — é do relógio que ela
        // precisava desde o começo.
        if self.captura.tela != Some(view) {
            self.captura.tela = Some(view);
            self.captura.assentou_em = (view != View::Hidden).then(std::time::Instant::now);
        } else if let Some(desde) = self.captura.assentou_em {
            if desde.elapsed() >= ASSENTAR {
                self.captura.assentou_em = None;
                self.captura.arquivo = Some(format!("{view:?}").to_lowercase());
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            } else {
                ctx.request_repaint();
            }
        }
    }

    fn act(&self, action: UiAction) {
        let _ = self.actions.send(action);
    }
}

/// As transcrições que o passeio de demonstração mostra.
///
/// Inventadas aqui, e não lidas do disco, por dois motivos: a imagem do README
/// precisa sair igual em qualquer clone, e o histórico de quem gera as imagens é
/// dele — não é para aparecer numa captura publicada.
///
/// Os instantes são relativos ao agora, para os rótulos de tempo ("há 5 min",
/// "ontem") saírem com sentido em qualquer dia em que as imagens sejam refeitas.
fn historico_de_exemplo() -> Vec<crate::historico::Entrada> {
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let exemplos: [(u64, &str, u64); 4] = [
        (
            90,
            "Confirmei com o time da manhã: o relatório de agosto sai na sexta, \
             com os números de julho já fechados.",
            7_400,
        ),
        (
            22 * 60,
            "Pede ao Marcelo para adiantar a parte de compras, que é a que sempre \
             atrasa.",
            4_100,
        ),
        (
            5 * 3_600,
            "Subimos o Kubernetes em São Paulo hoje à tarde; a migração do banco \
             fica para amanhã.",
            6_250,
        ),
        (
            26 * 3_600,
            "Reunião de terça remarcada para depois do almoço.",
            2_900,
        ),
    ];
    exemplos
        .into_iter()
        .map(|(atras, texto, duracao_ms)| crate::historico::Entrada {
            quando: agora.saturating_sub(atras),
            texto: texto.to_string(),
            audio: None,
            duracao_ms,
        })
        .collect()
}

/// Qual dos dois desenhos vale agora.
///
/// `DITADOR_TEMA=claro|escuro` atropela a configuração pelo tempo de uma
/// execução. É diagnóstico, como as outras variáveis daqui: é assim que as duas
/// versões das imagens do README saem sem mexer na configuração de ninguém.
fn escuro_agora(ap: &crate::config::Appearance) -> bool {
    match std::env::var("DITADOR_TEMA").as_deref() {
        Ok("claro") => false,
        Ok("escuro") => true,
        _ => ap.escuro(),
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Roda a cada `request_repaint`, inclusive com a janela escondida — é aqui
    /// que decidimos mostrá-la, redimensioná-la e posicioná-la.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // O Arc é clonado para que o guard não fique emprestando `self`: o
        // passeio de demonstração, logo abaixo, precisa de acesso exclusivo.
        let shared = self.shared.clone();
        let mut state = lock(&shared);

        if state.quitting {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        // O passeio de demonstração manda na tela, e por isso vem antes de tudo
        // que também mexe nela.
        if !self.demonstrar(ctx, &mut state) {
            // Fecha o resultado sozinho, se configurado.
            if state.view == View::Result && state.config.result_timeout_secs > 0 {
                let limite = Duration::from_secs(state.config.result_timeout_secs);
                if state.result_shown_at.is_some_and(|t| t.elapsed() >= limite) {
                    state.view = View::Hidden;
                }
            }
        }

        // `tela_visivel`, e não `state.view`: com a extensão do GNOME no ar a
        // gravação e a transcrição são anunciadas pelo OSD do Shell, e a nossa
        // janela fica fora do caminho.
        let view = state.tela_visivel();
        let modelo_carregando = state.model == ModelState::Loading;
        // Há um aviso de "copiado" contando os três segundos dele agora? É o que
        // faz a tela de configurações — que normalmente fica parada — se
        // redesenhar até ele sumir.
        let aviso_expirando = state.copied_at.is_some_and(|t| t.elapsed() < AVISO_COPIADO);
        // Ao abrir as configurações, dois controles precisam mostrar o que o
        // sistema realmente tem, não o que ficou gravado da última vez: o
        // interruptor de início automático e o tema, que o usuário pode ter
        // mudado no GNOME desde que o Ditador subiu.
        let abrindo_config = view == View::Settings && self.applied != Some(View::Settings);
        drop(state);

        if abrindo_config {
            // Os dois abrem processos (`systemctl`, `gsettings`). Ficam fora do
            // mutex: segurá-lo durante uma chamada de sistema prende o
            // controlador junto, e este é o único ponto do desenho que faz isso.
            let ligado = crate::autostart::ligado();
            tema::reler_o_sistema();
            let mut state = lock(&shared);
            state.draft.start_with_session = ligado;
            state.config.start_with_session = ligado;
        }

        if self.applied != Some(view) {
            apply_window(ctx, view);
            self.applied = Some(view);
            self.abertura = (view != View::Hidden).then(std::time::Instant::now);
            // Uma tela nova é o momento de conferir de novo o que está
            // instalado — quem acabou de rodar `apt install ydotool` com o
            // Ditador aberto vê a mudança sem reiniciar nada. Durante o
            // desenho, as respostas vêm da memória (ver `programas.rs`).
            crate::programas::reler();
        }

        match cadencia_de_repintura(view, modelo_carregando, aviso_expirando) {
            Some(intervalo) if intervalo.is_zero() => ctx.request_repaint(),
            Some(intervalo) => ctx.request_repaint_after(intervalo),
            None => {}
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
        let view = state.tela_visivel();
        if view == View::Hidden {
            return;
        }

        // Com as configurações abertas vale o rascunho, não o que está salvo:
        // assim o tema muda enquanto se escolhe, antes de salvar.
        let aparencia = if view == View::Settings {
            state.draft.appearance
        } else {
            state.config.appearance
        };
        if tema::definir_escuro(escuro_agora(&aparencia)) {
            ui.ctx().all_styles_mut(tema::estilo);
        }

        // Esc dispensa a janela — é o que todo mundo tenta primeiro numa
        // janela sem decoração e sempre-no-topo, e antes não fazia nada. Na
        // captura de atalho o Esc pertence à captura (ele cancela a escolha da
        // tecla), então ali este atalho não vale.
        if state.capturando.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.act(match view {
                View::Settings => UiAction::CloseSettings,
                View::Historico => UiAction::FecharHistorico,
                _ => UiAction::Hide,
            });
        }

        let opacidade = self.animar_abertura(ui, aparencia);
        self.painel(ui, opacidade);
        ui.multiply_opacity(opacidade);

        let margem = tema::FOLGA_SOMBRA as i8 + 18;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(Margin::same(margem)))
            .show(ui, |ui| match view {
                View::Recording => self.recording(ui, &state),
                View::Processing => self.processing(ui, &state),
                View::Result => self.result(ui, &mut state),
                View::Settings => self.settings(ui, &mut state),
                View::Historico => self.historico(ui, &state),
                View::Error => self.error(ui, &state),
                View::Hidden => {}
            });
    }
}

impl App {
    /// A superfície da janela: sombra, preenchimento e borda.
    ///
    /// No Windows a sombra, o canto arredondado e a borda são do sistema — a
    /// janela ali não é transparente, e desenhá-los por dentro produziria a
    /// moldura opaca que o `tema::FOLGA_SOMBRA` explica. Aqui sobra o
    /// preenchimento, que é o que a janela precisa ter atrás do conteúdo.
    fn painel(&self, ui: &egui::Ui, opacidade: f32) {
        let p = paleta();
        let rect = ui.max_rect().shrink(tema::FOLGA_SOMBRA);
        let painter = ui.ctx().layer_painter(LayerId::background());

        if cfg!(target_os = "windows") {
            painter.rect_filled(rect, CornerRadius::ZERO, p.fundo.gamma_multiply(opacidade));
            return;
        }

        let raio = CornerRadius::same(tema::RAIO_JANELA);
        let mut sombra = tema::sombra_janela();
        sombra.color = sombra.color.gamma_multiply(opacidade);
        painter.add(sombra.as_shape(rect, raio));
        painter.rect_filled(rect, raio, p.fundo.gamma_multiply(opacidade));
        painter.rect_stroke(
            rect,
            raio,
            Stroke::new(1.0, p.borda.gamma_multiply(opacidade)),
            StrokeKind::Inside,
        );
    }

    /// A janela entra subindo um fio e clareando. Devolve a opacidade do quadro.
    ///
    /// O movimento vai numa transformação da camada de fundo, então pega tudo de
    /// uma vez — superfície, texto e controles.
    fn animar_abertura(&mut self, ui: &mut egui::Ui, ap: crate::config::Appearance) -> f32 {
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
            return 1.0;
        }

        // Desaceleração cúbica: sai rápido e encosta devagar, sem ultrapassar.
        let t = 1.0 - (1.0 - x).powi(3);
        ctx.set_transform_layer(
            camada,
            egui::emath::TSTransform::from_translation(Vec2::new(0.0, 10.0 * (1.0 - t))),
        );
        ctx.request_repaint();
        (t * 1.4).min(1.0)
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
        // As telas altas vão para o meio da tela; as baixas, para o rodapé,
        // perto de onde se está escrevendo. Encostar uma janela de 620 pontos
        // no rodapé a empurraria para fora por cima — o `max(0.0)` a salvaria de
        // sair da tela, e ela ficaria com o rodapé (onde estão os botões) abaixo
        // da borda de baixo.
        let ao_centro = matches!(view, View::Settings | View::Historico);
        let y = if ao_centro {
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
        let p = paleta();

        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            let pulso = 0.5 + 0.5 * (tempo * 3.0).sin();
            let painter = ui.painter();
            // Ponto vermelho com um anel que abre e some, como a luz de um
            // gravador. Duas formas, sem desfoque nenhum.
            //
            // O centro é encostado à esquerda da caixa, e não no meio dela: é a
            // borda do ponto que precisa cair na mesma vertical do texto da
            // linha de baixo, não o eixo dele.
            let centro = Pos2::new(rect.left() + 5.0, rect.center().y);
            painter.circle_filled(centro, 5.0, p.gravando);
            painter.circle_stroke(
                centro,
                5.0 + 5.0 * pulso,
                Stroke::new(1.5, p.gravando.gamma_multiply(0.55 * (1.0 - pulso))),
            );

            ui.add_space(4.0);
            ui.label(medio("Ouvindo", 16.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(tema::tecnico(cronometro(decorrido), 14.0).color(p.texto_fraco));
            });
        });

        ui.add_space(12.0);
        self.waveform(ui, tempo);
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(nota("Solte"));
            widgets::keycap(ui, &keys::combo_label(&state.config.hotkey));
            ui.label(nota("para transcrever"));

            // A saída, do lado direito da mesma fileira. Ela precisa existir
            // aqui e não só na tecla: quem grava por alternar — pelo ícone da
            // barra, por um atalho do painel — não tem tecla segurada nenhuma,
            // e antes disto a única forma de desistir era esperar o Whisper
            // produzir um texto que ninguém queria.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let atalho = &state.config.atalho_de_cancelar;
                let dica = if atalho.is_empty() {
                    "Joga fora esta gravação sem transcrever".to_string()
                } else {
                    format!(
                        "Joga fora esta gravação sem transcrever ({})",
                        keys::combo_label(atalho)
                    )
                };
                if widgets::botao(ui, "Descartar")
                    .on_hover_text(dica)
                    .clicked()
                {
                    self.act(UiAction::Cancelar);
                }
            });
        });
    }

    /// As barras do nível do microfone: cinzas no silêncio, vermelhas na voz.
    fn waveform(&mut self, ui: &mut egui::Ui, tempo: f32) {
        let altura = 52.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), altura), Sense::hover());

        let leituras: Vec<f32> = if self.demo.is_some() {
            // No passeio de demonstração não há microfone: a soma de três senos
            // dá a irregularidade de uma frase falada, e depende só do índice —
            // então a imagem do README sai igual toda vez.
            (0..self.bars.len())
                .map(|i| {
                    let x = i as f32;
                    (0.34
                        + 0.30 * (x * 0.9).sin()
                        + 0.18 * (x * 2.3).sin()
                        + 0.10 * (x * 5.1).sin())
                    .clamp(0.04, 0.95)
                })
                .collect()
        } else {
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

        let p = paleta();
        let painter = ui.painter();
        let vao = 4.0;
        let largura = ((rect.width() - vao * (total as f32 - 1.0)) / total as f32).max(1.0);
        let meio = rect.center().y;
        let raio = CornerRadius::same((largura / 2.0) as u8);

        for (i, valor) in self.bars.iter().enumerate() {
            let x = rect.left() + i as f32 * (largura + vao);
            // Onda lenta atravessando as barras: mesmo em silêncio o painel
            // respira, deixando claro que está ouvindo.
            let onda = 0.5 + 0.5 * (tempo * 1.7 + i as f32 * 0.42).sin();
            let repouso = 4.0 + 8.0 * onda;
            let h = (valor * altura * 0.92).max(repouso);
            let barra = Rect::from_min_size(Pos2::new(x, meio - h / 2.0), Vec2::new(largura, h));
            // Só o que está alto de verdade fica vermelho; o resto do tempo a
            // barra é cinza, e a cor vira a informação.
            let cor = widgets::mistura(p.texto_fraco, p.gravando, valor.powf(1.8));
            painter.rect_filled(barra, raio, cor);
        }
    }

    // ----------------------------------------------------------- processando

    /// Quantos segundos de transcrição já são demora, e não trabalho normal.
    ///
    /// Uma frase comum vira texto em menos de um segundo nesta máquina. Seis é
    /// tempo de sobra para não incomodar quem ditou um parágrafo inteiro numa
    /// máquina modesta, e curto o bastante para o aviso chegar antes de a pessoa
    /// concluir que o programa travou.
    const DEMORA_DEMAIS: f64 = 6.0;

    fn processing(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        let agora = ui.input(|i| i.time);
        let tempo = agora as f32;

        // Quando esta transcrição começou, guardado na memória da própria
        // interface e chaveado pelo número do ditado. Assim não é preciso um
        // campo novo no estado compartilhado só para desenhar um aviso — e o
        // valor de um ditado nunca é confundido com o do seguinte, que é
        // justamente o tipo de coisa que este projeto já errou.
        let chave = egui::Id::new(("transcrevendo-desde", state.ditado_atual));
        let inicio = ui
            .ctx()
            .data_mut(|dados| *dados.get_temp_mut_or_insert_with(chave, || agora));

        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
            girando(ui.painter(), rect.center(), 13.0, tempo);

            ui.add_space(12.0);
            ui.label(medio("Transcrevendo…", 15.0));
            if !state.status.is_empty() {
                ui.label(nota(&state.status));
            } else if agora - inicio > Self::DEMORA_DEMAIS {
                // A primeira transcrição depois de instalar leva uns vinte
                // segundos com o backend Vulkan: o driver compila os pipelines
                // de shader antes de rodar qualquer coisa, e guarda o resultado
                // em cache — as seguintes voltam a levar meio segundo. Sem esta
                // linha, o que se vê é um indicador girando por vinte segundos,
                // que é indistinguível de um programa travado.
                ui.label(nota(
                    "A primeira transcrição depois de instalar demora:\n\
                     a placa de vídeo está preparando o modelo. As próximas são rápidas.",
                ));
            }
        });
    }

    // -------------------------------------------------------------- resultado

    fn result(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "resultado");

        ui.horizontal(|ui| {
            ui.label(titulo("Texto transcrito", 16.0));
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

        ui.add_space(8.0);

        // O que sobra depois do cartão: o espaço, a fileira de botões e o mesmo
        // respiro embaixo deles que as configurações têm.
        //
        // O piso não é zelo — sem ele o programa **caía**, e o caminho é comum:
        // ao trocar de tela, o comando de redimensionar a janela é enviado num
        // quadro e atendido no seguinte, então este desenho acontece uma vez com
        // a janela ainda no tamanho da tela anterior. Vindo da gravação (178
        // pontos) para o resultado (372), a sobra dava 92 e a conta abaixo, −6 —
        // e o `set_min_height` do egui entra em pânico com altura negativa
        // ("Negative height makes no sense"). O quadro seguinte já vem com a
        // janela certa; o que faltava era atravessar este.
        let altura_texto = altura_util(ui, 10.0 + widgets::ALTURA + 12.0 + RESPIRO);
        let mut editou = false;
        widgets::cartao(ui, |ui| {
            // O `- 24` é a margem vertical do cartão. O piso é o mesmo do
            // `altura_util`, e pelo mesmo motivo: um quadro só, sem derrubar.
            let dentro = (altura_texto - 24.0).max(24.0);
            ui.set_min_height(dentro);
            // Os dois ramos rolam. O editável não rolava, e o `TextEdit` do
            // egui não tem rolagem própria nem teto de altura: numa fala de uns
            // 45 s (o teto padrão é 120) o campo crescia além da janela, que é
            // de tamanho fixo e não redimensionável, e empurrava "Copiar" e
            // "Copiar e colar" para fora — sem rolagem, sem como alcançá-los, e
            // com o resto do texto invisível. E este é o ramo padrão.
            egui::ScrollArea::vertical()
                .max_height(dentro)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.config.editable_result {
                        editou = ui
                            .add(
                                egui::TextEdit::multiline(&mut state.text)
                                    .desired_width(f32::INFINITY)
                                    // O cartão já é a moldura; o campo entra sem a dele.
                                    .frame(egui::Frame::NONE)
                                    .margin(Margin::ZERO)
                                    .font(egui::TextStyle::Body),
                            )
                            .has_focus();
                    } else {
                        ui.label(RichText::new(&state.text).size(14.5));
                    }
                });
        });

        // Enquanto se digita, o relógio do fechamento automático não corre: a
        // janela sumia no meio de uma correção, e nenhum comando a traz de
        // volta — só o texto original, que já foi para a área de transferência,
        // sobreviveria.
        if editou {
            state.result_shown_at = Some(std::time::Instant::now());
        }

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let copiado = state.copied_at.is_some_and(|t| t.elapsed() < AVISO_COPIADO);

            let principal = Botao::new(if copiado { "Copiado" } else { "Copiar" })
                .principal()
                .largura_minima(120.0);
            if ui.add(principal).clicked() {
                self.act(UiAction::Copy);
            }

            if clipboard::paste_available() && widgets::botao(ui, "Copiar e colar").clicked() {
                self.act(UiAction::Paste);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.message.is_empty() {
                    // `truncate()`, e não um `encurtar` com número fixo: o que
                    // sobra aqui depende dos botões à esquerda, que mudam com o
                    // "Copiar e colar" e com o zoom. Sem ele, uma mensagem
                    // comprida — a do modelo ausente tem o caminho inteiro
                    // dentro — passava por cima dos dois botões, ilegível e
                    // deixando-os ilegíveis.
                    ui.add(
                        egui::Label::new(
                            RichText::new(&state.message)
                                .size(12.5)
                                .color(paleta().erro),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&state.message);
                } else if copiado {
                    ui.label(
                        RichText::new("na área de transferência")
                            .size(12.5)
                            .color(paleta().ok),
                    );
                } else if state.config.auto_copy {
                    ui.label(nota("cópia automática ligada"));
                }
            });
        });
    }

    // -------------------------------------------------------------- histórico

    /// A lista das transcrições guardadas.
    ///
    /// A rede que faltava: até a 0.6 o trabalho inteiro deste programa era
    /// produzir um texto que ele não guardava em lugar nenhum. Aqui ele volta.
    fn historico(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        drag_area(ui, "historico");

        ui.horizontal(|ui| {
            ui.label(titulo("Transcrições", 19.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::botao_icone(ui, Icone::Fechar, "Fechar").clicked() {
                    self.act(UiAction::FecharHistorico);
                }
                if widgets::botao_icone(ui, Icone::Ajustes, "Configurações").clicked() {
                    self.act(UiAction::OpenSettings);
                }
                if !state.historico.is_empty() {
                    widgets::etiqueta(
                        ui,
                        &format!(
                            "{} · {}",
                            state.historico.len(),
                            crate::modelo::tamanho_legivel(state.historico_em_disco)
                        ),
                        paleta().texto_fraco,
                    );
                }
            });
        });

        ui.add_space(8.0);

        let rodape = 10.0 + 1.0 + 14.0 + widgets::ALTURA + ui.spacing().item_spacing.y + RESPIRO;
        let altura = altura_util(ui, rodape);

        if state.historico.is_empty() {
            widgets::cartao(ui, |ui| {
                ui.set_min_height(altura - 24.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(altura / 3.0);
                    ui.label(medio("Nada guardado ainda.", 15.0));
                    ui.add_space(6.0);
                    ui.label(nota(if state.config.historico.ativo {
                        "As transcrições aparecem aqui assim que você ditar a primeira."
                    } else {
                        "O histórico está desligado em Configurações → Histórico."
                    }));
                });
            });
        } else {
            let agora = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            egui::ScrollArea::vertical()
                .max_height(altura)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (indice, entrada) in state.historico.iter().enumerate() {
                        widgets::cartao(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    tema::tecnico(entrada.ha_quanto_tempo(agora), 12.0)
                                        .color(paleta().texto_fraco),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if widgets::botao(ui, "Copiar").clicked() {
                                            self.act(UiAction::CopiarDoHistorico(indice));
                                        }
                                        if entrada.duracao_ms > 0 {
                                            ui.label(nota(format!(
                                                "{:.1} s",
                                                entrada.duracao_ms as f64 / 1000.0
                                            )));
                                        }
                                    },
                                );
                            });
                            ui.add_space(4.0);
                            // A frase inteira, até onde ela ainda serve para
                            // varrer a lista com o olho. Um ditado de dois
                            // minutos é uma parede de texto que empurra todos os
                            // outros para fora da tela — nesse caso vale o
                            // começo, e o resto fica no "Copiar" e no repouso do
                            // ponteiro.
                            const CABE_INTEIRO: usize = 320;
                            let texto = entrada.texto.trim();
                            if texto.chars().count() <= CABE_INTEIRO {
                                ui.label(RichText::new(texto).size(14.0));
                            } else {
                                ui.label(RichText::new(entrada.resumo(CABE_INTEIRO)).size(14.0))
                                    .on_hover_text(texto);
                            }
                        });
                        ui.add_space(6.0);
                    }
                });
        }

        ui.add_space(10.0);
        let (linha, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .rect_filled(linha, CornerRadius::ZERO, paleta().borda);
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            if ui
                .add(Botao::new("Fechar").principal().largura_minima(110.0))
                .clicked()
            {
                self.act(UiAction::FecharHistorico);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!state.historico.is_empty(), |ui| {
                    // Apagar tudo é um botão de perigo e sem confirmação. A
                    // confirmação seria uma segunda janela sobre uma janela sem
                    // decoração e sempre-no-topo, e o que se perde aqui é um
                    // registro de conveniência — não o trabalho de ninguém.
                    if ui
                        .add(Botao::new("Apagar tudo").perigo())
                        .on_hover_text("Apaga as transcrições guardadas e os áudios delas")
                        .clicked()
                    {
                        self.act(UiAction::LimparHistorico);
                    }
                });
                if !state.message.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(encurtar(&state.message, 60))
                            .size(12.5)
                            .color(paleta().erro),
                    )
                    .on_hover_text(&state.message);
                } else if state.copied_at.is_some_and(|t| t.elapsed() < AVISO_COPIADO) {
                    ui.label(
                        RichText::new("na área de transferência")
                            .size(12.5)
                            .color(paleta().ok),
                    );
                }
            });
        });
    }

    // ------------------------------------------------------------------ erro

    fn error(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        drag_area(ui, "erro");

        let carregando = state.model == ModelState::Loading;
        let p = paleta();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
            if carregando {
                girando(
                    ui.painter(),
                    rect.center(),
                    10.0,
                    ui.input(|i| i.time) as f32,
                );
            } else {
                let painter = ui.painter();
                painter.circle_filled(rect.center(), 11.0, p.erro.gamma_multiply(0.16));
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "!",
                    tema::fonte_forte(14.0),
                    p.erro,
                );
            }
            ui.add_space(4.0);
            ui.label(titulo("Ditador", 16.0));
        });

        ui.add_space(10.0);
        ui.label(RichText::new(&state.message).size(14.5));
        ui.add_space(12.0);

        let baixando = self.progresso_do_download(ui, state);
        let oferta = (!baixando)
            .then(|| self.oferta_de_download(state))
            .flatten();

        // Uma fileira só, sempre. A janela desta tela tem altura fixa, e o
        // botão de baixar já morou numa segunda fileira que não cabia.
        ui.horizontal(|ui| {
            match &oferta {
                Some(oferta) => {
                    ui.add_enabled_ui(oferta.baixavel, |ui| {
                        if ui
                            .add(Botao::new(oferta.rotulo.clone()).principal())
                            .on_hover_text(format!(
                                "Baixa {} de huggingface.co para {}",
                                oferta.modelo.nome,
                                crate::modelo::caminho(oferta.modelo.nome).display()
                            ))
                            .clicked()
                        {
                            self.act(UiAction::DownloadModel(oferta.modelo.nome.to_string()));
                        }
                    });
                }
                None => {
                    if !baixando
                        && state.model == ModelState::Failed
                        && ui.add(Botao::new("Tentar de novo").principal()).clicked()
                    {
                        self.act(UiAction::ReloadModel);
                    }
                }
            }
            if !baixando && widgets::botao(ui, "Configurações").clicked() {
                self.act(UiAction::OpenSettings);
            }
            // Durante o download a fileira é "Cancelar" mais "Fechar", e as
            // duas precisam existir: antes a tela saía sem fileira nenhuma e
            // ficava por cima de tudo, sempre-no-topo e sem decoração, pelos
            // cinco a dez minutos do download — bem embaixo de uma frase
            // prometendo "Pode fechar esta janela". Fechar esconde a janela e
            // deixa o download andando, que é o que a frase promete; cancelar é
            // a outra saída, a de quem clicou por engano.
            if baixando && widgets::botao(ui, "Cancelar o download").clicked() {
                self.act(UiAction::CancelDownload);
            }
            if widgets::botao(ui, "Fechar").clicked() {
                self.act(UiAction::Hide);
            }
        });

        if let Some(oferta) = &oferta {
            ui.add_space(6.0);
            ui.label(nota(oferta.nota));
        }
        self.rodape_do_atalho(ui, state);
    }

    /// O aviso de que o atalho global não está funcionando.
    ///
    /// Mora no rodapé, e não no corpo da mensagem, porque ele e o aviso do
    /// modelo faltando nascem juntos numa instalação nova — dividindo um campo
    /// só, um dos dois sumia sem nunca ter sido lido.
    fn rodape_do_atalho(&self, ui: &mut egui::Ui, state: &crate::state::Shared) {
        // Os dois saem juntos quando os dois existem, e é de propósito: um fala
        // do atalho que não responde, o outro da espera que não acaba, e quem
        // tiver os dois problemas precisa saber dos dois.
        for aviso in [&state.aviso_atalho, &state.aviso_desempenho]
            .into_iter()
            .flatten()
        {
            ui.add_space(8.0);
            ui.label(RichText::new(aviso).size(12.5).color(paleta().erro));
        }
    }

    /// A barra do download em curso, se houver um.
    ///
    /// Devolve `true` enquanto ele anda — aí a tela oferece só o "Fechar".
    fn progresso_do_download(&self, ui: &mut egui::Ui, state: &crate::state::Shared) -> bool {
        let Some(andamento) = &state.download else {
            return false;
        };
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
            ui.add_space(10.0);
            return true;
        }

        if let Some(Err(erro)) = &p.fim {
            ui.label(RichText::new(erro).size(12.5).color(paleta().erro));
            ui.add_space(8.0);
        }
        false
    }

    /// Vale oferecer o download do modelo nesta tela?
    ///
    /// Instalação nova: o programa está inteiro, mas o modelo — que tem
    /// centenas de megabytes e não cabe num pacote — ainda não foi baixado. Em
    /// vez de mandar a pessoa para o terminal, o botão resolve aqui.
    ///
    /// Também vale quando o arquivo existe e não abre. Antes a decisão era só
    /// `exists()`, e aí um arquivo truncado trancava a instalação inteira: o
    /// botão sumia, `--baixar-modelo` respondia "já existe", e o único botão
    /// restante recarregava eternamente o mesmo arquivo ruim.
    fn oferta_de_download(&self, state: &crate::state::Shared) -> Option<Oferta> {
        // Qual modelo oferecer: o que a configuração aponta, quando ele é do
        // catálogo, e senão o sugerido para esta máquina. A distinção importa
        // desde que existe um sugerido para CPU — antes, quem escolhesse o
        // modelo leve e apagasse o arquivo recebia a oferta de baixar os 574 MB
        // do padrão de volta.
        let modelo = crate::modelo::qual_e(&state.config.model_path).unwrap_or_else(|| {
            let com_gpu = state.config.use_gpu && stt::GPU_CAPABLE;
            crate::modelo::achar(crate::modelo::sugerido(com_gpu))
                .expect("o modelo sugerido está no catálogo")
        });
        let arquivo_ruim = state.model == ModelState::Failed
            && crate::modelo::qual_e(&state.config.model_path).is_some()
            && state.config.model_path.exists();
        if state.config.model_path.exists() && !arquivo_ruim {
            return None;
        }

        let baixavel = crate::modelo::disponivel();
        let tamanho = crate::modelo::tamanho_legivel(modelo.tamanho);
        Some(Oferta {
            baixavel,
            modelo,
            rotulo: if arquivo_ruim {
                format!("Baixar o modelo de novo ({tamanho})")
            } else {
                format!("Baixar o modelo ({tamanho})")
            },
            nota: if !baixavel {
                "Preciso do curl ou do wget para baixar: sudo apt install curl"
            } else if arquivo_ruim {
                "O arquivo que está aqui não abriu. Baixar de novo por cima dele \
                 costuma resolver."
            } else {
                "É a única coisa que falta. Depois disso tudo roda na sua máquina, \
                 sem internet."
            },
        })
    }

    // --------------------------------------------------------- configurações

    fn settings(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        drag_area(ui, "config");

        ui.horizontal(|ui| {
            ui.label(titulo("Configurações", 19.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::etiqueta(
                    ui,
                    &format!("v{} · {}", env!("CARGO_PKG_VERSION"), stt::BACKEND),
                    paleta().texto_fraco,
                );
            });
        });

        // O rodapé é fixo e a lista fica com o resto. A conta é a altura de tudo
        // que vem depois da lista: o espaço antes da linha, a linha, o espaço
        // depois dela, os botões — mais um espaçamento, que o egui insere
        // sozinho ao fechar a área de rolagem, e o respiro embaixo dos botões.
        let rodape = 10.0 + 1.0 + 14.0 + widgets::ALTURA + ui.spacing().item_spacing.y + RESPIRO;
        egui::ScrollArea::vertical()
            .max_height(altura_util(ui, rodape))
            .show(ui, |ui| {
                self.settings_atalho(ui, state);
                self.settings_aparencia(ui, state);
                self.settings_sistema(ui, state);
                self.settings_microfone(ui, state);
                self.settings_transcricao(ui, state);
                self.settings_dicionario(ui, state);
                self.settings_area_transferencia(ui, state);
                self.settings_historico(ui, state);
                self.settings_sons(ui, state);
                self.settings_desempenho(ui, state);
                self.settings_avancado(ui, state);
                ui.add_space(6.0);
            });

        // Uma linha separando a lista, que é rolável e por isso termina cortada,
        // dos botões que valem para a tela inteira.
        ui.add_space(10.0);
        let (linha, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .rect_filled(linha, CornerRadius::ZERO, paleta().borda);
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            if ui
                .add(Botao::new("Salvar").principal().largura_minima(110.0))
                .clicked()
            {
                self.act(UiAction::ApplyDraft);
            }
            if widgets::botao(ui, "Cancelar").clicked() {
                self.act(UiAction::CloseSettings);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(Botao::new("Encerrar o Ditador").perigo())
                    .on_hover_text("Fecha o aplicativo por completo")
                    .clicked()
                {
                    self.act(UiAction::Quit);
                }
                // Esta tela não desenhava `message` em lugar nenhum, e dois
                // caminhos de erro escrevem nele justamente enquanto ela está
                // visível: o Salvar que não consegue gravar o arquivo e o
                // interruptor de início automático que falha. Nos dois casos a
                // pessoa não via nada — no autostart a chave ainda voltava
                // sozinha, sem explicação.
                if !state.message.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(encurtar(&state.message, 70))
                            .size(12.5)
                            .color(paleta().erro),
                    )
                    .on_hover_text(&state.message);
                }
            });
        });

        self.rodape_do_atalho(ui, state);
    }

    fn settings_atalho(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Atalho");
        widgets::cartao(ui, |ui| {
            widgets::linha(ui, "Segure para falar", |ui| {
                if state.capturando == Some(QualAtalho::Ditar) {
                    ui.label(medio("pressione a combinação…", 14.0));
                    if widgets::botao(ui, "Cancelar").clicked() {
                        self.act(UiAction::CancelHotkeyCapture);
                    }
                } else {
                    let atual = keys::combo_label(&state.draft.hotkey);
                    if ui
                        .add(Botao::new(tema::tecnico(atual, 13.0)))
                        .on_hover_text("Clique e pressione a nova tecla ou combinação")
                        .clicked()
                    {
                        self.act(UiAction::StartHotkeyCapture(QualAtalho::Ditar));
                    }
                }
            });
            ui.add_space(2.0);
            ui.label(nota(
                "A leitura é passiva: a tecla continua funcionando normalmente nos \
                 outros programas. Prefira teclas sem função própria (Pause, F13…). \
                 Esc cancela a captura.",
            ));

            ui.add_space(10.0);
            widgets::linha(ui, "Descartar o ditado", |ui| {
                if state.capturando == Some(QualAtalho::Cancelar) {
                    ui.label(medio("pressione a combinação…", 14.0));
                    if widgets::botao(ui, "Cancelar").clicked() {
                        self.act(UiAction::CancelHotkeyCapture);
                    }
                } else {
                    // Os dois botões diretos existem por causa do Esc: ele é o
                    // padrão daqui e é também a tecla que **desiste** de uma
                    // captura, então não há como escolhê-lo apertando-o. Sem
                    // eles, quem trocasse o atalho uma vez não teria como
                    // voltar ao padrão nem como desligar o recurso.
                    if state.draft.atalho_de_cancelar.is_empty() {
                        if widgets::botao(ui, "Usar Esc").clicked() {
                            self.act(UiAction::DefinirAtalho(
                                QualAtalho::Cancelar,
                                vec!["KEY_ESC".to_string()],
                            ));
                        }
                    } else if widgets::botao(ui, "Desligar").clicked() {
                        self.act(UiAction::DefinirAtalho(QualAtalho::Cancelar, Vec::new()));
                    }

                    let atual = if state.draft.atalho_de_cancelar.is_empty() {
                        "desligado".to_string()
                    } else {
                        keys::combo_label(&state.draft.atalho_de_cancelar)
                    };
                    if ui
                        .add(Botao::new(tema::tecnico(atual, 13.0)))
                        .on_hover_text("Clique e pressione a nova tecla ou combinação")
                        .clicked()
                    {
                        self.act(UiAction::StartHotkeyCapture(QualAtalho::Cancelar));
                    }
                }
            });
            ui.add_space(2.0);
            ui.label(nota(
                "Joga fora a gravação em curso sem transcrever nada — para quando \
                 você apertou sem querer ou se enrolou no meio da frase. Fora de uma \
                 gravação a tecla não faz nada, então o Esc do dia a dia continua o \
                 mesmo.",
            ));
        });
    }

    /// Tema e animação. A mudança vale no quadro seguinte, antes mesmo de
    /// salvar, para dar para ver o que se está escolhendo.
    fn settings_aparencia(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Aparência");
        widgets::cartao(ui, |ui| {
            let ap = &mut state.draft.appearance;

            let opcoes: Vec<(Tema, &str)> = Tema::TODOS.iter().map(|t| (*t, t.nome())).collect();
            widgets::linha(ui, "Tema", |ui| {
                widgets::segmentado(ui, &mut ap.theme, &opcoes);
            });
            ui.add_space(2.0);
            ui.label(nota(match ap.theme {
                Tema::Sistema => {
                    "Acompanha o que estiver escolhido em Configurações → Aparência \
                     do GNOME, conferido toda vez que esta tela abre."
                }
                Tema::Claro => "Fundo branco, texto preto.",
                Tema::Escuro => "Fundo preto, texto branco.",
            }));

            ui.add_space(6.0);
            widgets::interruptor(ui, &mut ap.animation, "Animação ao abrir a janela");
            ui.add_enabled_ui(ap.animation, |ui| {
                let mut ms = ap.animation_ms as i64;
                if widgets::deslizante(ui, &mut ms, 0..=500, "Duração", |v| format!("{v} ms"))
                    .changed()
                {
                    ap.animation_ms = ms as u64;
                }
            });
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
            // A frase vem pronta da plataforma. A tela não tem o que fazer com a
            // diferença entre systemd, autostart do XDG e a chave `Run` do
            // Windows — e um `match` sobre um enum que só existe no Linux
            // colocaria um `cfg` no meio do desenho da interface.
            ui.label(nota(crate::autostart::explicacao()));

            ui.add_space(8.0);
            widgets::interruptor(
                ui,
                &mut state.draft.aviso_de_versao,
                "Avisar quando sair uma versão nova",
            );
            ui.add_space(2.0);
            ui.label(nota(
                "Uma consulta por dia ao GitHub, só para ler o número da última \
                 versão publicada. Nada é enviado — nem a versão que você tem, nem \
                 nada sobre a máquina — e o Ditador não se atualiza sozinho: quem \
                 atualiza é você, pelo mesmo caminho por onde instalou.",
            ));

            // O aviso mora aqui, ao lado da chave que o liga, e não no alto da
            // tela: quem já sabe da versão nova não precisa de um selo
            // acompanhando cada visita às configurações, e quem quiser desligar
            // o aviso encontra as duas coisas juntas.
            if let Some(nova) = state.versao_nova.clone() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    widgets::etiqueta(
                        ui,
                        &format!("Versão {} disponível", nova.versao),
                        paleta().ok,
                    );
                    if widgets::botao(ui, "Copiar o link").clicked() {
                        // Copiar, e não abrir o navegador: abrir exigiria uma
                        // dependência a mais e, na sessão gráfica de quem roda
                        // isto por serviço de usuário, nem sempre funciona. O
                        // link na área de transferência funciona sempre.
                        self.act(UiAction::CopiarOEnderecoDaVersao);
                    }
                    if state.copied_at.is_some_and(|t| t.elapsed() < AVISO_COPIADO) {
                        ui.label(
                            RichText::new("na área de transferência")
                                .size(12.5)
                                .color(paleta().ok),
                        );
                    }
                });
                ui.label(nota(&nova.endereco));
            }
        });
    }

    /// O microfone: qual, qual canal dele, e se ele fica aberto.
    ///
    /// Saiu de dentro de "Transcrição" quando ganhou três controles: o aparelho
    /// não é um detalhe da transcrição, é a metade da frente do programa.
    fn settings_microfone(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Microfone");
        widgets::cartao(ui, |ui| {
            widgets::linha(ui, "Aparelho", |ui| {
                let atual = state
                    .draft
                    .input_device
                    .clone()
                    .unwrap_or_else(|| "Padrão do sistema".to_string());
                let dispositivos = state.devices.clone();
                lista(ui, "microfone", &encurtar(&atual, 40)).show_ui(ui, |ui| {
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

            widgets::linha(ui, "Canal", |ui| {
                let atual = match state.draft.canal_do_microfone {
                    None => "Misturar todos".to_string(),
                    Some(n) => format!("Canal {}", n + 1),
                };
                lista(ui, "canal", &atual).show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut state.draft.canal_do_microfone,
                        None,
                        "Misturar todos",
                    );
                    // Oito é o que cabe numa interface de áudio comum. A
                    // configuração aceita qualquer número editada à mão, e o
                    // `audio.rs` confere contra o que o aparelho tem de verdade.
                    for n in 0u16..8 {
                        ui.selectable_value(
                            &mut state.draft.canal_do_microfone,
                            Some(n),
                            format!("Canal {}", n + 1),
                        );
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(nota(
                "Microfone comum tem um canal só e não precisa disto. Numa interface \
                 de áudio com várias entradas, misturar todas junta o chiado das que \
                 estão vazias à sua voz.",
            ));

            ui.add_space(8.0);
            widgets::interruptor(
                ui,
                &mut state.draft.microfone_sempre_aberto,
                "Manter o microfone aberto",
            );
            ui.label(nota(if state.draft.microfone_sempre_aberto {
                "Apertar a tecla começa a gravar na hora, e os 300 ms anteriores ao \
                 aperto entram junto — é o que impede a primeira sílaba de se perder. \
                 Nada é guardado fora de uma gravação, mas o indicador de \"microfone \
                 em uso\" do sistema fica aceso enquanto o Ditador está no ar."
            } else {
                "O microfone é aberto no instante em que você aperta a tecla, o que \
                 leva de 40 ms a algumas centenas — e a primeira sílaba pode se \
                 perder nesse intervalo."
            }));
        });
    }

    fn settings_transcricao(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Transcrição");
        widgets::cartao(ui, |ui| {
            widgets::linha(ui, "Idioma", |ui| {
                let atual = IDIOMAS
                    .iter()
                    .find(|(code, _)| *code == state.draft.language)
                    .map(|(_, nome)| *nome)
                    .unwrap_or("Personalizado");
                lista(ui, "idioma", atual).show_ui(ui, |ui| {
                    for (code, nome) in IDIOMAS {
                        ui.selectable_value(&mut state.draft.language, code.to_string(), *nome);
                    }
                });
            });

            widgets::interruptor(ui, &mut state.draft.translate, "Traduzir para inglês");

            ui.add_space(6.0);
            widgets::interruptor(
                ui,
                &mut state.draft.aparar_silencio,
                "Aparar o silêncio das pontas",
            );
            ui.add_space(2.0);
            ui.label(nota(
                "Todo ditado começa e termina com silêncio — o tempo entre apertar a \
                 tecla e falar, e entre calar e soltar. É nele que o Whisper inventa \
                 frase (\"Legendas pela comunidade\", \"Obrigado.\"). Cortando as \
                 pontas, ele não tem o que inventar. O silêncio do meio da fala fica \
                 onde está, e na dúvida nada é cortado.",
            ));
        });
    }

    /// Termos próprios: a lista e a régua de quanto o texto pode diferir.
    fn settings_dicionario(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Termos próprios");
        widgets::cartao(ui, |ui| {
            widgets::interruptor(
                ui,
                &mut state.draft.dicionario.ativo,
                "Corrigir termos meus no texto transcrito",
            );
            ui.add_space(4.0);

            ui.add_enabled_ui(state.draft.dicionario.ativo, |ui| {
                ui.label(nota(
                    "Um por linha — nomes, siglas, jargão da sua área. É esta grafia \
                     que vai para o texto.",
                ));
                // Um campo de texto com uma linha por termo, e não uma lista com
                // botões de mais e menos: colar dez termos de uma vez é o que
                // alguém faz na primeira vez que abre esta tela, e uma lista com
                // botões transforma isso em dez cliques.
                let mut texto = state.draft.dicionario.termos.join("\n");
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut texto)
                            .desired_rows(3)
                            .hint_text("Kubernetes\nChargeBee\nSão Paulo")
                            .margin(Margin::symmetric(10, 8))
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    // Sem `trim` de cada linha aqui: quem está digitando pode ter
                    // acabado de apertar Enter, e aparar agora apagaria a linha em
                    // branco embaixo do cursor. Quem apara é o `sanear`, na
                    // gravação.
                    state.draft.dicionario.termos = texto.split('\n').map(str::to_string).collect();
                }

                let mut sensibilidade = por_cento(state.draft.dicionario.sensibilidade);
                if widgets::deslizante(ui, &mut sensibilidade, 50..=100, "Exigência", |v| {
                    if v >= 100 {
                        "exata".to_string()
                    } else {
                        format!("{v}%")
                    }
                })
                .changed()
                {
                    state.draft.dicionario.sensibilidade = de_por_cento(sensibilidade);
                }
                ui.label(nota(
                    "Mais baixo corrige mais e erra mais. Em \"exata\" só a grafia \
                     conta — o que já resolve maiúsculas, acentos e palavras \
                     partidas. Termos de menos de oito letras nunca são \
                     aproximados: uma letra de diferença em nome curto é ambígua \
                     demais.",
                ));
            });
        });
    }

    /// O registro das transcrições.
    fn settings_historico(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Histórico");
        widgets::cartao(ui, |ui| {
            widgets::interruptor(
                ui,
                &mut state.draft.historico.ativo,
                "Guardar as transcrições",
            );
            ui.add_space(2.0);
            ui.label(nota(
                "A rede para quando a colagem cai na janela errada ou você fecha a \
                 janela sem querer. Abra pelo ícone da barra, ou pelo terminal com \
                 ditador --historico.",
            ));

            ui.add_enabled_ui(state.draft.historico.ativo, |ui| {
                ui.add_space(6.0);
                let mut limite = state.draft.historico.limite as i64;
                if widgets::deslizante(ui, &mut limite, 10..=2000, "Guardar as últimas", |v| {
                    v.to_string()
                })
                .changed()
                {
                    state.draft.historico.limite = limite as usize;
                }

                widgets::interruptor(
                    ui,
                    &mut state.draft.historico.guardar_audio,
                    "Guardar também o áudio",
                );
                if state.draft.historico.guardar_audio {
                    ui.label(nota(
                        "Cerca de 2 MB por minuto de fala. Serve para conferir se o \
                         modelo entendeu errado ou se você falou errado — o texto \
                         sozinho já responde \"o que eu falei mesmo?\".",
                    ));
                }
            });

            if widgets::botao(ui, "Ver as transcrições").clicked() {
                self.act(UiAction::AbrirHistorico);
            }
        });
    }

    /// Os avisos sonoros.
    fn settings_sons(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        widgets::secao(ui, "Sons");
        widgets::cartao(ui, |ui| {
            widgets::interruptor(ui, &mut state.draft.sons.ativo, "Avisar por som");
            ui.add_space(2.0);
            ui.label(nota(
                "Um tom subindo quando o microfone abre e um descendo quando o texto \
                 fica pronto — mais um grave se alguma coisa falhar. É a única \
                 confirmação que existe quando a janela de resultado está desligada.",
            ));

            ui.add_enabled_ui(state.draft.sons.ativo, |ui| {
                let mut volume = por_cento(state.draft.sons.volume);
                if widgets::deslizante(ui, &mut volume, 0..=100, "Volume", |v| format!("{v}%"))
                    .changed()
                {
                    state.draft.sons.volume = de_por_cento(volume);
                }
                if widgets::botao(ui, "Ouvir").clicked() {
                    crate::sons::tocar(crate::sons::Som::Inicio, state.draft.sons.volume);
                }
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

            let cola_sozinho = clipboard::paste_available();
            ui.add_enabled_ui(cola_sozinho, |ui| {
                widgets::interruptor(
                    ui,
                    &mut state.draft.auto_paste,
                    "Entregar o texto na janela em foco",
                );
            });
            if !cola_sozinho {
                ui.label(nota(clipboard::COMO_HABILITAR_A_COLAGEM));
            } else if state.draft.auto_paste {
                ui.add_space(4.0);
                widgets::linha(ui, "Como", |ui| {
                    let opcoes: Vec<(MetodoDeColagem, &str)> = MetodoDeColagem::TODOS
                        .iter()
                        .map(|m| (*m, m.nome()))
                        .collect();
                    lista(ui, "metodo-colagem", state.draft.metodo_de_colagem.nome()).show_ui(
                        ui,
                        |ui| {
                            for (metodo, nome) in &opcoes {
                                ui.selectable_value(
                                    &mut state.draft.metodo_de_colagem,
                                    *metodo,
                                    *nome,
                                );
                            }
                        },
                    );
                });
                ui.label(nota(state.draft.metodo_de_colagem.explicacao()));

                ui.add_space(4.0);
                widgets::linha(ui, "Depois de colar, apertar", |ui| {
                    let opcoes: Vec<(TeclaDeEnvio, &str)> =
                        TeclaDeEnvio::TODAS.iter().map(|t| (*t, t.nome())).collect();
                    widgets::segmentado(ui, &mut state.draft.tecla_de_envio, &opcoes);
                });
                if state.draft.tecla_de_envio != TeclaDeEnvio::Nenhuma {
                    ui.label(nota(
                        "Ditar num campo de chat vira falar e soltar: a mensagem já \
                         foi. Em vários programas — Slack, caixas de comentário — \
                         quem envia é o Ctrl+Enter, e o Enter sozinho quebra a linha.",
                    ));
                }

                ui.add_space(4.0);
                ui.label(nota(
                    "Com a entrega automática a janela de resultado não aparece — o \
                     texto vai direto para onde você estava escrevendo.",
                ));
                // As ressalvas da plataforma, ditas antes de a chave valer e não
                // depois de o texto ter ido para a janela errada. Elas não são as
                // mesmas no Linux e no Windows, e por isso quem as escreve é a
                // plataforma.
                ui.label(nota(clipboard::SOBRE_A_COLAGEM));
            }

            ui.add_space(6.0);
            widgets::interruptor(
                ui,
                &mut state.draft.espaco_no_fim,
                "Acrescentar um espaço no fim",
            );
            ui.label(nota(
                "Para ditar duas frases seguidas sem elas grudarem uma na outra.",
            ));

            if let Some(aviso) = clipboard::aviso_da_copia() {
                ui.label(nota(aviso));
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

            let mut threads = state.draft.threads as i64;
            if widgets::deslizante(ui, &mut threads, 1..=16, "Threads de CPU", |v| {
                v.to_string()
            })
            .changed()
            {
                state.draft.threads = threads as i32;
            }

            ui.add_space(6.0);
            widgets::interruptor(
                ui,
                &mut state.draft.descarregar_o_modelo.ativo,
                "Soltar o modelo da memória quando parado",
            );
            ui.add_space(2.0);
            ui.label(nota(
                "O modelo ocupa a memória dele o dia inteiro para trabalhar alguns \
                 minutos por dia. Ligando isto, ele sai quando você para de ditar e \
                 volta no instante em que você aperta a tecla de novo — carregando \
                 enquanto você fala, e não depois. Num ditado de uma palavra logo \
                 após uma pausa longa, dá para sentir.",
            ));
            ui.add_enabled_ui(state.draft.descarregar_o_modelo.ativo, |ui| {
                ui.add_space(4.0);
                widgets::linha(ui, "Depois de", |ui| {
                    let atual = state.draft.descarregar_o_modelo.minutos;
                    lista(ui, "ociosidade", &minutos_por_extenso(atual)).show_ui(ui, |ui| {
                        for minutos in crate::config::Ociosidade::MINUTOS {
                            ui.selectable_value(
                                &mut state.draft.descarregar_o_modelo.minutos,
                                minutos,
                                minutos_por_extenso(minutos),
                            );
                        }
                    });
                });
            });

            ui.add_space(8.0);
            self.escolha_do_modelo(ui, state);
        });
    }

    /// A escolha do modelo: uma lista do catálogo, e o caminho para o resto.
    ///
    /// Isto era um campo de texto com o caminho de um arquivo, e só. Funcionava
    /// para quem já sabia qual arquivo queria e onde ele estava — ou seja, para
    /// quem escreveu o programa. Quem não tem GPU não tinha como descobrir aqui
    /// que existe um modelo três vezes mais leve feito para a máquina dele.
    fn escolha_do_modelo(&self, ui: &mut egui::Ui, state: &mut crate::state::Shared) {
        use crate::modelo::{self, Porte};

        let com_gpu = state.draft.use_gpu && stt::GPU_CAPABLE;
        let escolhido = modelo::qual_e(&state.draft.model_path);

        ui.label("Modelo");
        ui.add_space(4.0);

        let rotulo_atual = match escolhido {
            Some(m) => format!("{} · {}", m.rotulo, modelo::tamanho_legivel(m.tamanho)),
            // Um arquivo de fora do catálogo continua sendo uma escolha
            // legítima — modelo afinado por conta própria, cópia noutra pasta —
            // e a lista precisa mostrar isso em vez de fingir que nada está
            // escolhido.
            None => "Outro arquivo".to_string(),
        };
        lista(ui, "modelo", &rotulo_atual).show_ui(ui, |ui| {
            // Uma dúzia de `exists()` por quadro, e só enquanto a lista está
            // aberta. É consulta ao disco dentro do desenho, que o `programas.rs`
            // deste projeto teve o cuidado de tirar do caminho — a diferença é a
            // ordem de grandeza: lá eram processos `which` a cada quadro da tela
            // inteira; aqui são doze chamadas de sistema baratas, num menu
            // suspenso que fica aberto por segundos. Guardar isto em memória
            // custaria decidir quando invalidar (o download que termina com a
            // lista aberta, o arquivo apagado por fora), e essa é a parte que
            // envelhece torta.
            for m in modelo::CATALOGO {
                let marca = if m.baixado() {
                    "✓"
                } else if m.nome == modelo::sugerido(com_gpu) {
                    "★"
                } else {
                    " "
                };
                let rotulo = format!(
                    "{marca} {} · {} · {}",
                    m.rotulo,
                    modelo::tamanho_legivel(m.tamanho),
                    m.parametros
                );
                let mut escolhido_agora = escolhido.is_some_and(|e| e.nome == m.nome);
                if ui
                    .selectable_value(&mut escolhido_agora, true, rotulo)
                    .clicked()
                {
                    state.draft.model_path = modelo::caminho(m.nome);
                }
            }
        });

        // Relido depois da lista, e não aproveitado de cima: clicar numa linha
        // troca o caminho **neste mesmo quadro**, e tudo daqui para baixo — a
        // nota, o aviso de porte, o botão de baixar — fala do modelo escolhido
        // agora. Com o valor de cima, o primeiro quadro depois do clique
        // mostraria o tamanho de um modelo ao lado do nome de outro.
        let escolhido = modelo::qual_e(&state.draft.model_path);

        // A nota do modelo escolhido, e o aviso de quem escolheu um porte que
        // não combina com o que a máquina vai usar para transcrever. O segundo é
        // o motivo de tudo isto existir: escolher o Large numa build só-CPU é
        // exatamente o caminho para "o Ditador é lento".
        if let Some(m) = escolhido {
            ui.add_space(4.0);
            ui.label(nota(m.nota));
            if m.porte == Porte::Gpu && !com_gpu {
                ui.label(
                    RichText::new(format!(
                        "Sem GPU, este modelo transcreve mais devagar do que você fala. \
                         O sugerido para CPU é o {}.",
                        modelo::achar(modelo::PADRAO_CPU).map_or(modelo::PADRAO_CPU, |s| s.rotulo)
                    ))
                    .size(12.5)
                    .color(paleta().erro),
                );
            }
        }

        // O caminho continua editável, embaixo da lista: escolher pela lista o
        // preenche, e escrever nele à mão desmarca a lista. As duas coisas
        // escrevem no mesmo campo da configuração, que é o que impede as duas
        // metades desta tela de discordarem.
        ui.add_space(6.0);
        let mut caminho = state.draft.model_path.display().to_string();
        if ui
            .add(
                egui::TextEdit::singleline(&mut caminho)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            )
            .on_hover_text("O arquivo do modelo. A lista acima preenche este campo.")
            .changed()
        {
            state.draft.model_path = caminho.into();
        }

        let existe = state.draft.model_path.exists();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if existe {
                    "Arquivo encontrado."
                } else {
                    "Ainda não baixado."
                })
                .size(12.5)
                .color(if existe {
                    paleta().texto_fraco
                } else {
                    paleta().erro
                }),
            );

            // O botão de baixar mora aqui desde que há mais de um modelo: antes,
            // o único caminho para trocar de modelo dentro do programa era
            // escolher o arquivo que ainda não existia, salvar, cair na tela de
            // erro e aceitar a oferta de lá.
            let baixando = state
                .download
                .as_ref()
                .is_some_and(|d| d.lock().unwrap_or_else(|e| e.into_inner()).andando());
            if let Some(m) = escolhido
                && !existe
                && !baixando
            {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(modelo::disponivel(), |ui| {
                        if widgets::botao(
                            ui,
                            format!("Baixar ({})", modelo::tamanho_legivel(m.tamanho)),
                        )
                        .on_hover_text(if modelo::disponivel() {
                            "Baixa de huggingface.co. Pode fechar esta janela — o \
                             download continua."
                        } else {
                            "Preciso do curl ou do wget: sudo apt install curl"
                        })
                        .clicked()
                        {
                            self.act(UiAction::DownloadModel(m.nome.to_string()));
                        }
                    });
                });
            }
        });

        if let Some(andamento) = &state.download {
            let p = andamento.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if p.andando() {
                ui.add_space(6.0);
                widgets::progresso(ui, p.fracao(), "Baixando o modelo");
            }
        }
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
                    .margin(Margin::symmetric(10, 8))
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

            let mut minimo = state.draft.min_recording_ms as i64;
            if widgets::deslizante(ui, &mut minimo, 0..=2000, "Gravação mínima", |v| {
                format!("{v} ms")
            })
            .changed()
            {
                state.draft.min_recording_ms = minimo as u64;
            }

            let mut maximo = state.draft.max_recording_secs as i64;
            if widgets::deslizante(ui, &mut maximo, 10..=600, "Gravação máxima", |v| {
                format!("{v} s")
            })
            .changed()
            {
                state.draft.max_recording_secs = maximo as u64;
            }

            let mut fechar = state.draft.result_timeout_secs as i64;
            if widgets::deslizante(ui, &mut fechar, 0..=120, "Fechar o resultado após", |v| {
                if v == 0 {
                    "nunca".to_string()
                } else {
                    format!("{v} s")
                }
            })
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

/// Indicador de trabalho: um anel apagado com um arco da cor do texto girando
/// por cima.
fn girando(painter: &egui::Painter, centro: Pos2, raio: f32, tempo: f32) {
    const ARCO: f32 = 1.9; // radianos ≈ 110°
    const PASSOS: usize = 14;

    let p = paleta();
    painter.circle_stroke(centro, raio, Stroke::new(2.5, p.superficie_forte));

    let inicio = tempo * 3.2;
    let pontos: Vec<Pos2> = (0..=PASSOS)
        .map(|i| centro + Vec2::angled(inicio + ARCO * i as f32 / PASSOS as f32) * raio)
        .collect();
    painter.add(egui::Shape::line(pontos, Stroke::new(2.5, p.texto)));
}

/// Lista suspensa ocupando o resto da linha.
///
/// Largura pela sobra e texto truncado, e não o contrário: assim todas terminam
/// na mesma vertical, na borda do cartão, em vez de cada uma ter a largura do
/// nome que estiver escolhido. A seta é a nossa (ver `widgets::seta`).
fn lista(ui: &egui::Ui, id: &str, selecionado: &str) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selecionado)
        .width(ui.available_width())
        .wrap_mode(egui::TextWrapMode::Truncate)
        .icon(widgets::seta)
}

/// A altura que sobra para o conteúdo depois de reservar o rodapé, nunca
/// negativa.
///
/// O piso existe porque a janela deste programa é redimensionada por comando: ao
/// trocar de tela, o pedido vai num quadro e é atendido no seguinte, então há
/// sempre um desenho feito com o tamanho da tela anterior. Quando a tela nova é
/// mais alta que a anterior, a sobra desse quadro é menor do que o rodapé pede — e
/// o `set_min_height` do egui entra em pânico com altura negativa, derrubando o
/// programa inteiro.
///
/// O mínimo é pequeno de propósito: ele vale por um quadro, e o que se quer dele é
/// atravessar aquele quadro sem derrubar nada — não desenhar bonito.
fn altura_util(ui: &egui::Ui, rodape: f32) -> f32 {
    const MINIMO: f32 = 24.0;
    (ui.available_height() - rodape).max(MINIMO)
}

/// De quanto em quanto tempo cada tela precisa se redesenhar sozinha.
///
/// `None` é "só quando alguma coisa mudar", que é o estado normal de uma janela
/// parada — o `Sinal` já pede a repintura a cada mudança de estado.
/// `Some(Duration::ZERO)` é "todo quadro", para o que está animando.
///
/// Mora fora do `logic` porque a resposta errada aqui não derruba nada, não
/// aparece em nenhum teste de tela e não deixa rastro no log: ela só faz um
/// número parar de andar. Foi o que aconteceu com a lista de transcrições —
/// ela mostra "há 5 min" ao lado de cada frase e o mesmo aviso de "na área de
/// transferência" que a tela de resultado apaga depois de três segundos, e sem
/// repintura os dois congelavam no instante em que a lista foi aberta, com o
/// aviso verde de uma cópia velha em cima de um botão que ninguém tinha
/// clicado.
/// Quanto tempo o "na área de transferência" fica na tela.
///
/// Uma constante só porque três telas a usam — resultado, transcrições e
/// configurações — e porque a `cadencia_de_repintura` precisa concordar com
/// elas: um aviso que expira numa tela que não se redesenha sozinha fica lá até
/// alguém mexer no mouse.
const AVISO_COPIADO: Duration = Duration::from_secs(3);

fn cadencia_de_repintura(
    view: View,
    modelo_carregando: bool,
    aviso_expirando: bool,
) -> Option<Duration> {
    /// Quatro vezes por segundo: o bastante para nenhum número na tela ficar
    /// mais de um quarto de segundo desatualizado, e longe de custar alguma
    /// coisa numa janela que não está animando.
    const SEM_PRESSA: Duration = Duration::from_millis(250);

    match view {
        // Animação da gravação e do indicador de trabalho.
        View::Recording | View::Processing => Some(Duration::ZERO),
        // A tela de erro também anima, mas só enquanto o modelo carrega: ela
        // desenha o anel girando, e sem repintura contínua ele ficava parado no
        // mesmo ângulo pelos vários segundos dos 574 MB — um indicador de
        // trabalho congelado se lê como aplicativo travado.
        View::Error if modelo_carregando => Some(Duration::ZERO),
        // Estas duas contam tempo na tela sem animar nada: o aviso de "copiado"
        // (três segundos), o fechamento automático do resultado e o "há tanto
        // tempo" de cada transcrição guardada.
        View::Result | View::Historico => Some(SEM_PRESSA),
        // As configurações não animam nada e não contam tempo — exceto nos três
        // segundos em que o "na área de transferência" está na tela, depois de
        // alguém copiar o link da versão nova. Sem esta linha o aviso ficaria lá
        // até a pessoa mexer no mouse, que é o defeito que a lista de
        // transcrições já teve.
        View::Settings if aviso_expirando => Some(SEM_PRESSA),
        View::Hidden | View::Settings | View::Error => None,
    }
}

/// Uma fração de 0 a 1 como a porcentagem inteira que o deslizante mostra.
///
/// **Arredonda, e não trunca.** Escrito como `(fracao * 100.0) as i64`, que é o
/// jeito óbvio, dois valores não voltavam: `0.53f32 * 100.0` dá 52,999998 e
/// `0.59f32 * 100.0` dá 58,999996, e o `as` corta a parte fracionária. Quem
/// escolhesse 53 % de volume — ou 53 de exigência do dicionário — reabria as
/// configurações e via 52, sem nada explicando por quê, e o valor gravado
/// mudava junto no primeiro Salvar seguinte.
/// "5 minutos", "1 hora" — o prazo da ociosidade como se diz em voz alta.
fn minutos_por_extenso(minutos: u64) -> String {
    match minutos {
        1 => "1 minuto".to_string(),
        60 => "1 hora".to_string(),
        n if n % 60 == 0 => format!("{} horas", n / 60),
        n => format!("{n} minutos"),
    }
}

fn por_cento(fracao: f32) -> i64 {
    (fracao * 100.0).round() as i64
}

/// A volta: a porcentagem do deslizante como fração de 0 a 1.
fn de_por_cento(inteiro: i64) -> f32 {
    inteiro as f32 / 100.0
}

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

fn encurtar(texto: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if texto.chars().count() <= max {
        texto.to_string()
    } else {
        let inicio: String = texto.chars().take(max - 1).collect();
        format!("{inicio}…")
    }
}

/// "1:07" — minutos e segundos decorridos.
///
/// A conta é inteira porque `{:.0}` do Rust arredonda em vez de truncar: aos
/// 31 s o mostrador dizia `1:31`, aos 59,7 s dizia `1:60`, e aos 60 s voltava
/// para `1:00`. Ou seja, na segunda metade de cada minuto o número saltava um
/// minuto à frente e depois andava para trás — numa fonte monoespaçada
/// escolhida justamente para o mostrador não dançar.
fn cronometro(decorrido: f32) -> String {
    let total = decorrido.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_porcentagem_dos_deslizantes_vai_e_volta_sem_perder_um_ponto() {
        // O deslizante de volume e o de exigência do dicionário guardam uma
        // fração e mostram um inteiro, então a conversão acontece nos dois
        // sentidos toda vez que a tela abre. Escrita com `as i64`, que trunca,
        // ela perdia um ponto em dois valores: `0.53f32 * 100.0` dá 52,999998 e
        // `0.59f32 * 100.0` dá 58,999996. Quem escolhesse 53 % via 52 ao
        // reabrir, e o valor gravado mudava junto no Salvar seguinte.
        let errados: Vec<i64> = (0..=100)
            .filter(|v| por_cento(de_por_cento(*v)) != *v)
            .collect();
        assert!(
            errados.is_empty(),
            "estes valores não voltam inteiros do deslizante: {errados:?}"
        );
    }

    #[test]
    fn o_cronometro_conta_para_a_frente_e_so_para_a_frente() {
        assert_eq!(cronometro(0.0), "0:00");
        assert_eq!(cronometro(9.4), "0:09");
        // O caso que quebrava: `{:.0}` arredondava e 31 s virava "1:31".
        assert_eq!(cronometro(30.0), "0:30");
        assert_eq!(cronometro(31.0), "0:31");
        assert_eq!(cronometro(59.7), "0:59");
        assert_eq!(cronometro(60.0), "1:00");
        assert_eq!(cronometro(119.9), "1:59");
        assert_eq!(cronometro(3_600.0), "60:00");

        // E nunca anda para trás de um segundo para o outro.
        let mut anterior = String::new();
        for centesimos in 0..12_000u32 {
            let agora = cronometro(centesimos as f32 / 100.0);
            assert!(
                agora >= anterior || anterior.len() != agora.len(),
                "{anterior} → {agora}"
            );
            anterior = agora;
        }
    }

    #[test]
    fn as_telas_que_contam_tempo_se_redesenham_sozinhas() {
        // A lista de transcrições ficou de fora desta conta por um bom tempo, e
        // o sintoma era discreto: o "há 5 min" de cada frase e o aviso verde de
        // "na área de transferência" — que dura três segundos na tela de
        // resultado — congelavam no instante em que a lista foi aberta.
        for tela in [View::Result, View::Historico] {
            assert_eq!(
                cadencia_de_repintura(tela, false, false),
                Some(Duration::from_millis(250)),
                "{tela:?} conta tempo na tela e precisa se redesenhar sozinha"
            );
        }

        // Quem anima repinta todo quadro.
        for tela in [View::Recording, View::Processing] {
            assert_eq!(
                cadencia_de_repintura(tela, false, false),
                Some(Duration::ZERO)
            );
        }
        assert_eq!(
            cadencia_de_repintura(View::Error, true, false),
            Some(Duration::ZERO),
            "o anel da carga do modelo precisa girar"
        );

        // E quem não tem nada andando fica quieto até o estado mudar.
        assert_eq!(cadencia_de_repintura(View::Error, false, false), None);
        assert_eq!(cadencia_de_repintura(View::Settings, false, false), None);
        assert_eq!(cadencia_de_repintura(View::Hidden, false, false), None);

        // Menos as configurações com um "na área de transferência" na tela: o
        // aviso expira em três segundos, e uma tela que não se redesenha o
        // deixaria lá até alguém mexer no mouse. É o mesmo defeito que a lista
        // de transcrições já teve, do outro lado da casa.
        assert_eq!(
            cadencia_de_repintura(View::Settings, false, true),
            Some(Duration::from_millis(250)),
            "o aviso de copiado das configurações precisa sumir sozinho"
        );
        // E isso não vale para quem já está parado por outros motivos.
        assert_eq!(cadencia_de_repintura(View::Hidden, false, true), None);
    }

    #[test]
    fn o_nome_comprido_do_microfone_e_encurtado_sem_estourar() {
        assert_eq!(encurtar("curto", 10), "curto");
        assert_eq!(encurtar("exatamente", 10), "exatamente");
        assert_eq!(encurtar("comprido demais", 10), "comprido …");
        // Contagem por caractere, não por byte: o nome do microfone tem acento.
        assert_eq!(encurtar("áéíóú", 5), "áéíóú");
        assert_eq!(encurtar("áéíóúà", 5), "áéíó…");
        // O `max - 1` não pode estourar quando `max` é zero.
        assert_eq!(encurtar("qualquer coisa", 0), "");
    }
}
