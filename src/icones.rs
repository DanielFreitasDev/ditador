//! Ícones embutidos no binário.
//!
//! Os PNGs vêm de `assets/gerar-icones.py`, que rasteriza os SVGs. Ficam dentro
//! do executável por dois motivos: a janela precisa do ícone antes de qualquer
//! instalação, e a barra superior precisa de uma reserva para quando o tema do
//! sistema não tiver os nossos símbolos (alguém rodando `cargo run` sem ter
//! passado pelo `instalar.sh`).

use crate::state::{ModelState, View};

/// Estados que a barra superior distingue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Pronto,
    Gravando,
    Trabalhando,
    Falhou,
}

impl Estado {
    /// Qual símbolo a barra mostra.
    ///
    /// `gravando` vem do `recording_since` e ganha da tela de propósito: a
    /// janela do resultado anterior pode estar por cima de um ditado em
    /// andamento, e nesse intervalo o ícone precisa continuar vermelho — é a
    /// única coisa na tela dizendo que o microfone está aberto.
    pub fn de(model: ModelState, view: View, gravando: bool) -> Self {
        match (model, gravando, view) {
            (ModelState::Loading, _, _) => Self::Trabalhando,
            (ModelState::Failed, _, _) => Self::Falhou,
            (_, true, _) => Self::Gravando,
            (_, _, View::Processing) => Self::Trabalhando,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_icone_da_barra_segue_o_microfone_e_nao_a_tela() {
        // A tela do resultado anterior pode estar por cima de um ditado em
        // andamento. Nesse intervalo o ícone precisa continuar vermelho: ele é
        // a única coisa visível dizendo que o microfone está aberto.
        assert_eq!(
            Estado::de(ModelState::Ready, View::Result, true),
            Estado::Gravando
        );
        assert_eq!(
            Estado::de(ModelState::Ready, View::Hidden, true),
            Estado::Gravando
        );
        // Sem gravação, quem manda é a tela.
        assert_eq!(
            Estado::de(ModelState::Ready, View::Processing, false),
            Estado::Trabalhando
        );
        assert_eq!(
            Estado::de(ModelState::Ready, View::Hidden, false),
            Estado::Pronto
        );
        // O estado do modelo ganha de tudo: sem ele não há ditado nenhum.
        assert_eq!(
            Estado::de(ModelState::Loading, View::Recording, true),
            Estado::Trabalhando
        );
        assert_eq!(
            Estado::de(ModelState::Failed, View::Recording, true),
            Estado::Falhou
        );
    }

    #[test]
    fn cada_estado_tem_um_simbolo_proprio_e_um_mapa_de_bits_de_reserva() {
        let mut nomes = std::collections::HashSet::new();
        for estado in [
            Estado::Pronto,
            Estado::Gravando,
            Estado::Trabalhando,
            Estado::Falhou,
        ] {
            assert!(
                nomes.insert(estado.nome()),
                "nome repetido: {}",
                estado.nome()
            );
            // Os PNGs são embutidos com include_bytes!: se algum não decodificar,
            // o ícone da barra some sem que nada quebre na compilação.
            assert_eq!(
                bandeja(estado).len(),
                2,
                "faltou resolução em {}",
                estado.nome()
            );
        }
    }
}
