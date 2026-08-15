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

#[cfg(test)]
mod tests {
    use super::*;

    /// O erro do cpal chega ao usuário como está. Um palpite acrescentado aqui
    /// apareceria colado em toda falha de microfone do Linux, inclusive nas que
    /// não têm nada a ver com permissão.
    #[test]
    fn o_erro_do_microfone_no_linux_vai_inteiro_e_sem_palpite() {
        assert!(explicar_falha("Access is denied").is_none());
        assert!(explicar_falha("DeviceNotAvailable").is_none());
    }
}
