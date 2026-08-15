//! O estado do Ditador como o mundo de fora o vê — um retrato só, para todos os
//! frontends.
//!
//! Há hoje três coisas desenhando o Ditador fora deste processo: a extensão do
//! GNOME Shell, o widget do Plasma e o `Ditador.Windows`. As duas primeiras leem
//! pelo D-Bus, a terceira pelo named pipe. O que elas leem é **o mesmo retrato**,
//! e é por isso que ele mora aqui e não dentro de um dos transportes: enquanto
//! esta struct nasceu dentro do `dbus.rs`, acrescentar o Windows significaria
//! copiá-la — e o `CLAUDE.md` deste projeto já diz, sobre o `EstadoPublico`, que
//! duas tabelas do mesmo estado são uma a mais do que se consegue manter iguais.
//!
//! O transporte é que muda: o D-Bus manda propriedade por propriedade, com
//! `PropertiesChanged`; o pipe manda a linha JSON inteira. Os dois saem daqui.
//!
//! ## Por que é um retrato e não uma leitura ao vivo
//!
//! Os métodos do D-Bus rodam na thread da conexão e as linhas do pipe na thread
//! do cliente. Travar o mutex principal em qualquer uma das duas seria deixar
//! quem está de fora decidir quando o controlador anda — um cliente lento
//! seguraria o programa. O retrato é tirado uma vez, sob o mutex, e depois é só
//! dado.

use crate::config::Config;
use crate::state::{EstadoPublico, SharedState, lock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Versão do protocolo de eventos do canal de controle.
///
/// Vai na primeira linha que o assinante recebe (`{"t":"ola","protocolo":1}`) e
/// existe para que um frontend antigo diante de um backend novo — coisa normal
/// quando os dois são instalados separadamente — possa dizer "não entendo esta
/// versão" em vez de interpretar campos errados em silêncio.
///
/// A regra de evolução é a mesma do contrato D-Bus, escrita no `CLAUDE.md`:
/// **acrescentar, nunca renomear**. Campo novo não muda este número; campo que
/// muda de significado, sim.
pub const PROTOCOLO: u32 = 1;

/// O que os frontends publicam, tirado do estado compartilhado.
#[derive(Clone, PartialEq)]
pub struct Retrato {
    pub estado: EstadoPublico,
    pub mensagem: String,
    /// O instante em que a gravação começou, guardado só para reconhecer se a
    /// que está correndo agora é a mesma de antes. Não viaja para lugar nenhum.
    pub inicio: Option<Instant>,
    /// O mesmo instante em milissegundos desde a época, que é o que viaja.
    pub gravando_desde: u64,
    pub modelo: String,
    pub idioma: String,
    pub atalho: String,
}

impl Retrato {
    /// Tira o retrato de agora. O anterior entra porque o `gravando_desde` é
    /// derivado, e derivá-lo de novo daria um número ligeiramente diferente a
    /// cada vez (veja `epoca_ms`) — o que faria quem desenha receber um "a
    /// gravação começou em outro instante" a cada mudança de estado, e reiniciar
    /// o cronômetro no meio da frase.
    pub fn tirar(shared: &SharedState, anterior: Option<&Retrato>) -> Self {
        let estado = lock(shared);
        let inicio = estado.recording_since;
        Self {
            estado: estado.estado_publico(),
            mensagem: estado.message.clone(),
            gravando_desde: match (inicio, anterior) {
                (None, _) => 0,
                // A mesma gravação continua: o valor publicado não muda.
                (Some(i), Some(a)) if a.inicio == Some(i) => a.gravando_desde,
                (Some(i), _) => epoca_ms(i),
            },
            inicio,
            modelo: nome_do_modelo(&estado.config),
            idioma: crate::config::nome_do_idioma(&estado.config.language).to_string(),
            atalho: crate::keys::combo_label(&estado.config.hotkey),
        }
    }

    /// O retrato como uma linha do protocolo do pipe.
    ///
    /// Os nomes saem em camelCase porque quem os lê do outro lado é C#, e é a
    /// convenção que o `System.Text.Json` espera sem configuração nenhuma. O
    /// `estado` sai pelo nome de protocolo do `EstadoPublico` — os mesmos cinco
    /// textos que a extensão do GNOME compara, e pelo mesmo motivo: um frontend
    /// que compara texto não pode ver dois vocabulários.
    pub fn linha_json(&self) -> String {
        serde_json::json!({
            "t": "estado",
            "estado": self.estado.nome(),
            "mensagem": self.mensagem,
            "gravandoDesde": self.gravando_desde,
            "modelo": self.modelo,
            "idioma": self.idioma,
            "atalho": self.atalho,
        })
        .to_string()
    }
}

/// Um `Instant` em milissegundos desde a época.
///
/// O `Instant` do Rust é monotônico e não tem origem conhecida — ele não
/// atravessa transporte nenhum. A conversão é "a hora de agora menos o quanto já
/// se passou", e erra pelos microssegundos entre as duas leituras do relógio:
/// para um cronômetro que conta segundos, é exato.
pub fn epoca_ms(inicio: Instant) -> u64 {
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    agora.saturating_sub(inicio.elapsed()).as_millis() as u64
}

/// O nome curto do modelo: `ggml-large-v3-turbo-q5_0.bin` vira
/// `large-v3-turbo-q5_0`. O prefixo e a extensão são iguais em todos eles, e o
/// que sobra é o que cabe numa linha de menu.
pub fn nome_do_modelo(config: &Config) -> String {
    let Some(nome) = config.model_path.file_stem() else {
        return String::new();
    };
    let nome = nome.to_string_lossy();
    nome.strip_prefix("ggml-").unwrap_or(&nome).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ModelState, Shared, View};
    use std::sync::{Arc, Mutex};

    fn bancada() -> SharedState {
        Arc::new(Mutex::new(Shared::new(Config::default(), Vec::new())))
    }

    #[test]
    fn o_nome_do_modelo_perde_o_ggml_e_a_extensao() {
        let config = Config {
            model_path: "/casa/modelos/ggml-large-v3-turbo-q5_0.bin".into(),
            ..Config::default()
        };
        assert_eq!(nome_do_modelo(&config), "large-v3-turbo-q5_0");

        // Um arquivo que não segue a convenção continua sendo dito por inteiro:
        // o caminho é escolhido pelo usuário e não temos o que prometer sobre ele.
        let outro = Config {
            model_path: "/casa/modelos/meu-modelo.bin".into(),
            ..Config::default()
        };
        assert_eq!(nome_do_modelo(&outro), "meu-modelo");
    }

    #[test]
    fn o_inicio_da_gravacao_nao_danca_enquanto_a_gravacao_e_a_mesma() {
        // O cronômetro dos frontends é desenhado a partir deste número. Se ele
        // mudar no meio da frase, o contador na tela volta para zero.
        let shared = bancada();
        {
            let mut estado = lock(&shared);
            estado.model = ModelState::Ready;
            estado.recording_since = Some(Instant::now());
            estado.view = View::Recording;
        }

        let primeiro = Retrato::tirar(&shared, None);
        assert_eq!(primeiro.estado, EstadoPublico::Gravando);
        assert_ne!(primeiro.gravando_desde, 0);

        // Outra coisa qualquer muda, e o retrato é tirado de novo.
        lock(&shared).message = "algo aconteceu".to_string();
        let segundo = Retrato::tirar(&shared, Some(&primeiro));
        assert_eq!(
            segundo.gravando_desde, primeiro.gravando_desde,
            "o começo da gravação foi recalculado no meio dela"
        );

        // Uma gravação nova é outro começo, e aí o número é recalculado. Os
        // cinco segundos para trás são só para o relógio de parede ter como
        // separar as duas: `Instant::now()` duas vezes seguidas cabe no mesmo
        // milissegundo, e o teste passaria por acidente em vez de por mérito.
        let cinco_segundos_atras = Instant::now() - std::time::Duration::from_secs(5);
        lock(&shared).recording_since = Some(cinco_segundos_atras);
        let terceiro = Retrato::tirar(&shared, Some(&segundo));
        let recuou = segundo.gravando_desde - terceiro.gravando_desde;
        assert!(
            (4_900..=5_100).contains(&recuou),
            "o começo devia ter recuado uns 5 s, e recuou {recuou} ms"
        );

        // E parar zera.
        lock(&shared).recording_since = None;
        let quarto = Retrato::tirar(&shared, Some(&terceiro));
        assert_eq!(quarto.gravando_desde, 0);
        assert_eq!(quarto.estado, EstadoPublico::Pronto);
    }

    #[test]
    fn a_linha_json_leva_o_estado_pelo_nome_de_protocolo() {
        let shared = bancada();
        {
            let mut estado = lock(&shared);
            estado.model = ModelState::Ready;
            estado.message = "com \"aspas\" e \\barra".to_string();
        }
        let linha = Retrato::tirar(&shared, None).linha_json();

        // Uma linha só: o protocolo é uma mensagem por linha, e um `\n` no meio
        // do JSON partiria a mensagem em duas.
        assert!(!linha.contains('\n'), "a linha JSON tem quebra dentro");
        // O texto que o frontend compara é o mesmo do D-Bus.
        assert!(linha.contains("\"estado\":\"pronto\""), "{linha}");
        assert!(linha.contains("\"t\":\"estado\""), "{linha}");
        // E o escape é do serde_json, não nosso — este é o teste de que ninguém
        // resolveu montar o JSON com `format!`.
        let devolta: serde_json::Value =
            serde_json::from_str(&linha).expect("a linha precisa ser JSON válido");
        assert_eq!(devolta["mensagem"], "com \"aspas\" e \\barra");
        assert_eq!(devolta["gravandoDesde"], 0);
    }
}
