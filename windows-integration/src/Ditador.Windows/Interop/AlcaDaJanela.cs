using Microsoft.UI.Xaml;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.Graphics.Dwm;
using Windows.Win32.Graphics.Gdi;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Ditador.Windows.Interop;

/// <summary>
/// O punhado de coisas que uma janela do WinUI ainda precisa pedir ao Win32.
/// </summary>
/// <remarks>
/// <para>
/// A documentação da própria Microsoft descreve o <c>AppWindow</c> como uma
/// camada sobre o modelo de janelas do Win32, e diz que o interop é esperado —
/// não é gambiarra. O que o <c>AppWindow</c> resolve bem (apresentador, tamanho,
/// posição, presença no Alt+Tab) é usado de lá; o que ele não expõe (os estilos
/// estendidos, o arredondamento do DWM, a área de trabalho de um monitor) vem
/// daqui.
/// </para>
/// <para>
/// Tudo o que é Win32 no frontend está nesta pasta. Espalhar <c>P/Invoke</c> pelas
/// telas seria o começo de um wrapper de Win32 dentro do projeto, que é
/// exatamente o que não se quer.
/// </para>
/// </remarks>
internal readonly struct AlcaDaJanela
{
    private readonly HWND _janela;

    public AlcaDaJanela(Window janela)
    {
        _janela = (HWND)WinRT.Interop.WindowNative.GetWindowHandle(janela);
    }

    public HWND Handle => _janela;

    /// <summary>Os pontos por polegada do monitor em que esta janela está.</summary>
    public uint Dpi() => PInvoke.GetDpiForWindow(_janela);

    /// <summary>
    /// Torna a janela passiva: não ativa, não entra no Alt+Tab, não recebe clique.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Três estilos, três motivos distintos — e nenhum deles copiado sem entender:
    /// </para>
    /// <para>
    /// <b><c>WS_EX_NOACTIVATE</c></b> — a documentação a define para janelas de
    /// nível superior que não devem virar primeiro plano quando clicadas. É o que
    /// impede o aviso de roubar o cursor do editor, do navegador ou do terminal
    /// no instante em que ele aparece. Sem ela, começar a ditar tiraria o foco de
    /// onde o texto vai ser colado.
    /// </para>
    /// <para>
    /// <b><c>WS_EX_TOOLWINDOW</c></b> — tira a janela do Alt+Tab e da barra de
    /// tarefas. O aviso não é um lugar para onde se alterna; é uma etiqueta.
    /// </para>
    /// <para>
    /// <b><c>WS_EX_TRANSPARENT</c></b> — faz o teste de acerto do mouse
    /// atravessar. Só é usada porque o aviso <b>não tem nada em que clicar</b>: é
    /// texto e uma barrinha. Numa janela com botão ela seria um defeito, e é por
    /// isso que o popup de status não a recebe.
    /// </para>
    /// <para>
    /// O que <b>não</b> se faz aqui é repassar o clique com <c>SendInput</c>
    /// depois de recebê-lo. A janela é passiva de verdade — o clique nunca chega a
    /// ela, e portanto não há nada a repassar.
    /// </para>
    /// </remarks>
    public void TornarPassiva()
    {
        var estilos = (WINDOW_EX_STYLE)PInvoke.GetWindowLongPtr(_janela, WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE);
        estilos |= WINDOW_EX_STYLE.WS_EX_NOACTIVATE
                   | WINDOW_EX_STYLE.WS_EX_TOOLWINDOW
                   | WINDOW_EX_STYLE.WS_EX_TRANSPARENT;
        PInvoke.SetWindowLongPtr(_janela, WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE, (nint)estilos);
    }

    /// <summary>
    /// Faz o clique atravessar também as janelas internas do WinUI.
    /// </summary>
    /// <remarks>
    /// <para>
    /// O <c>WS_EX_TRANSPARENT</c> aplicado à janela de nível superior não basta:
    /// o WinUI 3 desenha o conteúdo numa janela filha (a
    /// <c>Microsoft.UI.Content.DesktopChildSiteBridge</c>), e <c>WindowFromPoint</c>
    /// no meio do aviso devolve **ela**, não a janela de baixo. Por isso o estilo
    /// é aplicado às filhas também, e só depois de a janela aparecer — antes
    /// disso elas ainda não existem.
    /// </para>
    /// <para>
    /// <b>Até onde isto foi comprovado:</b> os quatro <c>HWND</c> (a janela e as
    /// três internas do WinUI) ficam com <c>WS_EX_TRANSPARENT</c> ligado — isso
    /// foi medido lendo <c>GWL_EXSTYLE</c> de cada um com o aviso na tela. O que
    /// <b>não</b> foi possível comprovar é o efeito: montar um teste de clique com
    /// automação, com uma janela conhecida embaixo, deu resultado inconclusivo até
    /// no grupo de controle, e um teste que não distingue os dois casos não prova
    /// nada. Fica registrado como está: o estilo aplicado onde a documentação
    /// manda, e a limitação escrita no README em vez de uma afirmação que não se
    /// sustenta. O impacto prático é pequeno — a faixa tem 360 por 78 pontos, vive
    /// alguns segundos e não cobre nada com que se interaja.
    /// </para>
    /// </remarks>
    public void AtravessarCliques()
    {
        TornarPassiva();

        var aplicar = new WNDENUMPROC((filha, _) =>
        {
            var estilos = (WINDOW_EX_STYLE)PInvoke.GetWindowLongPtr(filha, WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE);
            PInvoke.SetWindowLongPtr(
                filha,
                WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE,
                (nint)(estilos | WINDOW_EX_STYLE.WS_EX_TRANSPARENT));
            return true;
        });

        PInvoke.EnumChildWindows(_janela, aplicar, IntPtr.Zero);
        // O delegate precisa continuar vivo durante a enumeração inteira, e a
        // enumeração é síncrona — esta linha existe para que o compilador (e
        // quem lê) saiba que ele não pode ser coletado antes.
        GC.KeepAlive(aplicar);
    }

    /// <summary>
    /// Tira a janela do Alt+Tab e da barra de tarefas, mas a deixa clicável.
    /// </summary>
    /// <remarks>
    /// É o popup do ícone: ele recebe foco (precisa, para fechar quando se clica
    /// fora) e tem botões, então nada de <c>NOACTIVATE</c> nem de
    /// <c>TRANSPARENT</c>. O que ele não pode é virar uma entrada no Alt+Tab —
    /// um popup de bandeja não é uma janela para onde se alterna.
    /// </remarks>
    public void TornarPopup()
    {
        var estilos = (WINDOW_EX_STYLE)PInvoke.GetWindowLongPtr(_janela, WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE);
        estilos |= WINDOW_EX_STYLE.WS_EX_TOOLWINDOW;
        PInvoke.SetWindowLongPtr(_janela, WINDOW_LONG_PTR_INDEX.GWL_EXSTYLE, (nint)estilos);
    }

    /// <summary>Arredonda os cantos, como o Windows 11 faz com as janelas comuns.</summary>
    /// <remarks>
    /// Uma janela do WinUI sem barra de título sai com cantos retos, porque quem
    /// arredonda é a moldura que ela deixou de ter. Sem isto o aviso apareceria
    /// como um retângulo duro no meio de um sistema de cantos macios — o tipo de
    /// detalhe que ninguém sabe nomear e todo mundo percebe.
    /// </remarks>
    public unsafe void ArredondarCantos()
    {
        var preferencia = DWM_WINDOW_CORNER_PREFERENCE.DWMWCP_ROUND;
        PInvoke.DwmSetWindowAttribute(
            _janela,
            DWMWINDOWATTRIBUTE.DWMWA_WINDOW_CORNER_PREFERENCE,
            &preferencia,
            (uint)sizeof(DWM_WINDOW_CORNER_PREFERENCE));
    }

    // Não há aqui um `SemMolduraDoSistema` com `DWMWA_BORDER_COLOR`, e houve por
    // um tempo. A captura de tela mostrava uma linha clara em volta do aviso e a
    // conclusão fácil era "é a moldura do DWM"; medindo os pixels, o cinza era o
    // #4F da nossa própria `BorderThickness="1"` com o `CardStrokeColorDefaultBrush`
    // — o contorno que todo flyout do Windows 11 tem, e que ali está certo. O
    // código foi removido em vez de mantido "por precaução": ele resolvia um
    // problema que não existia, e sobreviveria como mistério.

    /// <summary>
    /// A área de trabalho do monitor em que a pessoa está trabalhando agora.
    /// </summary>
    /// <remarks>
    /// <para>
    /// "Área de trabalho" é o retângulo do monitor menos a barra de tarefas —
    /// esteja ela embaixo, em cima ou de lado, e valendo a tela inteira quando ela
    /// está em ocultar automático. É por isso que a conta é feita com
    /// <c>GetMonitorInfo</c> e não com a resolução: coordenadas fixas quebram nas
    /// quatro posições da barra, e nenhuma delas é exótica.
    /// </para>
    /// <para>
    /// O monitor é o da janela em primeiro plano, com o do cursor como reserva.
    /// Nunca o primário: numa mesa de várias telas, o primário raramente é aquele
    /// para onde a pessoa está olhando.
    /// </para>
    /// </remarks>
    public static RECT AreaDeTrabalhoEmUso()
    {
        var ativa = PInvoke.GetForegroundWindow();
        var monitor = ativa.IsNull
            ? MonitorDoCursor()
            : PInvoke.MonitorFromWindow(ativa, MONITOR_FROM_FLAGS.MONITOR_DEFAULTTONEAREST);

        var informacao = new MONITORINFO { cbSize = (uint)System.Runtime.InteropServices.Marshal.SizeOf<MONITORINFO>() };
        if (PInvoke.GetMonitorInfo(monitor, ref informacao))
        {
            return informacao.rcWork;
        }

        // Sem informação de monitor, um retângulo de tela inteira comum é melhor
        // do que uma exceção: o aviso aparece num lugar razoável e o programa
        // continua.
        return new RECT { left = 0, top = 0, right = 1920, bottom = 1080 };
    }

    private static HMONITOR MonitorDoCursor()
    {
        PInvoke.GetCursorPos(out var ponto);
        return PInvoke.MonitorFromPoint(ponto, MONITOR_FROM_FLAGS.MONITOR_DEFAULTTOPRIMARY);
    }
}
