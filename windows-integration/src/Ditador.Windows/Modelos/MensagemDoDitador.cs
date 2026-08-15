using System.Text.Json;

namespace Ditador.Windows.Modelos;

/// <summary>
/// Uma linha do fluxo que o Ditador manda depois do <c>assinar</c>.
/// </summary>
/// <remarks>
/// <para>
/// O protocolo é uma mensagem JSON por linha, com um campo <c>t</c> dizendo de
/// que tipo ela é. São três hoje: a apresentação, o estado e o nível do
/// microfone. A regra de evolução, herdada do contrato D-Bus do lado Linux, é
/// **acrescentar, nunca renomear** — então um tipo desconhecido não é erro, é
/// uma mensagem de um Ditador mais novo, e a resposta certa é ignorá-la.
/// </para>
/// <para>
/// Isto vive num tipo próprio, e não dentro do cliente do named pipe, por um
/// motivo prático: aqui é a única parte do frontend que tem regra de verdade — o
/// resto é desenhar — e é a única que dá para testar sem uma janela na tela.
/// </para>
/// </remarks>
public abstract record MensagemDoDitador
{
    private MensagemDoDitador()
    {
    }

    /// <summary>A primeira linha: quem respondeu e que versão do protocolo fala.</summary>
    public sealed record Ola(int Protocolo, string Aplicativo, string Versao, string Backend)
        : MensagemDoDitador;

    /// <summary>O estado do Ditador — o retrato de agora.</summary>
    public sealed record Estado(RetratoDoDitador Retrato) : MensagemDoDitador;

    /// <summary>O pico do microfone, de 0 a 1. Só chega durante a gravação.</summary>
    public sealed record Nivel(double Valor) : MensagemDoDitador;

    /// <summary>
    /// Lê uma linha do fluxo. <c>null</c> quando ela não é entendida.
    /// </summary>
    /// <remarks>
    /// Nunca lança. O canal é local e quem escreve nele é o nosso próprio
    /// backend, então uma linha estranha não é ataque: é versão incompatível, ou
    /// um pedaço de mensagem que chegou torto. Derrubar a conexão por causa disso
    /// tiraria o ícone da barra de quem só precisava atualizar um dos dois lados.
    /// </remarks>
    public static MensagemDoDitador? Ler(string? linha)
    {
        if (string.IsNullOrWhiteSpace(linha))
        {
            return null;
        }

        try
        {
            using var documento = JsonDocument.Parse(linha);
            var raiz = documento.RootElement;
            if (raiz.ValueKind != JsonValueKind.Object)
            {
                return null;
            }

            return Texto(raiz, "t") switch
            {
                "ola" => new Ola(
                    Numero(raiz, "protocolo"),
                    Texto(raiz, "aplicativo"),
                    Texto(raiz, "versao"),
                    Texto(raiz, "backend")),

                "estado" => new Estado(new RetratoDoDitador(
                    RetratoDoDitador.EstadoDoTexto(Texto(raiz, "estado")),
                    Texto(raiz, "mensagem"),
                    Longo(raiz, "gravandoDesde"),
                    Texto(raiz, "modelo"),
                    Texto(raiz, "idioma"),
                    Texto(raiz, "atalho"))),

                "nivel" => new Nivel(Decimal(raiz, "valor")),

                // Um tipo que este frontend não conhece. Veja o comentário do
                // tipo: silêncio é a resposta certa.
                _ => null,
            };
        }
        catch (JsonException)
        {
            return null;
        }
    }

    // Os quatro leitores abaixo tratam campo ausente e campo do tipo errado da
    // mesma forma: devolvendo o vazio. É o que permite um backend mais novo
    // acrescentar campos sem quebrar um frontend mais velho — e é o mesmo
    // princípio do `#[serde(default)]` que o lado Rust usa na configuração.

    private static string Texto(JsonElement raiz, string campo) =>
        raiz.TryGetProperty(campo, out var valor) && valor.ValueKind == JsonValueKind.String
            ? valor.GetString() ?? string.Empty
            : string.Empty;

    private static int Numero(JsonElement raiz, string campo) =>
        raiz.TryGetProperty(campo, out var valor) && valor.TryGetInt32(out var numero) ? numero : 0;

    private static long Longo(JsonElement raiz, string campo) =>
        raiz.TryGetProperty(campo, out var valor) && valor.TryGetInt64(out var numero) ? numero : 0;

    private static double Decimal(JsonElement raiz, string campo) =>
        raiz.TryGetProperty(campo, out var valor) && valor.TryGetDouble(out var numero) ? numero : 0;
}
