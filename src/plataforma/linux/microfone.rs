//! Traduzir a recusa do microfone para uma frase que resolve o problema — que,
//! no Linux, não é preciso.
//!
//! Não há interruptor de privacidade por aplicativo aqui. Quando o microfone não
//! abre, o motivo é o dispositivo ter sumido, estar ocupado em modo exclusivo ou
//! o PipeWire não estar de pé — e o erro que o cpal devolve já diz qual dos três,
//! em texto que o usuário consegue procurar. Inventar um palpite por cima disso
//! só atrapalharia.
//!
//! O aviso que o Linux realmente precisa dar é outro, e já existe: o do grupo
//! `input` para o atalho global (veja `Shared::aviso_atalho`).

/// Nenhuma ajuda a acrescentar nesta plataforma.
pub fn explicar_falha(_erro: &str) -> Option<&'static str> {
    None
}
