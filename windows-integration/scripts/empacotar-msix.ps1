# Monta um pacote MSIX do Ditador.
#
#     .\windows-integration\scripts\empacotar-msix.ps1
#     .\windows-integration\scripts\empacotar-msix.ps1 -Assinar
#
# ## Leia antes de usar
#
# Este **não** é o caminho de instalação padrão do Ditador no Windows — o padrão é
# o `instalar.ps1`, que não pede senha nenhuma. O MSIX está aqui porque é o
# formato certo para distribuir um dia pelo `winget` ou pela Microsoft Store, e
# porque um protótipo que existe vale mais do que uma promessa de que daria certo.
#
# O que ele produz é um `.msix` válido. O que ele **não** faz é instalá-lo: o
# Windows recusa pacotes assinados por um certificado em que a máquina não confia,
# e confiar num certificado de teste exige pôr o certificado no armazenamento de
# Pessoas Confiáveis da máquina — o que pede administrador. Está tudo escrito no
# `windows-integration/README.md`, com os comandos, para quem quiser fazer isso
# conscientemente numa máquina de testes.
#
# Nenhum certificado é versionado. O `.gitignore` do projeto já barra `*.pfx`.

[CmdletBinding()]
param(
    [ValidateSet('vulkan', 'cpu', 'cuda')]
    [string] $Backend = 'vulkan',

    # Gera um certificado de teste (se não houver) e assina o pacote com ele.
    [switch] $Assinar,

    # Usa o que já estiver compilado.
    [switch] $SemCompilar,

    [string] $Saida = (Join-Path $env:USERPROFILE '.ditador-build\msix')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$raiz = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$pastaDeBuild = Join-Path $env:USERPROFILE '.ditador-build'
$frontendBin = Join-Path $raiz 'windows-integration\src\Ditador.Windows\bin\x64\Release\net10.0-windows10.0.26100.0\win-x64'

function Etapa($texto) { Write-Host "`n$texto" -ForegroundColor Cyan }
function Feito($texto) { Write-Host "  $texto" }

function Find-FerramentaDoSdk {
    param([Parameter(Mandatory)][string] $Nome)

    # A ferramenta mora numa pasta com o número da versão do SDK. Pegar a mais
    # nova é o certo: elas são compatíveis para trás, e escrever a versão à mão
    # aqui quebraria na próxima atualização do Windows SDK.
    $bins = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (-not (Test-Path $bins)) {
        throw "não achei o Windows SDK em $bins. Instale-o pelo Visual Studio Installer."
    }

    $achada = Get-ChildItem $bins -Directory |
        Where-Object { $_.Name -match '^10\.' } |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName "x64\$Nome" } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1

    if (-not $achada) { throw "não achei $Nome em nenhuma versão do Windows SDK." }
    return $achada
}

# --------------------------------------------------------------------- versão
#
# Do `Cargo.toml`, que é a única fonte da verdade da versão do projeto — a mesma
# regra do `empacotar.sh` do Linux e do `metadata.json` do widget do Plasma. O
# MSIX exige quatro componentes e o último precisa ser zero (a Store o reserva).
$cargo = Get-Content (Join-Path $raiz 'Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw 'não consegui ler a versão do Cargo.toml'
}
$versao = "$($Matches[1]).0"
Feito "versão $versao"

# ------------------------------------------------------------------- compilar
if (-not $SemCompilar) {
    Etapa 'Compilando'
    & (Join-Path $PSScriptRoot 'build.ps1') -Backend $Backend -Configuracao Release
    if ($LASTEXITCODE) { throw 'a compilação falhou' }
}

$exeBackend = Join-Path $pastaDeBuild 'release\ditador.exe'
foreach ($obrigatorio in $exeBackend, (Join-Path $frontendBin 'Ditador.Windows.exe')) {
    if (-not (Test-Path $obrigatorio)) { throw "não achei $obrigatorio" }
}

# --------------------------------------------------------------------- layout
Etapa 'Montando o layout do pacote'
$layout = Join-Path $Saida 'layout'
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force $layout | Out-Null

Copy-Item (Join-Path $frontendBin '*') $layout -Recurse -Force
Copy-Item $exeBackend $layout -Force

# O manifesto, com a versão preenchida.
$manifesto = Get-Content (Join-Path $raiz 'windows-integration\packaging\AppxManifest.xml') -Raw
$manifesto = $manifesto -replace 'Version="0\.0\.0\.0"', "Version=`"$versao`""
Set-Content (Join-Path $layout 'AppxManifest.xml') $manifesto -Encoding UTF8

# Os logotipos que o manifesto exige. São recortes do mesmo ícone do aplicativo,
# gerados na hora para não versionar cinco PNGs que ninguém edita à mão.
Etapa 'Gerando os logotipos do pacote'
$icone = Join-Path $raiz 'windows-integration\src\Ditador.Windows\Assets\ditador-256.png'
$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { throw 'preciso do Python com Pillow para gerar os logotipos do pacote.' }
$destinoAssets = Join-Path $layout 'Assets'
New-Item -ItemType Directory -Force $destinoAssets | Out-Null
& $python.Source -c @"
from PIL import Image
import sys
origem, destino = sys.argv[1], sys.argv[2]
imagem = Image.open(origem).convert('RGBA')
for nome, lado in [('StoreLogo', 50), ('Square150x150Logo', 150), ('Square44x44Logo', 44), ('SmallTile', 71)]:
    imagem.resize((lado, lado), Image.LANCZOS).save(f'{destino}/{nome}.png')
print('  quatro logotipos')
"@ $icone $destinoAssets
if ($LASTEXITCODE) { throw 'não consegui gerar os logotipos' }

# ------------------------------------------------------------------ empacotar
Etapa 'Empacotando'
$makeappx = Find-FerramentaDoSdk -Nome 'makeappx.exe'
$pacote = Join-Path $Saida "Ditador_$versao`_x64.msix"
if (Test-Path $pacote) { Remove-Item $pacote -Force }
& $makeappx pack /d $layout /p $pacote /o
if ($LASTEXITCODE) { throw "makeappx falhou (código $LASTEXITCODE)" }
Feito $pacote

# -------------------------------------------------------------------- assinar
#
# Os caminhos ficam fora do `if` de propósito: o `Set-StrictMode` do topo derruba
# o script ao ler uma variável que nunca foi definida, e o roteiro impresso no
# fim cita o `.cer` mesmo quando ninguém pediu para assinar.
$pfx = Join-Path $Saida 'ditador-teste.pfx'
$cer = Join-Path $Saida 'ditador-teste.cer'

if ($Assinar) {
    Etapa 'Assinando com certificado de teste'
    $senha = 'ditador'

    # Faltando qualquer um dos dois, os dois são refeitos: são certificado de
    # desenvolvimento, e um par pela metade é pior do que um par novo.
    if (-not (Test-Path $pfx) -or -not (Test-Path $cer)) {
        # O nome precisa bater **exatamente** com o Publisher do manifesto, ou o
        # signtool recusa com uma mensagem que fala de certificado e não de nome.
        $certificado = New-SelfSignedCertificate `
            -Type Custom `
            -Subject 'CN=Ditador (Desenvolvimento)' `
            -KeyUsage DigitalSignature `
            -FriendlyName 'Ditador (certificado de teste)' `
            -CertStoreLocation 'Cert:\CurrentUser\My' `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
        $protegida = ConvertTo-SecureString -String $senha -Force -AsPlainText
        Export-PfxCertificate -Cert $certificado -FilePath $pfx -Password $protegida | Out-Null
        Feito "certificado de teste em $pfx (senha: $senha)"

        # E a parte pública ao lado dele. É este arquivo que a máquina de testes
        # importa para confiar no pacote — o `.pfx` tem a chave privada e não
        # deve sair daqui. O roteiro impresso no fim mandava importar um `.cer`
        # que nenhum passo produzia.
        Export-Certificate -Cert $certificado -FilePath $cer -Type CERT | Out-Null
        Feito "certificado público em $cer"
    }

    $signtool = Find-FerramentaDoSdk -Nome 'signtool.exe'
    & $signtool sign /fd SHA256 /a /f $pfx /p $senha $pacote
    if ($LASTEXITCODE) { throw "signtool falhou (código $LASTEXITCODE)" }
    Feito 'assinado'
}

Write-Host "`nPronto." -ForegroundColor Green
Write-Host @"
  $pacote

  Para instalar numa máquina de testes, o certificado precisa ser confiável.
  Isso pede uma janela **como administrador**, uma vez:

      Import-Certificate -FilePath '$cer' ``
          -CertStoreLocation Cert:\LocalMachine\TrustedPeople

  E então:  Add-AppxPackage '$pacote'

  Sem isso o Windows recusa o pacote — e é essa exigência de elevação que faz o
  caminho padrão do Ditador continuar sendo o instalar.ps1.
"@
