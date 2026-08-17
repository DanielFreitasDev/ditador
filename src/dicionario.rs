//! Corrige termos próprios no texto já transcrito.
//!
//! O `initial_prompt` que este programa sempre teve é uma *sugestão* ao
//! decodificador: ajuda, e não garante nada. Quem dita "Kubernetes" vinte vezes
//! por dia recebe "cuber netes", "kubernets" e "Kubernetes" em proporções que
//! mudam com o ruído da sala. Aqui a abordagem é outra e as duas convivem bem —
//! o prompt guia o modelo, e este módulo conserta o que saiu.
//!
//! ## A regra, e por que ela é conservadora
//!
//! Cada termo cadastrado vira uma **chave**: minúsculas, sem acento e sem nada
//! que não seja letra ou número. "Charge Bee", "chargebee" e "Charge-Bee" dão a
//! mesma chave. O texto é varrido em janelas de uma a quatro palavras, cada
//! janela vira uma chave do mesmo jeito, e as duas são comparadas.
//!
//! Casando exatamente, a troca é sempre segura: o que muda é a grafia que a
//! pessoa cadastrou — maiúsculas, acentos, espaços. É isso que transforma
//! "sao paulo" em "São Paulo" e "charge bee" em "ChargeBee".
//!
//! Não casando, entra a distância de edição — e aí há um limite que vale
//! explicar, porque ele é a diferença entre uma ferramenta útil e uma que
//! estraga texto bom:
//!
//! **Termo com menos de 8 caracteres na chave só é corrigido por casamento
//! exato.** Uma letra de diferença num termo curto é ambígua demais: "Marcelo" e
//! "Marcela" distam uma letra e são duas pessoas, "correto" e "carreto" distam
//! uma letra e são duas palavras. Não há sensibilidade que separe as duas
//! coisas, porque a informação não está no texto. Termos longos, ao contrário,
//! quase nunca colidem por acidente — "kubernetes" e "cubernetes" só podem ser a
//! mesma coisa.
//!
//! Acima de 8 caracteres o orçamento de erros cresce com o tamanho (um erro até
//! 15 caracteres, dois até 23, três daí para cima) e a `sensibilidade` do
//! usuário serve de piso por cima disso. Em 1,0 ela desliga o casamento
//! aproximado e sobra só o exato.
//!
//! ## Por que não há uma lista pronta de termos
//!
//! Porque ela seria de outra pessoa. Jargão é do ofício de quem dita, e uma
//! lista embutida acertaria os nomes de bibliotecas de programação de quem
//! programa enquanto trocaria nomes próprios de quem escreve sobre outra coisa.
//! A lista nasce vazia e o recurso nasce ligado: cadastrar o primeiro termo é a
//! única coisa que se faz para ele funcionar.

use crate::config::Dicionario;

/// O maior número de palavras que uma janela pode ter.
///
/// Quatro cobre o pior caso realista: um termo de duas palavras que o modelo
/// tenha partido em quatro pedaços. Mais do que isso multiplica o trabalho e
/// abre espaço para casamentos que ninguém pediu.
const MAXIMO_DE_PALAVRAS: usize = 4;

/// Abaixo disto, na chave, o termo só é corrigido por casamento exato. Veja o
/// bloco `//!`: é a regra que impede "Marcela" de virar "Marcelo".
const MINIMO_PARA_APROXIMAR: usize = 8;

/// Aplica o dicionário ao texto transcrito.
///
/// Devolve o texto como está quando não há nada a fazer — desligado, sem termos
/// ou sem casamento —, e por isso pode ser chamado sempre, sem o chamador ter de
/// decidir nada.
pub fn corrigir(texto: &str, dicionario: &Dicionario) -> String {
    if !dicionario.ativo || dicionario.termos.is_empty() || texto.is_empty() {
        return texto.to_string();
    }

    let termos: Vec<Termo> = dicionario
        .termos
        .iter()
        .filter_map(|t| Termo::novo(t))
        .collect();
    if termos.is_empty() {
        return texto.to_string();
    }

    let palavras = palavras_de(texto);
    if palavras.is_empty() {
        return texto.to_string();
    }

    let sensibilidade = dicionario.sensibilidade.clamp(0.5, 1.0);

    // **Todas** as janelas de todas as posições são medidas primeiro, e as
    // trocas são escolhidas da que se parece mais para a que se parece menos,
    // descartando as que se sobrepõem a uma já aceita.
    //
    // A varredura gulosa da esquerda para a direita — que é o que estava aqui —
    // estragava texto bom, e de um jeito instrutivo. Em "usei o kubernetes", a
    // chave da janela "o kubernetes" é "okubernetes", que está a **uma** edição
    // de "kubernetes": acrescentar uma letra é sempre uma edição, mesmo quando
    // a letra é uma palavra inteira. Chegando ao "o" primeiro, a varredura
    // aceitava aquela janela com 0,91 de semelhança e engolia o artigo — sem
    // nunca chegar a experimentar a palavra seguinte sozinha, que casa exato.
    //
    // Medindo tudo antes, o casamento exato (1,0) ganha do aproximado (0,91) e
    // o artigo fica onde estava. O tamanho da janela só desempata entre
    // semelhanças iguais: com "Charge" e "ChargeBee" cadastrados, as duas
    // janelas de "charge bee" casam exato, e aí a maior é a certa.
    let mut candidatos: Vec<Candidato> = Vec::new();
    for i in 0..palavras.len() {
        let maior = MAXIMO_DE_PALAVRAS.min(palavras.len() - i);
        for n in 1..=maior {
            if !janela_contigua(texto, &palavras[i..i + n]) {
                // Uma janela cortada por pontuação encerra as maiores também:
                // todas elas contêm esta.
                break;
            }
            let inicio = palavras[i].inicio;
            let fim = palavras[i + n - 1].fim;
            if let Some((termo, semelhanca)) =
                melhor_termo(&texto[inicio..fim], &termos, sensibilidade)
            {
                candidatos.push(Candidato {
                    primeira_palavra: i,
                    palavras: n,
                    inicio,
                    fim,
                    grafia: termo.escrito_como(&texto[inicio..fim]),
                    semelhanca,
                });
            }
        }
    }

    // Maior semelhança primeiro; empatando, a janela mais longa; persistindo, a
    // que vem antes no texto. `total_cmp` e não `partial_cmp` porque a
    // semelhança é um `f32` vindo de uma divisão: um `NaN`, que nada no tipo
    // impede, faria a ordenação devolver qualquer coisa.
    candidatos.sort_by(|a, b| {
        b.semelhanca
            .total_cmp(&a.semelhanca)
            .then(b.palavras.cmp(&a.palavras))
            .then(a.primeira_palavra.cmp(&b.primeira_palavra))
    });

    let mut ocupadas = vec![false; palavras.len()];
    let mut aceitos: Vec<Candidato> = Vec::new();
    for candidato in candidatos {
        let faixa = candidato.primeira_palavra..candidato.primeira_palavra + candidato.palavras;
        if ocupadas[faixa.clone()].iter().any(|o| *o) {
            continue;
        }
        ocupadas[faixa].fill(true);
        aceitos.push(candidato);
    }
    aceitos.sort_by_key(|c| c.inicio);

    let mut saida = String::with_capacity(texto.len());
    // Onde a cópia do original parou. Tudo entre isto e o começo de uma troca é
    // copiado tal e qual — é assim que a pontuação e os espaços do texto
    // sobrevivem sem este módulo precisar entendê-los.
    let mut copiado_ate = 0usize;
    for aceito in aceitos {
        saida.push_str(&texto[copiado_ate..aceito.inicio]);
        saida.push_str(&aceito.grafia);
        copiado_ate = aceito.fim;
    }
    saida.push_str(&texto[copiado_ate..]);
    saida
}

/// Uma troca possível: onde, por quê e o quanto ela casa.
struct Candidato {
    /// Índice da primeira palavra da janela, para detectar sobreposição.
    primeira_palavra: usize,
    palavras: usize,
    /// Onde a janela começa e acaba no texto, em bytes.
    inicio: usize,
    fim: usize,
    /// O que entra no lugar, já com a maiúscula do original respeitada.
    grafia: String,
    semelhanca: f32,
}

/// Um termo cadastrado, com a chave já calculada.
struct Termo<'a> {
    /// Como a pessoa escreveu. É isto que vai para o texto.
    grafia: &'a str,
    chave: String,
}

impl<'a> Termo<'a> {
    fn novo(grafia: &'a str) -> Option<Self> {
        let chave = chave_de(grafia);
        (!chave.is_empty()).then_some(Self { grafia, chave })
    }

    /// A grafia cadastrada, respeitando a maiúscula inicial do texto original.
    ///
    /// Quem cadastra "ffmpeg" quer "ffmpeg" no meio da frase — mas no começo
    /// dela, onde o modelo escreveu "Ffmpeg" ou "FFmpeg", devolver a minúscula
    /// deixaria a frase começando errado. A regra só sobe a primeira letra, e só
    /// quando o original a tinha maiúscula e o termo não.
    fn escrito_como(&self, original: &str) -> String {
        let original_comeca_maiusculo = original.chars().next().is_some_and(char::is_uppercase);
        let termo_comeca_minusculo = self.grafia.chars().next().is_some_and(char::is_lowercase);
        if !(original_comeca_maiusculo && termo_comeca_minusculo) {
            return self.grafia.to_string();
        }
        let mut chars = self.grafia.chars();
        match chars.next() {
            Some(primeira) => primeira.to_uppercase().collect::<String>() + chars.as_str(),
            None => self.grafia.to_string(),
        }
    }
}

/// Um pedaço do texto que é uma palavra, com onde ele começa e acaba em bytes.
struct Palavra {
    inicio: usize,
    fim: usize,
}

/// As palavras do texto: corridas máximas de letras e números.
///
/// A pontuação fica de fora de propósito. Ela é o que separa as janelas e é o
/// que precisa sobreviver intacto a uma troca — "kubernetes," vira
/// "Kubernetes," porque a vírgula nunca chegou a entrar na conta.
fn palavras_de(texto: &str) -> Vec<Palavra> {
    let mut palavras = Vec::new();
    let mut inicio: Option<usize> = None;
    for (i, c) in texto.char_indices() {
        if c.is_alphanumeric() {
            inicio.get_or_insert(i);
        } else if let Some(comeco) = inicio.take() {
            palavras.push(Palavra {
                inicio: comeco,
                fim: i,
            });
        }
    }
    if let Some(comeco) = inicio {
        palavras.push(Palavra {
            inicio: comeco,
            fim: texto.len(),
        });
    }
    palavras
}

/// As palavras da janela estão separadas só por espaços simples?
///
/// Se houver vírgula, ponto ou quebra de linha entre elas, elas não são um termo
/// partido — são duas coisas diferentes, e juntá-las apagaria a pontuação de
/// quem falou. "Charge, bee" continua sendo "Charge, bee".
fn janela_contigua(texto: &str, palavras: &[Palavra]) -> bool {
    palavras
        .windows(2)
        .all(|par| &texto[par[0].fim..par[1].inicio] == " ")
}

/// O termo que melhor casa com este pedaço de texto, e o quanto ele casa.
///
/// Ganha o de maior semelhança; empatando, o primeiro da lista — que é a ordem
/// em que a pessoa os cadastrou, e portanto a única ordem que ela pode
/// controlar.
fn melhor_termo<'a>(
    trecho: &str,
    termos: &'a [Termo<'a>],
    sensibilidade: f32,
) -> Option<(&'a Termo<'a>, f32)> {
    let chave = chave_de(trecho);
    if chave.is_empty() {
        return None;
    }

    let mut melhor: Option<(&Termo, f32)> = None;
    for termo in termos {
        let Some(semelhanca) = casa(&chave, &termo.chave, sensibilidade) else {
            continue;
        };
        // O texto que já está exatamente na grafia cadastrada não é trocado por
        // ele mesmo: sem isto, toda ocorrência certa entrava na saída por um
        // caminho de reconstrução que não tem por que existir.
        //
        // Continua sendo um casamento perfeito para quem chamou, e é importante
        // que seja: é ele que impede uma janela maior e aproximada de ganhar de
        // um trecho que já está certo.
        if semelhanca >= 1.0 && trecho == termo.grafia {
            return Some((termo, 1.0));
        }
        if melhor.is_none_or(|(_, anterior)| semelhanca > anterior) {
            melhor = Some((termo, semelhanca));
        }
    }
    melhor
}

/// Quanto estas duas chaves se parecem, ou `None` se não podem ser a mesma
/// coisa. 1,0 é casamento exato.
fn casa(texto: &str, termo: &str, sensibilidade: f32) -> Option<f32> {
    if texto == termo {
        return Some(1.0);
    }
    // Sensibilidade no máximo é "só casamento exato", e é a saída de quem quer o
    // dicionário sem nenhum palpite.
    if sensibilidade >= 1.0 {
        return None;
    }
    // Termo curto não tem folga: veja o bloco `//!`.
    let maior = texto.chars().count().max(termo.chars().count());
    if termo.chars().count() < MINIMO_PARA_APROXIMAR {
        return None;
    }
    // Uma diferença de tamanho grande já é distância grande, e a conta abaixo é
    // O(n·m) — sair antes é mais barato do que descobrir isso preenchendo a
    // matriz inteira.
    let orcamento = orcamento_de_erros(maior);
    if texto.chars().count().abs_diff(termo.chars().count()) > orcamento {
        return None;
    }

    let distancia = distancia_de_edicao(texto, termo);
    if distancia > orcamento {
        return None;
    }
    let semelhanca = 1.0 - distancia as f32 / maior as f32;
    (semelhanca >= sensibilidade).then_some(semelhanca)
}

/// Quantos erros de digitação uma chave deste tamanho pode ter.
///
/// Cresce com o tamanho porque a chance de dois termos diferentes colidirem cai
/// com ele: numa chave de trinta caracteres, três letras trocadas ainda deixam
/// vinte e sete iguais, e não existe segundo termo assim por acaso.
fn orcamento_de_erros(tamanho: usize) -> usize {
    match tamanho {
        0..=7 => 0,
        8..=15 => 1,
        16..=23 => 2,
        _ => 3,
    }
}

/// Minúsculas, sem acento, só letras e números.
fn chave_de(texto: &str) -> String {
    texto
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| sem_acento(c).to_lowercase().collect::<Vec<_>>())
        .collect()
}

/// Tira o acento das letras que o português usa.
///
/// É uma tabela, e não uma normalização Unicode de verdade, porque o alfabeto
/// deste programa é conhecido: um `unicode-normalization` inteiro para resolver
/// "São" seria uma dependência a mais para cobrir vinte e poucos caracteres. O
/// que não estiver aqui passa intacto e casa por igualdade, que é o
/// comportamento certo para qualquer outro alfabeto.
fn sem_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'Ç' => 'C',
        'Ñ' => 'N',
        'Ý' => 'Y',
        outro => outro,
    }
}

/// Distância de Levenshtein, em caracteres.
///
/// Duas linhas em vez da matriz inteira: as chaves aqui têm dezenas de
/// caracteres, mas isto roda uma vez por termo por janela por ditado, e a versão
/// de duas linhas custa o mesmo para escrever.
fn distancia_de_edicao(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut anterior: Vec<usize> = (0..=b.len()).collect();
    let mut atual = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        atual[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let troca = usize::from(ca != cb);
            atual[j + 1] = (atual[j] + 1)
                .min(anterior[j + 1] + 1)
                .min(anterior[j] + troca);
        }
        std::mem::swap(&mut anterior, &mut atual);
    }
    anterior[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dicionario(termos: &[&str]) -> Dicionario {
        Dicionario {
            ativo: true,
            termos: termos.iter().map(|t| t.to_string()).collect(),
            sensibilidade: Dicionario::SENSIBILIDADE_PADRAO,
        }
    }

    fn corrige(texto: &str, termos: &[&str]) -> String {
        corrigir(texto, &dicionario(termos))
    }

    #[test]
    fn a_grafia_cadastrada_vence_a_do_modelo() {
        // O caso mais comum e o mais seguro: o texto está certo, só não está
        // escrito como a pessoa escreve.
        assert_eq!(
            corrige("usei o kubernetes ontem", &["Kubernetes"]),
            "usei o Kubernetes ontem"
        );
        assert_eq!(
            corrige("moro em sao paulo", &["São Paulo"]),
            "moro em São Paulo"
        );
        assert_eq!(
            corrige("o ffmpeg converteu", &["FFmpeg"]),
            "o FFmpeg converteu"
        );
    }

    #[test]
    fn um_termo_partido_em_duas_palavras_e_juntado() {
        // O erro que o `initial_prompt` não resolve: o modelo ouviu certo e
        // separou errado.
        assert_eq!(
            corrige("integramos com o charge bee hoje", &["ChargeBee"]),
            "integramos com o ChargeBee hoje"
        );
        // E o contrário: junto quando devia ser separado.
        assert_eq!(
            corrige("moro em saopaulo", &["São Paulo"]),
            "moro em São Paulo"
        );
    }

    #[test]
    fn erros_de_transcricao_em_termos_longos_sao_corrigidos() {
        assert_eq!(
            corrige("subi no cuber netes", &["Kubernetes"]),
            "subi no Kubernetes"
        );
        assert_eq!(
            corrige("o kubernets caiu", &["Kubernetes"]),
            "o Kubernetes caiu"
        );
        assert_eq!(
            corrige("rodando no postgressql", &["PostgreSQL"]),
            "rodando no PostgreSQL"
        );
    }

    #[test]
    fn termo_curto_nao_atropela_palavra_parecida() {
        // A regra do bloco `//!`, e a razão de ela existir. Uma letra de
        // diferença num termo curto é ambígua, e o preço de errar é estragar
        // texto que estava certo.
        assert_eq!(
            corrige("o carreto chegou", &["Correto"]),
            "o carreto chegou"
        );
        assert_eq!(
            corrige("a Marcela ligou", &["Marcelo"]),
            "a Marcela ligou",
            "o termo curto foi aproximado e trocou o nome de outra pessoa"
        );
        assert_eq!(corrige("mandei o vale", &["Vale"]), "mandei o Vale");
    }

    #[test]
    fn a_pontuacao_e_os_espacos_do_original_sobrevivem() {
        assert_eq!(
            corrige("subiu, kubernetes; caiu.", &["Kubernetes"]),
            "subiu, Kubernetes; caiu."
        );
        assert_eq!(
            corrige("  sao paulo  ", &["São Paulo"]),
            "  São Paulo  ",
            "os espaços em volta não são do dicionário e não podem sumir"
        );
        // Uma vírgula entre as duas palavras significa que elas não são um termo
        // partido — juntá-las apagaria a pontuação de quem falou.
        assert_eq!(
            corrige("charge, bee", &["ChargeBee"]),
            "charge, bee",
            "juntou duas palavras que a pontuação separava"
        );
        // Nem quebra de linha.
        assert_eq!(corrige("charge\nbee", &["ChargeBee"]), "charge\nbee");
    }

    #[test]
    fn a_maiuscula_do_comeco_da_frase_e_respeitada() {
        // Quem cadastra "ffmpeg" quer minúscula no meio da frase, mas devolver
        // minúscula no começo dela deixaria a frase começando errado.
        assert_eq!(corrige("Ffmpeg converteu", &["ffmpeg"]), "Ffmpeg converteu");
        assert_eq!(corrige("o ffmpeg", &["ffmpeg"]), "o ffmpeg");
        // O contrário não vale: um termo que já começa maiúsculo entra como está.
        assert_eq!(
            corrige("kubernetes caiu", &["Kubernetes"]),
            "Kubernetes caiu"
        );
    }

    #[test]
    fn o_texto_ja_certo_atravessa_sem_ser_tocado() {
        let texto = "Subimos o Kubernetes em São Paulo com o FFmpeg.";
        assert_eq!(
            corrige(texto, &["Kubernetes", "São Paulo", "FFmpeg"]),
            texto
        );
    }

    #[test]
    fn desligado_ou_sem_termos_o_texto_passa_intacto() {
        let texto = "usei o kubernetes";
        assert_eq!(corrigir(texto, &Dicionario::default()), texto);
        let desligado = Dicionario {
            ativo: false,
            ..dicionario(&["Kubernetes"])
        };
        assert_eq!(corrigir(texto, &desligado), texto);
        // Termo em branco não casa com tudo — casaria, se a chave vazia
        // chegasse à comparação.
        assert_eq!(corrigir(texto, &dicionario(&["", "   "])), texto);
    }

    #[test]
    fn a_sensibilidade_no_maximo_deixa_so_o_casamento_exato() {
        let mut d = dicionario(&["Kubernetes"]);
        d.sensibilidade = 1.0;
        // O aproximado sai…
        assert_eq!(corrigir("subi no cuber netes", &d), "subi no cuber netes");
        // …e o exato fica, que é o que a grafia cadastrada resolve.
        assert_eq!(corrigir("subi no kubernetes", &d), "subi no Kubernetes");
    }

    #[test]
    fn a_janela_maior_ganha_da_menor() {
        // Com "Charge" e "ChargeBee" cadastrados, "charge bee" precisa virar
        // "ChargeBee" inteiro, e não "Charge bee".
        assert_eq!(
            corrige("o charge bee subiu", &["Charge", "ChargeBee"]),
            "o ChargeBee subiu"
        );
    }

    #[test]
    fn varias_ocorrencias_na_mesma_frase() {
        assert_eq!(
            corrige(
                "o kubernetes do sao paulo fala com o kubernetes do rio",
                &["Kubernetes", "São Paulo"]
            ),
            "o Kubernetes do São Paulo fala com o Kubernetes do rio"
        );
    }

    #[test]
    fn a_distancia_de_edicao_conta_o_que_deve() {
        assert_eq!(distancia_de_edicao("", ""), 0);
        assert_eq!(distancia_de_edicao("", "abc"), 3);
        assert_eq!(distancia_de_edicao("abc", ""), 3);
        assert_eq!(distancia_de_edicao("abc", "abc"), 0);
        assert_eq!(distancia_de_edicao("abc", "abd"), 1);
        assert_eq!(distancia_de_edicao("kitten", "sitting"), 3);
        // Em caracteres, e não em bytes: senão todo acento contaria por dois.
        assert_eq!(distancia_de_edicao("são", "sao"), 1);
    }

    #[test]
    fn a_chave_ignora_caixa_acento_e_pontuacao() {
        assert_eq!(chave_de("São Paulo"), "saopaulo");
        assert_eq!(chave_de("Charge-Bee!"), "chargebee");
        assert_eq!(chave_de("ÇÃO"), "cao");
        assert_eq!(chave_de("   "), "");
        assert_eq!(chave_de("C3PO"), "c3po");
        // Alfabeto que a tabela não conhece atravessa inteiro e casa por
        // igualdade, que é o comportamento certo.
        assert_eq!(chave_de("Ωμέγα"), "ωμέγα");
    }

    #[test]
    fn o_orcamento_de_erros_cresce_com_o_tamanho() {
        assert_eq!(orcamento_de_erros(7), 0);
        assert_eq!(orcamento_de_erros(8), 1);
        assert_eq!(orcamento_de_erros(15), 1);
        assert_eq!(orcamento_de_erros(16), 2);
        assert_eq!(orcamento_de_erros(24), 3);
    }

    #[test]
    fn texto_com_acento_e_indices_de_byte_nao_se_atropelam() {
        // As posições das palavras são em bytes e o texto é UTF-8: um índice
        // calculado em caracteres cortaria no meio de um "ã" e derrubaria o
        // programa. Este teste existe para essa fatia.
        assert_eq!(
            corrige("ação, coração e kubernetes à vontade", &["Kubernetes"]),
            "ação, coração e Kubernetes à vontade"
        );
    }

    #[test]
    fn o_texto_vazio_e_o_so_de_pontuacao_nao_derrubam_nada() {
        assert_eq!(corrige("", &["Kubernetes"]), "");
        assert_eq!(corrige("...", &["Kubernetes"]), "...");
        assert_eq!(corrige(" ", &["Kubernetes"]), " ");
    }
}
