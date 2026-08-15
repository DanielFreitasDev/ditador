using Ditador.Windows.Modelos;
using Xunit;

namespace Ditador.Windows.Testes;

/// <summary>
/// A leitura do fluxo que o Ditador manda depois do <c>assinar</c>.
/// </summary>
/// <remarks>
/// As linhas usadas aqui são cópias literais do que o backend em Rust escreve —
/// tiradas do fluxo de verdade, não inventadas. É isso que faz destes testes um
/// contrato, e não um espelho das nossas próprias suposições.
/// </remarks>
public class LeituraDoProtocolo
{
    [Fact]
    public void A_apresentacao_traz_a_versao_do_protocolo()
    {
        var mensagem = MensagemDoDitador.Ler(
            """{"aplicativo":"ditador","backend":"Vulkan","protocolo":1,"t":"ola","versao":"0.5.0"}""");

        var ola = Assert.IsType<MensagemDoDitador.Ola>(mensagem);
        Assert.Equal(1, ola.Protocolo);
        Assert.Equal("ditador", ola.Aplicativo);
        Assert.Equal("0.5.0", ola.Versao);
        Assert.Equal("Vulkan", ola.Backend);
    }

    [Fact]
    public void O_estado_chega_inteiro()
    {
        var mensagem = MensagemDoDitador.Ler(
            """{"atalho":"Pause/Break","estado":"pronto","gravandoDesde":0,"idioma":"Português","mensagem":"","modelo":"large-v3-turbo-q5_0","t":"estado"}""");

        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(Modelos.Estado.Pronto, estado.Retrato.Estado);
        Assert.Equal("Pause/Break", estado.Retrato.Atalho);
        Assert.Equal("large-v3-turbo-q5_0", estado.Retrato.Modelo);
        Assert.Equal("Português", estado.Retrato.Idioma);
        Assert.Equal(0, estado.Retrato.GravandoDesde);
        Assert.False(estado.Retrato.Gravando);
    }

    [Theory]
    [InlineData("carregando", Modelos.Estado.Carregando)]
    [InlineData("pronto", Modelos.Estado.Pronto)]
    [InlineData("gravando", Modelos.Estado.Gravando)]
    [InlineData("transcrevendo", Modelos.Estado.Transcrevendo)]
    [InlineData("erro", Modelos.Estado.Erro)]
    public void Os_cinco_nomes_de_estado_sao_o_contrato(string texto, Modelos.Estado esperado)
    {
        // Estes cinco textos nascem no `EstadoPublico::nome` do Rust e são lidos
        // também pela extensão do GNOME e pelo widget do Plasma. Há teste do
        // outro lado garantindo que eles não mudem; este é o par dele.
        var mensagem = MensagemDoDitador.Ler($$"""{"t":"estado","estado":"{{texto}}"}""");
        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(esperado, estado.Retrato.Estado);
    }

    [Fact]
    public void O_comeco_da_gravacao_atravessa_como_numero_grande()
    {
        // Milissegundos desde a época não cabem em 32 bits desde 1970 + 24 dias.
        // Se este campo for lido como `int`, o cronômetro nasce com uma data
        // aleatória — e é o tipo de defeito que só aparece em produção.
        var mensagem = MensagemDoDitador.Ler(
            """{"t":"estado","estado":"gravando","gravandoDesde":1786000000000}""");

        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(1786000000000L, estado.Retrato.GravandoDesde);
        Assert.True(estado.Retrato.Gravando);
    }

    [Fact]
    public void O_nivel_do_microfone_e_um_numero_entre_zero_e_um()
    {
        var mensagem = MensagemDoDitador.Ler("""{"t":"nivel","valor":0.42}""");
        var nivel = Assert.IsType<MensagemDoDitador.Nivel>(mensagem);
        Assert.Equal(0.42, nivel.Valor, precision: 5);
    }

    [Fact]
    public void Um_campo_a_mais_nao_atrapalha()
    {
        // A regra de evolução do protocolo é "acrescentar, nunca renomear". Um
        // backend mais novo mandando um campo que este frontend não conhece
        // precisa continuar sendo entendido no que ele tem de conhecido — senão
        // atualizar um lado quebraria o outro, que é justamente o que a regra
        // existe para evitar.
        var mensagem = MensagemDoDitador.Ler(
            """{"t":"estado","estado":"gravando","novidade":{"qualquer":[1,2,3]},"atalho":"Pause"}""");

        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(Modelos.Estado.Gravando, estado.Retrato.Estado);
        Assert.Equal("Pause", estado.Retrato.Atalho);
    }

    [Fact]
    public void Um_campo_a_menos_vira_vazio_e_nao_excecao()
    {
        var mensagem = MensagemDoDitador.Ler("""{"t":"estado"}""");
        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(string.Empty, estado.Retrato.Modelo);
        Assert.Equal(string.Empty, estado.Retrato.Mensagem);
        Assert.Equal(0, estado.Retrato.GravandoDesde);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData(null)]
    [InlineData("isto não é json")]
    [InlineData("{ isto tampouco }")]
    [InlineData("[1,2,3]")]
    [InlineData("""{"t":"algo-que-ainda-nao-existe"}""")]
    [InlineData("""{"sem":"tipo"}""")]
    public void O_que_nao_se_entende_e_ignorado_em_silencio(string? linha)
    {
        // Nunca lançar é requisito, e não zelo: uma exceção aqui derrubaria a
        // conexão do frontend com o backend, e o usuário perderia o ícone da
        // barra por causa de uma linha malformada.
        Assert.Null(MensagemDoDitador.Ler(linha));
    }

    [Fact]
    public void O_texto_do_erro_atravessa_com_acento_e_aspas()
    {
        // O Ditador é um programa em português e as mensagens dele vêm com
        // acento; o JSON do backend é escrito pelo serde, que escapa o que
        // precisa. Se a leitura estragar isso, o que aparece na tela do usuário é
        // um erro sobre um erro.
        var mensagem = MensagemDoDitador.Ler(
            """{"t":"estado","estado":"erro","mensagem":"Não achei o \"microfone\" configurado"}""");

        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal("Não achei o \"microfone\" configurado", estado.Retrato.Mensagem);
    }

    [Fact]
    public void Um_estado_desconhecido_nao_derruba_a_interface()
    {
        // Um Ditador mais novo pode publicar um estado que este frontend não
        // conhece. Mostrar "pronto" por alguns segundos é melhor do que fechar o
        // programa — e o `ola` já avisou no log que as versões não batem.
        var mensagem = MensagemDoDitador.Ler("""{"t":"estado","estado":"hibernando"}""");
        var estado = Assert.IsType<MensagemDoDitador.Estado>(mensagem);
        Assert.Equal(Modelos.Estado.Pronto, estado.Retrato.Estado);
    }

    [Fact]
    public void A_descricao_do_estado_e_uma_frase_para_a_tela()
    {
        Assert.Equal("Gravando", new RetratoDoDitador(
            Modelos.Estado.Gravando, "", 0, "", "", "").Descricao);
        Assert.Equal("Indisponível", RetratoDoDitador.Indisponivel.Descricao);
    }
}
