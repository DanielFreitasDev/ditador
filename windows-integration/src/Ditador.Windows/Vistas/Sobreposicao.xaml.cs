using Ditador.Windows.Interop;
using Ditador.Windows.Modelos;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Media.Animation;
using Windows.UI.ViewManagement;

namespace Ditador.Windows.Vistas;

/// <summary>
/// O aviso de gravação: uma faixa passiva no rodapé do monitor em uso.
/// </summary>
/// <remarks>
/// <para>
/// É o equivalente do OSD que a extensão do GNOME Shell desenha no Linux — e o
/// motivo de o backend, no Windows, esconder a janela do egui quando este
/// frontend está no ar (<c>Integracoes::mostram_o_aviso</c>, no Rust). Dois
/// avisos do mesmo ditado seriam o mesmo recado duas vezes, e um deles roubaria
/// o foco.
/// </para>
/// <para>
/// <b>Passiva quer dizer passiva.</b> Não ativa quando aparece, não entra no
/// Alt+Tab, não aparece na barra de tarefas e não recebe cliques — o ponteiro
/// atravessa e chega à janela de baixo. Cada uma dessas quatro coisas é um estilo
/// da janela, e cada estilo está justificado onde é aplicado.
/// </para>
/// </remarks>
public sealed partial class Sobreposicao : Window
{
    /// <summary>
    /// Os símbolos, na Segoe Fluent Icons — a fonte de ícones do Windows 11.
    /// </summary>
    /// <remarks>
    /// Escritos como escapes, e não como o caractere solto: eles moram na área de
    /// uso privado do Unicode, aparecem como quadradinho em qualquer editor que
    /// não tenha a fonte e sobrevivem mal a uma conversão de codificação. Assim o
    /// arquivo continua legível e o número fica conferível na documentação.
    /// </remarks>
    private static readonly string GlifoMicrofone = char.ConvertFromUtf32(0xE720);
    private static readonly string GlifoTrabalhando = char.ConvertFromUtf32(0xE895);
    private static readonly string GlifoAlerta = char.ConvertFromUtf32(0xE7BA);

    /// <summary>Tamanho da faixa, em pixels lógicos (antes do DPI).</summary>
    private const double LarguraLogica = 360;
    private const double AlturaLogica = 78;

    /// <summary>Distância entre a faixa e a borda de baixo da área de trabalho.</summary>
    /// <remarks>
    /// Da <b>área de trabalho</b>, e não da tela: é o retângulo que exclui a
    /// barra de tarefas, esteja ela embaixo, em cima ou de lado. Com a barra em
    /// ocultar automático a área volta a ser a tela inteira, e o aviso desce
    /// junto — que é o certo, porque nesse modo não há barra ocupando o lugar.
    /// </remarks>
    private const double MargemLogica = 48;

    private readonly AppWindow _janela;
    private readonly DispatcherTimer _relogio = new() { Interval = TimeSpan.FromSeconds(1) };
    private readonly UISettings _preferencias = new();
    private RetratoDoDitador _retrato = RetratoDoDitador.Indisponivel;
    private bool _visivel;
    private bool _cliquesJaAtravessam;

    public Sobreposicao()
    {
        InitializeComponent();

        _janela = AppWindow;

        // Sem barra de título e sem borda: isto não é uma janela com que se
        // interaja, é um cartão. O `OverlappedPresenter` é o único que permite
        // desligar as duas coisas mantendo uma janela comum — o
        // `CompactOverlayPresenter` foi pensado para vídeo em miniatura e impõe
        // proporções e tamanho mínimo que não têm nada a ver com isto.
        var apresentador = OverlappedPresenter.Create();
        apresentador.SetBorderAndTitleBar(false, false);
        apresentador.IsAlwaysOnTop = true;
        apresentador.IsResizable = false;
        apresentador.IsMaximizable = false;
        apresentador.IsMinimizable = false;
        _janela.SetPresenter(apresentador);

        // Fora do Alt+Tab e da Visão de Tarefas. O `WS_EX_TOOLWINDOW` aplicado
        // logo abaixo faz o mesmo pelo caminho do Win32; os dois juntos cobrem o
        // Shell e o compositor, que consultam coisas diferentes.
        _janela.IsShownInSwitchers = false;

        _janela.Changed += (_, argumentos) =>
        {
            // O DPI mudou (a janela foi parar noutro monitor): refazer a conta do
            // tamanho, senão a faixa fica com o tamanho do monitor anterior.
            if (argumentos.DidSizeChange || argumentos.DidPositionChange)
            {
                return;
            }
        };

        var alca = new AlcaDaJanela(this);
        alca.TornarPassiva();
        alca.ArredondarCantos();

        Title = "Ditador";
        _relogio.Tick += (_, _) => AtualizarCronometro();

        // O leitor de tela não enxerga uma janela que nunca recebe foco. O que
        // ele enxerga é isto: um nome no elemento e o aviso de que ele muda
        // sozinho. Quem depende do Narrator recebe o mesmo recado pelas
        // notificações do sistema, que é o caminho que ele lê de verdade.
        AutomationProperties.SetName(Cartao, "Estado do Ditador");
        AutomationProperties.SetLiveSetting(Cartao, AutomationLiveSetting.Polite);
    }

    /// <summary>Ajusta o aviso ao estado de agora, mostrando ou escondendo.</summary>
    public void Mostrar(RetratoDoDitador retrato)
    {
        _retrato = retrato;

        switch (retrato.Estado)
        {
            case Estado.Gravando:
                Simbolo.Glyph = GlifoMicrofone;
                Rotulo.Text = "Gravando";
                Medidor.Visibility = Visibility.Visible;
                AtualizarCronometro();
                _relogio.Start();
                Aparecer();
                break;

            case Estado.Transcrevendo:
                Simbolo.Glyph = GlifoTrabalhando;
                Rotulo.Text = "Processando fala…";
                Cronometro.Text = string.Empty;
                Medidor.Visibility = Visibility.Collapsed;
                // O relógio continua batendo, agora contando a paciência: se a
                // transcrição passar de alguns segundos, o aviso abaixo explica
                // por quê.
                _transcrevendoDesde = DateTimeOffset.UtcNow;
                _relogio.Start();
                Aparecer();
                break;

            case Estado.Erro:
                Simbolo.Glyph = GlifoAlerta;
                Rotulo.Text = Enxugar(retrato.Mensagem, "Alguma coisa falhou");
                Cronometro.Text = string.Empty;
                Medidor.Visibility = Visibility.Collapsed;
                _relogio.Stop();
                Aparecer();
                break;

            default:
                // Pronto, carregando e indisponível não aparecem na tela. O aviso
                // existe para dizer que o microfone está aberto ou que a máquina
                // está trabalhando; "está tudo bem" é o estado normal do Ditador e
                // não merece uma faixa por cima do que a pessoa está fazendo. Quem
                // quiser saber olha o ícone da barra.
                _relogio.Stop();
                Sumir();
                break;
        }
    }

    /// <summary>O pico do microfone agora, de 0 a 1.</summary>
    public void MostrarNivel(double nivel)
    {
        if (!_visivel || Medidor.Visibility != Visibility.Visible)
        {
            return;
        }

        // A raiz quadrada é o que dá presença visual à fala baixa: a energia de
        // uma voz normal fica na parte de baixo da escala, e uma barra linear
        // passaria a impressão de microfone quase mudo. O backend manda o valor
        // cru de propósito — cada superfície faz a sua correção, e a janela do
        // egui no Linux faz esta mesma.
        var largura = Medidor.ActualWidth * Math.Sqrt(Math.Clamp(nivel, 0, 1));
        Preenchimento.Width = double.IsNaN(largura) ? 0 : largura;
    }

    public void Fechar()
    {
        _relogio.Stop();
        Close();
    }

    /// <summary>Quanto tempo de transcrição já é demora, e não trabalho normal.</summary>
    /// <remarks>
    /// A primeira transcrição depois de instalar leva uns vinte segundos com o
    /// backend Vulkan: o driver compila os pipelines de shader antes de rodar
    /// qualquer coisa, e guarda o resultado — as seguintes voltam a levar meio
    /// segundo. Sem aviso, o que se vê é uma faixa dizendo "processando" por
    /// vinte segundos, que é indistinguível de um programa travado. Seis
    /// segundos é folga para um parágrafo longo numa máquina modesta e é pouco o
    /// bastante para o aviso chegar antes da desconfiança.
    /// </remarks>
    private static readonly TimeSpan DemoraDemais = TimeSpan.FromSeconds(6);

    private DateTimeOffset? _transcrevendoDesde;

    private void AtualizarCronometro()
    {
        if (_retrato.Estado == Estado.Transcrevendo)
        {
            if (_transcrevendoDesde is { } desde && DateTimeOffset.UtcNow - desde > DemoraDemais)
            {
                Cronometro.Text = "preparando a placa de vídeo…";
            }

            return;
        }

        _transcrevendoDesde = null;

        if (_retrato.GravandoDesde <= 0)
        {
            Cronometro.Text = string.Empty;
            return;
        }

        // A conta é sempre "agora menos o começo", com o começo vindo do backend.
        // Contar de um em um daqui — somando um segundo a cada tique — pareceria
        // mais simples e erraria: um tique perdido enquanto a máquina está
        // ocupada nunca mais seria recuperado, e o cronômetro atrasaria para
        // sempre.
        var inicio = DateTimeOffset.FromUnixTimeMilliseconds(_retrato.GravandoDesde);
        var decorrido = DateTimeOffset.UtcNow - inicio;
        if (decorrido < TimeSpan.Zero)
        {
            decorrido = TimeSpan.Zero;
        }

        Cronometro.Text = decorrido.TotalHours >= 1
            ? decorrido.ToString(@"h\:mm\:ss")
            : decorrido.ToString(@"m\:ss");
    }

    private static string Enxugar(string mensagem, string reserva)
    {
        const int Limite = 46;
        var linha = mensagem.ReplaceLineEndings(" ").Trim();
        if (linha.Length == 0)
        {
            return reserva;
        }

        return linha.Length <= Limite ? linha : string.Concat(linha.AsSpan(0, Limite - 1), "…");
    }

    private void Aparecer()
    {
        Posicionar();

        if (!_visivel)
        {
            _visivel = true;
            // `Show(false)` é a peça central deste arquivo: mostra a janela
            // **sem ativá-la**. Com `true` — que é o padrão — o aviso roubaria o
            // cursor do editor de quem está ditando, exatamente no instante em
            // que a pessoa começou a falar.
            _janela.Show(activateWindow: false);

            // Só depois de aparecer é que as janelas internas do WinUI existem,
            // e é nelas que o clique parava apesar do `WS_EX_TRANSPARENT` da
            // janela de fora. Uma vez basta: elas não são recriadas.
            if (!_cliquesJaAtravessam)
            {
                _cliquesJaAtravessam = true;
                new AlcaDaJanela(this).AtravessarCliques();
            }
        }

        Desvanecer(para: 1);
    }

    private void Sumir()
    {
        if (!_visivel)
        {
            return;
        }

        _visivel = false;
        Desvanecer(para: 0, aoTerminar: () => _janela.Hide());
    }

    /// <summary>Transição curta de opacidade, quando o sistema as permite.</summary>
    /// <remarks>
    /// 140 ms: o bastante para o aviso não piscar na cara de quem está lendo, e
    /// pouco o bastante para não parecer lento. Quem desligou animações em
    /// Configurações → Acessibilidade → Efeitos visuais recebe a troca seca — a
    /// preferência existe para pessoas com sensibilidade a movimento, e um
    /// aplicativo que a ignora é um aplicativo que passa mal-estar.
    /// </remarks>
    private void Desvanecer(double para, Action? aoTerminar = null)
    {
        if (!_preferencias.AnimationsEnabled)
        {
            Cartao.Opacity = para;
            aoTerminar?.Invoke();
            return;
        }

        var animacao = new DoubleAnimation
        {
            To = para,
            Duration = new Duration(TimeSpan.FromMilliseconds(140)),
            EnableDependentAnimation = true,
        };
        Storyboard.SetTarget(animacao, Cartao);
        Storyboard.SetTargetProperty(animacao, "Opacity");

        var roteiro = new Storyboard();
        roteiro.Children.Add(animacao);
        if (aoTerminar is not null)
        {
            roteiro.Completed += (_, _) => aoTerminar();
        }

        roteiro.Begin();
    }

    /// <summary>
    /// Põe a faixa no rodapé do monitor em que a pessoa está trabalhando.
    /// </summary>
    /// <remarks>
    /// <para>
    /// "Em que está trabalhando" é o monitor da janela em primeiro plano — quem
    /// dita está com o cursor num editor, e é lá que ela vai olhar. O ponteiro do
    /// mouse serve de reserva para o caso de não haver janela ativa nenhuma (logo
    /// depois de um login, por exemplo).
    /// </para>
    /// <para>
    /// O monitor primário <b>não</b> entra nessa conta. Numa mesa de três telas,
    /// o primário costuma ser o do meio e a pessoa costuma estar em outro: o
    /// aviso apareceria fora do campo de visão dela, que é o mesmo que não
    /// aparecer.
    /// </para>
    /// </remarks>
    private void Posicionar()
    {
        var alca = new AlcaDaJanela(this);
        var area = AlcaDaJanela.AreaDeTrabalhoEmUso();
        var dpi = alca.Dpi();
        var escala = dpi / 96.0;

        var largura = (int)Math.Round(LarguraLogica * escala);
        var altura = (int)Math.Round(AlturaLogica * escala);
        var margem = (int)Math.Round(MargemLogica * escala);

        var x = area.left + ((area.right - area.left) - largura) / 2;
        var y = area.bottom - altura - margem;

        _janela.MoveAndResize(new global::Windows.Graphics.RectInt32(x, y, largura, altura));
    }
}
