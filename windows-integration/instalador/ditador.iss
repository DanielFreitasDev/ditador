; O instalador do Ditador para Windows: um .exe que qualquer pessoa executa.
;
; Compilado pelo Inno Setup 6. Quem o chama é o
; `windows-integration\scripts\empacotar-exe.ps1` (na sua máquina) e o
; `release.yml` (na CI) — os dois passando os mesmos três símbolos:
;
;     ISCC.exe /DMyAppVersion=0.6.0 /DBackend=gpu /DOrigem=<pasta com os binários> ditador.iss
;
; ## Por que um instalador, se já existe o instalar.ps1
;
; O `instalar.ps1` compila e instala; ele é para quem tem o código-fonte e a
; caixa de ferramentas inteira na máquina. Este aqui é para quem só quer usar o
; programa: baixa um arquivo, dá dois cliques, e tem o Ditador funcionando —
; sem Rust, sem Visual Studio, sem PowerShell.
;
; O que os dois fazem é **o mesmo**, de propósito: mesma pasta de destino, mesmo
; atalho, mesma chave `Run`, mesmas duas dependências conferidas. Instalar por um
; e desinstalar pelo outro funciona.
;
; ## Sem administrador, e é de propósito
;
; Tudo vai para `%LOCALAPPDATA%\Programs\Ditador`, que é do usuário. Nada em
; `Arquivos de Programas`, nada em `HKLM`, nada de serviço do Windows, nada de
; UAC. O Ditador lê o teclado, abre o microfone e escreve na área de
; transferência — três coisas que uma conta comum faz. Pedir elevação daria a
; ele um poder que ele não usa e o deixaria fora do alcance de quem usa uma
; máquina administrada por outra pessoa.

#define MyAppName "Ditador"
#define MyAppPublisher "Daniel Freitas"
#define MyAppURL "https://github.com/DanielFreitasDev/ditador"
#define MyAppExeName "Ditador.Windows.exe"
#define MyBackendExeName "ditador.exe"

; Os três símbolos que vêm de fora, com padrão para quem chamar sem eles.
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef Backend
  #define Backend "gpu"
#endif
#ifndef Origem
  #define Origem "..\..\publicar\windows"
#endif

; `SemArquivos` compila o script **sem** exigir os binários. É o que a CI usa
; para conferir a sintaxe deste arquivo a cada push: um erro aqui só apareceria
; na hora de publicar a versão, que é o pior momento possível para descobri-lo.
#ifdef SemArquivos
  #define TemArquivos 0
#else
  #define TemArquivos 1
#endif

[Setup]
; O AppId é a identidade da instalação: é por ele que o Windows sabe que a
; versão nova substitui a velha em vez de instalar duas. **Nunca mude este
; GUID** — mudá-lo faria cada versão aparecer como um programa diferente na
; lista de aplicativos instalados.
AppId={{7B3A9C21-4E5D-4F0B-9A6C-1D2E3F4A5B6C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases/latest
VersionInfoVersion={#MyAppVersion}
VersionInfoDescription=Ditado por voz offline com Whisper

DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
; Sem página de grupo do menu Iniciar: é um atalho só, e perguntar em que pasta
; do menu ele deve morar é uma decisão que ninguém quer tomar.
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; O x64 é o único alvo: o whisper.cpp e o Windows App SDK 2.4 são compilados
; para ele, e um instalador que aceitasse rodar noutro lugar só adiaria o erro.
;
; `x64`, e não o `x64compatible` que a documentação de hoje prefere: aquele só
; existe do Inno Setup 6.3 em diante, e a imagem do agente do GitHub nem sempre
; está nessa versão. O `x64` é aceito pelas duas, e aqui a diferença entre eles
; (máquinas ARM emulando x64) não muda nada — o whisper.cpp compilado para x64
; rodaria emulado do mesmo jeito.
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

OutputDir=..\..\target\instalador
OutputBaseFilename=ditador-v{#MyAppVersion}-windows-x64-{#Backend}
SetupIconFile=..\src\Ditador.Windows\Assets\ditador.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} {#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Quem encerra os processos é o `PrepareToInstall`, com o `--encerrar` do canal
; de controle — que fecha o microfone e grava a configuração antes de sair. O
; Gerenciador de Reinicialização do Windows mataria os dois na marra e ainda
; abriria uma tela a mais para a pessoa responder.
CloseApplications=no

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
; Desmarcada por padrão: decidir sozinho que um programa recém-instalado passa a
; abrir em todo login é justamente o que faz as pessoas irem caçar coisas no
; Gerenciador de Tarefas. Quem quer, marca — ou usa depois o interruptor nas
; configurações do próprio Ditador, que escreve na mesma chave.
Name: "iniciarcomowindows"; Description: "Iniciar o Ditador junto com o Windows"; GroupDescription: "Ao entrar na sessão:"; Flags: unchecked

[Files]
#if TemArquivos
; O frontend inteiro (executável, DLLs do .NET e do WinUI, Assets) e o backend
; ao lado dele. É essa vizinhança que faz o frontend achar o backend sem
; procurar no PATH — veja `ClienteDoDitador.IniciarBackend`.
Source: "{#Origem}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
#endif

[Icons]
Name: "{userprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\Assets\ditador.ico"; Comment: "Ditado por voz offline"

[Registry]
; A chave `Run` do usuário atual, e nada mais. Não é tarefa agendada (que
; pediria privilégio para pouco) nem serviço (que roda noutra sessão e não
; enxergaria nem o microfone nem a área de trabalho).
;
; Só o frontend entra aqui: ele sobe o backend quando percebe que ele não está
; no ar, e assim há **um** item de inicialização em vez de dois disputando quem
; chega primeiro.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Ditador"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: iniciarcomowindows

; A identidade das notificações, criada pelo próprio frontend quando ele
; registra o `AppNotificationManager`. Ela não é criada aqui — o `dontcreatekey`
; diz isso —, mas **é** removida na desinstalação: sem isso o Windows continuaria
; listando o Ditador em Configurações → Sistema → Notificações depois de
; desinstalado, um fantasma no painel de outra pessoa.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\DanielFreitasDev.Ditador"; Flags: dontcreatekey uninsdeletekey

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Iniciar o Ditador agora"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Pelo canal de controle, e não à força: assim ele fecha o microfone, grava a
; configuração e sai limpo.
Filename: "{app}\{#MyBackendExeName}"; Parameters: "--encerrar"; Flags: runhidden skipifdoesntexist; RunOnceId: "EncerrarBackend"

[Code]
const
  { O instalador oficial do .NET 10 Desktop Runtime e o do Windows App Runtime
    2.4, pelos endereços curtos que a Microsoft mantém apontando para o patch
    estável mais novo de cada linha. São os mesmos dois que o `instalar.ps1`
    usa — uma fonte só para os dois caminhos de instalação. }
  URL_DOTNET = 'https://aka.ms/dotnet/10.0/windowsdesktop-runtime-win-x64.exe';
  URL_WINAPPRUNTIME = 'https://aka.ms/windowsappsdk/2.4/latest/windowsappruntimeinstall-x64.exe';

var
  PaginaDeDownload: TDownloadWizardPage;

{ ─── as duas dependências ──────────────────────────────────────────────────

  O frontend é dependente de framework: ele usa o .NET e o Windows App Runtime
  que estiverem instalados, em vez de carregar a própria cópia. A escolha está
  explicada no `Ditador.Windows.csproj`; em uma linha, é o que faz as correções
  de segurança dos dois chegarem pelo Windows Update em vez de esperarem uma
  versão nossa.

  O preço dessa escolha é este bloco: alguém precisa garantir que os dois estão
  lá. Esse alguém é o instalador, e não a pessoa que só quer ditar. }

function TemDotNet(): Boolean;
var
  Registros: TFindRec;
  Pasta: String;
begin
  Result := False;
  { A presença é conferida pela pasta do runtime compartilhado, e não pelo
    registro: o `dotnet --list-runtimes` exigiria o SDK no PATH, que quem só vai
    usar o programa não tem. }
  Pasta := ExpandConstant('{commonpf64}\dotnet\shared\Microsoft.WindowsDesktop.App');
  if FindFirst(Pasta + '\10.*', Registros) then begin
    try
      Result := True;
    finally
      FindClose(Registros);
    end;
  end;
end;

function TemWindowsAppRuntime(): Boolean;
var
  Codigo: Integer;
begin
  { Pacote MSIX não tem pasta que se possa conferir sem privilégio, e a chave de
    registro dele não é contrato público. O `Get-AppxPackage` é a pergunta que a
    Microsoft documenta — e é a mesma que o `instalar.ps1` faz. }
  Result := Exec('powershell.exe',
    '-NoProfile -NonInteractive -Command "if (Get-AppxPackage -Name Microsoft.WindowsAppRuntime.2) { exit 0 } else { exit 1 }"',
    '', SW_HIDE, ewWaitUntilTerminated, Codigo) and (Codigo = 0);
end;

function InstalarBaixado(const Arquivo, Parametros, Nome: String): Boolean;
var
  Codigo: Integer;
begin
  Result := Exec(ExpandConstant('{tmp}\') + Arquivo, Parametros, '', SW_SHOWNORMAL,
                 ewWaitUntilTerminated, Codigo) and (Codigo = 0);
  if not Result then
    MsgBox('Não consegui instalar o ' + Nome + ' (código ' + IntToStr(Codigo) + ').' + #13#10#13#10 +
           'A instalação do Ditador continua, mas a interface pode não abrir até que ele seja instalado.',
           mbError, MB_OK);
end;

function AoBaixar(const Url, NomeDoArquivo: String; const Progresso, Total: Int64): Boolean;
begin
  Result := True;
end;

procedure InitializeWizard();
begin
  PaginaDeDownload := CreateDownloadPage(
    'Baixando o que falta',
    'O Ditador precisa de dois componentes do Windows que ainda não estão nesta máquina.',
    @AoBaixar);
end;

{ ─── parar o que estiver rodando ───────────────────────────────────────────

  Trocar um .exe que está em execução falha no Windows — o arquivo fica travado
  pelo sistema, e a mensagem fala de acesso negado sem dizer por quê. Encerrar
  os dois antes de copiar é o que torna instalar por cima uma operação normal, e
  é o que faz "atualizar" ser só executar o instalador novo. }

procedure EncerrarOQueEstiverRodando();
var
  Codigo: Integer;
  CaminhoDoBackend: String;
begin
  CaminhoDoBackend := ExpandConstant('{app}\{#MyBackendExeName}');
  if FileExists(CaminhoDoBackend) then begin
    { Primeiro pelo canal de controle: ele fecha o microfone e grava a
      configuração. Devolve erro quando não há ninguém ouvindo, e tudo bem. }
    Exec(CaminhoDoBackend, '--encerrar', '', SW_HIDE, ewWaitUntilTerminated, Codigo);
    Sleep(700);
  end;
  { E depois à força, para o que não tiver saído — inclusive uma instância de
    uma versão antiga que não conhecesse o `--encerrar`. }
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM Ditador.Windows.exe', '', SW_HIDE, ewWaitUntilTerminated, Codigo);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM ditador.exe', '', SW_HIDE, ewWaitUntilTerminated, Codigo);
  Sleep(300);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  FaltaDotNet, FaltaRuntime: Boolean;
begin
  Result := '';
  EncerrarOQueEstiverRodando();

  FaltaDotNet := not TemDotNet();
  FaltaRuntime := not TemWindowsAppRuntime();
  if not (FaltaDotNet or FaltaRuntime) then
    Exit;

  PaginaDeDownload.Clear;
  if FaltaDotNet then
    PaginaDeDownload.Add(URL_DOTNET, 'windowsdesktop-runtime.exe', '');
  if FaltaRuntime then
    PaginaDeDownload.Add(URL_WINAPPRUNTIME, 'windowsappruntime.exe', '');

  PaginaDeDownload.Show;
  try
    try
      PaginaDeDownload.Download;
    except
      { Sem internet, ou o endereço mudou. Não é motivo para desistir da
        instalação: o Ditador em si funciona: quem não abre sem estes dois é o
        frontend, e a pessoa pode instalá-los depois. Dizer isso é melhor do que
        abortar tudo com um erro de rede. }
      MsgBox('Não consegui baixar os componentes que faltam:' + #13#10#13#10 +
             GetExceptionMessage + #13#10#13#10 +
             'A instalação continua. Instale depois o .NET 10 Desktop Runtime e o ' +
             'Windows App Runtime 2.x, ou rode o instalador de novo com internet.',
             mbInformation, MB_OK);
      Exit;
    end;

    if FaltaDotNet then
      InstalarBaixado('windowsdesktop-runtime.exe', '/install /quiet /norestart', '.NET 10 Desktop Runtime');
    if FaltaRuntime then
      InstalarBaixado('windowsappruntime.exe', '--quiet', 'Windows App Runtime 2.x');
  finally
    PaginaDeDownload.Hide;
  end;
end;

{ ─── a desinstalação ───────────────────────────────────────────────────────

  Sai tudo o que o instalador pôs. Os dados do usuário — configuração, modelos
  e logs — só saem se a pessoa disser que sim, e a pergunta diz o tamanho do que
  vai embora: o modelo tem 574 MB e leva um bom tempo para baixar, e quem
  desinstala para reinstalar não quer perdê-lo. }

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Config, Dados: String;
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;

  Config := ExpandConstant('{userappdata}\ditador');
  Dados := ExpandConstant('{localappdata}\ditador');
  if not (DirExists(Config) or DirExists(Dados)) then
    Exit;

  if MsgBox('Apagar também a configuração, os modelos do Whisper e os logs?' + #13#10#13#10 +
            Config + #13#10 + Dados + #13#10#13#10 +
            'O modelo tem cerca de 574 MB e precisaria ser baixado de novo. ' +
            'Se você pretende reinstalar o Ditador, responda Não.',
            mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then begin
    DelTree(Config, True, True, True);
    DelTree(Dados, True, True, True);
  end;
end;
