//! Ícones embutidos no binário.
//!
//! Os PNGs vêm de `assets/gerar-icones.py`, que rasteriza os SVGs. Ficam dentro
//! do executável por dois motivos: a janela precisa do ícone antes de qualquer
//! instalação, e a barra superior precisa de uma reserva para quando o tema do
//! sistema não tiver os nossos símbolos (alguém rodando `cargo run` sem ter
//! passado pelo `instalar.sh`).

use crate::state::EstadoPublico;

/// Estados que a barra superior distingue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Pronto,
    Gravando,
    Trabalhando,
    Falhou,
}

impl Estado {
    /// Qual símbolo a barra mostra, para cada estado publicado.
    ///
    /// Carregar o modelo e transcrever chegam ao mesmo desenho: para quem olha a
    /// barra de relance, os dois querem dizer "espere". A distinção entre eles
    /// existe no `EstadoPublico`, para quem tem espaço de sobra para dizê-la —
    /// hoje, a extensão do GNOME.
    ///
    /// A regra de qual estado é qual mora no `EstadoPublico::de`, num lugar só.
    /// Estava escrita duas vezes, e a segunda cópia era exatamente o tipo de
    /// coisa que envelhece torto quando alguém mexe na primeira.
    pub fn do_publico(estado: EstadoPublico) -> Self {
        match estado {
            EstadoPublico::Carregando | EstadoPublico::Transcrevendo => Self::Trabalhando,
            EstadoPublico::Erro => Self::Falhou,
            EstadoPublico::Gravando => Self::Gravando,
            EstadoPublico::Pronto => Self::Pronto,
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

/// Um ícone já decodificado, do tamanho que estiver.
///
/// Existe porque este módulo não pode mais devolver `ksni::Icon`: o ksni é o
/// StatusNotifierItem, que é do Linux, e o Windows precisa dos mesmos pixels
/// para montar um `HICON`. Cada plataforma converte no lado dela — são duas
/// linhas de cópia — e o que atravessa a fronteira é só o mapa de bits.
///
/// O `argb` está em ARGB de 32 bits na ordem de rede (o byte mais alto
/// primeiro), que é o que o protocolo StatusNotifierItem pede. Não é o que o
/// Win32 pede (lá é BGRA, de baixo para cima); a conversão mora em
/// `plataforma::windows`, junto do resto do que é peculiaridade do Windows, e
/// não aqui.
pub struct Bitmap {
    pub largura: u32,
    pub altura: u32,
    pub argb: Vec<u8>,
}

/// Ícones da bandeja, nas duas resoluções que os hospedeiros costumam pedir.
///
/// Só é decodificado quando o estado muda, então não vale a pena guardar.
pub fn bandeja(estado: Estado) -> Vec<Bitmap> {
    estado
        .png()
        .into_iter()
        .filter_map(|bytes| {
            let imagem = decodificar(bytes)?;
            let (largura, altura) = imagem.dimensions();
            let mut argb = Vec::with_capacity(largura as usize * altura as usize * 4);
            for pixel in imagem.pixels() {
                let [r, g, b, a] = pixel.0;
                argb.extend_from_slice(&[a, r, g, b]);
            }
            Some(Bitmap {
                largura,
                altura,
                argb,
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
    use crate::state::{ModelState, View};

    /// O caminho inteiro, do estado bruto ao símbolo — que é o que a barra
    /// percorre a cada mudança.
    fn de(model: ModelState, view: View, gravando: bool) -> Estado {
        Estado::do_publico(EstadoPublico::de(model, view, gravando))
    }

    #[test]
    fn o_icone_da_barra_segue_o_microfone_e_nao_a_tela() {
        // A tela do resultado anterior pode estar por cima de um ditado em
        // andamento. Nesse intervalo o ícone precisa continuar vermelho: ele é
        // a única coisa visível dizendo que o microfone está aberto.
        assert_eq!(de(ModelState::Ready, View::Result, true), Estado::Gravando);
        assert_eq!(de(ModelState::Ready, View::Hidden, true), Estado::Gravando);
        // Sem gravação, quem manda é a tela.
        assert_eq!(
            de(ModelState::Ready, View::Processing, false),
            Estado::Trabalhando
        );
        assert_eq!(de(ModelState::Ready, View::Hidden, false), Estado::Pronto);
        // O estado do modelo ganha de tudo: sem ele não há ditado nenhum.
        assert_eq!(
            de(ModelState::Loading, View::Recording, true),
            Estado::Trabalhando
        );
        assert_eq!(
            de(ModelState::Failed, View::Recording, true),
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
