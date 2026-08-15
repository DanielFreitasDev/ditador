using System.Runtime.InteropServices;
using Ditador.Windows.Interop;
using Ditador.Windows.Modelos;
using Microsoft.Win32;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.UI.Shell;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Ditador.Windows.Servicos;

/// <summary>
/// O ícone do Ditador na área de notificação, com a dica, o menu e o clique.
/// </summary>
/// <remarks>
/// <para>
/// <b>Só existe um, e ele é deste processo.</b> O backend em Rust nunca cria
/// ícone no Windows — a decisão está escrita em <c>src/plataforma/windows/tray.rs</c>.
/// Em resumo: <c>Shell_NotifyIcon</c> não responde "alguém já mostra este
/// aplicativo?", então dois processos tentando produziriam dois ícones lado a
/// lado sem nenhum dos dois perceber. O dono foi decidido em tempo de projeto, e
/// é quem tem janela e laço de mensagens: este.
/// </para>
/// <para>
/// <b>Versão 4 do protocolo do ícone.</b> É a recomendada pela documentação e a
/// que traz a posição do ícone junto com o clique — sem ela, para saber onde
/// abrir o menu, sobraria a posição do cursor, que não é a mesma coisa quando o
/// clique vem do teclado ou de acessibilidade.
/// </para>
/// </remarks>
internal sealed class IconeDaBandeja : IDisposable
{
    /// <summary>A mensagem que o Shell manda para a nossa janela a cada clique.</summary>
    /// <remarks>
    /// <c>WM_APP</c> em diante é a faixa reservada para o aplicativo definir o que
    /// quiser; abaixo dela estão as do sistema e as das bibliotecas de controle.
    /// </remarks>
    private const uint MensagemDoIcone = PInvoke.WM_APP + 1;

    /// <summary>
    /// A identidade do ícone, estável para sempre.
    /// </summary>
    /// <remarks>
    /// <para>
    /// É por este GUID que o Windows lembra onde o usuário pôs o ícone — se está
    /// visível na barra ou escondido no estouro — e a preferência sobrevive a
    /// reinstalações e atualizações. Gerar um novo a cada execução, ou a cada
    /// versão, jogaria essa escolha fora toda vez.
    /// </para>
    /// <para>
    /// <b>Não troque este número.</b> Ele é tão parte do contrato com o sistema
    /// quanto os nomes de estado são com a extensão do GNOME.
    /// </para>
    /// </remarks>
    private static readonly Guid Identidade = new("7C3F1E4A-5B62-4B0E-9A1F-2D8C6E5F0A31");

    private readonly JanelaDeMensagens _janela;
    private readonly uint _taskbarCriada;
    private HICON _icone;
    private Estado _estadoDesenhado = (Estado)(-1);
    private bool _temaClaroDesenhado;
    private string _dica = "Ditador";
    private RetratoDoDitador _retrato = RetratoDoDitador.Indisponivel;
    private bool _adicionado;
    private bool _semGuid;
    private bool _descartado;

    /// <summary>Clique com o botão esquerdo: abre o popup de status.</summary>
    public event Action? Clicado;

    /// <summary>Um item do menu foi escolhido.</summary>
    public event Action<ItemDoMenu>? MenuEscolhido;

    public IconeDaBandeja(JanelaDeMensagens janela)
    {
        _janela = janela;

        // A mensagem que o Explorer difunde quando ele volta a existir. O nome é
        // registrado (e não um número fixo) porque é assim que o Windows publica
        // mensagens de sistema desde sempre: quem registra o mesmo nome recebe o
        // mesmo número.
        _taskbarCriada = PInvoke.RegisterWindowMessage("TaskbarCreated");

        var anterior = janela.AoReceber;
        janela.AoReceber = (mensagem, wParam, lParam) =>
            Tratar(mensagem, wParam, lParam) ?? anterior?.Invoke(mensagem, wParam, lParam);
    }

    /// <summary>Põe o ícone na área de notificação.</summary>
    public void Mostrar()
    {
        Redesenhar(forcar: true);
    }

    /// <summary>
    /// Onde o ícone está na tela agora, em pixels físicos.
    /// </summary>
    /// <remarks>
    /// <para>
    /// É o que o popup usa para se encostar nele. Vem do
    /// <c>Shell_NotifyIconGetRect</c>, que é quem sabe a resposta — o ícone pode
    /// estar na barra, escondido na área de estouro (e aí o retângulo é o do
    /// botão de estouro), num monitor secundário, ou numa barra de tarefas
    /// vertical.
    /// </para>
    /// <para>
    /// <c>null</c> quando não dá para saber, o que acontece de verdade enquanto o
    /// Explorer reinicia. Quem chama tem um plano B.
    /// </para>
    /// </remarks>
    public unsafe RECT? Retangulo()
    {
        var identificacao = new NOTIFYICONIDENTIFIER
        {
            cbSize = (uint)sizeof(NOTIFYICONIDENTIFIER),
            hWnd = _janela.Handle,
        };

        if (_semGuid)
        {
            identificacao.uID = 1;
        }
        else
        {
            identificacao.guidItem = Identidade;
        }

        return PInvoke.Shell_NotifyIconGetRect(identificacao, out var retangulo).Succeeded
            ? retangulo
            : null;
    }

    /// <summary>Atualiza o ícone e a dica para o estado de agora.</summary>
    public void Atualizar(RetratoDoDitador retrato)
    {
        _retrato = retrato;
        _dica = MontarDica(retrato);
        Redesenhar(forcar: false);
    }

    /// <summary>
    /// A dica que aparece ao pousar o ponteiro — e que o Narrator lê.
    /// </summary>
    /// <remarks>
    /// É aqui que a distinção fina entre "carregando" e "transcrevendo" continua
    /// existindo: o desenho de 16 pixels junta os dois num símbolo de "espere",
    /// porque em 16 pixels não cabe mais do que isso, mas o texto cabe. Também é
    /// por isso que o estado nunca é dito só pela cor do emblema.
    /// </remarks>
    private static string MontarDica(RetratoDoDitador retrato) => retrato.Estado switch
    {
        Estado.Indisponivel => "Ditador — indisponível",
        Estado.Erro when retrato.Mensagem.Length > 0 => $"Ditador — erro: {Enxugar(retrato.Mensagem)}",
        _ => $"Ditador — {retrato.Descricao.TrimEnd('…')}",
    };

    /// <summary>
    /// Encurta um texto para caber na dica.
    /// </summary>
    /// <remarks>
    /// O campo da dica tem 128 caracteres contando o terminador, e o Shell
    /// simplesmente trunca o que passa — sem reticências e, pior, no meio de um
    /// par substituto se o texto tiver emoji ou acento fora do plano básico. Aqui
    /// o corte é nosso, num limite folgado, e com reticências.
    /// </remarks>
    private static string Enxugar(string texto)
    {
        const int Limite = 80;
        var linha = texto.ReplaceLineEndings(" ");
        return linha.Length <= Limite ? linha : string.Concat(linha.AsSpan(0, Limite - 1), "…");
    }

    private LRESULT? Tratar(uint mensagem, WPARAM wParam, LPARAM lParam)
    {
        if (mensagem == _taskbarCriada)
        {
            // O Explorer reiniciou e levou todos os ícones com ele. Quem estava
            // lá antes precisa se apresentar de novo — e como o nosso registro
            // anterior morreu junto, isto não duplica nada.
            Registro.Info("o Explorer reiniciou; recolocando o ícone");
            _adicionado = false;
            Redesenhar(forcar: true);
            return new LRESULT(0);
        }

        if (mensagem == PInvoke.WM_SETTINGCHANGE)
        {
            // O usuário trocou entre tema claro e escuro. O Windows não recolore
            // ícone de bandeja: quem troca somos nós.
            var qual = lParam.Value == 0 ? null : Marshal.PtrToStringUni(lParam);
            if (qual == "ImmersiveColorSet")
            {
                Redesenhar(forcar: false);
            }

            return null;
        }

        if (mensagem != MensagemDoIcone)
        {
            return null;
        }

        // Na versão 4 do protocolo, o `wParam` traz a posição do ícone na tela e
        // o `lParam` traz o evento. Nas versões antigas era o contrário — e é por
        // isso que exemplos da internet parecem trocados.
        var evento = (uint)(lParam.Value & 0xFFFF);
        var x = (short)(wParam.Value & 0xFFFF);
        var y = (short)((wParam.Value >> 16) & 0xFFFF);

        switch (evento)
        {
            case PInvoke.WM_LBUTTONUP:
                Clicado?.Invoke();
                return new LRESULT(0);

            case PInvoke.WM_CONTEXTMENU:
            case PInvoke.WM_RBUTTONUP:
                AbrirMenu(x, y);
                return new LRESULT(0);
        }

        return null;
    }

    /// <summary>Os itens do menu do ícone.</summary>
    /// <remarks>
    /// Os números importam: são eles que o <c>TrackPopupMenuEx</c> devolve. Ficam
    /// explícitos para que acrescentar um item no meio não mude o significado dos
    /// outros.
    /// </remarks>
    internal enum ItemDoMenu
    {
        DitarAgora = 1,
        Configuracoes = 2,
        Encerrar = 3,
        IniciarBackend = 4,
    }

    private void AbrirMenu(int x, int y)
    {
        var menu = PInvoke.CreatePopupMenu_SafeHandle();
        if (menu.IsInvalid)
        {
            Registro.Aviso("não consegui criar o menu do ícone");
            return;
        }

        using (menu)
        {
            // Primeira linha: o estado, sem ação. É o mesmo cabeçalho que o menu
            // da extensão do GNOME tem, e serve para a pergunta mais frequente
            // ("está gravando?") ser respondida sem precisar clicar em nada.
            PInvoke.InsertMenu(menu, uint.MaxValue,
                MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING | MENU_ITEM_FLAGS.MF_DISABLED,
                0, $"Ditador — {_retrato.Descricao}");
            PInvoke.InsertMenu(menu, uint.MaxValue,
                MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_SEPARATOR, 0, (string?)null);

            var disponivel = _retrato.Estado != Estado.Indisponivel;
            if (disponivel)
            {
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING
                    | (_retrato.Estado == Estado.Pronto || _retrato.Gravando
                        ? MENU_ITEM_FLAGS.MF_ENABLED
                        : MENU_ITEM_FLAGS.MF_GRAYED),
                    (uint)ItemDoMenu.DitarAgora,
                    _retrato.Gravando ? "Parar de ditar" : "Ditar agora");
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING,
                    (uint)ItemDoMenu.Configuracoes, "Configurações");
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_SEPARATOR, 0, (string?)null);
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING,
                    (uint)ItemDoMenu.Encerrar, "Encerrar Ditador");
            }
            else
            {
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING,
                    (uint)ItemDoMenu.IniciarBackend, "Iniciar o Ditador");
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_SEPARATOR, 0, (string?)null);
                PInvoke.InsertMenu(menu, uint.MaxValue,
                    MENU_ITEM_FLAGS.MF_BYPOSITION | MENU_ITEM_FLAGS.MF_STRING,
                    (uint)ItemDoMenu.Encerrar, "Fechar este ícone");
            }

            // Sem isto o menu não fecha quando se clica fora dele: o Windows só
            // desfaz o menu de quem está em primeiro plano, e o nosso processo
            // não está — a janela dona do menu é invisível. É um dos truques mais
            // antigos do Win32 e continua sendo o caminho documentado.
            PInvoke.SetForegroundWindow(_janela.Handle);

            var escolha = PInvoke.TrackPopupMenuEx(
                menu,
                (uint)(TRACK_POPUP_MENU_FLAGS.TPM_RIGHTBUTTON | TRACK_POPUP_MENU_FLAGS.TPM_RETURNCMD),
                x, y, _janela.Handle, null);

            // E sem isto o menu fica pendurado na tela depois de escolhido, pelo
            // mesmo motivo: a fila de mensagens precisa de um empurrão para o
            // Windows perceber que perdemos o primeiro plano.
            PInvoke.PostMessage(_janela.Handle, PInvoke.WM_NULL, default, default);

            if (escolha != 0)
            {
                MenuEscolhido?.Invoke((ItemDoMenu)escolha.Value);
            }
        }
    }

    /// <summary>
    /// A barra de tarefas está em tema claro?
    /// </summary>
    /// <remarks>
    /// <c>SystemUsesLightTheme</c>, e não <c>AppsUseLightTheme</c>: o primeiro é o
    /// tema da barra e do menu Iniciar — que é onde o nosso ícone aparece — e o
    /// segundo é o das janelas dos aplicativos. O Windows deixa escolher os dois
    /// separadamente, e quem usa "aplicativos claros com barra escura" veria um
    /// ícone invisível se olhássemos o campo errado.
    /// </remarks>
    private static bool BarraClara()
    {
        try
        {
            using var chave = Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            return chave?.GetValue("SystemUsesLightTheme") is int valor && valor != 0;
        }
        catch (Exception e)
        {
            Registro.Aviso($"não consegui ler o tema do sistema: {e.Message}");
            return false;
        }
    }

    private void Redesenhar(bool forcar)
    {
        var estado = EstadoDoIcone(_retrato.Estado);
        var claro = BarraClara();

        if (!forcar && _adicionado && estado == _estadoDesenhado && claro == _temaClaroDesenhado)
        {
            // Nada mudou no desenho; só a dica pode ter mudado, e ela é barata.
            Enviar(NOTIFY_ICON_MESSAGE.NIM_MODIFY);
            return;
        }

        if (_adicionado && claro != _temaClaroDesenhado)
        {
            // Vale uma linha no log: "o ícone do Ditador sumiu da barra" é uma
            // queixa possível, e o tema é a primeira coisa a conferir quando ela
            // aparece — um glifo da cor errada é invisível, não ausente.
            Registro.Detalhe($"a barra passou a tema {(claro ? "claro" : "escuro")}; trocando o ícone");
        }

        var novo = Carregar(estado, claro);
        if (!novo.IsNull)
        {
            var velho = _icone;
            _icone = novo;
            _estadoDesenhado = estado;
            _temaClaroDesenhado = claro;
            // O ícone antigo só é destruído depois de o novo estar no lugar: o
            // Shell ainda pode estar desenhando o anterior neste instante.
            if (!velho.IsNull)
            {
                PInvoke.DestroyIcon(velho);
            }
        }

        Enviar(_adicionado ? NOTIFY_ICON_MESSAGE.NIM_MODIFY : NOTIFY_ICON_MESSAGE.NIM_ADD);
    }

    /// <summary>
    /// Qual dos quatro desenhos representa este estado.
    /// </summary>
    /// <remarks>
    /// A mesma redução que o ícone da barra faz no Linux (<c>icones::Estado</c>):
    /// carregar o modelo e transcrever viram os dois "trabalhando". A regra de
    /// qual estado é qual continua morando no Rust; aqui só se escolhe a imagem.
    /// </remarks>
    private static Estado EstadoDoIcone(Estado estado) => estado switch
    {
        Estado.Carregando or Estado.Transcrevendo => Estado.Transcrevendo,
        Estado.Gravando => Estado.Gravando,
        Estado.Erro or Estado.Indisponivel => Estado.Erro,
        _ => Estado.Pronto,
    };

    private HICON Carregar(Estado estado, bool claro)
    {
        var nome = estado switch
        {
            Estado.Gravando => "gravando",
            Estado.Transcrevendo => "trabalhando",
            Estado.Erro => "falhou",
            _ => "pronto",
        };
        var arquivo = Path.Combine(
            AppContext.BaseDirectory, "Assets", $"bandeja-{nome}-{(claro ? "claro" : "escuro")}.ico");

        // O tamanho vem do DPI da janela, e não de um 16 fixo: em 150% o Shell
        // quer 24 pixels, em 200% quer 32, e um bitmap de 16 esticado é
        // exatamente o borrão que se vê nos aplicativos que não fazem esta conta.
        // O .ico tem todos esses tamanhos desenhados de verdade (veja
        // `scripts/gerar-icones.py`), então aqui não há ampliação nenhuma.
        var dpi = PInvoke.GetDpiForWindow(_janela.Handle);
        var lado = (int)PInvoke.GetSystemMetricsForDpi(SYSTEM_METRICS_INDEX.SM_CXSMICON, dpi);

        var carregado = PInvoke.LoadImage(
            null, arquivo, GDI_IMAGE_TYPE.IMAGE_ICON, lado, lado,
            IMAGE_FLAGS.LR_LOADFROMFILE | IMAGE_FLAGS.LR_DEFAULTCOLOR);

        if (carregado.IsInvalid)
        {
            Registro.Aviso($"não consegui carregar {arquivo}: {Marshal.GetLastPInvokeErrorMessage()}");
            return HICON.Null;
        }

        // O handle sai do invólucro seguro de propósito: quem o destrói é o
        // `Redesenhar`, depois de o Shell já estar com o ícone novo, e um
        // `SafeHandle` o liberaria no fim deste método.
        return (HICON)carregado.DangerousGetHandle();
    }

    private unsafe void Enviar(NOTIFY_ICON_MESSAGE acao)
    {
        var dados = new NOTIFYICONDATAW
        {
            cbSize = (uint)sizeof(NOTIFYICONDATAW),
            hWnd = _janela.Handle,
            uFlags = NOTIFY_ICON_DATA_FLAGS.NIF_ICON
                     | NOTIFY_ICON_DATA_FLAGS.NIF_MESSAGE
                     | NOTIFY_ICON_DATA_FLAGS.NIF_TIP
                     | NOTIFY_ICON_DATA_FLAGS.NIF_SHOWTIP,
            uCallbackMessage = MensagemDoIcone,
            hIcon = _icone,
        };

        if (_semGuid)
        {
            dados.uID = 1;
        }
        else
        {
            dados.uFlags |= NOTIFY_ICON_DATA_FLAGS.NIF_GUID;
            dados.guidItem = Identidade;
        }

        _dica.AsSpan(0, Math.Min(_dica.Length, 127)).CopyTo(dados.szTip.AsSpan());

        if (PInvoke.Shell_NotifyIcon(acao, dados))
        {
            if (acao == NOTIFY_ICON_MESSAGE.NIM_ADD)
            {
                _adicionado = true;
                // A versão precisa ser declarada **depois** do NIM_ADD; antes
                // dele não há a quem declarar. É esta chamada que liga o
                // comportamento moderno: posição junto com o clique, dica pelo
                // sistema e mensagem de menu de contexto própria.
                var versao = dados;
                versao.Anonymous.uVersion = PInvoke.NOTIFYICON_VERSION_4;
                PInvoke.Shell_NotifyIcon(NOTIFY_ICON_MESSAGE.NIM_SETVERSION, versao);
            }

            return;
        }

        var erro = Marshal.GetLastPInvokeError();

        // Um ícone identificado por GUID fica preso ao **caminho do executável**
        // que o registrou. Rodar a partir de outra pasta — o que acontece o tempo
        // todo entre compilar em `bin\` e instalar em `%LOCALAPPDATA%` — faz o
        // NIM_ADD falhar, e a mensagem não diz nada disso. Um NIM_DELETE limpa o
        // registro antigo e a segunda tentativa passa.
        if (acao == NOTIFY_ICON_MESSAGE.NIM_ADD && !_semGuid)
        {
            Registro.Aviso($"o ícone não entrou (erro {erro}); limpando o registro anterior do GUID");
            PInvoke.Shell_NotifyIcon(NOTIFY_ICON_MESSAGE.NIM_DELETE, dados);
            if (PInvoke.Shell_NotifyIcon(NOTIFY_ICON_MESSAGE.NIM_ADD, dados))
            {
                _adicionado = true;
                var versao = dados;
                versao.Anonymous.uVersion = PInvoke.NOTIFYICON_VERSION_4;
                PInvoke.Shell_NotifyIcon(NOTIFY_ICON_MESSAGE.NIM_SETVERSION, versao);
                return;
            }

            // Ainda não. Sem o GUID perde-se a memória de onde o usuário pôs o
            // ícone, mas ter ícone é melhor do que não ter.
            Registro.Aviso("desistindo do GUID e usando identificação por número");
            _semGuid = true;
            Enviar(NOTIFY_ICON_MESSAGE.NIM_ADD);
            return;
        }

        Registro.Aviso($"Shell_NotifyIcon({acao}) falhou: erro {erro}");
    }

    public void Dispose()
    {
        if (_descartado)
        {
            return;
        }

        _descartado = true;
        if (_adicionado)
        {
            unsafe
            {
                var dados = new NOTIFYICONDATAW
                {
                    cbSize = (uint)sizeof(NOTIFYICONDATAW),
                    hWnd = _janela.Handle,
                };
                if (_semGuid)
                {
                    dados.uID = 1;
                }
                else
                {
                    dados.uFlags = NOTIFY_ICON_DATA_FLAGS.NIF_GUID;
                    dados.guidItem = Identidade;
                }

                PInvoke.Shell_NotifyIcon(NOTIFY_ICON_MESSAGE.NIM_DELETE, dados);
            }
        }

        if (!_icone.IsNull)
        {
            PInvoke.DestroyIcon(_icone);
        }
    }
}
