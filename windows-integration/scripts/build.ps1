# Compila o Ditador para Windows — o backend em Rust e o frontend em WinUI.
#
#     .\windows-integration\scripts\build.ps1                # os dois, Vulkan
#     .\windows-integration\scripts\build.ps1 -Backend cpu
#     .\windows-integration\scripts\build.ps1 -Backend cuda
#     .\windows-integration\scripts\build.ps1 -Testar        # fmt, clippy e testes antes
#     .\windows-integration\scripts\build.ps1 -SemFrontend   # só o Rust
#     .\windows-integration\scripts\build.ps1 -SomenteFrontend
#
# Este arquivo é, sobretudo, um registro. Compilar o whisper.cpp no Windows exige
# quatro ajustes de ambiente que não são óbvios, e cada um deles falha com uma
# mensagem que aponta para o lugar errado. Estão todos aqui, comentados, para que
# ninguém precise descobri-los duas vezes.

[CmdletBinding()]
param(
    [ValidateSet('vulkan', 'cpu', 'cuda')]
    [string] $Backend = 'vulkan',

    [switch] $Testar,

    # Compila só o backend em Rust.
    [switch] $SemFrontend,

    # Compila só o frontend em C#. Não precisa de nada do lado do Rust — nem do
    # Visual Studio, nem do CMake, nem do Vulkan SDK.
    [switch] $SomenteFrontend,

    [ValidateSet('Debug', 'Release')]
    [string] $Configuracao = 'Release',

    # Onde o cargo grava os artefatos. Não mexa sem ler o comentário sobre
    # MAX_PATH mais abaixo — este valor não é gosto pessoal.
    [string] $PastaDeBuild = (Join-Path $env:USERPROFILE '.ditador-build')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-MscVer {
    # O `_MSC_VER` do compilador ativo, que é como o nvcc identifica o host.
    # A banner do cl.exe traz "Versão 19.50.35726"; `_MSC_VER` são os dois
    # primeiros componentes grudados: 19.50 → 1950.
    $banner = (& cl.exe 2>&1 | Select-Object -First 2) -join ' '
    if ($banner -match '(\d+)\.(\d+)\.\d+') {
        return [int]("$($Matches[1])$($Matches[2])")
    }
    throw "não consegui descobrir a versão do cl.exe a partir de: $banner"
}

function Select-CudaCompativel {
    # Escolhe um CUDA Toolkit que aceite o compilador que temos — e explica com
    # números quando não houver nenhum.
    #
    # Isto existe porque o nvcc tem uma lista **fechada** de versões de MSVC
    # suportadas, escrita num `#error` dentro de `include/crt/host_config.h`.
    # Uma versão de MSVC mais nova do que o CUDA conhece é recusada, mesmo que
    # fosse compilar perfeitamente. Nesta máquina, em agosto de 2026:
    #
    #     CUDA 12.8 recusa _MSC_VER >= 1950
    #     CUDA 13.2 recusa _MSC_VER >= 1960
    #     Visual Studio 2026 é _MSC_VER 1950
    #
    # ou seja, o 12.8 recusa o VS 2026 **por um número**. E o `CUDA_PATH` do
    # sistema apontava justamente para o 12.8, porque foi o instalado primeiro.
    # A mensagem que se recebe no fim de vários minutos de configuração é
    # "unsupported Microsoft Visual Studio version", sem dizer qual é a sua, qual
    # é o limite, nem que existe outro toolkit na máquina que serviria.
    $msc = Get-MscVer
    $raizes = Get-ChildItem 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA' -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending

    if (-not $raizes) {
        throw "O backend cuda precisa do CUDA Toolkit (nvcc). Baixe-o em developer.nvidia.com/cuda-downloads."
    }

    $recusas = @()
    foreach ($r in $raizes) {
        $header = Join-Path $r.FullName 'include\crt\host_config.h'
        # Sem o cabeçalho não dá para saber; tentar é melhor do que descartar.
        $limite = $null
        if (Test-Path $header) {
            $linha = Select-String -Path $header -Pattern '_MSC_VER >= (\d+)' | Select-Object -First 1
            if ($linha) { $limite = [int]$linha.Matches[0].Groups[1].Value }
        }

        if (-not $limite -or $msc -lt $limite) {
            $env:CUDA_PATH = $r.FullName
            $env:CUDAToolkit_ROOT = $r.FullName
            $env:Path = (Join-Path $r.FullName 'bin') + ';' + $env:Path

            # `CUDACXX` é a única das quatro que o CMake de fato obedece para
            # **escolher o compilador**. Com só `CUDA_PATH`, `CUDAToolkit_ROOT` e
            # o PATH ajustados — os três apontando para o toolkit certo, e o bin
            # dele na frente do PATH —, o CMake ainda assim resolveu
            # `CMAKE_CUDA_COMPILER` para o nvcc do *outro* toolkit, e o build
            # morreu com "unsupported Microsoft Visual Studio version" que já
            # tinha sido resolvida. Custou duas rodadas de vinte minutos.
            $env:CUDACXX = Join-Path $r.FullName 'bin\nvcc.exe'

            Write-Host "  CUDA           $($r.Name) (aceita _MSC_VER $msc)"
            return
        }
        $recusas += "$($r.Name) recusa _MSC_VER >= $limite"
    }

    throw @"
Nenhum CUDA Toolkit instalado aceita este compilador.
    seu MSVC: _MSC_VER $msc
    $($recusas -join "`n    ")
Instale um CUDA mais novo, ou compile com -Backend vulkan (que nesta máquina
é tão rápido quanto e não depende do toolkit).
"@
}

function Build-Frontend {
    param([string] $Raiz, [string] $Configuracao)

    # O frontend não passa por nada do ambiente do Visual Studio: ele é C# puro,
    # construído pelo SDK do .NET, e a única coisa de que precisa é do `dotnet` no
    # caminho. É por isso que ele pode ser compilado sozinho, com `-SomenteFrontend`,
    # numa máquina que nunca compilou o Rust.
    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if (-not $dotnet) {
        $padrao = Join-Path $env:ProgramFiles 'dotnet\dotnet.exe'
        if (Test-Path $padrao) {
            $dotnet = Get-Item $padrao
        } else {
            throw @"
O SDK do .NET 10 não foi encontrado. Instale com:
    winget install --id Microsoft.DotNet.SDK.10
"@
        }
    }

    $solucao = Join-Path $Raiz 'windows-integration\Ditador.Windows.sln'
    Write-Host "`nCompilando o frontend WinUI ($Configuracao)" -ForegroundColor Cyan
    & $dotnet.Source build $solucao -c $Configuracao --nologo
    if ($LASTEXITCODE) { throw 'dotnet build falhou' }

    $saida = Join-Path $Raiz "windows-integration\src\Ditador.Windows\bin\x64\$Configuracao\net10.0-windows10.0.26100.0\win-x64\Ditador.Windows.exe"
    if (-not (Test-Path $saida)) { throw "o build terminou sem erro mas não produziu $saida" }
    Write-Host "  $saida"
}

$raiz = Resolve-Path (Join-Path $PSScriptRoot '..\..')

if ($SomenteFrontend) {
    Build-Frontend -Raiz $raiz -Configuracao $Configuracao
    Write-Host "`nPronto." -ForegroundColor Green
    return
}

Push-Location $raiz
try {
    . (Join-Path $PSScriptRoot 'ambiente.ps1')

    # ---------------------------------------------------------------- MAX_PATH
    #
    # O caminho do `target/` precisa ser CURTO. Não é manha: é aritmética.
    #
    # O backend Vulkan do ggml compila um gerador de shaders como sub-projeto
    # CMake, e o caminho que ele monta a partir do `target/` tem **210
    # caracteres fixos**:
    #
    #   …\build\whisper-rs-sys-<hash>\out\build\ggml\src\ggml-vulkan
    #     \vulkan-shaders-gen-prefix\src\vulkan-shaders-gen-build
    #     \CMakeFiles\CMakeScratch\TryCompile-XXXXXX\CMakeFiles\cmTC_XXXXX.dir
    #     \vc140.pdb
    #
    # Com o limite de 260 do Windows, sobram 50 caracteres para o `target/`. Um
    # projeto em `C:\Users\<nome>\<pasta>\<projeto>\target` passa disso com
    # facilidade, e o erro que aparece é
    #
    #     fatal error C1041: não é possível abrir banco de dados do programa …
    #     Se mais de um CL.EXE escrever no mesmo arquivo .PDB, use /FS
    #
    # que fala de PDB e de paralelismo e não menciona comprimento de caminho —
    # mandando quem lê investigar concorrência de build, que não é o problema.
    #
    # E não adianta ligar `LongPathsEnabled` no registro: o `cl.exe` não declara
    # suporte a caminhos longos no manifesto dele, então a chave não vale para
    # ele. Foi verificado nesta máquina, com a chave já ligada.
    $env:CARGO_TARGET_DIR = $PastaDeBuild
    $orcamento = 50
    if ($PastaDeBuild.Length -gt $orcamento -and $Backend -eq 'vulkan') {
        throw @"
A pasta de build tem $($PastaDeBuild.Length) caracteres e o limite prático é $orcamento.
    $PastaDeBuild
O backend Vulkan monta caminhos de 210 caracteres a partir dela, e o cl.exe
para de funcionar ao passar dos 260 do Windows. Use -PastaDeBuild com um
caminho mais curto, por exemplo C:\Users\$env:USERNAME\.ditador-build
"@
    }

    # ------------------------------------------------------- o gerador do CMake
    #
    # `Ninja`, e não o gerador do Visual Studio. O sub-projeto de shaders do
    # ggml-vulkan faz a própria configuração do CMake do zero, e com o gerador do
    # Visual Studio ela não encontra o compilador:
    #
    #     No CMAKE_C_COMPILER could be found.
    #
    # ainda que o `cl.exe` esteja no PATH da sessão. Com o Ninja ele acha, porque
    # o Ninja procura o compilador no PATH em vez de pedir o toolset ao MSBuild.
    #
    # As três variáveis abaixo precisam ser esvaziadas junto: o crate `cmake`
    # passa a *instância* do Visual Studio adiante, e o Ninja recusa
    # ("does not support instance specification").
    $env:CMAKE_GENERATOR = 'Ninja'
    $env:CMAKE_GENERATOR_INSTANCE = ''
    $env:CMAKE_GENERATOR_PLATFORM = ''
    $env:CMAKE_GENERATOR_TOOLSET = ''

    # ------------------------------------------- o que cada backend exige a mais
    switch ($Backend) {
        'vulkan' {
            if (-not $env:VULKAN_SDK -or -not (Test-Path $env:VULKAN_SDK)) {
                throw "O backend vulkan precisa do Vulkan SDK (não só do driver): winget install --id KhronosGroup.VulkanSDK"
            }
        }
        'cuda' { Select-CudaCompativel }
    }

    $features = @('--no-default-features', '--features', $Backend)

    if ($Testar) {
        Write-Host "`nConferindo o código" -ForegroundColor Cyan
        # A mesma ordem do CLAUDE.md, e todos precisam passar.
        cargo fmt --check
        if ($LASTEXITCODE) { throw 'cargo fmt --check reprovou. Rode: cargo fmt' }

        cargo test @features
        if ($LASTEXITCODE) { throw 'cargo test falhou' }

        cargo clippy @features --all-targets
        if ($LASTEXITCODE) { throw 'cargo clippy reprovou' }
    }

    Write-Host "`nCompilando (backend $Backend)" -ForegroundColor Cyan
    cargo build --release @features
    if ($LASTEXITCODE) { throw "cargo build falhou (backend $Backend)" }

    $exe = Join-Path $PastaDeBuild 'release\ditador.exe'
    if (-not (Test-Path $exe)) { throw "o build terminou sem erro mas não produziu $exe" }

    $tamanho = [math]::Round((Get-Item $exe).Length / 1MB, 1)
    Write-Host "`nBackend pronto." -ForegroundColor Green
    Write-Host "  $exe  ($tamanho MB)"
    Write-Host "  Confira com: & '$exe' --versao"

    if (-not $SemFrontend) {
        Build-Frontend -Raiz $raiz -Configuracao $Configuracao
    }

    Write-Host "`nPronto." -ForegroundColor Green
} finally {
    Pop-Location
}
