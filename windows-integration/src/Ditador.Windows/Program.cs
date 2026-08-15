using Ditador.Windows.Servicos;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;

namespace Ditador.Windows;

/// <summary>
/// A porta de entrada do processo — escrita à mão para caber a instância única.
/// </summary>
/// <remarks>
/// <para>
/// O modelo de projeto do WinUI gera um <c>Main</c> sozinho, e ele serve para
/// quase tudo. Aqui não serve por um motivo de ordem: a decisão "já existe um
/// Ditador.Windows nesta sessão?" precisa ser tomada <b>antes</b> de o XAML
/// subir. Depois de <c>Application.Start</c> já há uma janela sendo criada, e
/// desistir nesse ponto faria a segunda instância piscar na tela antes de morrer.
/// Por isso o <c>DISABLE_XAML_GENERATED_MAIN</c> no .csproj e este arquivo.
/// </para>
/// <para>
/// <b>Instância única pelo <c>AppInstance</c>, e não por invenção nossa.</b> É a
/// API de ciclo de vida do Windows App SDK, e ela faz duas coisas que um mutex
/// nomeado não faz: registra a instância por uma chave (aqui, o AppUserModelID) e
/// **redireciona a ativação** para quem chegou primeiro. Assim, clicar de novo no
/// atalho não abre um segundo ícone na bandeja — ele leva o painel de status à
/// tela, que é o que a pessoa queria.
/// </para>
/// <para>
/// Repare que este é o segundo mecanismo de instância única do Ditador, e não uma
/// duplicata: o backend em Rust tem o dele, o dono do named pipe. São dois
/// processos independentes, cada um garantindo que não há dois de si mesmo.
/// </para>
/// </remarks>
public static class Program
{
    [STAThread]
    private static int Main(string[] argumentos)
    {
        // Obrigatório antes de qualquer coisa de WinRT num aplicativo
        // desempacotado com Main próprio. Sem isto, a primeira chamada de
        // AppInstance falha com um erro de COM que não explica nada.
        WinRT.ComWrappersSupport.InitializeComWrappers();

        if (JaHaOutro())
        {
            return 0;
        }

        Application.Start(parametros =>
        {
            // O contexto de sincronização é o que faz um `await` num tratador de
            // clique voltar para a thread da interface. Sem ele, a continuação
            // cairia numa thread do pool e a primeira propriedade de XAML tocada
            // ali derrubaria o processo — com `Main` gerado isto vem de graça, e
            // com `Main` próprio é responsabilidade nossa.
            _ = parametros;
            var contexto = new DispatcherQueueSynchronizationContext(
                DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(contexto);
            _ = new App();
        });

        return 0;
    }

    /// <summary>
    /// Já existe um Ditador.Windows nesta sessão? Se sim, entrega a vez a ele.
    /// </summary>
    private static bool JaHaOutro()
    {
        try
        {
            // A chave é o AppUserModelID: um identificador que já existe, já é
            // estável e já significa "este aplicativo". Inventar uma segunda
            // string aqui seria criar mais uma coisa para manter igual.
            var principal = AppInstance.FindOrRegisterForKey(App.Identidade);
            if (principal.IsCurrent)
            {
                return false;
            }

            Registro.Info("já há um Ditador.Windows nesta sessão; passando a vez para ele");
            var ativacao = AppInstance.GetCurrent().GetActivatedEventArgs();
            principal.RedirectActivationToAsync(ativacao).AsTask().GetAwaiter().GetResult();
            return true;
        }
        catch (Exception e)
        {
            // Falhar aqui não pode impedir o programa de subir: sem o registro de
            // instância, o pior que acontece é haver dois ícones — chato, não
            // fatal. Ficar sem interface porque o registro de instância falhou
            // seria trocar o todo pela parte.
            Registro.Aviso($"não consegui registrar a instância: {e.Message}");
            return false;
        }
    }
}
