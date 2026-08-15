using System.Runtime.InteropServices;
using Ditador.Windows.Servicos;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Ditador.Windows.Interop;

/// <summary>
/// Uma janela invisível que existe só para receber mensagens do Windows.
/// </summary>
/// <remarks>
/// <para>
/// O ícone da área de notificação precisa de um <c>HWND</c> para onde mandar os
/// cliques, e o Explorer precisa de um <c>HWND</c> para avisar que reiniciou. Esta
/// é essa janela: nasce sem tamanho, sem ser mostrada, fora da barra de tarefas e
/// fora do Alt+Tab.
/// </para>
/// <para>
/// <b>De nível superior, e não "message-only".</b> Uma janela criada com
/// <c>HWND_MESSAGE</c> seria mais barata e é a escolha óbvia para quem só quer
/// receber mensagens — mas ela <b>não recebe mensagens de difusão</b>, e
/// <c>TaskbarCreated</c> (o aviso de que o Explorer voltou) é uma difusão. Com
/// ela, o ícone do Ditador sumiria no primeiro reinício do Explorer e não
/// voltaria nunca, sem nada no log dizendo por quê. É o tipo de detalhe que só
/// aparece meses depois, na máquina de outra pessoa.
/// </para>
/// <para>
/// A janela é criada na thread da interface e todas as mensagens chegam nela — é
/// a mesma thread que desenha, e é isso que permite abrir o popup direto do
/// tratador sem despachar nada.
/// </para>
/// </remarks>
internal sealed class JanelaDeMensagens : IDisposable
{
    private readonly WNDPROC _tratador;
    private readonly string _classe;
    private readonly FreeLibrarySafeHandle _modulo;
    private bool _descartada;

    /// <summary>
    /// Chamado para cada mensagem. Devolver um valor significa "tratei"; devolver
    /// <c>null</c> deixa o Windows fazer o de sempre.
    /// </summary>
    public Func<uint, WPARAM, LPARAM, LRESULT?>? AoReceber { get; set; }

    public HWND Handle { get; }

    public unsafe JanelaDeMensagens(string nome)
    {
        _classe = $"Ditador.{nome}";
        _modulo = PInvoke.GetModuleHandle((string?)null);

        // O delegate precisa de um campo: o Windows guarda o ponteiro dele na
        // classe da janela e o chama por anos. Se ele fosse uma variável local, o
        // coletor de lixo o levaria embora e a primeira mensagem depois disso
        // derrubaria o processo dentro do laço de mensagens do Windows — uma
        // falha sem pilha gerenciada, das piores de diagnosticar.
        _tratador = Tratar;

        fixed (char* classe = _classe)
        {
            var descricao = new WNDCLASSEXW
            {
                // `Marshal.SizeOf`, e não `sizeof`: a estrutura carrega o
                // delegate do tratador, o que a torna um tipo gerenciado aos
                // olhos do C#. O tamanho que o Windows espera é o da forma
                // empacotada, que é o que o `Marshal` mede.
                cbSize = (uint)Marshal.SizeOf<WNDCLASSEXW>(),
                lpfnWndProc = _tratador,
                hInstance = (HINSTANCE)_modulo.DangerousGetHandle(),
                lpszClassName = classe,
            };

            if (PInvoke.RegisterClassEx(descricao) == 0)
            {
                throw new InvalidOperationException(
                    $"não consegui registrar a classe de janela {_classe}: " +
                    Marshal.GetLastPInvokeErrorMessage());
            }
        }

        Handle = PInvoke.CreateWindowEx(
            // `TOOLWINDOW` mantém a janela fora da barra de tarefas e do Alt+Tab
            // mesmo se algum dia ela for mostrada por engano.
            WINDOW_EX_STYLE.WS_EX_TOOLWINDOW,
            _classe,
            "Ditador",
            WINDOW_STYLE.WS_POPUP,
            0, 0, 0, 0,
            HWND.Null,
            null,
            _modulo,
            null);

        if (Handle.IsNull)
        {
            throw new InvalidOperationException(
                "não consegui criar a janela de mensagens: " + Marshal.GetLastPInvokeErrorMessage());
        }
    }

    private LRESULT Tratar(HWND janela, uint mensagem, WPARAM wParam, LPARAM lParam)
    {
        try
        {
            var resposta = AoReceber?.Invoke(mensagem, wParam, lParam);
            if (resposta.HasValue)
            {
                return resposta.Value;
            }
        }
        catch (Exception e)
        {
            // Uma exceção que atravessasse daqui de volta para o Windows encerra
            // o processo sem aviso — é código nativo do outro lado, e ele não tem
            // o que fazer com uma exceção do .NET. Registrar e seguir é a única
            // saída que preserva o programa.
            Registro.Aviso($"erro tratando a mensagem 0x{mensagem:X}: {e}");
        }

        return PInvoke.DefWindowProc(janela, mensagem, wParam, lParam);
    }

    public void Dispose()
    {
        if (_descartada)
        {
            return;
        }

        _descartada = true;
        if (!Handle.IsNull)
        {
            PInvoke.DestroyWindow(Handle);
        }

        PInvoke.UnregisterClass(_classe, _modulo);

        // Nada de `_modulo.Dispose()` aqui. O handle veio de `GetModuleHandle`,
        // que — diz a documentação, em tantas palavras — **não** incrementa a
        // contagem de referências do módulo; liberá-lo decrementa uma contagem
        // que nunca foi nossa. Como é o módulo do próprio executável, o estrago
        // hoje é teórico, e é exatamente por isso que passaria despercebido até
        // o dia em que não fosse.
        _modulo.SetHandleAsInvalid();
    }
}
