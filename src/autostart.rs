//! Subir junto com a sessão gráfica.
//!
//! Três perguntas, e nenhuma delas depende de sistema operacional:
//!
//! * está armado?
//! * arme (ou desarme);
//! * e o que dizer ao usuário sobre como isso foi feito.
//!
//! O *como* muda bastante — serviço do systemd ou `.desktop` do XDG no Linux,
//! chave `Run` do usuário no Windows —, e mora em `plataforma::autostart`. A
//! terceira pergunta existe justamente para que a tela de configurações não
//! precise saber a diferença: ela pede a frase pronta e a mostra.

/// Está armado para subir com a sessão?
pub fn ligado() -> bool {
    crate::plataforma::autostart::ligado()
}

/// Arma ou desarma.
pub fn definir(ligar: bool) -> anyhow::Result<()> {
    crate::plataforma::autostart::definir(ligar)
}

/// A frase que a tela mostra embaixo do interruptor, explicando por onde o
/// programa vai subir nesta máquina.
pub fn explicacao() -> &'static str {
    crate::plataforma::autostart::explicacao()
}
