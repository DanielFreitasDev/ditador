# Monta o instalador .exe do Ditador — o arquivo que vai na release.
#
#     .\windows-integration\scripts\empacotar-exe.ps1                 # backend vulkan → …-gpu.exe
#     .\windows-integration\scripts\empacotar-exe.ps1 -Backend cpu    # …-cpu.exe
#     .\windows-integration\scripts\empacotar-exe.ps1 -SemCompilar    # usa o que já foi compilado
#
# É o par do `empacotar.sh` do Linux: compila, junta o que precisa ir junto e
# produz **um** arquivo para outra pessoa instalar. O trabalho de instalador em
# si — pasta de destino, atalho, chave `Run`, dependências, desinstalador — está
# todo no `..\instalador\ditador.iss`, que é quem o Inno Setup lê.
#
# A CI chama exatamente este script. Não há um caminho de empacotamento "de
# verdade" escondido no YAML do workflow: o que se publica é o que sai daqui, e
# dá para reproduzi-lo na sua máquina antes de qualquer coisa ir para o GitHub.

[CmdletBinding()]
param(
    [ValidateSet('vulkan', 'cpu', 'cuda')]
    [string] $Backend = 'vulkan',

    # Usa os binários já compilados em vez de compilar de novo.
    [switch] $SemCompilar,

    [string] $PastaDeBuild = (Join-Path $env:USERPROFILE '.ditador-build')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$raiz = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$frontendBin = Join-Path $raiz 'windows-integration\src\Ditador.Windows\bin\x64\Release\net10.0-windows10.0.26100.0\win-x64'
$iss = Join-Path $raiz 'windows-integration\instalador\ditador.iss'
$estagio = Join-Path $raiz "target\instalador\estagio-$Backend"
$saida = Join-Path $raiz 'target\instalador'

function Etapa($texto) { Write-Host "`n$texto" -ForegroundColor Cyan }
function Feito($texto) { Write-Host "  $texto" }

# O nome do arquivo diz a variante, e não o backend: "gpu" e "cpu" é o que quem
# baixa precisa decidir. "vulkan" é detalhe de implementação nosso, e obrigaria a
# pessoa a saber o que é Vulkan para escolher um arquivo.
$rotulo = @{ vulkan = 'gpu'; cpu = 'cpu'; cuda = 'cuda' }[$Backend]

# O Cargo.toml é a única fonte da verdade da versão no projeto inteiro.
$versao = (Select-String -Path (Join-Path $raiz 'Cargo.toml') -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value

function Find-Iscc {
    $candidatos = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
    )
    foreach ($c in $candidatos) { if (Test-Path $c) { return $c } }

    $doPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($doPath) { return $doPath.Source }

    throw @"
O Inno Setup 6 não foi encontrado. Instale com:
    winget install --id JRSoftware.InnoSetup
"@
}

# ------------------------------------------------------------------ compilar
if (-not $SemCompilar) {
    Etapa "Compilando (backend $Backend)"
    & (Join-Path $PSScriptRoot 'build.ps1') -Backend $Backend
    if ($LASTEXITCODE) { throw 'a compilação falhou' }
}

$exeBackend = Join-Path $PastaDeBuild 'release\ditador.exe'
if (-not (Test-Path $exeBackend)) { throw "não achei $exeBackend. Rode sem -SemCompilar." }
if (-not (Test-Path (Join-Path $frontendBin 'Ditador.Windows.exe'))) {
    throw "não achei o frontend em $frontendBin. Rode sem -SemCompilar."
}

# ------------------------------------------------------------------ estágio
#
# Uma pasta com exatamente o que vai ser instalado, e nada mais. O `.iss` copia
# `{#Origem}\*` inteiro — apontá-lo direto para a saída do dotnet levaria junto
# os .pdb e os arquivos de build, e apontá-lo para dois lugares não é possível.
Etapa 'Juntando o que vai no pacote'
if (Test-Path $estagio) { Remove-Item $estagio -Recurse -Force }
New-Item -ItemType Directory -Force $estagio | Out-Null

Copy-Item -Path (Join-Path $frontendBin '*') -Destination $estagio -Recurse -Force
# Só os símbolos de depuração saem. Eles são uma boa fatia do tamanho e não
# servem para nada na máquina de quem instala. Nada além deles: um `.xml` ou um
# `.json` que pareça sobra pode ser o manifesto de que o WinUI depende, e um
# instalador menor não vale um programa que não abre.
Get-ChildItem $estagio -Recurse -Filter '*.pdb' | Remove-Item -Force -ErrorAction SilentlyContinue
Copy-Item -Path $exeBackend -Destination $estagio -Force

$mb = [math]::Round((Get-ChildItem $estagio -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1MB, 1)
Feito "$mb MB em $estagio"

# --------------------------------------------------------------- o instalador
Etapa 'Inno Setup'
$iscc = Find-Iscc
Feito $iscc

& $iscc /Qp `
    "/DMyAppVersion=$versao" `
    "/DBackend=$rotulo" `
    "/DOrigem=$estagio" `
    "/O$saida" `
    $iss
if ($LASTEXITCODE) { throw "o Inno Setup falhou (código $LASTEXITCODE)" }

$pacote = Join-Path $saida "ditador-v$versao-windows-x64-$rotulo.exe"
if (-not (Test-Path $pacote)) { throw "o Inno Setup terminou sem erro mas não produziu $pacote" }

$tamanho = [math]::Round((Get-Item $pacote).Length / 1MB, 1)
Write-Host "`nPronto." -ForegroundColor Green
Write-Host "  $pacote  ($tamanho MB)"
