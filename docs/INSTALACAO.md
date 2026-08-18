# Instalar, atualizar e remover

Este documento é a fonte única das instruções de instalação: ele está no
repositório e é copiado inteiro para o corpo de cada versão publicada, com os
nomes de arquivo já apontando para a versão certa. Corrigir aqui corrige lá.

## Os arquivos desta versão

| Arquivo | Para quem |
|---|---|
| `ditador-vX.Y.Z-linux-amd64-gpu.deb` | Ubuntu 24.04 ou mais novo, **com** GPU (Vulkan) |
| `ditador-vX.Y.Z-linux-amd64-cpu.deb` | Ubuntu 24.04 ou mais novo, **sem** GPU |
| `ditador-vX.Y.Z-windows-x64-gpu.exe` | Windows 11 x64, **com** GPU (Vulkan) |
| `ditador-vX.Y.Z-windows-x64-cpu.exe` | Windows 11 x64, **sem** GPU |
| `ditador-gnome-extension-vX.Y.Z.zip` | a extensão do GNOME Shell 50, opcional |
| `SHA256SUMS` | as somas de verificação de todos os acima |
| `Source code (zip)` / `(tar.gz)` | o código-fonte, gerado pelo GitHub a partir da tag |

**GPU ou CPU?** A variante GPU usa Vulkan e transcreve em uma fração do tempo;
a variante CPU não depende de placa nenhuma e funciona em qualquer máquina, mais
devagar. Na dúvida, comece pela GPU: se a máquina não tiver Vulkan, o programa
avisa e você troca pelo pacote CPU sem perder configuração nem modelo. Quem usa
NVIDIA e prefere CUDA compila do código-fonte (`./instalar.sh cuda`) — o CUDA não
sai pronto porque exige o toolkit da NVIDIA para compilar, e ele não cabe numa
release.

**O modelo de transcrição não vem junto.** O programa oferece baixá-lo na
primeira tela, com barra de progresso, e o `ditador --baixar-modelo` faz o mesmo
pelo terminal. Qual modelo depende da máquina: **~574 MB** do
`large-v3-turbo-q5_0` para quem tem GPU, **~190 MB** do `small-q5_1` para quem
vai transcrever na CPU, que é a escolha certa lá (o grande, sem GPU, transcreve
mais devagar do que se fala). `ditador --baixar-modelo --lista` mostra os doze
que o programa conhece, com tamanho e para quem cada um serve. O modelo fica fora
da área da instalação de propósito: atualizar ou reinstalar o Ditador nunca o
apaga nem o baixa de novo.

**Conferir o que você baixou** (opcional, mas é para isso que o `SHA256SUMS`
existe):

```bash
sha256sum --ignore-missing --check SHA256SUMS      # Linux
```

```powershell
# Windows: compare a linha do seu arquivo com a do SHA256SUMS
Get-FileHash .\ditador-vX.Y.Z-windows-x64-gpu.exe -Algorithm SHA256
```

---

# Linux (Ubuntu 24.04 ou mais novo)

## 1. Instalação do zero

Numa máquina que nunca teve o Ditador:

```bash
sudo apt install ./ditador-vX.Y.Z-linux-amd64-gpu.deb    # ou …-cpu.deb
sudo usermod -aG input $USER
```

A segunda linha não é opcional se você quiser o atalho global. O Ditador lê a
tecla direto do `/dev/input/event*` — é assim que ele funciona em qualquer
aplicativo, no Wayland inclusive —, e isso exige o usuário no grupo `input`. Sem
isso o programa abre, transcreve pelo botão da janela, e o atalho simplesmente
não faz nada.

**Saia da sessão e entre de novo.** É o que faz o novo grupo valer.

Depois:

```bash
ditador --diagnostico              # confere teclado, modelo, microfone, área de transferência
ditador --baixar-modelo            # o sugerido para esta máquina (ou baixe pela primeira tela)
ditador --baixar-modelo --lista    # todos os modelos, com tamanho e para quem servem
systemctl --user enable --now ditador   # subir junto com a sessão
```

O `--diagnostico` é a primeira coisa a rodar quando alguma coisa não acontece:
ele confere item por item tudo de que o programa depende e diz o que está
faltando, em vez de deixar você adivinhar.

Abra o **Ditador** pelo menu de aplicativos. Segure **Pause/Break**, fale,
solte: o texto aparece na janela e vai para a área de transferência.

## 2. Atualizar de uma versão anterior

Mesmo comando da instalação:

```bash
sudo apt install ./ditador-vX.Y.Z-linux-amd64-gpu.deb
```

Isso é uma atualização, não uma reinstalação: o `apt` substitui os arquivos do
programa e mantém tudo o que é seu.

**O que é preservado**, porque nunca fica dentro da área do pacote:

| O quê | Onde |
|---|---|
| Configuração (atalho, idioma, microfone, tema) | `~/.config/ditador/config.json` |
| Modelos do Whisper | `~/.local/share/ditador/models/` |
| Extensão do GNOME e widget do Plasma | as pastas do usuário, intocadas |

O pacote encerra a instância em execução antes de trocar o binário e a religa
depois, se ela estava de pé — você não precisa fazer nada. Se por algum motivo
ela não voltar, `systemctl --user restart ditador` resolve.

**Trocando de variante** (de GPU para CPU ou o contrário): instale o outro
pacote direto, sem remover o anterior. Eles se substituem — `ditador` e
`ditador-cpu` declaram `Conflicts`/`Replaces` um do outro — e sua configuração e
seus modelos continuam onde estão.

**Reinstalar a mesma versão:** o `apt` compara o número da versão, não o
conteúdo, e responde "já é a versão mais nova" sem fazer nada. Para forçar:
`sudo apt install --reinstall ./ditador-vX.Y.Z-linux-amd64-gpu.deb`.

## 3. Remoção completa e instalação limpa

Na ordem. Cada bloco é independente: pare no que for suficiente para você.

**a) Parar e desativar o serviço**

```bash
systemctl --user disable --now ditador
ditador --encerrar 2>/dev/null || true
```

**b) Remover o programa**

```bash
sudo apt purge ditador ditador-cpu ditador-cuda   # os que existirem; ignore os erros dos outros
```

Se você instalou compilando (`./instalar.sh`) em vez de pelo pacote, o que sai é
outro conjunto de arquivos — todos do seu usuário, nenhum precisa de `sudo`:

```bash
rm -f ~/.local/bin/ditador
rm -f ~/.local/share/applications/ditador.desktop
rm -f ~/.config/systemd/user/ditador.service
rm -f ~/.local/share/icons/hicolor/scalable/apps/ditador.svg
rm -f ~/.local/share/icons/hicolor/*/apps/ditador.png
rm -f ~/.local/share/icons/hicolor/symbolic/apps/ditador-*.svg
systemctl --user daemon-reload
```

**c) Remover as integrações de área de trabalho**

GNOME:

```bash
gnome-extensions disable ditador@danielfreitasdev.github.io
gnome-extensions uninstall ditador@danielfreitasdev.github.io
dconf reset -f /org/gnome/shell/extensions/ditador/
```

KDE Plasma — pelo script do repositório, se você o tiver:

```bash
./kde-plasma/desinstalar.sh
```

ou à mão (o widget é seu; o plugin é do sistema e por isso pede senha):

```bash
kpackagetool6 --type Plasma/Applet --remove io.github.danielfreitasdev.ditador
sudo rm -rf "$(qmake6 -query QT_INSTALL_QML)/io/github/danielfreitasdev/ditador"
systemctl --user restart plasma-plasmashell
```

**d) Remover configuração, modelos, cache e resíduos**

```bash
rm -rf ~/.config/ditador           # configuração
rm -rf ~/.local/share/ditador      # modelos do Whisper (~574 MB) e dados
rm -f  ~/.config/autostart/ditador.desktop   # o outro caminho de "iniciar com a sessão"
rm -f  "${XDG_RUNTIME_DIR:-/run/user/$UID}/ditador.sock"   # o canal de controle, se sobrou
```

O `ditador.sock` some sozinho quando o processo termina; a linha está aqui para
o caso de o processo ter morrido de um jeito que não o deixou limpar.

Não há log a apagar: no Linux o Ditador escreve no journal do systemd, que rotaciona
sozinho e não é dele. Para conferir que não sobrou nada rodando nem registrado:

```bash
pgrep -a ditador                                        # não deve responder nada
systemctl --user list-unit-files 'ditador*'             # nem isto
ls ~/.config/ditador ~/.local/share/ditador 2>/dev/null # nem isto
```

**e) Instalação limpa depois disso**

Volte ao passo 1. Nada da instalação anterior sobrevive aos comandos acima — a
próxima instalação vai pedir o download do modelo outra vez e nascer com o
atalho, o idioma e o tema padrão. O grupo `input` **continua** valendo: ele é do
seu usuário no sistema, não do programa, e não precisa ser refeito.

---

# Linux com GNOME (extensão opcional)

A extensão põe o Ditador na barra superior, nas Configurações rápidas e desenha
o aviso de gravação na tela. É **opcional** e independente do `.deb`: o programa
funciona inteiro sem ela, com o ícone dele na bandeja do sistema. Alvo: **GNOME
Shell 50**.

Com a extensão ligada, o ícone da bandeja é recolhido e as janelas de gravação
dão lugar ao aviso na tela — não há duplicidade, e voltar atrás é só desligar a
extensão.

## Instalação do zero

Baixe o `ditador-gnome-extension-vX.Y.Z.zip` desta versão e:

```bash
gnome-extensions install --force ditador-gnome-extension-vX.Y.Z.zip
```

**Saia da sessão e entre de novo.** Não tem jeito de pular esse passo numa
primeira instalação: o GNOME Shell varre a pasta de extensões uma única vez, no
arranque, e não há vigia de diretório. (No Wayland também não existe o
`Alt+F2` + `r` que reiniciava o Shell no X11.)

Depois de voltar:

```bash
gnome-extensions enable ditador@danielfreitasdev.github.io
```

Do código-fonte, o caminho é `./gnome-extension/instalar.sh`, que empacota e
instala numa tacada.

## Atualizar

```bash
gnome-extensions install --force ditador-gnome-extension-vX.Y.Z.zip
gnome-extensions disable ditador@danielfreitasdev.github.io
gnome-extensions enable ditador@danielfreitasdev.github.io
```

O par desabilitar/habilitar carrega o código novo sem sair da sessão — isso vale
para uma extensão **já instalada** antes; só a primeira instalação exige o
relogin. As preferências da extensão ficam no dconf e sobrevivem.

## Remoção completa

```bash
gnome-extensions disable ditador@danielfreitasdev.github.io
gnome-extensions uninstall ditador@danielfreitasdev.github.io
dconf reset -f /org/gnome/shell/extensions/ditador/
rm -rf ~/.local/share/gnome-shell/extensions/ditador@danielfreitasdev.github.io
```

O último comando é redundante depois do `uninstall` — está aqui para o caso de
uma instalação feita à mão, copiando a pasta. Removida a extensão, o ícone do
Ditador volta sozinho para a bandeja do sistema, sem reiniciar nada: quem decide
isso é o nome sumir do barramento D-Bus.

---

# Linux com KDE Plasma (widget opcional)

O widget põe o Ditador no painel como um applet de verdade, com popup próprio.
É **opcional** e independente do `.deb`. Alvo: **Plasma 6.6 / Qt 6 / KF6 /
Wayland**. Não existe versão para Plasma 5.

Ao contrário do GNOME, aqui **o aviso de gravação na tela continua sendo do
aplicativo**: o Plasma não oferece um OSD utilizável por terceiros (o porquê
está apurado em `kde-plasma/README.md`), então só o ícone da bandeja é
recolhido.

## Instalação do zero

O widget não sai pronto na release: ele tem uma metade em C++ que precisa ser
compilada contra o Qt da sua máquina. Baixe o código-fonte desta versão
(`Source code (tar.gz)`), descompacte e:

```bash
sudo apt install -y cmake g++ extra-cmake-modules \
    qt6-base-dev qt6-declarative-dev qt6-declarative-dev-tools
./kde-plasma/instalar.sh
```

O script pede senha **uma vez**, para o plugin C++: o Qt 6 não tem diretório de
módulos QML por usuário, e o módulo precisa ir para o do sistema. O widget em si
é instalado no seu escopo, sem senha.

Depois, clique com o botão direito no painel → *Adicionar ou gerenciar widgets* →
procure por **Ditador**.

## Atualizar

Mesmo comando, com o código-fonte da versão nova:

```bash
./kde-plasma/instalar.sh
systemctl --user restart plasma-plasmashell
```

O reinício do `plasmashell` não é superstição: o `kpackagetool6` troca os
arquivos no disco, mas o `plasmashell` continua com a compilação anterior na
memória — já foi visto ele repetir no journal um erro de QML que não existia mais
no arquivo instalado. Sem reiniciar, você corrige o que já estava corrigido.

O widget que estiver no painel continua lá, na mesma posição.

## Remoção completa

```bash
./kde-plasma/desinstalar.sh          # do código-fonte
systemctl --user restart plasma-plasmashell
```

ou à mão:

```bash
kpackagetool6 --type Plasma/Applet --remove io.github.danielfreitasdev.ditador
sudo rm -rf "$(qmake6 -query QT_INSTALL_QML)/io/github/danielfreitasdev/ditador"
systemctl --user restart plasma-plasmashell
```

Se o widget estava no painel, ele vira um espaço vazio até o `plasmashell`
reiniciar — daí a última linha. O ícone do Ditador volta para a bandeja assim que
o widget solta o nome dele no barramento.

---

# Windows 11 (x64)

O Ditador no Windows são **dois** executáveis: o `ditador.exe`, que faz o
trabalho (atalho, áudio, Whisper, área de transferência), e o
`Ditador.Windows.exe`, que faz a interface (ícone na área de notificação, aviso
na tela, painel de status, notificações). O instalador põe os dois no lugar; o
segundo sobe o primeiro quando precisa.

## 1. Instalação do zero

Baixe o `ditador-vX.Y.Z-windows-x64-gpu.exe` (ou `-cpu.exe`) e execute.

**Não pede administrador**, e é de propósito: tudo vai para
`%LOCALAPPDATA%\Programs\Ditador`, que é seu. Nada em `Arquivos de Programas`,
nada em `HKLM`, nada de serviço do Windows. O Ditador lê o teclado, abre o
microfone e escreve na área de transferência — três coisas que uma conta comum
faz. O SmartScreen pode avisar que o editor é desconhecido (o instalador não é
assinado por um certificado comercial): *Mais informações* → *Executar assim
mesmo*.

O instalador confere duas dependências e instala as que faltarem:

- **.NET 10 Desktop Runtime** e **Windows App Runtime 2.x**, que são o que o
  frontend WinUI usa. Eles não vão embutidos de propósito: assim as correções de
  segurança dos dois chegam pelo Windows Update, em vez de esperarem uma versão
  nossa.

Na tela de tarefas você escolhe se o Ditador **inicia junto com o Windows**. O
padrão é não — isso é escolha de quem instala, não efeito colateral de instalar.
Dá para mudar depois pelo interruptor nas configurações do próprio Ditador, ou
em *Configurações → Aplicativos → Inicializar*.

Terminada a instalação, o ícone está na área de notificação (pode estar escondido
atrás do `^` — arraste-o para a barra se quiser vê-lo sempre). Segure
**Pause/Break**, fale, solte.

Primeira vez? O modelo ainda não está aqui. Baixe pelas configurações do
programa, ou:

```powershell
& "$env:LOCALAPPDATA\Programs\Ditador\ditador.exe" --baixar-modelo
& "$env:LOCALAPPDATA\Programs\Ditador\ditador.exe" --diagnostico
```

O `--diagnostico` confere teclado, modelo, microfone, área de transferência e
onde está o log — e é a primeira coisa a rodar se algo não acontecer. Um caso
comum e silencioso é a **permissão de microfone**: *Configurações → Privacidade
e segurança → Microfone → Permitir que aplicativos de área de trabalho acessem
seu microfone*.

## 2. Atualizar de uma versão anterior

Execute o instalador da versão nova. Ele encerra o que estiver rodando, escreve
por cima e sobe de novo. Não desinstale antes — não é preciso, e desinstalar
primeiro só aumenta a chance de você perder alguma escolha.

**O que é preservado:**

| O quê | Onde |
|---|---|
| Configuração (atalho, idioma, microfone, tema) | `%APPDATA%\ditador\config.json` |
| Modelos do Whisper (~574 MB) | `%LOCALAPPDATA%\ditador\models\` |
| Log | `%LOCALAPPDATA%\ditador\logs\` |
| A escolha de iniciar com o Windows | mantida como estava |

A divisão entre `%APPDATA%` (Roaming) e `%LOCALAPPDATA%` (Local) é deliberada: a
configuração acompanha o usuário entre máquinas de um domínio, e o modelo de meio
giga não pode atravessar a rede a cada login.

**Trocando de variante** (GPU ↔ CPU): execute o instalador da outra variante por
cima. Ele substitui os executáveis e não toca nos seus dados.

## 3. Remoção completa e instalação limpa

**a) Desinstalar**

*Configurações → Aplicativos → Aplicativos instalados → Ditador → Desinstalar*
(ou o `unins000.exe` dentro de `%LOCALAPPDATA%\Programs\Ditador`).

O desinstalador encerra os dois processos, remove os executáveis, o atalho do
menu Iniciar, o registro de inicialização (`HKCU\…\Run\Ditador`) e a identidade
de notificações — sem ela o Windows continuaria listando o Ditador em
*Configurações → Sistema → Notificações* depois de desinstalado.

Ele **pergunta** se você quer apagar também a configuração, os modelos e os logs.
Responder "não" é o certo para quem vai reinstalar; responder "sim" é o que
deixa a máquina como se o programa nunca tivesse existido.

**b) Se você respondeu "não" e mudou de ideia**

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\ditador"        # configuração
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\ditador"   # modelos e logs
```

**c) Conferir que não sobrou nada**

```powershell
Get-Process ditador, Ditador.Windows -ErrorAction SilentlyContinue   # nada
Test-Path "$env:LOCALAPPDATA\Programs\Ditador"                       # False
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Ditador -ErrorAction SilentlyContinue   # nada
Test-Path 'HKCU:\Software\Classes\AppUserModelId\DanielFreitasDev.Ditador'   # False
Test-Path "$env:APPDATA\ditador", "$env:LOCALAPPDATA\ditador"        # False, False
```

Quem instalou do código-fonte, pelo `instalar.ps1`, desinstala com o par dele:
`.\windows-integration\scripts\desinstalar.ps1 -ApagarDados`.

**d) Instalação limpa depois disso**

Volte ao passo 1. O .NET e o Windows App Runtime **continuam instalados** — são
componentes do sistema, usados por outros programas, e nenhum desinstalador do
Ditador os remove. Se você quiser mesmo tirá-los, é pelo próprio painel de
aplicativos do Windows, cientes de que outra coisa pode depender deles.

---

# Compilando do código-fonte

Vale para qualquer sistema e é o único caminho para o backend CUDA. O
`Source code (tar.gz)` desta versão traz tudo.

**Linux:**

```bash
sudo apt install -y build-essential cmake libasound2-dev libvulkan-dev glslc wl-clipboard
./instalar.sh              # ou: ./instalar.sh cpu   |   ./instalar.sh cuda
ditador --baixar-modelo
```

**Windows** (precisa de Visual Studio Build Tools com C++, Rust, CMake, LLVM e —
para a variante GPU — o Vulkan SDK):

```powershell
.\windows-integration\scripts\instalar.ps1              # ou -Backend cpu
```

O `windows-integration/README.md` tem a lista completa com os `winget install`
de cada dependência, e o `build.ps1` traz comentado cada um dos ajustes de
ambiente que o whisper.cpp exige lá — todos com o erro que eles evitam.
