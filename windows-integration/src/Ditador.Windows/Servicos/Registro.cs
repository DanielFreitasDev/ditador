using System.Diagnostics;
using System.Text;

namespace Ditador.Windows.Servicos;

/// <summary>
/// O log deste processo: um arquivo de texto por usuário, e mais nada.
/// </summary>
/// <remarks>
/// <para>
/// Um aplicativo que fica de pé o dia inteiro sem janela precisa deixar rastro,
/// senão a única resposta possível a "o ícone sumiu" é dar de ombros. E precisa
/// deixá-lo onde a pessoa consiga pegar: um arquivo de texto em
/// <c>%LOCALAPPDATA%\ditador\logs\</c>, que se abre com dois cliques e se anexa a
/// um relato de problema.
/// </para>
/// <para>
/// <b>Nada de Log de Eventos do Windows.</b> Escrever nele exige registrar uma
/// fonte de eventos, e registrar uma fonte exige administrador — elevação para
/// gravar linha de diagnóstico é um preço que não se paga. Nada de ETW também:
/// ler um ETW pede ferramenta que quem usa o Ditador não tem.
/// </para>
/// <para>
/// <b>Nada sai desta máquina.</b> Sem telemetria, sem envio automático de falha.
/// O Ditador transcreve local; um log que viajasse contradiria a única promessa
/// que o programa faz.
/// </para>
/// </remarks>
internal static class Registro
{
    /// <summary>De que tamanho o arquivo é aposentado, em bytes.</summary>
    /// <remarks>
    /// Um mega é muita linha para um programa que escreve um punhado por dia, e
    /// pouco espaço para quem tem o disco cheio. Ao passar disso, o arquivo vira
    /// <c>.old</c> e um novo começa — duas gerações, sem data no nome e sem
    /// varredura de pasta antiga para fazer.
    /// </remarks>
    private const long TamanhoMaximo = 1024 * 1024;

    private static readonly Lock Tranca = new();
    private static readonly string Arquivo = EscolherArquivo();

    /// <summary>O arquivo de log, para quem quiser mostrá-lo na interface.</summary>
    public static string Caminho => Arquivo;

    public static void Info(string mensagem) => Escrever("info", mensagem);

    public static void Aviso(string mensagem) => Escrever("aviso", mensagem);

    /// <summary>
    /// Detalhe de funcionamento normal — conectou, desconectou, o clique chegou.
    /// </summary>
    /// <remarks>
    /// Vai para o arquivo como qualquer outra linha. A filtragem fina que o lado
    /// Rust tem (<c>RUST_LOG</c>) não se justifica aqui: este processo escreve na
    /// ordem de dezenas de linhas por dia, não de milhares.
    /// </remarks>
    public static void Detalhe(string mensagem) => Escrever("detalhe", mensagem);

    private static string EscolherArquivo()
    {
        var pasta = PastaDosLogs();
        Directory.CreateDirectory(pasta);
        return Path.Combine(pasta, "Ditador.Windows.log");
    }

    /// <summary>
    /// A pasta dos logs: a do modo portátil, quando há marcador; senão, a de
    /// <c>%LOCALAPPDATA%</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// É o espelho do <c>src/portatil.rs</c> do backend, com as mesmas regras:
    /// um arquivo <c>portatil</c> (ou <c>portable</c>) ao lado do executável
    /// põe tudo na pasta <c>Dados\</c> vizinha, e o modo só vale depois de uma
    /// escrita de verdade dar certo — um marcador esquecido ao lado de um
    /// executável numa pasta somente-leitura não pode deixar o processo sem
    /// log. Sem este espelho, a versão portátil gravava o log do frontend em
    /// <c>%LOCALAPPDATA%</c> da máquina emprestada: rastro onde a proposta é
    /// não deixar nenhum, e um log que fica para trás quando o pendrive vai
    /// embora — justamente o arquivo que se quer anexar ao relato de um
    /// problema.
    /// </para>
    /// <para>
    /// O caminho é <c>Dados\dados\logs</c>, e não <c>Dados\logs</c>, porque é
    /// onde o backend escreve o dele (o <c>data_dir()</c> portátil é
    /// <c>Dados\dados</c>): os dois logs lado a lado, como no modo instalado.
    /// </para>
    /// </remarks>
    private static string PastaDosLogs()
    {
        var aoLado = AppContext.BaseDirectory;
        if (File.Exists(Path.Combine(aoLado, "portatil")) ||
            File.Exists(Path.Combine(aoLado, "portable")))
        {
            try
            {
                var dados = Path.Combine(aoLado, "Dados", "dados", "logs");
                Directory.CreateDirectory(dados);
                var teste = Path.Combine(dados, ".escrita-de-teste");
                File.WriteAllText(teste, "ditador");
                File.Delete(teste);
                return dados;
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                // A pasta não aceita escrita — o mesmo caso que o backend trata:
                // segue para o LocalAppData, porque perder o log é pior do que
                // gravá-lo no lugar de sempre. Quem explica o porquê no log é o
                // relato do modo portátil do backend, que enxerga o mesmo estado.
            }
        }

        // `LocalApplicationData`, e não `ApplicationData`: log não acompanha o
        // usuário entre máquinas de um domínio — ele descreve *esta* máquina, e
        // sincronizá-lo pela rede seria trocar utilidade por tráfego. É a mesma
        // divisão que o backend faz entre a configuração (que viaja) e os
        // modelos (que não).
        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ditador",
            "logs");
    }

    private static void Escrever(string nivel, string mensagem)
    {
        var linha = $"{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff} {nivel,-8} {mensagem}";

        // Em desenvolvimento, o depurador também mostra — é o console que este
        // aplicativo não tem.
        Debug.WriteLine(linha);

        try
        {
            lock (Tranca)
            {
                Aposentar();
                File.AppendAllText(Arquivo, linha + Environment.NewLine, Encoding.UTF8);
            }
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            // Disco cheio ou pasta sem permissão não podem derrubar o programa:
            // perder o log é ruim, perder o ditado é pior. E não há para onde
            // reclamar — reclamar exigiria justamente o log.
        }
    }

    private static void Aposentar()
    {
        var informacao = new FileInfo(Arquivo);
        if (!informacao.Exists || informacao.Length < TamanhoMaximo)
        {
            return;
        }

        var velho = Arquivo + ".old";
        File.Delete(velho);
        File.Move(Arquivo, velho);
    }
}
