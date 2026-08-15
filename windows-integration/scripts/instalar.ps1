# Instala o Ditador no Windows, para o usuário atual.
#
#     .\windows-integration\scripts\instalar.ps1
#     .\windows-integration\scripts\instalar.ps1 -Backend cpu
#     .\windows-integration\scripts\instalar.ps1 -SemCompilar     # usa o que já foi compilado
#     .\windows-integration\scripts\instalar.ps1 -SemIniciarComOWindows
#
# ## Sem administrador, e é de propósito
#
# Tudo vai para `%LOCALAPPDATA%\Programs\Ditador`, que é do usuário. Nada em
# `Program Files`, nada em `HKLM`, nada de serviço do Windows, nada de UAC. O
# Ditador lê o teclado, abre o microfone e escreve na área de transferência —
# três coisas que uma conta comum faz. Pedir elevação para instalá-lo daria a ele
# um poder que ele não usa e deixaria o programa fora do alcance de quem usa uma
# máquina administrada por outra pessoa.
#
# ## O que é instalado
#
#     %LOCALAPPDATA%\Programs\Ditador\
#         ditador.exe            o backend em Rust (áudio, Whisper, atalho)
#         Ditador.Windows.exe    o frontend (ícone, aviso, popup)
#         Assets\, *.dll         o que os dois precisam
#
# O modelo do Whisper **não** vem junto: são 574 MB que não mudam entre versões,
# o programa os baixa sozinho e o `ditador --baixar-modelo` resolve por terminal.

[CmdletBinding()]
param(
    [ValidateSet('vulkan', 'cpu', 'cuda')]
    [string] $Backend = 'vulkan',

    # Usa os binários já compilados em vez de compilar de novo.
    [switch] $SemCompilar,

    # Não registra o Ditador para subir com a sessão.
    [switch] $SemIniciarComOWindows,

    # Não tenta instalar o .NET nem o Windows App Runtime que faltarem.
    [switch] $SemDependencias,

    [string] $Destino = (Join-Path $env:LOCALAPPDATA 'Programs\Ditador')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$raiz = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$frontend = Join-Path $raiz 'windows-integration\src\Ditador.Windows'
$pastaDeBuild = Join-Path $env:USERPROFILE '.ditador-build'

function Etapa($texto) { Write-Host "`n$texto" -ForegroundColor Cyan }
function Feito($texto) { Write-Host "  $texto" }
function Alerta($texto) { Write-Host "  $texto" -ForegroundColor DarkYellow }

# ------------------------------------------------------- as duas dependências
#
# O frontend é dependente de framework: ele usa o .NET e o Windows App Runtime
# que estiverem instalados, em vez de carregar a própria cópia. A escolha está
# explicada no `Ditador.Windows.csproj` e no README; em uma linha, é o que faz as
# correções de segurança dos dois chegarem pelo Windows Update em vez de esperar
# um lançamento nosso.
#
# O preço dessa escolha é este bloco: alguém precisa garantir que os dois estão
# lá. Esse alguém é o instalador, e não a pessoa que só quer ditar.

function Test-DotNet {
    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if (-not $dotnet) { return $false }
    $runtimes = & dotnet --list-runtimes 2>$null
    return [bool]($runtimes | Where-Object { $_ -match '^Microsoft\.WindowsDesktop\.App 10\.' })
}

function Test-WindowsAppRuntime {
    # A linha 2.x do Windows App SDK. O `-like` cobre 2.4.0 e os patches de
    # servicing da mesma linha, que são compatíveis por definição.
    $pacote = Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2' -ErrorAction SilentlyContinue |
        Where-Object { $_.Architecture -eq 'X64' }
    return [bool]$pacote
}

function Install-DotNet {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw @"
Falta o .NET 10 Desktop Runtime e não há winget nesta máquina para instalá-lo.
Baixe-o em https://dotnet.microsoft.com/download/dotnet/10.0 (Desktop Runtime, x64)
e rode este script de novo.
"@
    }
    Alerta 'instalando o .NET 10 Desktop Runtime pelo winget…'
    winget install --id Microsoft.DotNet.DesktopRuntime.10 --accept-source-agreements --accept-package-agreements --silent
    if ($LASTEXITCODE) { throw "o winget não conseguiu instalar o .NET 10 (código $LASTEXITCODE)" }
}

function Install-WindowsAppRuntime {
    # O instalador oficial, pelo endereço curto que a Microsoft mantém apontando
    # para a versão estável mais nova da linha 2.x. Ele instala por máquina se
    # puder e por usuário se não puder — e não pede elevação no segundo caso.
    $url = 'https://aka.ms/windowsappsdk/2.4/latest/windowsappruntimeinstall-x64.exe'
    $arquivo = Join-Path $env:TEMP 'windowsappruntimeinstall-x64.exe'
    Alerta 'baixando o Windows App Runtime 2.4…'
    Invoke-WebRequest -Uri $url -OutFile $arquivo -UseBasicParsing
    Alerta 'instalando…'
    $processo = Start-Process $arquivo -ArgumentList '--quiet' -Wait -PassThru
    if ($processo.ExitCode -ne 0) {
        throw "o instalador do Windows App Runtime devolveu $($processo.ExitCode)"
    }
    Remove-Item $arquivo -ErrorAction SilentlyContinue
}

Etapa 'Conferindo as dependências'
if (Test-DotNet) {
    Feito '.NET 10 Desktop Runtime: presente'
} elseif ($SemDependencias) {
    throw 'falta o .NET 10 Desktop Runtime (e -SemDependencias foi pedido)'
} else {
    Install-DotNet
    Feito '.NET 10 Desktop Runtime: instalado'
}

if (Test-WindowsAppRuntime) {
    Feito 'Windows App Runtime 2.x: presente'
} elseif ($SemDependencias) {
    throw 'falta o Windows App Runtime 2.x (e -SemDependencias foi pedido)'
} else {
    Install-WindowsAppRuntime
    Feito 'Windows App Runtime 2.x: instalado'
}

# ------------------------------------------------------------------ compilar
if (-not $SemCompilar) {
    Etapa "Compilando o backend (backend $Backend)"
    & (Join-Path $PSScriptRoot 'build.ps1') -Backend $Backend -SemFrontend
    if ($LASTEXITCODE) { throw 'a compilação do backend falhou' }

    Etapa 'Compilando o frontend'
    & (Join-Path $PSScriptRoot 'build.ps1') -SomenteFrontend
    if ($LASTEXITCODE) { throw 'a compilação do frontend falhou' }
}

$exeBackend = Join-Path $pastaDeBuild 'release\ditador.exe'
$saidaFrontend = Join-Path $frontend 'bin\x64\Release\net10.0-windows10.0.26100.0\win-x64'
if (-not (Test-Path $exeBackend)) { throw "não achei $exeBackend. Rode sem -SemCompilar." }
if (-not (Test-Path (Join-Path $saidaFrontend 'Ditador.Windows.exe'))) {
    throw "não achei o frontend em $saidaFrontend. Rode sem -SemCompilar."
}

# ------------------------------------------------------------------- parar
#
# Trocar um .exe que está em execução falha no Windows — o arquivo fica travado
# pelo sistema, e a mensagem fala de acesso negado sem dizer por quê. Encerrar os
# dois antes de copiar é o que torna reinstalar por cima uma operação normal.
Etapa 'Encerrando o que estiver rodando'
$rodando = Get-Process -Name 'Ditador.Windows' -ErrorAction SilentlyContinue
if ($rodando) {
    $rodando | Stop-Process -Force
    Feito 'frontend encerrado'
}
if (Test-Path (Join-Path $Destino 'ditador.exe')) {
    # Pelo canal de controle, e não à força: assim ele fecha o microfone, grava a
    # configuração e sai limpo. O `--encerrar` devolve erro quando não há
    # ninguém, e isso aqui não é um problema.
    & (Join-Path $Destino 'ditador.exe') --encerrar 2>$null | Out-Null
    Start-Sleep -Milliseconds 500
}
Get-Process -Name 'ditador' -ErrorAction SilentlyContinue | Stop-Process -Force
Feito 'nada mais rodando'

# ------------------------------------------------------------------- copiar
Etapa "Instalando em $Destino"
New-Item -ItemType Directory -Force $Destino | Out-Null

# O frontend inteiro (executável, DLLs do .NET e do WinUI, ícones) e o backend
# ao lado dele. É essa vizinhança que faz o frontend achar o backend sem
# procurar no PATH — veja `ClienteDoDitador.IniciarBackend`.
Copy-Item -Path (Join-Path $saidaFrontend '*') -Destination $Destino -Recurse -Force
Copy-Item -Path $exeBackend -Destination $Destino -Force
Feito "$((Get-ChildItem $Destino -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1MB -as [int]) MB copiados"

# ---------------------------------------------------------------- menu Iniciar
$menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Ditador.lnk'
$shell = New-Object -ComObject WScript.Shell
$atalho = $shell.CreateShortcut($menu)
$atalho.TargetPath = Join-Path $Destino 'Ditador.Windows.exe'
$atalho.WorkingDirectory = $Destino
$atalho.IconLocation = Join-Path $Destino 'Assets\ditador.ico'
$atalho.Description = 'Ditado por voz offline'
$atalho.Save()
Feito 'atalho no menu Iniciar'

# ------------------------------------------------------------------- startup
#
# A chave `Run` do **usuário atual**, e nada mais. Não é uma tarefa agendada (que
# pediria privilégio para pouco), não é um serviço (que roda noutra sessão e não
# enxergaria nem o microfone nem a área de trabalho) e não é a chave da máquina.
#
# Quem quiser desligar depois não precisa deste script: o item aparece no
# Gerenciador de Tarefas → Aplicativos de Inicialização e em Configurações →
# Aplicativos → Inicializar, com o interruptor que o Windows dá a qualquer
# programa. É por isso que a chave `Run` continua sendo o caminho certo para um
# aplicativo desempacotado — o `StartupTask` do MSIX faz o mesmo, e só existe
# para quem está empacotado.
$chave = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ($SemIniciarComOWindows) {
    Remove-ItemProperty -Path $chave -Name 'Ditador' -ErrorAction SilentlyContinue
    Feito 'não vai iniciar com a sessão (a seu pedido)'
} else {
    # Só o frontend entra aqui. Ele sobe o backend quando percebe que ele não
    # está no ar, e assim há **um** item de inicialização em vez de dois
    # disputando quem chega primeiro.
    Set-ItemProperty -Path $chave -Name 'Ditador' `
        -Value ('"{0}"' -f (Join-Path $Destino 'Ditador.Windows.exe'))
    Feito 'vai iniciar com a sessão'
}

# -------------------------------------------------------------------- subir
Etapa 'Iniciando'
Start-Process (Join-Path $Destino 'Ditador.Windows.exe')
Start-Sleep -Seconds 3
$backendVivo = $null -ne (Get-Process -Name 'ditador' -ErrorAction SilentlyContinue)
$frontendVivo = $null -ne (Get-Process -Name 'Ditador.Windows' -ErrorAction SilentlyContinue)
Feito "frontend: $(if ($frontendVivo) {'no ar'} else {'não subiu'})"
Feito "backend:  $(if ($backendVivo) {'no ar'} else {'não subiu'})"

Write-Host "`nPronto." -ForegroundColor Green
Write-Host @"
  O ícone do Ditador está na área de notificação (pode estar escondido no
  botão "^" — arraste-o para a barra se quiser vê-lo sempre).

  Segure a tecla Pause, fale, solte. O texto vai para a área de transferência.

  Primeira vez? O modelo ainda não está aqui. Baixe-o pelas configurações do
  próprio programa ou por:
      & '$(Join-Path $Destino "ditador.exe")' --baixar-modelo

  Diagnóstico:   & '$(Join-Path $Destino "ditador.exe")' --diagnostico
  Desinstalar:   .\windows-integration\scripts\desinstalar.ps1
"@
