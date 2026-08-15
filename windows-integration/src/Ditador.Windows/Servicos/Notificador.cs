using Ditador.Windows.Modelos;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;

namespace Ditador.Windows.Servicos;

/// <summary>
/// As notificações do sistema — usadas com parcimônia.
/// </summary>
/// <remarks>
/// <para>
/// <b>Só o que exige ação.</b> Microfone que não abriu, modelo que falta, backend
/// que não subiu: coisas que a pessoa precisa resolver e que ela pode não estar
/// olhando na hora em que acontecem. Começou a gravar, terminou de transcrever,
/// deu tudo certo — nada disso vira notificação: o aviso na tela já disse, e um
/// toast por ditado seria dezenas por dia na Central de Ações de alguém.
/// </para>
/// <para>
/// <b><c>Microsoft.Windows.AppNotifications</c>, do Windows App SDK</b>, e não os
/// balões do <c>Shell_NotifyIcon</c>. São coisas diferentes: o balão é uma bolha
/// efêmera presa ao ícone, e a notificação moderna respeita o Assistente de Foco,
/// o Não Perturbe e as preferências por aplicativo, e fica na Central de Ações
/// para ser lida depois. Quem estava numa reunião com o Foco ligado encontra o
/// aviso quando volta.
/// </para>
/// <para>
/// Nada aqui sai da máquina. As notificações são locais, montadas neste processo.
/// </para>
/// </remarks>
internal sealed class Notificador : IDisposable
{
    private string _ultimoErro = string.Empty;
    private bool _registrado;

    public Notificador()
    {
        try
        {
            // Em um aplicativo desempacotado é este registro que cria a
            // identidade de notificação no registro do usuário, ligada ao
            // AppUserModelID que o `App` já definiu. Sem ele, nenhuma notificação
            // aparece — e sem erro nenhum, o que torna a falha difícil de achar.
            AppNotificationManager.Default.Register();
            _registrado = true;
        }
        catch (Exception e)
        {
            // Ficar sem notificação é perder um aviso; derrubar o programa é
            // perder o ditado. O log guarda o motivo.
            Registro.Aviso($"não consegui registrar as notificações: {e.Message}");
        }
    }

    /// <summary>Decide se este estado merece uma notificação.</summary>
    public void Avaliar(RetratoDoDitador retrato)
    {
        if (retrato.Estado != Estado.Erro || retrato.Mensagem.Length == 0)
        {
            // Saiu do erro: a próxima falha, mesmo com o mesmo texto, volta a ser
            // notícia. Sem isto, um problema que se repete depois de resolvido
            // ficaria em silêncio.
            _ultimoErro = string.Empty;
            return;
        }

        if (retrato.Mensagem == _ultimoErro)
        {
            // O mesmo erro republicado — o backend manda o retrato inteiro a cada
            // mudança, e mexer no volume do microfone não é motivo para notificar
            // de novo.
            return;
        }

        _ultimoErro = retrato.Mensagem;
        Falha("O Ditador encontrou um problema", retrato.Mensagem);
    }

    /// <summary>Mostra uma notificação de falha.</summary>
    public void Falha(string titulo, string corpo)
    {
        if (!_registrado)
        {
            return;
        }

        try
        {
            var aviso = new AppNotificationBuilder()
                .AddText(titulo)
                .AddText(corpo)
                .BuildNotification();

            // Sem som e sem insistência: o Ditador avisa, não interrompe.
            aviso.SuppressDisplay = false;
            AppNotificationManager.Default.Show(aviso);
            Registro.Info($"notificação: {titulo} — {corpo}");
        }
        catch (Exception e)
        {
            Registro.Aviso($"não consegui notificar: {e.Message}");
        }
    }

    public void Dispose()
    {
        if (!_registrado)
        {
            return;
        }

        try
        {
            AppNotificationManager.Default.Unregister();
        }
        catch (Exception e)
        {
            Registro.Aviso($"não consegui desregistrar as notificações: {e.Message}");
        }
    }
}
