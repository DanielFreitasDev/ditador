namespace Ditador.Windows.Modelos;

/// <summary>
/// Em que pé o Ditador está, do ponto de vista de quem desenha.
/// </summary>
/// <remarks>
/// <para>
/// Cinco destes vêm do backend, pelos mesmos textos que a extensão do GNOME e o
/// widget do Plasma leem — <c>carregando</c>, <c>pronto</c>, <c>gravando</c>,
/// <c>transcrevendo</c> e <c>erro</c>. Eles nascem no <c>EstadoPublico</c> do
/// Rust e são protocolo, não rótulo: há teste do lado de lá para que ninguém os
/// mude sem perceber.
/// </para>
/// <para>
/// O sexto, <see cref="Indisponivel"/>, não vem do backend e não poderia vir:
/// ele é justamente a ausência de resposta. Quem o descobre é quem pergunta.
/// </para>
/// <para>
/// Não há um "iniciando" à parte, e a razão está escrita no Rust: neste programa
/// o arranque <em>é</em> a carga do modelo, que começa antes de tudo. Inventar um
/// estado a mais aqui criaria uma diferença entre o que o Windows mostra e o que
/// o GNOME mostra sem nada por baixo que a justificasse.
/// </para>
/// </remarks>
public enum Estado
{
    /// <summary>O backend não está no ar, ou ainda não respondeu.</summary>
    Indisponivel,

    /// <summary>Carregando o modelo — o arranque do Ditador.</summary>
    Carregando,

    /// <summary>Pronto para ditar.</summary>
    Pronto,

    /// <summary>O microfone está aberto.</summary>
    Gravando,

    /// <summary>Transcrevendo o que foi dito.</summary>
    Transcrevendo,

    /// <summary>Alguma coisa falhou; a mensagem diz o quê.</summary>
    Erro,
}

/// <summary>
/// O retrato do Ditador que chega pelo canal de controle.
/// </summary>
/// <param name="Estado">O estado publicado.</param>
/// <param name="Mensagem">Erro ou aviso; vazio quando não há.</param>
/// <param name="GravandoDesde">
/// Quando a gravação em curso começou, em milissegundos desde a época; zero
/// quando não há gravação.
///
/// É a fonte da verdade do cronômetro: quem desenha subtrai deste número e nunca
/// conta o tempo por conta própria. Enquanto a gravação é a mesma, o backend
/// publica sempre o mesmo valor — recalculá-lo daria um número ligeiramente
/// diferente a cada mensagem e o contador voltaria para zero no meio da frase.
/// </param>
/// <param name="Modelo">O modelo em uso, pelo nome curto.</param>
/// <param name="Idioma">O idioma configurado, por extenso.</param>
/// <param name="Atalho">O atalho global, como se escreve numa frase.</param>
public sealed record RetratoDoDitador(
    Estado Estado,
    string Mensagem,
    long GravandoDesde,
    string Modelo,
    string Idioma,
    string Atalho)
{
    /// <summary>O que se mostra antes de o backend responder pela primeira vez.</summary>
    public static RetratoDoDitador Indisponivel { get; } = new(
        Estado.Indisponivel,
        string.Empty,
        0,
        string.Empty,
        string.Empty,
        string.Empty);

    /// <summary>
    /// O nome do estado numa frase curta, para a dica do ícone e para o popup.
    /// </summary>
    public string Descricao => Estado switch
    {
        Estado.Indisponivel => "Indisponível",
        Estado.Carregando => "Carregando o modelo…",
        Estado.Pronto => "Pronto",
        Estado.Gravando => "Gravando",
        Estado.Transcrevendo => "Processando fala…",
        Estado.Erro => "Erro",
        _ => "Desconhecido",
    };

    /// <summary>O microfone está aberto agora?</summary>
    public bool Gravando => Estado == Estado.Gravando;

    /// <summary>
    /// Traduz o texto de protocolo do backend para o enum daqui.
    /// </summary>
    /// <remarks>
    /// Um texto desconhecido vira <see cref="Modelos.Estado.Pronto"/> e não uma
    /// exceção: o backend pode ser mais novo que este frontend — eles são
    /// instalados juntos, mas atualizados separadamente — e um estado novo que
    /// derrubasse a interface seria pior do que um ícone otimista por alguns
    /// segundos.
    /// </remarks>
    public static Estado EstadoDoTexto(string? texto) => texto switch
    {
        "carregando" => Modelos.Estado.Carregando,
        "pronto" => Modelos.Estado.Pronto,
        "gravando" => Modelos.Estado.Gravando,
        "transcrevendo" => Modelos.Estado.Transcrevendo,
        "erro" => Modelos.Estado.Erro,
        _ => Modelos.Estado.Pronto,
    };
}
