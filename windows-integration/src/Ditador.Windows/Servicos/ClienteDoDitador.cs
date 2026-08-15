using System.Diagnostics;
using System.IO.Pipes;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using Ditador.Windows.Modelos;
using Microsoft.UI.Dispatching;

namespace Ditador.Windows.Servicos;

/// <summary>
/// A ponta do canal de controle deste lado: conecta ao <c>ditador.exe</c>, assina
/// o estado e reconecta sozinho quando a conexão cai.
/// </summary>
/// <remarks>
/// <para>
/// O transporte é o named pipe <c>\\.\pipe\Ditador-&lt;SID&gt;</c>, com o SID do
/// usuário no nome porque o espaço de nomes de pipes é global na máquina e no
/// Windows é rotina ter duas sessões abertas ao mesmo tempo. A permissão é
/// escrita pelo backend, e só o dono entra — quem quiser os detalhes, estão em
/// <c>src/plataforma/windows/ipc.rs</c>.
/// </para>
/// <para>
/// <b>Nada de pergunta em laço.</b> A conexão é aberta uma vez e fica: o backend
/// manda uma linha quando o estado muda, e mais nada quando não muda. Um
/// aplicativo de bandeja que perguntasse "e agora?" a cada 100 ms gastaria CPU o
/// dia inteiro para descobrir tarde — e este fica de pé o dia inteiro.
/// </para>
/// <para>
/// <b>A reconexão é a parte que importa.</b> As duas pontas sobem em ordem
/// imprevisível: no login, o Windows pode iniciar este processo antes do
/// backend; o backend pode ser reiniciado à mão; a máquina volta da suspensão. Em
/// todos esses casos o certo é o mesmo — tentar de novo, com espera crescente,
/// sem encher o log e sem queimar CPU.
/// </para>
/// </remarks>
public sealed class ClienteDoDitador : IDisposable
{
    /// <summary>De quanto em quanto tempo tentamos de novo, em segundos.</summary>
    /// <remarks>
    /// Começa quase imediato — o caso comum é o backend estar subindo junto com
    /// este processo, e meio segundo depois ele já atende — e cresce até meio
    /// minuto, que é a espera de quem desligou o Ditador de propósito e não quer
    /// um processo cutucando o sistema para sempre. A escada é explícita em vez
    /// de calculada: são sete números, e lê-los é mais fácil do que derivá-los.
    /// </remarks>
    private static readonly int[] Espera = [1, 1, 2, 4, 8, 15, 30];

    private readonly DispatcherQueue _interface;
    private readonly CancellationTokenSource _parar = new();
    private readonly string _nomeDoPipe;
    private bool _descartado;
    private bool _avisouQueNaoAchou;

    public ClienteDoDitador(DispatcherQueue interfaceDoAplicativo)
    {
        _interface = interfaceDoAplicativo;
        // O SID textual desta conta, o mesmo que o backend usa para montar o
        // nome. `WindowsIdentity` já o entrega pronto; não há o que converter.
        var sid = WindowsIdentity.GetCurrent().User?.Value ?? "desconhecido";
        _nomeDoPipe = $"Ditador-{sid}";
    }

    /// <summary>O último retrato recebido. Nunca nulo.</summary>
    public RetratoDoDitador Retrato { get; private set; } = RetratoDoDitador.Indisponivel;

    /// <summary>Disparado na thread da interface a cada retrato novo.</summary>
    public event Action<RetratoDoDitador>? Mudou;

    /// <summary>
    /// O pico do microfone, de 0 a 1, umas quinze vezes por segundo — e só
    /// enquanto se grava. Disparado na thread da interface.
    /// </summary>
    public event Action<double>? Nivel;

    /// <summary>Sobe o laço de conexão. Volta na hora; o trabalho é em segundo plano.</summary>
    public void Comecar() => _ = Task.Run(() => Laco(_parar.Token));

    /// <summary>
    /// Manda um comando e devolve a resposta, ou <c>null</c> se não houver
    /// ninguém atendendo.
    /// </summary>
    /// <remarks>
    /// Numa conexão curta, e não pela conexão assinada. São dois motivos: a
    /// conexão assinada é um fluxo de mão única depois do <c>assinar</c> (é assim
    /// que o backend a trata), e um comando que morresse junto com ela deixaria o
    /// clique do usuário sem resposta justamente quando o backend está
    /// reiniciando. Abrir e fechar um pipe local custa menos de um milissegundo.
    /// </remarks>
    public async Task<string?> EnviarAsync(string comando)
    {
        try
        {
            using var cano = new NamedPipeClientStream(
                ".", _nomeDoPipe, PipeDirection.InOut, PipeOptions.Asynchronous);
            await cano.ConnectAsync(2000, _parar.Token).ConfigureAwait(false);

            using var escrita = new StreamWriter(cano, Utf8Cru, leaveOpen: true) { AutoFlush = true };
            await escrita.WriteAsync(comando + "\n").ConfigureAwait(false);

            using var leitura = new StreamReader(cano, Utf8Cru, false, 1024, leaveOpen: true);
            return await leitura.ReadLineAsync(_parar.Token).ConfigureAwait(false);
        }
        catch (Exception e) when (e is IOException or TimeoutException or OperationCanceledException
                                      or UnauthorizedAccessException)
        {
            // Backend fora do ar é o caso normal deste caminho, não uma
            // excepcionalidade: o usuário clicou no menu enquanto o Ditador
            // reiniciava. Quem chamou decide o que dizer na tela.
            Registro.Detalhe($"comando \"{comando}\" não chegou: {e.Message}");
            return null;
        }
    }

    /// <summary>
    /// UTF-8 sem marca de ordem de bytes.
    /// </summary>
    /// <remarks>
    /// O padrão do <see cref="StreamWriter"/> escreveria um BOM de três bytes na
    /// frente da primeira linha, e do outro lado o Rust compararia
    /// <c>"﻿assinar"</c> com <c>"assinar"</c> e responderia "comando
    /// desconhecido" — uma falha que só aparece na primeira mensagem e some se
    /// alguém testar mandando a segunda.
    /// </remarks>
    private static readonly UTF8Encoding Utf8Cru = new(encoderShouldEmitUTF8Identifier: false);

    private async Task Laco(CancellationToken token)
    {
        var tentativa = 0;
        while (!token.IsCancellationRequested)
        {
            var conectou = false;
            try
            {
                conectou = await Assinar(token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception e)
            {
                Registro.Detalhe($"a assinatura caiu: {e.Message}");
            }

            if (token.IsCancellationRequested)
            {
                return;
            }

            // Quem conectou e caiu recomeça do topo da escada: a queda pode ter
            // sido um backend reiniciando, e nesse caso ele volta em segundos.
            tentativa = conectou ? 0 : Math.Min(tentativa + 1, Espera.Length - 1);
            Publicar(RetratoDoDitador.Indisponivel);

            try
            {
                await Task.Delay(TimeSpan.FromSeconds(Espera[tentativa]), token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                return;
            }
        }
    }

    /// <summary>
    /// Uma vida inteira de conexão: conecta, assina, lê até acabar.
    /// </summary>
    /// <returns><c>true</c> se chegou a conectar.</returns>
    private async Task<bool> Assinar(CancellationToken token)
    {
        using var cano = new NamedPipeClientStream(
            ".", _nomeDoPipe, PipeDirection.InOut, PipeOptions.Asynchronous);

        try
        {
            // Um segundo de paciência: o pipe ou está lá ou não está, e esperar
            // mais só atrasaria a mensagem de "indisponível" na tela.
            await cano.ConnectAsync(1000, token).ConfigureAwait(false);
        }
        catch (Exception e) when (e is TimeoutException or IOException)
        {
            // Uma linha na primeira falha e mais nenhuma enquanto o quadro não
            // mudar. Um aplicativo que fica de pé o dia inteiro tentando
            // reconectar encheria o arquivo de log com a mesma frase milhares de
            // vezes, e o log deixaria de servir para o que existe.
            if (!_avisouQueNaoAchou)
            {
                _avisouQueNaoAchou = true;
                Registro.Info($"o Ditador não está atendendo em {_nomeDoPipe}; seguindo a tentar");
            }

            return false;
        }

        _avisouQueNaoAchou = false;
        Registro.Info("conectado ao Ditador");

        await using var escrita = new StreamWriter(cano, Utf8Cru, leaveOpen: true) { AutoFlush = true };
        await escrita.WriteAsync("assinar\n").ConfigureAwait(false);

        using var leitura = new StreamReader(cano, Utf8Cru, false, 4096, leaveOpen: true);
        while (!token.IsCancellationRequested)
        {
            var linha = await leitura.ReadLineAsync(token).ConfigureAwait(false);
            if (linha is null)
            {
                // Fim do fluxo: o backend encerrou ou foi morto.
                Registro.Detalhe("o Ditador encerrou a assinatura");
                return true;
            }

            Interpretar(linha);
        }

        return true;
    }

    private void Interpretar(string linha)
    {
        switch (MensagemDoDitador.Ler(linha))
        {
            case MensagemDoDitador.Ola ola when ola.Protocolo != ProtocoloConhecido:
                // Falar mesmo assim é melhor do que desistir: os campos do estado
                // só crescem, pela regra de "acrescentar, nunca renomear", e o que
                // não for entendido vira vazio em vez de exceção. O aviso fica no
                // log para quando alguém investigar.
                Registro.Aviso(
                    $"o Ditador fala o protocolo {ola.Protocolo} e este frontend conhece o "
                    + $"{ProtocoloConhecido}; atualize os dois lados juntos.");
                break;

            case MensagemDoDitador.Estado estado:
                Publicar(estado.Retrato);
                break;

            case MensagemDoDitador.Nivel nivel:
                _interface.TryEnqueue(() => Nivel?.Invoke(nivel.Valor));
                break;
        }
    }

    /// <summary>A versão do protocolo que este frontend sabe ler.</summary>
    private const int ProtocoloConhecido = 1;

    private void Publicar(RetratoDoDitador retrato)
    {
        // A primeira publicação sai mesmo que o valor seja igual ao inicial. O
        // estado inicial é "indisponível", e sem esta exceção a primeira
        // tentativa de conexão fracassada não avisaria ninguém — nem o ícone
        // (que já nasce assim), nem quem precisa decidir se sobe o backend. O
        // sintoma era exatamente esse: com o backend desligado, o frontend ficava
        // esperando para sempre sem nunca tentar iniciá-lo.
        if (!_primeiraPublicacao && retrato == Retrato)
        {
            return;
        }

        _primeiraPublicacao = false;
        Retrato = retrato;
        _interface.TryEnqueue(() => Mudou?.Invoke(retrato));
    }

    private bool _primeiraPublicacao = true;

    /// <summary>
    /// Sobe o backend, se ele estiver instalado ao lado deste executável.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Pelo caminho conhecido da instalação, e não por busca no PATH nem por
    /// <c>cmd.exe /c</c>: este processo sabe onde ele próprio está, e o
    /// <c>ditador.exe</c> é instalado na mesma pasta. Um interpretador de comandos
    /// no meio só acrescentaria uma janela piscando e uma forma de alguém
    /// substituir o que vai ser executado.
    /// </para>
    /// <para>
    /// Devolve <c>false</c> quando não achou o executável — e nesse caso o certo é
    /// dizer isso na tela, não tentar de novo. O backend tem instância única
    /// própria (o dono do pipe), então uma segunda chamada não cria um segundo
    /// Ditador; mas chamar em laço criaria um processo que morre a cada tentativa,
    /// e é isso que este método não faz por conta própria.
    /// </para>
    /// </remarks>
    public static bool IniciarBackend()
    {
        var exe = Path.Combine(AppContext.BaseDirectory, "ditador.exe");
        if (!File.Exists(exe))
        {
            Registro.Aviso($"não achei o backend em {exe}");
            return false;
        }

        try
        {
            using var processo = Process.Start(new ProcessStartInfo(exe)
            {
                UseShellExecute = false,
                CreateNoWindow = true,
                WorkingDirectory = AppContext.BaseDirectory,
            });
            Registro.Info($"iniciei o backend: {exe}");
            return processo is not null;
        }
        catch (Exception e)
        {
            Registro.Aviso($"não consegui iniciar o backend: {e.Message}");
            return false;
        }
    }

    public void Dispose()
    {
        if (_descartado)
        {
            return;
        }

        _descartado = true;
        _parar.Cancel();
        _parar.Dispose();
    }
}
