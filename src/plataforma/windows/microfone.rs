//! Traduzir a recusa do microfone para uma frase que resolve o problema.
//!
//! O Windows tem um interruptor de privacidade por aplicativo. Quando ele está
//! desligado, o que o WASAPI devolve é `E_ACCESSDENIED` — que sobe pelo cpal até
//! aqui como um texto em inglês com um `0x80070005` no meio. Mostrar isso a
//! quem só quer ditar é o mesmo que não mostrar nada: o problema tem conserto, o
//! conserto tem três cliques, e nada disso está no HRESULT.
//!
//! A frase é acrescentada ao erro, nunca substitui: o técnico continua vendo o
//! código, e quem não é técnico ganha o caminho. É o mesmo princípio do aviso do
//! grupo `input` no Linux.
//!
//! O `ms-settings:privacy-microphone` é o URI documentado pela Microsoft para
//! essa página das Configurações. Ele não é aberto sozinho — o Ditador não
//! sequestra a tela de ninguém —, só é dito.

/// A ajuda que cabe a este erro, se couber alguma.
pub fn explicar_falha(erro: &str) -> Option<&'static str> {
    let baixo = erro.to_lowercase();
    // O texto vem do sistema, então pode chegar em inglês ou traduzido; o código
    // do HRESULT é o único pedaço que não muda de idioma.
    let negado = baixo.contains("0x80070005")
        || baixo.contains("access is denied")
        || baixo.contains("acesso negado")
        || baixo.contains("access denied")
        || baixo.contains("permission denied");

    negado.then_some(
        "O Windows está bloqueando o acesso ao microfone. Abra Configurações → \
         Privacidade e segurança → Microfone (ms-settings:privacy-microphone), \
         ligue \"Acesso ao microfone\" e deixe que aplicativos da área de \
         trabalho o usem.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_hresult_de_acesso_negado_vira_uma_frase_com_conserto() {
        let ajuda = explicar_falha("BackendSpecific { err: ... (0x80070005) }")
            .expect("o 0x80070005 precisa ser reconhecido em qualquer idioma");
        assert!(ajuda.contains("ms-settings:privacy-microphone"));
    }

    #[test]
    fn o_texto_traduzido_tambem_e_reconhecido() {
        assert!(explicar_falha("Falha ao iniciar: acesso negado").is_some());
        assert!(explicar_falha("Failed to initialize: Access is denied.").is_some());
    }

    #[test]
    fn um_erro_que_nao_e_de_permissao_passa_sem_palpite() {
        assert!(explicar_falha("formato de amostra não suportado: F64").is_none());
        assert!(explicar_falha("DeviceNotAvailable").is_none());
    }
}
