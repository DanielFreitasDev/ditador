//! Ícones embutidos no binário.
//!
//! Os PNGs vêm de `assets/gerar-icones.py`, que rasteriza os SVGs. Ficam dentro
//! do executável por dois motivos: a janela precisa do ícone antes de qualquer
//! instalação, e a barra superior precisa de uma reserva para quando o tema do
//! sistema não tiver os nossos símbolos (alguém rodando `cargo run` sem ter
//! passado pelo `instalar.sh`).

use crate::state::{ModelState, View};

/// Estados que a barra superior distingue.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Pronto,
    Gravando,
    Trabalhando,
    Falhou,
}

impl Estado {
    pub fn de(model: ModelState, view: View) -> Self {
        match (model, view) {
            (ModelState::Loading, _) => Self::Trabalhando,
            (ModelState::Failed, _) => Self::Falhou,
            (_, View::Recording) => Self::Gravando,
            (_, View::Processing) => Self::Trabalhando,
            _ => Self::Pronto,
        }
    }

    /// Nome no tema de ícones, instalado por `instalar.sh`.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Pronto => "ditador-symbolic",
            Self::Gravando => "ditador-gravando-symbolic",
            Self::Trabalhando => "ditador-carregando-symbolic",
            Self::Falhou => "ditador-falhou-symbolic",
        }
    }

    /// Os mesmos símbolos, já em branco (a barra do GNOME é escura em qualquer
    /// tema), nas duas resoluções que os hospedeiros costumam pedir.
    fn png(self) -> [&'static [u8]; 2] {
        match self {
            Self::Pronto => [
                include_bytes!("../assets/png/bandeja-pronto-22.png"),
                include_bytes!("../assets/png/bandeja-pronto-44.png"),
            ],
            Self::Gravando => [
                include_bytes!("../assets/png/bandeja-gravando-22.png"),
                include_bytes!("../assets/png/bandeja-gravando-44.png"),
            ],
            Self::Trabalhando => [
                include_bytes!("../assets/png/bandeja-carregando-22.png"),
                include_bytes!("../assets/png/bandeja-carregando-44.png"),
            ],
            Self::Falhou => [
                include_bytes!("../assets/png/bandeja-falhou-22.png"),
                include_bytes!("../assets/png/bandeja-falhou-44.png"),
            ],
        }
    }
}

/// Ícone da bandeja em ARGB32, que é o formato do protocolo StatusNotifierItem.
///
/// Só é decodificado quando o estado muda, então não vale a pena guardar.
pub fn bandeja(estado: Estado) -> Vec<ksni::Icon> {
    estado
        .png()
        .into_iter()
        .filter_map(|bytes| {
            let imagem = decodificar(bytes)?;
            let (largura, altura) = imagem.dimensions();
            let mut data = Vec::with_capacity(largura as usize * altura as usize * 4);
            for pixel in imagem.pixels() {
                let [r, g, b, a] = pixel.0;
                // ARGB de 32 bits na ordem de rede (o byte mais alto primeiro).
                data.extend_from_slice(&[a, r, g, b]);
            }
            Some(ksni::Icon {
                width: largura as i32,
                height: altura as i32,
                data,
            })
        })
        .collect()
}

/// Ícone da janela, para o alternador de janelas e a lista de aplicativos.
pub fn janela() -> egui::IconData {
    let vazio = egui::IconData {
        rgba: Vec::new(),
        width: 0,
        height: 0,
    };
    let Some(imagem) = decodificar(include_bytes!("../assets/png/ditador-128.png")) else {
        return vazio;
    };
    let (width, height) = imagem.dimensions();
    egui::IconData {
        rgba: imagem.into_raw(),
        width,
        height,
    }
}

fn decodificar(bytes: &[u8]) -> Option<image::RgbaImage> {
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
        Ok(imagem) => Some(imagem.into_rgba8()),
        Err(e) => {
            log::warn!("ícone embutido ilegível: {e}");
            None
        }
    }
}
