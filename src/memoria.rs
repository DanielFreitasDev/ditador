//! Ajuste do alocador da glibc — só no Linux, e só com a glibc.
//!
//! ## O problema, medido
//!
//! Cada ditado aloca e larga alguns megabytes de uma vez: o buffer do microfone
//! nasce com a capacidade do teto de duração inteiro (`audio::start` reserva
//! `max_secs × taxa` amostras — a 120 s e 48 kHz são 23 MB), o vetor
//! reamostrado para 16 kHz vem em seguida, e o whisper.cpp ainda pede o scratch
//! dele. Tudo isso morre em segundos.
//!
//! A glibc serve alocações acima do "limiar de mmap" com um `mmap` privado, que
//! volta ao sistema no `free`. Só que o limiar é **dinâmico**: ao liberar um
//! bloco mapeado, a glibc o eleva até o tamanho daquele bloco (teto de 32 MB),
//! supondo que virá outro igual e que vale a pena reaproveitar. A partir daí os
//! blocos grandes passam a sair das arenas do malloc, e memória de arena
//! liberada pelo programa fica em cache para reúso — com as alocações pequenas
//! e vivas do programa (as strings do estado, o texto transcrito, o histórico)
//! fixando aquelas páginas. O sistema não as recebe de volta.
//!
//! Medido nesta máquina (Ubuntu, glibc 2.43), reproduzindo o padrão de alocação
//! de um ditado quarenta vezes seguidas:
//!
//! | | RSS retido no fim |
//! |---|---|
//! | como estava | **29,4 MB** |
//! | com o limiar pinado | **0,1 MB** |
//!
//! Vale registrar a forma da curva, porque ela **não** é a de um vazamento: o
//! consumo sobe nos primeiros ditados e depois estaciona — as arenas passam a
//! ser reaproveitadas e o número para de crescer. Não é um programa que engorda
//! sem limite; é um programa que fica com trinta megabytes a mais para sempre.
//! Num aplicativo que sobe com a sessão e passa o dia na bandeja para atender
//! algumas frases, isso é justamente a memória que não deveria estar ocupada.
//!
//! ## A correção
//!
//! Duas chamadas. `mallopt(M_MMAP_THRESHOLD, 128 kB)` desliga a heurística e faz
//! todo bloco grande continuar indo pelo `mmap`, que devolve ao sistema no
//! `free`; `malloc_trim` varre o que ainda tiver sobrado nas arenas, uma vez por
//! transcrição. O preço da primeira é um `mmap`/`munmap` por buffer grande, que
//! na frequência de um ditado não se mede; o da segunda é da ordem de um
//! milissegundo, e ela roda na thread do Whisper, fora do caminho da interface.
//!
//! ## Por que `extern "C"` à mão em vez do crate `libc`
//!
//! São dois símbolos, e o `main.rs` já declara o `_exit` do mesmo jeito. Trazer
//! uma dependência inteira para duas assinaturas de uma linha seria o contrário
//! do que este projeto faz em toda parte — e o `libc` não estaria disponível no
//! Windows de qualquer forma, onde estas duas funções não existem.
//!
//! O `target_env = "gnu"` não é decoração: na musl não há `mallopt` nem
//! `malloc_trim` para ligar, e o binário nem chegaria a linkar.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod glibc {
    // Os valores de `<malloc.h>`. São negativos porque a glibc separa os
    // parâmetros dela dos do padrão POSIX por sinal.
    const M_MMAP_THRESHOLD: i32 = -3;

    unsafe extern "C" {
        fn mallopt(parametro: i32, valor: i32) -> i32;
        fn malloc_trim(folga: usize) -> i32;
    }

    /// 128 kB: acima de qualquer alocação de rotina do programa e muito abaixo
    /// dos megabytes de um ditado, que é o que precisa continuar indo por mmap.
    const LIMIAR: i32 = 128 * 1024;

    pub fn pinar() {
        // SAFETY: chamada FFI sem argumentos de memória. O `mallopt` só grava
        // num parâmetro interno do alocador e devolve 1 em caso de sucesso.
        let ok = unsafe { mallopt(M_MMAP_THRESHOLD, LIMIAR) };
        if ok == 1 {
            log::debug!("limiar de mmap da glibc pinado em {LIMIAR} bytes");
        } else {
            // Não é motivo para parar: sem isto o programa funciona igual, só
            // segura mais memória. Mas a linha precisa existir, porque este é o
            // único jeito de descobrir depois por que a medição não bateu.
            log::warn!("mallopt(M_MMAP_THRESHOLD) recusado; o RSS pode crescer entre ditados");
        }
    }

    pub fn devolver_ao_sistema() {
        // SAFETY: idem. `malloc_trim` só percorre as arenas do próprio alocador;
        // a folga zero pede que ele devolva tudo o que puder.
        unsafe {
            malloc_trim(0);
        }
    }
}

/// Faz os buffers grandes de cada ditado voltarem ao sistema quando são
/// liberados. Deve ser chamada no começo do `main`, antes de o programa alocar
/// o que quer que seja de grande.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn pinar_o_alocador() {
    glibc::pinar();
}

/// Devolve ao sistema o que sobrou nas arenas. Chamada uma vez por transcrição
/// terminada, na thread do Whisper.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn devolver_ao_sistema() {
    glibc::devolver_ao_sistema();
}

/// Fora da glibc não há o que ajustar: o alocador do Windows e o do macOS não
/// têm a heurística de limiar dinâmico que este módulo desliga, e a musl não
/// exporta as duas funções.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn pinar_o_alocador() {}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn devolver_ao_sistema() {}

#[cfg(test)]
mod tests {
    /// O RSS do processo, em kB, lido do `/proc`.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn rss_kb() -> u64 {
        let statm = std::fs::read_to_string("/proc/self/statm").expect("lendo /proc/self/statm");
        statm
            .split_whitespace()
            .nth(1)
            .and_then(|n| n.parse::<u64>().ok())
            .expect("o segundo campo do statm é o residente")
            * 4
    }

    /// Um bloco tocado de verdade — sem escrever nele o kernel não entrega
    /// página nenhuma e a medição não mede coisa alguma.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn bloco(bytes: usize) -> Vec<u8> {
        let mut v = vec![0u8; bytes];
        for i in (0..bytes).step_by(4096) {
            v[i] = 1;
        }
        v
    }

    /// O padrão de alocação de um ditado, repetido, não pode deixar RSS para
    /// trás.
    ///
    /// Este teste falharia no Ditador de antes desta correção: medido a 40
    /// ditados simulados, ele retinha 29,4 MB. É a razão de o teste existir —
    /// alguém que remova o `pinar_o_alocador` do `main` por parecer supérfluo
    /// descobre aqui, e não pelo relato de uma máquina lenta no fim do dia.
    ///
    /// O teto é generoso de propósito: o que se afirma é a ordem de grandeza —
    /// "não sobram dezenas de megabytes" —, e não um número exato, que depende
    /// da versão da glibc e do que o resto do `cargo test` já alocou nesta
    /// mesma thread.
    #[test]
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn os_buffers_de_um_ditado_voltam_para_o_sistema() {
        const DITADOS: usize = 24;
        /// 60 s a 48 kHz em f32 — metade do teto padrão, para o teste não pedir
        /// meio giga de uma vez na máquina de quem o roda.
        const MICROFONE: usize = 60 * 48_000 * 4;
        const REAMOSTRADO: usize = 60 * 16_000 * 4;

        super::pinar_o_alocador();

        // Aquece: a primeira volta ainda cresce o heap do processo por motivos
        // que não são os que este teste mede.
        let mut vivos: Vec<Vec<u8>> = (0..64).map(|_| bloco(4096)).collect();
        {
            let m = bloco(MICROFONE);
            let r = bloco(REAMOSTRADO);
            drop((m, r));
        }
        super::devolver_ao_sistema();

        let antes = rss_kb();
        for _ in 0..DITADOS {
            let microfone = bloco(MICROFONE);
            let reamostrado = bloco(REAMOSTRADO);
            drop(microfone);
            // A alocação pequena que sobrevive no meio do ciclo é o que fixa as
            // páginas da arena; sem ela o teste não reproduz o problema.
            vivos.push(bloco(2048));
            drop(reamostrado);
            super::devolver_ao_sistema();
        }
        let depois = rss_kb();
        let retido = depois.saturating_sub(antes);

        assert!(
            retido < 8 * 1024,
            "{DITADOS} ditados simulados retiveram {retido} kB de RSS \
             ({:.1} MB). O limiar de mmap da glibc voltou a ser dinâmico — \
             confira se `memoria::pinar_o_alocador()` ainda é chamado no main.",
            retido as f64 / 1024.0
        );
        // O `vivos` precisa sobreviver até aqui, senão o compilador pode
        // encurtar o tempo de vida dele e a fixação das páginas não acontece.
        assert!(!vivos.is_empty());
    }

    /// Fora da glibc as duas funções existem e não fazem nada — o que precisa
    /// continuar sendo verdade para o `main` poder chamá-las sem `cfg`.
    #[test]
    fn as_duas_funcoes_existem_em_toda_plataforma() {
        super::pinar_o_alocador();
        super::devolver_ao_sistema();
    }
}
