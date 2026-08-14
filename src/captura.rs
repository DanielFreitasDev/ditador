//! A tela de verdade, para o vidro ter o que refratar.
//!
//! O `glass_gpu` sempre precisou de uma imagem do que está atrás da janela, e
//! até aqui usava o papel de parede da área de trabalho. Só que o papel de
//! parede *não é* o que está atrás: com um navegador branco embaixo, o vidro
//! continuava mostrando o azul do desktop. Este módulo troca essa imagem pela
//! tela de verdade.
//!
//! # Por que pelo portal
//!
//! Nenhum compositor do Wayland entrega o que está atrás de uma janela para a
//! própria janela, e o GNOME nega a API de captura (`org.gnome.Shell.Screenshot`)
//! a aplicativos comuns desde a versão 41. Sob XWayland o `XGetImage` na raiz
//! responde `BadMatch` — a raiz não tem conteúdo nenhum. O que sobra, e
//! funciona, é o `org.freedesktop.portal.Screenshot` do xdg-desktop-portal, que
//! devolve a tela composta inteira. Custa uns 450 ms por chamada.
//!
//! # Por que a captura é feita com a janela escondida
//!
//! Uma captura tirada com a janela na tela conteria a própria janela, e o vidro
//! passaria a refratar a si mesmo — o espelho infinito. Não há como pedir ao
//! portal que exclua a nossa janela do quadro. Então a captura só acontece em
//! momentos em que a janela comprovadamente não está visível:
//!
//!   * uma na abertura do programa, que fica de reserva; e
//!   * uma depois de cada vez que a janela se esconde, com uma folga para o
//!     compositor terminar a animação de fechamento.
//!
//! O efeito colateral é que a imagem é sempre a de *antes* de a janela abrir, e
//! fica congelada enquanto ela estiver aberta: um vídeo tocando atrás não se
//! mexe no vidro. É o mais perto do certo que dá para chegar sem uma extensão
//! rodando dentro do próprio compositor, que é o caminho que a extensão de
//! GNOME de referência usa e um aplicativo comum não tem.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Folga entre a janela sumir e a captura sair, para o compositor terminar de
/// fechá-la. Abaixo disso a janela ainda aparece no quadro.
const FOLGA_APOS_ESCONDER: Duration = Duration::from_millis(700);

/// Uma tela capturada, já decodificada em RGBA.
pub struct Tela {
    pub pixels: Vec<u8>,
    pub largura: u32,
    pub altura: u32,
}

static ULTIMA: Mutex<Option<Arc<Tela>>> = Mutex::new(None);
static EM_CURSO: AtomicBool = AtomicBool::new(false);
/// Desliga o módulo depois de uma falha: sem portal, não adianta insistir a
/// cada vez que a janela fecha. Volta a valer só no próximo início.
static DESISTIU: AtomicBool = AtomicBool::new(false);
/// Avisa quem quiser que chegou tela nova (o `glass_gpu` releria o fundo).
static AO_CHEGAR: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// A tela mais recente que temos, se houver.
pub fn ultima() -> Option<Arc<Tela>> {
    ULTIMA.lock().ok()?.clone()
}

/// Liga o módulo e tira a primeira captura. Deve ser chamado com a janela
/// ainda escondida, que é o caso na abertura do programa.
pub fn iniciar(ao_chegar: impl Fn() + Send + Sync + 'static) {
    let _ = AO_CHEGAR.set(Box::new(ao_chegar));
    capturar_em_segundo_plano(Duration::ZERO);
}

/// A interface avisa aqui toda vez que troca de tela. Só a ida para
/// `Hidden` interessa: é a única hora em que dá para fotografar sem pegar a
/// própria janela no quadro.
pub fn escondeu() {
    capturar_em_segundo_plano(FOLGA_APOS_ESCONDER);
}

fn capturar_em_segundo_plano(espera: Duration) {
    if DESISTIU.load(Ordering::Relaxed) {
        return;
    }
    // Uma de cada vez: sem isto, fechar a janela várias vezes seguidas
    // empilharia threads paradas esperando o portal.
    if EM_CURSO.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("captura-da-tela".into())
        .spawn(move || {
            if !espera.is_zero() {
                std::thread::sleep(espera);
            }
            match portal::capturar() {
                Ok(tela) => {
                    log::info!(
                        "tela capturada em {}×{} para o vidro refratar",
                        tela.largura,
                        tela.altura
                    );
                    if let Ok(mut guarda) = ULTIMA.lock() {
                        *guarda = Some(Arc::new(tela));
                    }
                    EM_CURSO.store(false, Ordering::Relaxed);
                    if let Some(avisar) = AO_CHEGAR.get() {
                        avisar();
                    }
                }
                Err(e) => {
                    log::warn!(
                        "não consegui capturar a tela ({e}); \
                         o vidro fica com o papel de parede"
                    );
                    DESISTIU.store(true, Ordering::Relaxed);
                    EM_CURSO.store(false, Ordering::Relaxed);
                }
            }
        })
        .inspect_err(|_| EM_CURSO.store(false, Ordering::Relaxed));
}

mod portal {
    use super::Tela;
    use anyhow::{Context, Result, anyhow};
    use std::collections::HashMap;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    const DESTINO: &str = "org.freedesktop.portal.Desktop";
    const CAMINHO: &str = "/org/freedesktop/portal/desktop";

    /// Pede uma captura ao portal e devolve a imagem já decodificada.
    ///
    /// O protocolo do portal é em duas etapas: o método devolve na hora o
    /// caminho de um objeto `Request`, e a resposta de verdade chega depois,
    /// como sinal naquele objeto. O `handle_token` existe para podermos montar
    /// esse caminho antes da chamada e já estar escutando — se a assinatura
    /// fosse feita depois, uma resposta rápida passaria batido.
    pub fn capturar() -> Result<Tela> {
        let conexao = Connection::session().context("sem barramento de sessão")?;

        let unico = conexao
            .inner()
            .unique_name()
            .ok_or_else(|| anyhow!("conexão sem nome único"))?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let ficha = "ditador_vidro";
        let pedido = format!("{CAMINHO}/request/{unico}/{ficha}");

        let escuta = Proxy::new(
            &conexao,
            DESTINO,
            pedido.as_str(),
            "org.freedesktop.portal.Request",
        )
        .context("não consegui escutar o pedido")?;
        let mut respostas = escuta
            .receive_signal("Response")
            .context("não consegui assinar a resposta")?;

        let portal = Proxy::new(&conexao, DESTINO, CAMINHO, "org.freedesktop.portal.Screenshot")
            .context("portal de captura ausente")?;
        let opcoes: HashMap<&str, Value> = HashMap::from([
            ("handle_token", Value::from(ficha)),
            // Sem caixa de diálogo: no GNOME a permissão fica guardada no
            // serviço de permissões e a chamada sai calada.
            ("interactive", Value::from(false)),
        ]);
        let _: OwnedObjectPath = portal
            .call("Screenshot", &("", opcoes))
            .context("o portal recusou a captura")?;

        let resposta = respostas
            .next()
            .ok_or_else(|| anyhow!("o portal fechou sem responder"))?;
        let (codigo, resultados): (u32, HashMap<String, OwnedValue>) = resposta
            .body()
            .deserialize()
            .context("resposta do portal em formato inesperado")?;
        if codigo != 0 {
            return Err(anyhow!("captura negada ou cancelada (código {codigo})"));
        }

        let uri: &str = resultados
            .get("uri")
            .ok_or_else(|| anyhow!("resposta sem o caminho da imagem"))?
            .downcast_ref()
            .context("caminho da imagem em formato inesperado")?;
        ler_e_apagar(uri)
    }

    /// Lê o PNG que o portal gravou e o remove em seguida.
    ///
    /// O apagar não é higiene opcional: o portal do GNOME grava em
    /// `~/Imagens/Screenshot.png`, e uma captura por fechamento de janela
    /// encheria a pasta de imagens do usuário.
    fn ler_e_apagar(uri: &str) -> Result<Tela> {
        let caminho = crate::glass_gpu::caminho_do_uri(uri)
            .ok_or_else(|| anyhow!("caminho da imagem não é um arquivo local: {uri}"))?;

        let lido = image::ImageReader::open(&caminho)
            .with_context(|| format!("não consegui abrir {caminho}"))?
            .with_guessed_format()
            .context("formato da captura irreconhecível")?
            .decode()
            .context("não consegui decodificar a captura");

        let _ = std::fs::remove_file(&caminho);

        let imagem = lido?.to_rgba8();
        Ok(Tela {
            largura: imagem.width(),
            altura: imagem.height(),
            pixels: imagem.into_raw(),
        })
    }
}
