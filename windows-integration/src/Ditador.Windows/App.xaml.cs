using Ditador.Windows.Interop;
using Ditador.Windows.Modelos;
using Ditador.Windows.Servicos;
using Ditador.Windows.Vistas;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using Windows.Win32;

namespace Ditador.Windows;

/// <summary>
/// O aplicativo: um ícone na área de notificação, um aviso na tela e um popup.
/// </summary>
/// <remarks>
/// <para>
/// Ele não tem janela principal. Sobe com a sessão, fica no canto e só aparece
/// quando há o que dizer — que é o que se espera de um companheiro de bandeja, e
/// o oposto do que uma janela vazia no Alt+Tab faria.
/// </para>
/// <para>
/// <b>Tudo o que este processo faz é mostrar.</b> Quem lê o teclado, abre o
/// microfone, roda o Whisper e escreve na área de transferência é o
/// <c>ditador.exe</c> em Rust, num processo à parte. Se este aqui morrer, o
/// ditado continua — perde-se o ícone e o aviso, não o programa. O contrário
/// também vale: sem o backend, isto aqui mostra "indisponível" e oferece
/// iniciá-lo.
/// </para>
/// </remarks>
public partial class App : Application
{
    private ClienteDoDitador? _cliente;
    private JanelaDeMensagens? _janelaOculta;
    private IconeDaBandeja? _icone;
    private Sobreposicao? _aviso;
    private JanelaDeStatus? _popup;
    private Notificador? _notificador;
    private bool _jaTenteiSubirOBackend;

    public App()
    {
        InitializeComponent();

        // Uma exceção não tratada na thread da interface fecharia o processo sem
        // deixar rastro nenhum — e este é um programa que fica horas de pé sem
        // ninguém olhando. Registrar antes de morrer é o mínimo.
        UnhandledException += (_, e) =>
        {
            Registro.Aviso($"exceção não tratada: {e.Exception}");
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // A identidade do aplicativo aos olhos do Shell. Precisa vir antes de
        // qualquer janela e antes das notificações: é ela que associa o ícone, os
        // avisos e o atalho ao mesmo aplicativo, em vez de tratar cada caminho de
        // instalação como um programa diferente.
        PInvoke.SetCurrentProcessExplicitAppUserModelID(Identidade);

        Registro.Info($"Ditador.Windows subindo — {AppContext.BaseDirectory}");

        var fila = DispatcherQueue.GetForCurrentThread();
        _cliente = new ClienteDoDitador(fila);

        _janelaOculta = new JanelaDeMensagens("Bandeja");
        _icone = new IconeDaBandeja(_janelaOculta);
        _aviso = new Sobreposicao();
        _popup = new JanelaDeStatus(_cliente);
        _notificador = new Notificador();

        _icone.Clicado += () => _popup?.Alternar(_icone?.Retangulo());
        _icone.MenuEscolhido += EscolheuNoMenu;

        // Alguém abriu o Ditador.Windows de novo — pelo atalho do menu Iniciar,
        // por exemplo. A segunda instância já se encerrou e mandou a ativação
        // para cá (veja o `Program.cs`); o que ela queria era ver o programa, e
        // é o painel que se mostra.
        AppInstance.GetCurrent().Activated += (_, _) =>
            fila.TryEnqueue(() => _popup?.Alternar(_icone?.Retangulo()));

        _cliente.Mudou += MudouOEstado;
        _cliente.Nivel += nivel => _aviso?.MostrarNivel(nivel);

        _icone.Atualizar(RetratoDoDitador.Indisponivel);
        _icone.Mostrar();
        _cliente.Comecar();
    }

    /// <summary>
    /// O AppUserModelID, estável para sempre.
    /// </summary>
    /// <remarks>
    /// Segue a convenção da Microsoft, <c>Empresa.Produto</c>, e é o mesmo
    /// identificador que o atalho do menu Iniciar carrega (veja o
    /// <c>instalar.ps1</c>) — as duas metades precisam bater para o Windows
    /// entender que o aviso que aparece e o ícone que está na barra são do mesmo
    /// programa. Trocá-lo faz o sistema esquecer tudo o que sabe sobre ele.
    /// </remarks>
    public const string Identidade = "DanielFreitasDev.Ditador";

    private void MudouOEstado(RetratoDoDitador retrato)
    {
        _icone?.Atualizar(retrato);
        _aviso?.Mostrar(retrato);
        _popup?.Atualizar(retrato);
        _notificador?.Avaliar(retrato);

        // O backend não está no ar. Uma tentativa de subi-lo, uma só, e nunca
        // mais: o laço de reconexão continua tentando falar, mas criar processo
        // em laço é como se fabrica uma máquina de fazer processos zumbis. Se
        // esta tentativa não resolveu, quem resolve é a pessoa, pelo menu.
        if (retrato.Estado == Estado.Indisponivel && !_jaTenteiSubirOBackend)
        {
            _jaTenteiSubirOBackend = true;
            ClienteDoDitador.IniciarBackend();
        }
    }

    private async void EscolheuNoMenu(IconeDaBandeja.ItemDoMenu item)
    {
        switch (item)
        {
            case IconeDaBandeja.ItemDoMenu.DitarAgora:
                await Comandar("toggle");
                break;

            case IconeDaBandeja.ItemDoMenu.Configuracoes:
                // Abre a janela de configurações **do backend** — a mesma tela do
                // Linux, com o modelo, o microfone, o atalho e o tema. Não há uma
                // segunda cópia dela em WinUI, e não deve haver: seriam duas
                // telas para manter iguais, e a que já existe funciona.
                await Comandar("settings");
                break;

            case IconeDaBandeja.ItemDoMenu.IniciarBackend:
                if (!ClienteDoDitador.IniciarBackend())
                {
                    _notificador?.Falha(
                        "Não encontrei o Ditador",
                        "O ditador.exe deveria estar na mesma pasta deste programa. "
                        + "Reinstale com o instalar.ps1.");
                }

                break;

            case IconeDaBandeja.ItemDoMenu.Encerrar:
                await Encerrar();
                break;
        }
    }

    private async Task Comandar(string comando)
    {
        if (_cliente is null)
        {
            return;
        }

        if (await _cliente.EnviarAsync(comando) is null)
        {
            _notificador?.Falha(
                "O Ditador não respondeu",
                "O programa que grava e transcreve não está no ar. "
                + "Use \"Iniciar o Ditador\" no menu do ícone.");
        }
    }

    /// <summary>
    /// Encerra o Ditador inteiro — este processo e o backend.
    /// </summary>
    /// <remarks>
    /// <para>
    /// É a única ação que fecha o backend, e é explícita de propósito. Fechar o
    /// popup ou o aviso não encerra nada: eles somem e o Ditador continua no
    /// canto, pronto para o próximo atalho. É o comportamento que todo aplicativo
    /// de bandeja tem, e o que faz sentido para um programa cujo uso normal é
    /// nunca ser aberto.
    /// </para>
    /// <para>
    /// A ordem importa: primeiro o backend, depois nós. Ao contrário, o ícone
    /// sumiria da barra e o processo pesado ficaria mais um instante de pé — e
    /// quem estivesse olhando concluiria que "fechar não funcionou".
    /// </para>
    /// </remarks>
    private async Task Encerrar()
    {
        if (_cliente is not null)
        {
            await _cliente.EnviarAsync("quit");
        }

        Registro.Info("encerrando a pedido do usuário");
        Descartar();
        Exit();
    }

    private void Descartar()
    {
        _icone?.Dispose();
        _janelaOculta?.Dispose();
        _cliente?.Dispose();
        _aviso?.Fechar();
        _popup?.Fechar();
    }
}
