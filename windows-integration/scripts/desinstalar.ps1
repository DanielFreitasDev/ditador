# Desinstala o Ditador do Windows.
#
#     .\windows-integration\scripts\desinstalar.ps1
#     .\windows-integration\scripts\desinstalar.ps1 -ApagarDados
#
# ## O que sai e o que fica
#
# Sai: os executáveis, o atalho do menu Iniciar e o registro de inicialização
# com a sessão. Ou seja, tudo o que o `instalar.ps1` pôs.
#
# **Fica**: a configuração, os modelos do Whisper e os logs. Não é esquecimento —
# é o contrário. O modelo tem 574 MB e leva um bom tempo para baixar; a
# configuração guarda o atalho, o idioma e o microfone escolhidos. Quem
# desinstala para reinstalar (uma versão nova, uma pasta diferente) não quer
# perder nenhum dos dois, e quem quer mesmo apagar tudo tem o `-ApagarDados`,
# que diz na cara o que vai remover antes de remover.
#
# Nada aqui toca no repositório nem em coisa alguma do Linux.

[CmdletBinding()]
param(
    # Apaga também a configuração, os modelos e os logs.
    [switch] $ApagarDados,

    [string] $Destino = (Join-Path $env:LOCALAPPDATA 'Programs\Ditador')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Etapa($texto) { Write-Host "`n$texto" -ForegroundColor Cyan }
function Feito($texto) { Write-Host "  $texto" }

Etapa 'Encerrando'
if (Test-Path (Join-Path $Destino 'ditador.exe')) {
    # Pelo canal de controle: assim ele fecha o microfone e grava a configuração.
    & (Join-Path $Destino 'ditador.exe') --encerrar 2>$null | Out-Null
    Start-Sleep -Milliseconds 500
}
foreach ($nome in 'Ditador.Windows', 'ditador') {
    $processos = Get-Process -Name $nome -ErrorAction SilentlyContinue
    if ($processos) {
        $processos | Stop-Process -Force
        Feito "$nome encerrado"
    }
}

Etapa 'Removendo o registro de inicialização'
$chave = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ((Get-ItemProperty -Path $chave -Name 'Ditador' -ErrorAction SilentlyContinue)) {
    Remove-ItemProperty -Path $chave -Name 'Ditador'
    Feito 'não inicia mais com a sessão'
} else {
    Feito 'não estava registrado'
}

Etapa 'Removendo o atalho'
$menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Ditador.lnk'
if (Test-Path $menu) {
    Remove-Item $menu -Force
    Feito 'atalho do menu Iniciar removido'
} else {
    Feito 'não havia atalho'
}

Etapa 'Removendo os arquivos do programa'
if (Test-Path $Destino) {
    Remove-Item $Destino -Recurse -Force
    Feito $Destino
} else {
    Feito 'nada instalado em ' + $Destino
}

# ------------------------------------------------------------------- os dados
#
# Os três caminhos abaixo são os mesmos que o backend usa, e a divisão entre
# `Roaming` e `Local` é deliberada: a configuração acompanha o usuário entre
# máquinas de um domínio, o modelo de 574 MB não pode atravessar a rede a cada
# login. Está explicado no `windows-integration/README.md`.
$dados = @(
    (Join-Path $env:APPDATA 'ditador'),
    (Join-Path $env:LOCALAPPDATA 'ditador')
)

if ($ApagarDados) {
    Etapa 'Apagando os dados do usuário'
    foreach ($pasta in $dados) {
        if (Test-Path $pasta) {
            $tamanho = [math]::Round(((Get-ChildItem $pasta -Recurse -File -ErrorAction SilentlyContinue |
                        Measure-Object -Property Length -Sum).Sum / 1MB), 1)
            Remove-Item $pasta -Recurse -Force
            Feito "$pasta ($tamanho MB)"
        }
    }
} else {
    Etapa 'O que ficou'
    foreach ($pasta in $dados) {
        if (Test-Path $pasta) { Feito $pasta }
    }
    Write-Host '  (configuração, modelos e logs. Para apagar: -ApagarDados)' -ForegroundColor DarkGray
}

# A identidade das notificações, criada pelo próprio frontend quando ele
# registrou o `AppNotificationManager`. Sem removê-la, o Windows continuaria
# listando o Ditador em Configurações → Sistema → Notificações depois de
# desinstalado — um fantasma no painel de outra pessoa.
$identidade = 'HKCU:\Software\Classes\AppUserModelId\DanielFreitasDev.Ditador'
if (Test-Path $identidade) {
    Remove-Item $identidade -Recurse -Force
    Feito 'identidade de notificações removida'
}

Write-Host "`nPronto." -ForegroundColor Green
