# Gera a versão portátil do Ditador para Windows: um .zip que se descompacta e
# roda, sem instalador, sem administrador e sem nada para instalar antes.
#
#     .\windows-integration\scripts\empacotar-portatil.ps1                 # backend vulkan → …-gpu-portatil.zip
#     .\windows-integration\scripts\empacotar-portatil.ps1 -Backend cpu
#     .\windows-integration\scripts\empacotar-portatil.ps1 -SemCompilar    # reusa o ditador.exe já compilado
#     .\windows-integration\scripts\empacotar-portatil.ps1 -ComModelo      # leva o modelo dentro
#     .\windows-integration\scripts\empacotar-portatil.ps1 -ComModelo -Modelo small-q5_1
#
# É o par do `empacotar-portatil.sh` do Linux, e existe pelo mesmo motivo: o
# instalador .exe pressupõe poder instalar — pouco, mas instala —, e há máquina
# de trabalho onde nem isso se pode. O .zip usa o modo portátil que o programa
# já tem (src/portatil.rs, espelhado no Registro.cs do frontend): o arquivo
# `portatil` ao lado dos executáveis faz tudo morar na pasta `Dados\` vizinha.
#
# ## Autocontido, e por que isto contradiz o instalador de propósito
#
# O frontend WinUI é dependente de framework no instalador — o .csproj explica:
# assim as correções de segurança do .NET e do Windows App Runtime chegam pelo
# Windows Update. O portátil publica **autocontido** (`SelfContained` e
# `WindowsAppSDKSelfContained`), porque a premissa dele é a contrária: numa
# máquina onde não se instala nada, não há quem ponha os dois runtimes lá — e o
# instalador deles é justamente o que a máquina restrita recusa. O preço é o
# tamanho e runtimes congelados na data do build; quem atualiza o pacote
# atualiza os dois junto, e o LEIA-ME diz isso.
#
# A escolha é feita aqui, na linha de comando do publish, e não no .csproj: o
# projeto continua dependente de framework para todo o resto (build, testes,
# instalador), e só este empacotamento pede diferente.
#
# ## O modelo dentro do pacote
#
# Como no Linux: o pacote da release não leva o modelo (centenas de megabytes
# que não mudam entre versões; o programa baixa sozinho). O `-ComModelo` é para
# a máquina sem internet — gera-se o pacote gordo numa máquina que tem, e o
# pendrive leva tudo.

[CmdletBinding()]
param(
    [ValidateSet('vulkan', 'cpu', 'cuda')]
    [string] $Backend = 'vulkan',

    # Reusa o ditador.exe já compilado em vez de compilar de novo. O publish do
    # frontend roda sempre: ele é outra compilação (autocontida), e não a que o
    # build.ps1 faz.
    [switch] $SemCompilar,

    # Leva o modelo do Whisper dentro do pacote, para máquina sem internet.
    [switch] $ComModelo,

    # Qual modelo levar; vazio usa o sugerido para o backend escolhido.
    [string] $Modelo = '',

    [string] $PastaDeBuild = (Join-Path $env:USERPROFILE '.ditador-build')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$raiz = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$projeto = Join-Path $raiz 'windows-integration\src\Ditador.Windows\Ditador.Windows.csproj'
$publicado = Join-Path $raiz "target\portatil\publish-$Backend"
$estagio = Join-Path $raiz 'target\portatil\Ditador-Portatil'
$saida = Join-Path $raiz 'target\portatil'

function Etapa($texto) { Write-Host "`n$texto" -ForegroundColor Cyan }
function Feito($texto) { Write-Host "  $texto" }

# O rótulo do arquivo diz a variante, não o backend — mesma regra do
# empacotar-exe.ps1: quem baixa decide entre "gpu" e "cpu", não entre APIs.
$rotulo = @{ vulkan = 'gpu'; cpu = 'cpu'; cuda = 'cuda' }[$Backend]

# Os sugeridos são os mesmos do CATALOGO de src/modelo.rs (PADRAO e PADRAO_CPU),
# e há um teste em Rust lendo estas linhas para que os lados não se separem: na
# CPU o modelo grande transcreve mais devagar do que se fala, e embutir 574 MB
# do modelo errado seria pior do que não embutir nada.
if (-not $Modelo) {
    $Modelo = if ($Backend -eq 'cpu') { 'small-q5_1' } else { 'large-v3-turbo-q5_0' }
}

# O Cargo.toml é a única fonte da verdade da versão no projeto inteiro.
$versao = (Select-String -Path (Join-Path $raiz 'Cargo.toml') -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value

# ------------------------------------------------------------------ o backend
if (-not $SemCompilar) {
    Etapa "Compilando o backend (backend $Backend)"
    & (Join-Path $PSScriptRoot 'build.ps1') -Backend $Backend -SemFrontend
    if ($LASTEXITCODE) { throw 'a compilação do backend falhou' }
}

$exeBackend = Join-Path $PastaDeBuild 'release\ditador.exe'
if (-not (Test-Path $exeBackend)) { throw "não achei $exeBackend. Rode sem -SemCompilar." }

# ----------------------------------------------------- o frontend, autocontido
#
# `-o` com pasta nossa, em vez de adivinhar onde o publish resolveu escrever: o
# caminho padrão muda com a plataforma e com a configuração, e um `Copy-Item` de
# uma pasta errada empacotaria a compilação dependente de framework — que abre
# nesta máquina, que tem os runtimes, e não abre na de destino, que é o único
# lugar onde o pacote importa.
Etapa 'Publicando o frontend WinUI (autocontido)'
if (Test-Path $publicado) { Remove-Item $publicado -Recurse -Force }
dotnet publish $projeto -c Release -r win-x64 -p:Platform=x64 `
    --self-contained true -p:WindowsAppSDKSelfContained=true `
    -o $publicado --nologo
if ($LASTEXITCODE) { throw "dotnet publish falhou (código $LASTEXITCODE)" }

# ------------------------------------------------------------------ o estágio
Etapa 'Juntando o que vai no pacote'
if (Test-Path $estagio) { Remove-Item $estagio -Recurse -Force }
New-Item -ItemType Directory -Force $estagio | Out-Null

Copy-Item -Path (Join-Path $publicado '*') -Destination $estagio -Recurse -Force
# Só os símbolos de depuração saem — mesma regra do empacotar-exe.ps1: qualquer
# outra "sobra" pode ser um manifesto de que o WinUI depende.
Get-ChildItem $estagio -Recurse -Filter '*.pdb' | Remove-Item -Force -ErrorAction SilentlyContinue
Copy-Item -Path $exeBackend -Destination $estagio -Force
Copy-Item -Path (Join-Path $raiz 'LICENSE') -Destination $estagio -Force

# As duas provas de que o publish saiu autocontido de verdade. Sem elas, um
# publish que silenciosamente voltasse a ser dependente de framework passaria
# por aqui e só falharia na máquina de destino, sem os runtimes — que é
# exatamente a máquina que não tem como instalá-los.
if (-not (Test-Path (Join-Path $estagio 'coreclr.dll'))) {
    throw 'o publish não trouxe o runtime do .NET (falta coreclr.dll) — saiu dependente de framework'
}
if (-not (Test-Path (Join-Path $estagio 'Microsoft.ui.xaml.dll'))) {
    throw 'o publish não trouxe o Windows App SDK (falta Microsoft.ui.xaml.dll) — o WindowsAppSDKSelfContained não valeu'
}

# O marcador é o interruptor do modo portátil (src/portatil.rs). O conteúdo é
# livre — o programa só olha se o arquivo existe —, então ele carrega a própria
# explicação.
$utf8 = New-Object System.Text.UTF8Encoding($true)
[System.IO.File]::WriteAllText((Join-Path $estagio 'portatil'), @"
Este arquivo liga o modo portátil do Ditador: a configuração, os modelos e o
histórico ficam na pasta Dados\, aqui ao lado, em vez de %APPDATA% e
%LOCALAPPDATA%. Apague-o para o programa voltar às pastas do sistema.
"@, $utf8)

$sobreOModelo = if ($ComModelo) {
    @"
O modelo de transcrição (ggml-$Modelo.bin) já vem dentro, em
Dados\dados\models\ — o programa funciona sem internet desde a primeira vez.
"@
} else {
    @"
O modelo de transcrição não vem no pacote: a primeira janela oferece baixá-lo,
com barra de progresso, ou rode  .\ditador.exe --baixar-modelo  num terminal
aqui dentro. Para máquina sem internet, gere o pacote com o modelo dentro
(empacotar-portatil.ps1 -ComModelo) numa máquina que tenha.
"@
}

$sobreAGpu = if ($rotulo -eq 'gpu') {
    @"
Esta variante usa a GPU via Vulkan, que vem com o driver de vídeo de qualquer
placa deste tempo. Se a transcrição falhar por falta de Vulkan (o programa
avisa), use a variante "cpu", que roda em qualquer máquina.
"@
} elseif ($rotulo -eq 'cuda') {
    @"
Esta variante usa CUDA: a máquina precisa do driver da NVIDIA.
"@
} else {
    @"
Esta variante roda só na CPU e não depende de placa de vídeo nenhuma. O piso
de processador é AVX2 (todo Intel desde 2013, todo AMD Ryzen).
"@
}

[System.IO.File]::WriteAllText((Join-Path $estagio 'LEIA-ME.txt'), @"
Ditador $versao — versão portátil ($rotulo)
========================================

Ditado por voz offline com Whisper: segure uma tecla (Pause/Break, por padrão),
fale, solte, e o texto aparece e vai para a área de transferência. O áudio não
sai da máquina.

Para usar
---------
Dê dois cliques em Ditador.Windows.exe. Ele põe o ícone na área de notificação
(pode estar atrás do ^) e sobe o motor de transcrição sozinho. Não precisa
instalar nada antes: o .NET e o WinUI já vêm dentro desta pasta.

O SmartScreen pode avisar que o editor é desconhecido (o programa não é
assinado por certificado comercial): "Mais informações" → "Executar assim
mesmo".

Tudo o que o programa grava — configuração, modelos, histórico, logs — fica na
pasta Dados\, aqui dentro. Nada vai para AppData, e mover ou copiar esta pasta
inteira (para um pendrive, para outra máquina) leva tudo junto.

$sobreOModelo
$sobreAGpu
Se algo não acontecer
---------------------
Num terminal, dentro desta pasta:

    .\ditador.exe --diagnostico

confere item por item tudo de que o programa depende e diz o que está faltando.
Um caso comum e silencioso é a permissão de microfone: Configurações →
Privacidade e segurança → Microfone → "Permitir que aplicativos de área de
trabalho acessem seu microfone".

Para atualizar
--------------
Baixe a versão nova e descompacte por cima desta pasta: os programas — e os
runtimes que vêm junto — são substituídos, e a Dados\, que é sua, fica como
está.

Windows 10 (2004) ou mais novo, x64. Licença MIT (arquivo LICENSE).
Código e versões novas: https://github.com/DanielFreitasDev/ditador
"@, $utf8)

# ------------------------------------------------------------------- o modelo
$sufixo = ''
if ($ComModelo) {
    Etapa "Levando o modelo ggml-$Modelo.bin"
    $origem = Join-Path $env:LOCALAPPDATA "ditador\models\ggml-$Modelo.bin"
    if (-not (Test-Path $origem)) {
        # Quem baixa é o próprio backend, com as três conferências dele —
        # tamanho, assinatura e soma (src/modelo.rs). O modelo fica na pasta de
        # quem empacota, de propósito: o próximo pacote sai sem download.
        & $exeBackend --baixar-modelo $Modelo
        if ($LASTEXITCODE) { throw "o download do modelo falhou (código $LASTEXITCODE)" }
    }
    # A assinatura vale também para o arquivo que já estava aqui: embutir um
    # modelo truncado num pacote feito para máquina sem internet é o pior lugar
    # possível para se descobrir o problema. No disco ela é little-endian:
    # os quatro primeiros bytes são 6c 6d 67 67 ("lmgg"), e não "ggml".
    $fluxo = [System.IO.File]::OpenRead($origem)
    try {
        $cabeca = New-Object byte[] 4
        $lidos = $fluxo.Read($cabeca, 0, 4)
    } finally {
        $fluxo.Close()
    }
    $hex = ($cabeca | ForEach-Object { $_.ToString('x2') }) -join ''
    if ($lidos -ne 4 -or $hex -ne '6c6d6767') {
        throw "o arquivo em $origem não é um modelo do Whisper (começa com $hex). Apague-o e rode de novo, que o download refaz."
    }
    # Exatamente onde o modo portátil procura: data_dir() = Dados\dados, e os
    # modelos em models\ dentro dela (src/config.rs).
    $modelos = Join-Path $estagio 'Dados\dados\models'
    New-Item -ItemType Directory -Force $modelos | Out-Null
    Copy-Item -Path $origem -Destination $modelos -Force
    $sufixo = '-com-modelo'
}

$mb = [math]::Round((Get-ChildItem $estagio -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1MB, 1)
Feito "$mb MB em $estagio"

# ---------------------------------------------------------------------- o zip
Etapa 'Compactando'
$pacote = Join-Path $saida "ditador-v$versao-windows-x64-$rotulo-portatil$sufixo.zip"
if (Test-Path $pacote) { Remove-Item $pacote -Force }
Compress-Archive -Path $estagio -DestinationPath $pacote -CompressionLevel Optimal

$tamanho = [math]::Round((Get-Item $pacote).Length / 1MB, 1)
Write-Host "`nPronto." -ForegroundColor Green
Write-Host "  $pacote  ($tamanho MB)"
Write-Host ''
Write-Host 'Para usar em outra máquina: descompacte e execute Ditador-Portatil\Ditador.Windows.exe'
