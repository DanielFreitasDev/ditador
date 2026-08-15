using Ditador.Windows.Interop;
using Ditador.Windows.Modelos;
using Ditador.Windows.Servicos;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Ditador.Windows.Vistas;

/// <summary>
/// O painel que abre ao clicar no ícone: estado, uma ação e três informações.
/// </summary>
/// <remarks>
/// Fecha ao clicar fora, como todo popup de bandeja do Windows. Fechá-lo não
/// encerra nada — o Ditador continua no canto, e o atalho continua funcionando.
/// Encerrar de verdade é uma escolha explícita no menu do botão direito.
/// </remarks>
public sealed partial class JanelaDeStatus : Window
{
    /// <summary>Largura do painel em pixels lógicos.</summary>
    /// <remarks>
    /// Larga o bastante para "large-v3-turbo-q5_0" caber sem cortar, estreita o
    /// bastante para não parecer uma janela de aplicativo. As alturas são
    /// medidas, não escritas: o aviso aparece e some, e uma altura fixa deixaria
    /// um vão embaixo ou cortaria a última linha.
    /// </remarks>
    private const double LarguraLogica = 300;

    private static readonly string GlifoPronto = char.ConvertFromUtf32(0xE73E); // marca de confirmação
    private static readonly string GlifoGravando = char.ConvertFromUtf32(0xE720); // microfone
    private static readonly string GlifoTrabalhando = char.ConvertFromUtf32(0xE895); // sincronizar
    private static readonly string GlifoAlerta = char.ConvertFromUtf32(0xE7BA); // alerta

    private readonly ClienteDoDitador _cliente;
    private readonly AppWindow _janela;
    private bool _visivel;

    public JanelaDeStatus(ClienteDoDitador cliente)
    {
        _cliente = cliente;
        InitializeComponent();

        _janela = AppWindow;

        var apresentador = OverlappedPresenter.Create();
        apresentador.SetBorderAndTitleBar(false, false);
        // Por cima, mas não passivo: este painel tem botões e precisa de foco
        // para saber quando o usuário clicou fora.
        apresentador.IsAlwaysOnTop = true;
        apresentador.IsResizable = false;
        apresentador.IsMaximizable = false;
        apresentador.IsMinimizable = false;
        _janela.SetPresenter(apresentador);
        _janela.IsShownInSwitchers = false;

        var alca = new AlcaDaJanela(this);
        alca.TornarPopup();
        alca.ArredondarCantos();

        Title = "Ditador";

        // Clicar fora fecha, que é o comportamento de todo popup de bandeja do
        // Windows. Sem isto o painel ficaria por cima de tudo até alguém
        // procurar um jeito de fechá-lo — e não há barra de título para isso.
        Activated += (_, argumentos) =>
        {
            if (argumentos.WindowActivationState == WindowActivationState.Deactivated)
            {
                Esconder();
            }
        };

        AutomationProperties.SetName(BotaoDitar, "Começar ou parar o ditado");
        AutomationProperties.SetName(BotaoConfiguracoes, "Abrir as configurações do Ditador");
    }

    /// <summary>Abre o painel junto ao ícone, ou o fecha se já estiver aberto.</summary>
    /// <remarks>
    /// <c>internal</c> porque o <c>RECT</c> que ele recebe é um tipo gerado pelo
    /// CsWin32, e o gerador os cria internos ao assembly — de propósito, para que
    /// tipos do Win32 não vazem para a API pública de quem os usa. Aqui não faz
    /// falta: quem chama este método é o próprio aplicativo.
    /// </remarks>
    internal void Alternar(RECT? ancora = null)
    {
        if (_visivel)
        {
            Esconder();
            return;
        }

        Atualizar(_cliente.Retrato);
        Posicionar(ancora);
        _visivel = true;
        _janela.Show();
        // Sem isto o painel abre atrás da janela que estava em primeiro plano em
        // alguns casos — o clique no ícone não transfere o primeiro plano para
        // nós, e o "sempre por cima" governa a ordem entre janelas, não o foco.
        PInvoke.SetForegroundWindow(new AlcaDaJanela(this).Handle);
    }

    public void Atualizar(RetratoDoDitador retrato)
    {
        Estado.Text = retrato.Descricao;
        Simbolo.Glyph = retrato.Estado switch
        {
            Modelos.Estado.Gravando => GlifoGravando,
            Modelos.Estado.Carregando or Modelos.Estado.Transcrevendo => GlifoTrabalhando,
            Modelos.Estado.Erro or Modelos.Estado.Indisponivel => GlifoAlerta,
            _ => GlifoPronto,
        };

        var disponivel = retrato.Estado != Modelos.Estado.Indisponivel;
        Detalhes.Visibility = disponivel ? Visibility.Visible : Visibility.Collapsed;
        BotaoConfiguracoes.IsEnabled = disponivel;
        BotaoDitar.IsEnabled = disponivel
                               && retrato.Estado is Modelos.Estado.Pronto or Modelos.Estado.Gravando;
        BotaoDitar.Content = retrato.Gravando ? "Parar de ditar" : "Ditar agora";

        Atalho.Text = string.IsNullOrEmpty(retrato.Atalho) ? "—" : retrato.Atalho;
        Modelo.Text = string.IsNullOrEmpty(retrato.Modelo) ? "—" : retrato.Modelo;
        Idioma.Text = string.IsNullOrEmpty(retrato.Idioma) ? "—" : retrato.Idioma;

        if (!disponivel)
        {
            Aviso.Severity = InfoBarSeverity.Warning;
            Aviso.Title = "O Ditador não está no ar";
            Aviso.Message = "Sem ele não há gravação nem transcrição. "
                            + "Use \"Iniciar o Ditador\" no menu do botão direito.";
            Aviso.IsOpen = true;
        }
        else if (retrato.Estado == Modelos.Estado.Erro && retrato.Mensagem.Length > 0)
        {
            Aviso.Severity = InfoBarSeverity.Error;
            Aviso.Title = "Erro";
            Aviso.Message = retrato.Mensagem;
            Aviso.IsOpen = true;
        }
        else if (retrato.Mensagem.Length > 0)
        {
            // Mensagem sem estado de erro é aviso: o modelo faltando, o atalho
            // que não pôde ser lido. Vale mostrar, mas não vale pintar de
            // vermelho.
            Aviso.Severity = InfoBarSeverity.Informational;
            Aviso.Title = string.Empty;
            Aviso.Message = retrato.Mensagem;
            Aviso.IsOpen = true;
        }
        else
        {
            Aviso.IsOpen = false;
        }

        if (_visivel)
        {
            // O conteúdo mudou de altura (o aviso apareceu ou sumiu): refaz a
            // janela em volta dele.
            Posicionar(null);
        }
    }

    public void Fechar() => Close();

    private void Esconder()
    {
        if (!_visivel)
        {
            return;
        }

        _visivel = false;
        _janela.Hide();
    }

    private async void DitarAgora(object remetente, RoutedEventArgs argumentos)
    {
        Esconder();
        if (await _cliente.EnviarAsync("toggle") is null)
        {
            Registro.Aviso("o comando de ditar não chegou ao backend");
        }
    }

    private async void AbrirConfiguracoes(object remetente, RoutedEventArgs argumentos)
    {
        Esconder();
        // A tela de configurações é a do backend — a mesma do Linux. Ele a abre
        // no próprio processo, com o egui.
        if (await _cliente.EnviarAsync("settings") is null)
        {
            Registro.Aviso("o comando de configurações não chegou ao backend");
        }
    }

    /// <summary>
    /// Encosta o painel no ícone, respeitando monitor, barra de tarefas e DPI.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Quem faz a conta é o <c>CalculatePopupWindowPosition</c>, a mesma função
    /// que o Windows usa para os próprios menus. Ela recebe o ponto de ancoragem,
    /// o tamanho da janela e o retângulo a evitar — o do ícone — e devolve uma
    /// posição que cabe na área de trabalho do monitor certo.
    /// </para>
    /// <para>
    /// Escrever <c>larguraDaTela - 300</c> pareceria mais simples e estaria errado
    /// em todo lugar que importa: com dois monitores, com a barra de tarefas em
    /// cima ou de lado, com escalas diferentes por monitor e com a barra em
    /// ocultar automático. São quatro configurações comuns, não exóticas.
    /// </para>
    /// </remarks>
    private unsafe void Posicionar(RECT? ancora)
    {
        var alca = new AlcaDaJanela(this);
        var escala = alca.Dpi() / 96.0;

        // Medir antes de posicionar: a altura depende de o aviso estar aberto ou
        // não, e do texto dentro dele.
        var raiz = (FrameworkElement)Content;
        raiz.Measure(new global::Windows.Foundation.Size(LarguraLogica, double.PositiveInfinity));
        var alturaLogica = Math.Max(raiz.DesiredSize.Height, 160);

        var largura = (int)Math.Round(LarguraLogica * escala);
        var altura = (int)Math.Round(alturaLogica * escala);

        if (ancora is { } icone)
        {
            var ponto = new System.Drawing.Point(icone.right, icone.top);
            var tamanho = new System.Drawing.Size(largura, altura);

            var excluir = icone;
            if (PInvoke.CalculatePopupWindowPosition(
                    ponto,
                    tamanho,
                    (uint)(TRACK_POPUP_MENU_FLAGS.TPM_VERTICAL | TRACK_POPUP_MENU_FLAGS.TPM_RIGHTALIGN | TRACK_POPUP_MENU_FLAGS.TPM_BOTTOMALIGN),
                    excluir,
                    out var posicao))
            {
                _janela.MoveAndResize(new global::Windows.Graphics.RectInt32(
                    posicao.left, posicao.top, largura, altura));
                return;
            }
        }

        // Sem o retângulo do ícone — pode acontecer se o Explorer estiver
        // reiniciando —, o canto inferior direito da área de trabalho é o lugar
        // certo em uma barra de tarefas na posição padrão, e um lugar razoável em
        // qualquer outra.
        var area = AlcaDaJanela.AreaDeTrabalhoEmUso();
        var folga = (int)Math.Round(12 * escala);
        _janela.MoveAndResize(new global::Windows.Graphics.RectInt32(
            area.right - largura - folga,
            area.bottom - altura - folga,
            largura,
            altura));
    }
}
