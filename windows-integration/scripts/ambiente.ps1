# Carrega, na sessão atual do PowerShell, o ambiente de compilação do Visual
# Studio — e confere que o resto da caixa de ferramentas está no lugar.
#
# Existe porque compilar o Ditador no Windows não depende só do cargo. O
# whisper.cpp é C++ construído por CMake no meio do `cargo build`, e o backend
# Vulkan vai mais fundo ainda: o `ggml-vulkan` compila um gerador de shaders como
# um sub-projeto CMake à parte, que faz a *própria* configuração do zero. Essa
# configuração aninhada não herda nada do MSBuild que a chamou — ela procura
# `cl.exe` no PATH e, não achando, morre com
#
#     No CMAKE_C_COMPILER could be found.
#
# que é uma mensagem que não menciona Vulkan, nem shaders, nem cargo, e manda
# quem a lê procurar o problema no lugar errado. Um `cargo build` num PowerShell
# comum falha assim; num Developer PowerShell, passa. A diferença é só o PATH, e
# é isso que este arquivo resolve.
#
# Uso:
#     . .\windows-integration\scripts\ambiente.ps1      # com o ponto na frente
#
# O ponto importa: sem ele o script roda num escopo filho, ajusta o ambiente
# daquele escopo e o descarta ao terminar — a sessão de quem chamou não muda.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Sync-AmbienteDoRegistro {
    # Relê do registro as variáveis de máquina e de usuário.
    #
    # Um processo do Windows recebe uma *cópia* do ambiente quando nasce e nunca
    # mais a atualiza. Quem instalou o Vulkan SDK, o LLVM ou o Rust com um
    # terminal já aberto continua sem enxergá-los ali — o instalador escreveu no
    # registro, e este processo está lendo a cópia de antes. O sintoma é o pior
    # possível: a ferramenta está instalada, o usuário a vê no Explorer, e o
    # build insiste que ela não existe.
    #
    # O PATH é concatenado (máquina primeiro, como o Windows faz) em vez de
    # substituído, porque o do processo pode ter acréscimos que só existem aqui.
    foreach ($escopo in 'Machine', 'User') {
        foreach ($par in [System.Environment]::GetEnvironmentVariables($escopo).GetEnumerator()) {
            if ($par.Key -eq 'Path') { continue }
            if (-not (Test-Path "Env:$($par.Key)")) {
                Set-Item -Path "Env:$($par.Key)" -Value $par.Value
            }
        }
    }

    $daMaquina = [System.Environment]::GetEnvironmentVariable('Path', 'Machine')
    $doUsuario = [System.Environment]::GetEnvironmentVariable('Path', 'User')
    $atual = $env:Path -split ';'
    $novos = @($daMaquina, $doUsuario) -join ';' -split ';' |
        Where-Object { $_ -and ($atual -notcontains $_) }
    if ($novos) {
        $env:Path = ($atual + $novos | Where-Object { $_ }) -join ';'
    }
}

function Find-VisualStudio {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe não encontrado. Instale o Visual Studio Build Tools com o workload 'Desenvolvimento para desktop com C++'."
    }

    # `-latest` com `-products *` pega Build Tools, Community, Professional ou
    # Enterprise, que servem igualmente — o que precisamos é do compilador, e ele
    # é o mesmo nos quatro. O requisito de componente é explícito para que a
    # falha diga *o que* instalar, e não só "não achei".
    $caminho = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath 2>$null | Select-Object -First 1

    if (-not $caminho) {
        throw "Nenhum Visual Studio com as ferramentas C++ x64. No Visual Studio Installer, acrescente 'MSVC v14x - VS C++ x64/x86 build tools'."
    }
    return $caminho
}

function Import-VsDevEnv {
    param([Parameter(Mandatory)] [string] $InstallationPath)

    $vsdevcmd = Join-Path $InstallationPath 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path $vsdevcmd)) {
        throw "VsDevCmd.bat não encontrado em $InstallationPath."
    }

    # O VsDevCmd.bat é um script do cmd.exe: não há como ele alterar o ambiente
    # de um PowerShell diretamente. O jeito suportado é rodá-lo no cmd, mandar
    # imprimir o ambiente resultante e trazer as variáveis de volta uma a uma.
    #
    # `-arch=x64 -host_arch=x64` porque o alvo é x64 e o compilador também deve
    # ser o de 64 bits — o padrão do VsDevCmd ainda é o x86 hospedeiro, que
    # compilaria o whisper.cpp num processo de 32 bits e ficaria sem memória em
    # arquivos grandes.
    $linhas = & "$env:ComSpec" /s /c "`"$vsdevcmd`" -arch=x64 -host_arch=x64 -no_logo && set" 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "VsDevCmd.bat falhou (código $LASTEXITCODE):`n$($linhas -join "`n")"
    }

    $importadas = 0
    foreach ($linha in $linhas) {
        if ($linha -match '^([^=]+)=(.*)$') {
            Set-Item -Path "Env:$($Matches[1])" -Value $Matches[2]
            $importadas++
        }
    }
    return $importadas
}

# ---------------------------------------------------------------- as dependências
#
# Cada uma destas falta de um jeito diferente e nenhuma delas se anuncia. Ficam
# aqui, conferidas de uma vez, para que a resposta venha antes do build de
# quinze minutos e não no meio dele.

function Assert-Ferramenta {
    param(
        [Parameter(Mandatory)] [string] $Comando,
        [Parameter(Mandatory)] [string] $Nome,
        [Parameter(Mandatory)] [string] $ComoInstalar
    )
    $achado = Get-Command $Comando -ErrorAction SilentlyContinue
    if (-not $achado) {
        throw "$Nome não encontrado ($Comando). Instale com:`n    $ComoInstalar"
    }
    return $achado.Source
}

Write-Host 'Ambiente de compilação do Ditador para Windows' -ForegroundColor Cyan

Sync-AmbienteDoRegistro

$vs = Find-VisualStudio
$n = Import-VsDevEnv -InstallationPath $vs
Write-Host "  Visual Studio  $vs ($n variáveis)"

$cl = Assert-Ferramenta -Comando 'cl' -Nome 'Compilador C++ da Microsoft' `
    -ComoInstalar "winget install --id Microsoft.VisualStudio.2026.BuildTools"
Write-Host "  cl.exe         $cl"

$cargo = Assert-Ferramenta -Comando 'cargo' -Nome 'Rust' `
    -ComoInstalar "winget install --id Rustlang.Rustup"
Write-Host "  cargo          $((& cargo --version) -replace '^cargo ', '')"

Assert-Ferramenta -Comando 'cmake' -Nome 'CMake' `
    -ComoInstalar "winget install --id Kitware.CMake" | Out-Null
Write-Host "  cmake          $((& cmake --version | Select-Object -First 1) -replace '^cmake version ', '')"

# O bindgen do whisper-rs-sys precisa da libclang para gerar as bindings. No
# Linux o crate traz um bindings.rs pronto e ninguém percebe; no MSVC não há
# pronto, e sem isto o build morre dizendo "Unable to find libclang", que também
# não menciona whisper nem Rust.
if (-not $env:LIBCLANG_PATH) {
    $palpite = Join-Path $env:ProgramFiles 'LLVM\bin'
    if (Test-Path (Join-Path $palpite 'libclang.dll')) {
        $env:LIBCLANG_PATH = $palpite
    }
}
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
    throw "libclang.dll não encontrada. Instale o LLVM (winget install --id LLVM.LLVM) ou aponte LIBCLANG_PATH para a pasta que a contém."
}
Write-Host "  libclang       $env:LIBCLANG_PATH"

# O Vulkan SDK só é exigido pela feature `vulkan`; quem compila só CPU ou CUDA
# não precisa dele, e transformar sua ausência em erro fecharia a porta para
# esses dois sem motivo. Por isso aqui é aviso, e a cobrança fica no build.ps1,
# que sabe qual backend foi pedido.
if ($env:VULKAN_SDK -and (Test-Path $env:VULKAN_SDK)) {
    Write-Host "  Vulkan SDK     $env:VULKAN_SDK"
} else {
    Write-Host "  Vulkan SDK     ausente (só faz falta para --features vulkan)" -ForegroundColor DarkYellow
}

if (Get-Command nvcc -ErrorAction SilentlyContinue) {
    Write-Host "  CUDA           $((& nvcc --version | Select-String 'release') -replace '.*release ', '' -replace ',.*', '')"
} else {
    Write-Host "  CUDA           ausente (só faz falta para --features cuda)" -ForegroundColor DarkYellow
}

Write-Host 'Pronto.' -ForegroundColor Green
