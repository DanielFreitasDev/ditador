//! Área de transferência e colagem automática, do lado do Windows.
//!
//! ## A cópia não precisa de nada nosso
//!
//! O `arboard`, que no Linux é a reserva para quando o `wl-copy` não está
//! disponível, no Windows é o caminho principal e único — ele fala
//! `OpenClipboard`/`SetClipboardData` direto, que é a API oficial e não tem
//! concorrente. Por isso `copiar` aqui devolve `Err` de propósito: não é falha,
//! é dizer "não há caminho nativo melhor, use a reserva", e o `crate::clipboard`
//! entende exatamente isso.
//!
//! Escrever um segundo caminho em Win32 cru daria a mesma coisa com mais linhas
//! e mais chances de vazar um handle de clipboard — que, quando vaza, trava a
//! área de transferência do Windows inteiro até o processo morrer.
//!
//! ## A colagem automática não existe aqui, e é uma decisão
//!
//! No Linux o Ditador cola com `ydotool`, que é opcional e que o usuário escolhe
//! instalar. O equivalente no Windows seria `SendInput` sintetizando Ctrl+V, e o
//! prompt desta portabilidade é explícito: *"não introduza `SendInput` novo
//! apenas para 'melhorar'"*.
//!
//! Concordo com a decisão, e por motivos que vão além do pedido:
//!
//! * um Ctrl+V sintético vai para **onde quer que o foco esteja** no instante em
//!   que a transcrição termina, que não é necessariamente onde estava quando a
//!   pessoa começou a falar — ditar uma frase longa e trocar de janela no meio é
//!   comum, e o texto acabaria numa conversa, num campo de senha, num terminal
//!   com uma linha pela metade;
//! * `SendInput` não alcança janelas de integridade mais alta (UIPI). O texto
//!   simplesmente não aparece, sem erro nenhum — e "não funciona às vezes" é
//!   pior do que "não existe";
//! * alguns antivírus tratam injeção de teclado como comportamento de
//!   *keylogger*. O Ditador já lê o teclado globalmente por Raw Input; somar
//!   escrita sintética a isso é o par exato que dispara heurística.
//!
//! Então no Windows a chave de "colar automaticamente" fica desligada e
//! explicada. O texto vai para a área de transferência e o Ctrl+V é da pessoa.

use anyhow::{Result, anyhow};

/// No Windows não há variável de ambiente a preservar antes que o programa mexa
/// nelas — o equivalente Linux existe por causa do `WAYLAND_DISPLAY`, que o modo
/// X11 remove.
pub fn lembrar_o_ambiente() {}

/// Não há caminho nativo melhor que o `arboard`. Veja o comentário do módulo.
pub fn copiar(_texto: &str) -> Result<()> {
    Err(anyhow!("no Windows a cópia vai direto pelo arboard"))
}

pub fn colagem_disponivel() -> bool {
    false
}

pub fn colar() -> Result<()> {
    Err(anyhow!(
        "a colagem automática não existe na versão Windows; o texto está na área \
         de transferência, cole com Ctrl+V"
    ))
}

/// O que dizer na tela quando a colagem automática não está disponível.
pub const COMO_HABILITAR_A_COLAGEM: &str = "A colagem automática não existe no Windows: o texto vai para a área de \
     transferência e você cola com Ctrl+V, na janela que quiser.";

/// A cópia no Windows não tem caminho degradado — ou funciona, ou o erro aparece
/// na hora.
pub fn aviso_da_copia() -> Option<&'static str> {
    None
}
