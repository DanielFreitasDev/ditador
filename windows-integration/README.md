# Ditador no Windows 11

Documentação técnica do suporte a Windows. Para o uso normal, veja o `README.md`
da raiz.

> **Estado em 15/08/2026.** Completo e em uso nesta máquina: o núcleo em Rust
> compila e transcreve, o atalho global funciona em hardware real, e o frontend
> `Ditador.Windows` (WinUI 3) põe o ícone na área de notificação, desenha o aviso
> de gravação na tela e abre o painel de status. Instala-se com um comando e sem
> pedir senha.
>
> **O lado Linux não foi recompilado depois desta portabilidade.** Ela mexeu em
> código compartilhado, e a máquina onde foi feita é Windows. É o primeiro item
> da lista no fim deste arquivo.

## As duas metades

```
┌──────────────────────────┐        ┌──────────────────────────────┐
│      ditador.exe         │        │    Ditador.Windows.exe       │
│      (Rust)              │        │    (C# · WinUI 3)            │
│                          │        │                              │
│  Raw Input (atalho)      │◀──────▶│  ícone na área de notificação│
│  áudio (WASAPI/cpal)     │  named │  aviso de gravação na tela   │
│  Whisper (Vulkan/CPU)    │  pipe  │  painel de status            │
│  área de transferência   │        │  notificações do sistema     │
│  estado e regras         │        │                              │
└──────────────────────────┘        └──────────────────────────────┘
     a fonte da verdade                   só desenha o que recebe
```

O backend não depende do frontend para nada: sem ele o atalho, a transcrição e a
área de transferência continuam funcionando — perde-se o ícone e o aviso. O
contrário também vale: sem o backend, o frontend mostra "indisponível" e oferece
iniciá-lo.

## O que muda entre Linux e Windows

O núcleo é o mesmo código: máquina de estados do ditado, Whisper, configuração,
interface do egui, regras de transcrição. O que muda mora todo em
`src/plataforma/`, com um contrato de sete módulos que o compilador cobra.

| | Linux | Windows |
|---|---|---|
| Atalho global | evdev (`/dev/input/event*`) | Raw Input (`WM_INPUT`, `RIDEV_INPUTSINK`) |
| Nomes de tecla | tabela do evdev | tabela `VK_*` → código canônico |
| Canal de controle | socket Unix em `$XDG_RUNTIME_DIR` | named pipe `\\.\pipe\Ditador-<SID>` |
| Instância única (backend) | `connect()` no socket | `FILE_FLAG_FIRST_PIPE_INSTANCE` |
| Instância única (frontend) | — | `AppInstance` do Windows App SDK |
| Observador de estado | D-Bus (`PropertiesChanged`) | `assinar` no mesmo pipe |
| Ícone na barra | StatusNotifierItem (ksni) | `Shell_NotifyIcon` v4, no frontend |
| Aviso de gravação | OSD da extensão do GNOME | janela `WS_EX_NOACTIVATE` do frontend |
| Integração de desktop | nomes no barramento D-Bus | presença do assinante no pipe |
| Início automático | systemd `--user` ou `.desktop` do XDG | `HKCU\…\CurrentVersion\Run` |
| Colagem automática | `ydotool` (opcional) | não existe, por decisão |
| Área de transferência | `wl-copy`, com `arboard` de reserva | `arboard` |
| Configuração | `~/.config/ditador/` | `%APPDATA%\ditador\` |
| Modelos | `~/.local/share/ditador/models/` | `%LOCALAPPDATA%\ditador\models\` |
| Log do frontend | — | `%LOCALAPPDATA%\ditador\logs\` |

Duas escolhas merecem explicação, e as duas estão comentadas por extenso no
código:

**O código de tecla é o do evdev nos dois sistemas.** O `config.json` de quem já
usa o Ditador guarda o atalho como `["KEY_PAUSE"]`, e é isso que a extensão do
GNOME e o widget do Plasma leem. No Windows a tradução `VK_PAUSE → 119` acontece
na borda, dentro do Raw Input. Uma configuração escrita no Linux vale no Windows
e vice-versa.

**Os modelos vão para `LocalAppData`, a configuração para `Roaming`.** O modelo
tem 574 MB: no Roaming ele atravessaria a rede a cada login num perfil de
domínio. A configuração são poucos quilobytes de preferências, e acompanhar o
usuário entre máquinas é o comportamento desejável. Há teste para os dois.

## Instalar

```powershell
.\windows-integration\scripts\instalar.ps1
```

Compila os dois lados, confere as dependências, instala em
`%LOCALAPPDATA%\Programs\Ditador`, cria o atalho no menu Iniciar, registra o
início com a sessão e sobe o programa. **Sem administrador** — nada em
`Program Files`, nada em `HKLM`, nada de serviço.

```powershell
.\windows-integration\scripts\instalar.ps1 -Backend cpu          # sem GPU
.\windows-integration\scripts\instalar.ps1 -SemCompilar          # usa o já compilado
.\windows-integration\scripts\instalar.ps1 -SemIniciarComOWindows
.\windows-integration\scripts\desinstalar.ps1                    # e -ApagarDados
```

Atualizar é rodar o `instalar.ps1` de novo: ele encerra o que está rodando (pelo
canal de controle, para o microfone fechar e a configuração ser gravada), troca
os arquivos e sobe outra vez.

### As duas dependências, e por que elas não vêm dentro

O frontend é **dependente de framework**: usa o .NET 10 Desktop Runtime e o
Windows App Runtime 2.x que estiverem instalados. O `instalar.ps1` confere os
dois e instala o que faltar (winget para o .NET, instalador oficial para o App
Runtime).

A alternativa — autocontido — poria uma cópia dos dois dentro da pasta do
Ditador, o que resolveria a instalação e criaria um problema pior: correções de
segurança do .NET e do WinUI chegariam pelo Windows Update para o sistema e
**não** para a nossa cópia, que só se atualizaria quando o Ditador lançasse uma
versão nova. Para um programa que fica de pé o dia inteiro lendo o teclado, essa
troca não vale a pena.

## Compilar

```powershell
.\windows-integration\scripts\build.ps1                 # os dois, Vulkan
.\windows-integration\scripts\build.ps1 -Backend cpu
.\windows-integration\scripts\build.ps1 -SomenteFrontend
.\windows-integration\scripts\build.ps1 -Testar         # fmt, testes e clippy antes
```

Requisitos:

```powershell
winget install --id Rustlang.Rustup                          # alvo stable-x86_64-pc-windows-msvc
winget install --id Microsoft.VisualStudio.2026.BuildTools   # workload C++
winget install --id Kitware.CMake
winget install --id LLVM.LLVM                                # libclang, para o bindgen
winget install --id KhronosGroup.VulkanSDK                   # só para a feature vulkan
winget install --id Microsoft.DotNet.SDK.10                  # só para o frontend
```

O CUDA Toolkit é opcional e só serve à feature `cuda`; baixe-o pela NVIDIA. Para
trabalhar à mão numa sessão: `. .\windows-integration\scripts\ambiente.ps1`.

## As cinco armadilhas do build

Cada uma falha com uma mensagem que aponta para o lugar errado. Estão todas
resolvidas no `build.ps1`; ficam aqui porque quem lê um log não lê um script.

**1. `Unable to find libclang`.** No Windows o `whisper-rs-sys` gera as bindings
com `bindgen`; no Linux ele traz um `bindings.rs` pronto e ninguém percebe.
Instale o LLVM ou aponte `LIBCLANG_PATH` para a pasta da `libclang.dll`.

**2. `No CMAKE_C_COMPILER could be found`, com o `cl.exe` no PATH.** O backend
Vulkan do ggml compila um gerador de shaders como sub-projeto CMake, que faz a
própria configuração do zero e **não** herda o ambiente do MSBuild que o chamou.
A saída é usar o gerador **Ninja**, que procura o compilador no PATH. Junto com
`CMAKE_GENERATOR=Ninja` é preciso esvaziar `CMAKE_GENERATOR_INSTANCE`,
`_PLATFORM` e `_TOOLSET` — o crate `cmake` passa a instância do Visual Studio
adiante e o Ninja a recusa.

**3. `fatal error C1041: … vc140.pdb … use /FS`.** É `MAX_PATH`, e a mensagem não
menciona caminho nenhum. O Vulkan monta 210 caracteres fixos a partir do
`target/`, o que deixa 50 para ele dentro do limite de 260 do Windows. Um projeto
em `C:\Users\<nome>\<pasta>\<projeto>\target` passa disso. Use um
`CARGO_TARGET_DIR` curto — o `build.ps1` usa `%USERPROFILE%\.ditador-build`.

E **não adianta ligar `LongPathsEnabled` no registro**: o `cl.exe` não declara
suporte a caminhos longos no manifesto dele. Verificado nesta máquina com a
chave já ligada. (O `Ditador.Windows.exe` declara — veja o `app.manifest` —, e por
isso ele lida bem com nomes de usuário compridos.)

**4. `unsupported Microsoft Visual Studio version`.** O `nvcc` tem uma lista
fechada de versões de MSVC, num `#error` dentro de `include/crt/host_config.h`.
Em agosto de 2026, nesta máquina:

```
CUDA 12.8 recusa _MSC_VER >= 1950
CUDA 13.2 recusa _MSC_VER >= 1960
Visual Studio 2026 é _MSC_VER 1950
```

O 12.8 recusa o VS 2026 por um número. O `build.ps1` lê esse cabeçalho de cada
toolkit instalado e escolhe um compatível — ou falha em segundos dizendo os
números.

E a variável que o CMake obedece para escolher o compilador é **`CUDACXX`**. Com
`CUDA_PATH`, `CUDAToolkit_ROOT` e o PATH todos apontando para o toolkit certo, o
CMake ainda resolveu `CMAKE_CUDA_COMPILER` para o `nvcc` do outro.

**5. Trocar de gerador ou de toolkit sobre um `CMakeCache` existente produz erros
que apontam para o lugar errado.** Aconteceu duas vezes: uma virou um erro de
`add_subdirectory` inexistente, outra fez parecer que o CUDA 13.2 também recusava
o compilador. Ao mudar qualquer coisa do ambiente de build, rode
`cargo clean -p whisper-rs-sys` antes.

## Qual backend usar

Medido nesta máquina — Windows 11 25H2, RTX 3060 de 12 GB, modelo
`large-v3-turbo-q5_0`, 17,7 s da mesma fala, duas rodadas independentes de três
passadas cada:

| Backend | Carga do modelo | Transcrição | vs. tempo real |
|---|---|---|---|
| CPU | 0,57 s | **18,9 s** | 0,9× |
| **Vulkan** | 1,04 s | **0,42 s** | **42×** |
| CUDA | 0,80 s | **0,47 s** | 38× |

**Use Vulkan.** É o padrão, é o mais rápido nesta placa — por uns 11% sobre o
CUDA, o contrário do que se costuma supor —, roda em AMD e Intel também e não
exige um toolkit de 3 GB para compilar.

A CPU não serve para ditar: a 0,9× o tempo real, uma frase de dez segundos leva
onze para virar texto. Ela existe para máquina sem GPU.

Na primeira transcrição depois de instalar, o Vulkan compila os shaders e leva
alguns segundos a mais; o driver guarda o resultado e as seguintes não pagam
isso.

Para reproduzir a medição na sua máquina:

```powershell
$env:DITADOR_AUDIO_DE_TESTE = "caminho\para\frase.wav"   # PCM 16 bits, mono
cargo test --release --no-default-features --features vulkan `
  -- --ignored --nocapture mede_o_backend
```

Um WAV reproduzível sai da síntese de voz do próprio Windows, sem microfone e
sem depender de alguém falar a mesma frase duas vezes:

```powershell
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.SelectVoice('Microsoft Maria Desktop')
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)
$s.SetOutputToWaveFile("frase.wav", $fmt)
$s.Speak("a frase que você quiser")
$s.Dispose()
```

## Decisões de arquitetura

### O named pipe carrega o SID, e a ACL é escrita à mão

`\\.\pipe\Ditador-<SID do usuário>`.

O espaço de nomes de pipes é global na máquina. Sem o SID, o segundo usuário a
entrar — com troca rápida de usuário ou área de trabalho remota, que é rotina no
Windows — ficaria sem Ditador.

E a DACL é explícita: `D:P(A;;GA;;;<SID>)`. A documentação da Microsoft avisa que
o descritor **padrão** de um named pipe concede leitura a grupos amplos,
inclusive à sessão anônima em certos cenários. Aceitá-lo significaria qualquer
conta da máquina podendo mandar `quit` no Ditador de outra pessoa, ou abrir o
microfone dela com `toggle`. O nome com SID esconde o pipe, mas esconder não é
proteger — o nome é enumerável.

Sem SYSTEM e sem Administradores: nenhum dos dois tem o que fazer com "começar a
gravar". Há teste conferindo que `WD`, `AN`, `BU`, `BA` e `SY` não estão na
lista.

**Conferindo na máquina de verdade** (com o Ditador rodando):

```powershell
$sid = ([Security.Principal.WindowsIdentity]::GetCurrent()).User.Value
(Get-Acl "\\.\pipe\Ditador-$sid").Sddl
```

O que sai é, exatamente:

```
O:S-1-5-21-…-1001G:S-1-5-21-…-1001D:P(A;;FA;;;S-1-5-21-…-1001)
```

Dono e grupo são o usuário; a DACL é protegida (`P`) e tem **uma** entrada:
acesso total para esse mesmo usuário. Nenhum `WD` (Everyone), nenhum `AN`
(anônimo), nenhum `BA` (administradores), nenhum `SY` (SYSTEM). Outro usuário da
máquina, mesmo sabendo o nome do pipe, recebe acesso negado.

### Instância única sai de graça

`FILE_FLAG_FIRST_PIPE_INSTANCE` faz o `CreateNamedPipeW` falhar se o nome já
existir. Isso é exatamente a pergunta "já há um Ditador nesta sessão?",
respondida pelo próprio sistema — sem mutex separado e sem arquivo de trava que
sobreviva a um travamento. Não há corrida entre conferir e criar, porque as duas
coisas são a mesma chamada. (O socket Unix precisa de um segundo `connect` depois
do erro justamente por não ter isso.)

Detalhe contraintuitivo: o erro devolvido quando o nome já existe é
`ERROR_ACCESS_DENIED`, que parece problema de permissão.

O **frontend** tem a instância única dele, e por outro caminho:
`AppInstance.FindOrRegisterForKey` do Windows App SDK. Ela faz o que um mutex não
faz — **redireciona a ativação** para quem chegou primeiro —, então clicar de novo
no atalho não cria um segundo ícone: leva o painel de status à tela.

### O protocolo do canal de controle

Uma linha de comando, uma linha de resposta, terminadas por `\n`. É o mesmo
protocolo do socket Unix do Linux, com um comando a mais:

| Comando | Resposta |
|---|---|
| `status` | uma linha com modelo, atalho, microfone e backend |
| `toggle` | `ok` |
| `iniciar` / `parar` | `ok` |
| `settings` | `ok` |
| `quit` | `encerrando` |
| `integracoes` | `frontend` ou `nenhuma` |
| `assinar` | **a conexão vira um fluxo de eventos** |

Depois de `assinar`, o backend manda uma linha JSON por mensagem:

```json
{"t":"ola","protocolo":1,"aplicativo":"ditador","versao":"0.5.0","backend":"Vulkan"}
{"t":"estado","estado":"pronto","mensagem":"","gravandoDesde":0,"modelo":"large-v3-turbo-q5_0","idioma":"Português","atalho":"Pause/Break"}
{"t":"nivel","valor":0.42}
```

O `ola` primeiro, para um frontend antigo poder desistir com uma frase em vez de
interpretar campos que não conhece. O `estado` logo em seguida é o retrato de
agora — quem conecta não espera a próxima mudança para saber em que pé as coisas
estão. Depois, uma linha a cada mudança, e mais nada quando nada muda: **não há
pergunta em laço em lugar nenhum deste programa**.

O `nivel` é o pico do microfone, a 15 Hz e só durante a gravação — as mesmas
decisões do sinal `Nivel` do D-Bus, pelos mesmos motivos.

Dá para ver o fluxo sem compilar nada:

```powershell
$sid = ([Security.Principal.WindowsIdentity]::GetCurrent()).User.Value
$cano = New-Object System.IO.Pipes.NamedPipeClientStream('.', "Ditador-$sid", 'InOut')
$cano.Connect(3000)
$w = New-Object System.IO.StreamWriter($cano); $w.AutoFlush = $true
$r = New-Object System.IO.StreamReader($cano)
$w.Write("assinar`n")
while ($true) { $r.ReadLine() }
```

**A regra de evolução é a do contrato D-Bus: acrescentar, nunca renomear.** Um
comando novo é invisível para quem não o conhece; um renomeado quebra o atalho
que alguém configurou no painel do sistema para chamar `ditador --alternar`.

### Raw Input, e não `WH_KEYBOARD_LL` nem `RegisterHotKey`

`RegisterHotKey` entrega um evento só, sem o "soltou" — não serve a
segurar-para-falar, que é a semântica inteira deste programa.

`WH_KEYBOARD_LL` funciona, mas é um gancho global: toda tecla digitada em
qualquer aplicativo passaria pelo nosso processo antes de chegar ao destino. A
própria documentação da Microsoft recomenda Raw Input para monitoramento e avisa
que um gancho lento é **removido em silêncio** pelo sistema — falha que aparece
como "o atalho parou de funcionar depois de um tempo". Fica como reserva, e não
foi preciso.

Usamos `RIDEV_INPUTSINK` e **nada além**. Em particular, nada de
`RIDEV_NOLEGACY`: o Ditador observa o teclado, não o consome.

**O atalho não pode ser testado com tecla sintética, e isso é uma qualidade.**
`keybd_event` e `SendInput` inserem a tecla na fila de mensagens do sistema, não
na pilha de entrada bruta: eles produzem `WM_KEYDOWN` para quem tem foco e
**nenhum `WM_INPUT`**. Foi verificado aqui — segurar `Pause` por software não
abre o microfone, e segurar a tecla de verdade abre. A consequência prática é
boa: o Ditador não é acionável por automação de software, e o microfone de quem
o usa não abre por um script. A consequência chata é que a verificação do atalho
é manual, com um teclado, e não há como automatizá-la.

Duas peculiaridades tratadas em `plataforma/windows/teclado.rs`:

* **A tecla Pause** — o atalho padrão — tem scan code `E1 1D 45` e chega em
  **duas** mensagens, a primeira com `VKey = 0xFF`, que não é tecla nenhuma. Sem
  descartá-la, o Ditador via um evento a mais a cada aperto do próprio atalho.
* **Os modificadores** chegam genéricos (`VK_SHIFT`, `VK_CONTROL`, `VK_MENU`),
  sem dizer o lado. O Shift se resolve pelo scan code com
  `MapVirtualKey(MAPVK_VSC_TO_VK_EX)`; Ctrl e Alt, pelo sinalizador E0. E o Ctrl
  do "Break" (Ctrl+Pause) chega com E1, não E0 — lido como E0 viraria "Ctrl
  direito apertado" sem que ninguém tivesse encostado nele.

### O ícone da barra é do frontend, e o dono é único

No Linux o Ditador publica o próprio StatusNotifierItem e o recolhe quando uma
integração nativa aparece — o protocolo permite descobrir isso em tempo de
execução, e o barramento avisa sozinho quando ela sai.

O Windows não tem equivalente: `Shell_NotifyIcon` não responde "alguém já mostra
este aplicativo?". Se os dois processos tentassem, o usuário veria dois ícones do
Ditador lado a lado e nenhum dos dois teria como perceber. Então o dono é
decidido em tempo de projeto, e é o frontend — ele já tem janela e laço de
mensagens para o OSD e o popup, o menu de clique é interface, e o
`TaskbarCreated` (o reinício do Explorer) precisa de um tratador só.

O backend nunca cria ícone no Windows. Sem o frontend não há ícone — e o atalho,
a transcrição e a área de transferência continuam funcionando.

**A janela oculta que recebe os cliques é de nível superior, e não
"message-only".** Uma janela `HWND_MESSAGE` seria a escolha óbvia e estaria
errada: ela **não recebe mensagens de difusão**, e `TaskbarCreated` é uma
difusão. Com ela, o ícone sumiria no primeiro reinício do Explorer e não voltaria
nunca — sem nada no log dizendo por quê.

**Os ícones são dois conjuntos, claro e escuro.** O Windows não recolore ícone de
bandeja: o que o `Shell_NotifyIcon` recebe é o que aparece. Um glifo branco some
na barra clara e um preto some na escura, então o frontend troca de conjunto
quando o tema do sistema muda (`WM_SETTINGCHANGE` com `ImmersiveColorSet`). Os
`.ico` são gerados por `scripts/gerar-icones.py` e commitados; cada um traz oito
tamanhos, de 16 a 256 px, e o tamanho pedido ao `LoadImage` vem de
`GetSystemMetricsForDpi`.

Cada estado tem **forma** própria, e não só cor: ponto cheio para gravando, anel
para trabalhando, triângulo para erro. Em 16 pixels e em tela monocromática eles
continuam distinguíveis, e a dica de ferramenta diz o estado por extenso — que é
o que o Narrator lê.

### O aviso na tela é uma janela passiva

Uma faixa no rodapé do monitor em uso, com o estado, o cronômetro e o nível do
microfone. Três estilos a tornam passiva, e cada um resolve uma coisa diferente:

* `WS_EX_NOACTIVATE` — não vira primeiro plano quando aparece. Sem ele, começar a
  ditar tiraria o foco do editor onde o texto vai ser colado.
* `WS_EX_TOOLWINDOW` — fora do Alt+Tab e da barra de tarefas.
* `WS_EX_TRANSPARENT` — para o clique atravessar. Só é usada porque o aviso não
  tem nada em que clicar; o painel de status, que tem botões, não a recebe.
  Ela é aplicada à janela **e às três janelas internas do WinUI**, porque é uma
  delas (a `DesktopChildSiteBridge`) que responde ao ponteiro — conferido lendo
  o `GWL_EXSTYLE` dos quatro `HWND` com o aviso na tela.

Mais `AppWindow.Show(activateWindow: false)` e `IsShownInSwitchers = false`. O
que **não** se faz é receber o clique e repassá-lo com `SendInput`: a janela é
passiva de verdade, e não há nada a repassar.

O monitor é o da janela em primeiro plano, com o do cursor como reserva — nunca o
primário. Numa mesa de várias telas, o primário raramente é aquele para onde a
pessoa está olhando. A posição sai da **área de trabalho** do monitor
(`GetMonitorInfo`), que já exclui a barra de tarefas esteja ela embaixo, em cima
ou de lado.

E o backend sabe disso: enquanto o frontend está assinando, `Integracoes::frontend`
fica ligado e a janela do egui para de desenhar as telas de gravação e de
transcrição (`Shared::tela_visivel`). Dois avisos do mesmo ditado seriam o mesmo
recado duas vezes, e um deles roubaria o foco.

### O menu do ícone é Win32, e o painel é WinUI

O menu de contexto usa `TrackPopupMenuEx`. Um `MenuFlyout` do WinUI precisaria de
uma janela para se ancorar, e essa janela apareceria no Alt+Tab, roubaria foco e
chegaria um quadro depois do clique.

O painel de status é WinUI, com os controles do sistema (`InfoBar`, `Button`
com `AccentButtonStyle`, `HyperlinkButton`) e nenhuma cor escrita à mão — o que
faz ele seguir o tema claro, o escuro e o de alto contraste sem uma linha nossa.
Ele se posiciona com `Shell_NotifyIconGetRect` + `CalculatePopupWindowPosition`,
que são as funções que sabem encaixar um retângulo ao lado de outro respeitando
monitor, barra de tarefas e DPI.

**Limitação conhecida: o menu do botão direito sai em tema claro mesmo com o
Windows em tema escuro.** Menus clássicos do Win32 só seguem o tema escuro por
APIs não documentadas do `uxtheme` (ordinais 133/135), e o Ditador não usa API
não documentada — o preço seria quebrar numa atualização do Windows sem aviso. O
painel de status, que é WinUI, segue o tema corretamente.

### A colagem automática não existe no Windows

No Linux o Ditador cola com `ydotool`, que é opcional e que o usuário escolhe
instalar. O equivalente aqui seria `SendInput` sintetizando Ctrl+V, e ele fica de
fora por três motivos:

* vai para **onde o foco estiver** no instante em que a transcrição termina, que
  não é necessariamente onde estava quando a pessoa começou a falar — o texto
  acabaria numa conversa, num campo de senha, num terminal;
* não alcança janelas de integridade mais alta (UIPI): não aparece, sem erro
  nenhum, e "não funciona às vezes" é pior que "não existe";
* o Ditador já lê o teclado globalmente por Raw Input; somar escrita sintética a
  isso é o par exato que dispara heurística de antivírus.

O texto vai para a área de transferência e o Ctrl+V é da pessoa.

### Empacotado ou não: por que o padrão não é MSIX

O MSIX traz coisas boas de verdade — instalação e desinstalação atômicas,
identidade de pacote, `StartupTask` com interruptor no painel do Windows,
atualização diferencial — e o projeto tem um: `packaging/AppxManifest.xml`, gerado
e assinado por `scripts/empacotar-msix.ps1`, que produz um `.msix` válido.

O que impede o MSIX de ser o caminho padrão **não é técnico**: um pacote precisa
estar assinado por um certificado em que a máquina confie, e pôr um certificado
de teste no armazenamento de Pessoas Confiáveis exige **administrador**. Um
programa que só lê o teclado do usuário não deveria precisar de elevação para ser
instalado — e não precisa, pelo `instalar.ps1`. Quando houver um certificado de
assinatura de verdade, essa ressalva cai e o MSIX passa a ser o caminho
recomendado, inclusive para `winget` e Microsoft Store.

```powershell
.\windows-integration\scripts\empacotar-msix.ps1 -Assinar
# → %USERPROFILE%\.ditador-build\msix\Ditador_0.5.0.0_x64.msix
```

Para instalar numa máquina de testes, uma vez, **como administrador**:

```powershell
Import-Certificate -FilePath <certificado.cer> -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage 'C:\…\Ditador_0.5.0.0_x64.msix'
```

Nenhum certificado é versionado: o `.gitignore` barra `*.pfx` e `*.snk`.

## Diagnóstico

```powershell
ditador.exe --diagnostico     # confere modelo, microfone, área de transferência, integração
ditador.exe --status          # pergunta à instância em execução
ditador.exe --microfones      # lista os dispositivos de entrada
ditador.exe --versao          # versão e backend compilado
```

O `--diagnostico` roda em **outro processo** e por isso não consegue dizer se o
atalho está funcionando — quem observa o teclado é a instância em execução. Essa
linha aparece como informativa (`--`), não como falha. Para a resposta de
verdade, use `--status`.

Onde ficam os rastros:

| O quê | Onde |
|---|---|
| Log do frontend | `%LOCALAPPDATA%\ditador\logs\Ditador.Windows.log` (e `.old`) |
| Log do backend | no console de quem o iniciou; para ver, rode-o de um terminal com `$env:RUST_LOG='ditador=debug'` |
| Configuração | `%APPDATA%\ditador\config.json` |
| Modelos | `%LOCALAPPDATA%\ditador\models\` |
| Falhas do sistema | Visualizador de Eventos → Aplicativo; Monitor de Confiabilidade |

O Ditador **não** envia nada para lugar nenhum: sem telemetria, sem análise de
uso, sem envio automático de falhas. O log é um arquivo de texto na máquina, e é
só isso.

Processos esperados: `ditador.exe` (o que grava e transcreve) e
`Ditador.Windows.exe` (o que desenha). Os dois juntos, em repouso, medidos nesta
máquina:

| | Memória (working set) | Privada | CPU em repouso | Threads |
|---|---|---|---|---|
| `ditador.exe` | 175 MB | 1,5 GB¹ | 0,01% | 27 |
| `Ditador.Windows.exe` | 128 MB | 85 MB | 0,06% | 37 |

¹ o modelo de 574 MB mapeado mais os buffers do Vulkan; o que está de fato
residente é a coluna do working set.

## Testes

O que foi verificado nesta máquina (Windows 11 Pro 25H2, build 26200.8875, RTX
3060, um monitor a 1920×1080 em 100%):

- [x] `cargo fmt`, `cargo test` (84) e `cargo clippy` limpos
- [x] `dotnet build` Debug e Release, **zero avisos** (o projeto trata aviso como
      erro)
- [x] o atalho global **com um teclado de verdade**: segurar `Pause` com outra
      janela em foco abre o microfone, soltar transcreve, e o texto chega à área
      de transferência. Não há como automatizar isto — veja acima por que a tecla
      sintética não serve
- [x] o ciclo inteiro pelo canal de controle (`--alternar` para gravar e parar),
      com o aviso na tela acompanhando: "Gravando" com cronômetro, depois
      "Processando fala…", depois some
- [x] o fluxo `assinar` pelo pipe, conferido com PowerShell puro
- [x] ícone na área de notificação, com os quatro estados e a dica
- [x] aviso na tela com cronômetro e nível do microfone
- [x] **o foco não muda**: com o Bloco de Notas em primeiro plano, o ciclo
      inteiro de gravação foi disparado e `GetForegroundWindow` continuou
      apontando para ele antes, durante e depois — que é a razão de existir o
      `WS_EX_NOACTIVATE` e o requisito mais importante de um aviso passivo
- [x] painel de status pelo clique esquerdo, posicionado junto ao ícone
- [x] menu de contexto pelo clique direito
- [x] instância única do frontend: a segunda execução redireciona e abre o painel
- [x] **reinício do Explorer**: o ícone volta sozinho, sem duplicar
- [x] backend morto e reiniciado: o frontend reconecta em ~1 s
- [x] frontend iniciado sem backend: sobe o backend uma vez e conecta
- [x] ACL do named pipe conferida na máquina (uma ACE, só o usuário)
- [x] **notificação do sistema**: escondendo o modelo, o backend entra em erro e
      o aviso aparece na Central de Ações com o ícone e o texto do Ditador —
      `AppNotificationManager` funcionando em aplicativo desempacotado
- [x] instalação, atualização por cima e desinstalação limpas
- [x] MSIX gerado e assinado com certificado de teste
- [x] memória e CPU em repouso medidas
- [x] troca de tema do sistema: com a difusão de `WM_SETTINGCHANGE`, o ícone
      passa para o conjunto claro e volta para o escuro (conferido no log —
      mudar só a chave do registro **não** dispara a mensagem, e foi assim que o
      primeiro teste passou sem testar nada)

O que **não** foi verificado, e por quê:

- **DPI diferente de 100%** e **múltiplos monitores**: esta máquina tem um
  monitor a 100%. O código usa `GetDpiForWindow`, `GetSystemMetricsForDpi` e a
  área de trabalho do monitor da janela ativa — nada é coordenada fixa —, mas
  isso é revisão, não teste.
- **Suspender e retomar**: interromperia a sessão em que este trabalho foi feito.
  O caminho de código é o mesmo da reconexão, que foi testado matando o backend.
- **Tema claro do sistema**: os `.ico` claros existem e a troca acontece em
  `WM_SETTINGCHANGE`, mas a máquina esteve em tema escuro o tempo todo.
- **Instalação limpa numa VM do zero**: o `instalar.ps1` instala o .NET e o
  Windows App Runtime que faltarem, mas esta máquina já tinha os dois.
- **Narrator**: os nomes de automação estão definidos, sem verificação com o
  leitor de tela ligado.
- **O clique atravessando o aviso**: o `WS_EX_TRANSPARENT` está aplicado nos
  quatro `HWND` (medido), mas o efeito não foi comprovado. Montar o teste com
  automação de mouse e uma janela conhecida embaixo deu resultado inconclusivo
  **até no grupo de controle** — o clique fora da área do aviso também não
  ativou a janela de baixo —, e um teste que não distingue os dois casos não
  prova nada. Fica registrado como não verificado em vez de marcado como feito.
  O impacto, se ele não atravessar, é pequeno: uma faixa de 360 por 78 pontos no
  rodapé, por alguns segundos, e um clique perdido.

## O que falta

- [ ] **Compilar e testar no Linux.** A portabilidade mexeu em código
      compartilhado: `dbus.rs` e `tray.rs` mudaram de lugar, o `hotkey.rs` foi
      partido entre a máquina de teclas e a leitura do evdev, o `Retrato` saiu do
      `dbus.rs` para `src/retrato.rs`, e `state.rs`, `ipc.rs`, `icones.rs`,
      `stt.rs`, `keys.rs`, `clipboard.rs`, `autostart.rs`, `ui.rs` e `main.rs`
      foram editados. **Nada disso foi compilado no Linux** — a máquina onde o
      porte foi feito é Windows e não tem WSL. Houve revisão estática e mais nada.

      O que confere de uma vez: `cargo fmt --check`, `cargo test`,
      `cargo clippy`, `cargo build --release` e o
      `o_contrato_canonico_bate_com_os_tres_lados`, que só existe no build Linux
      e é quem garante que o XML, o zbus e os dois clientes continuam dizendo a
      mesma coisa. Depois, `./gnome-extension/scripts/testar.sh` e
      `./kde-plasma/testar.sh`.

      As pastas `gnome-extension/`, `kde-plasma/` e o `dbus/contrato.xml` não
      foram tocados — verificado por `git diff --name-only`.

- [ ] os testes que a máquina não permitiu: DPI misto, multimonitor, suspender e
      retomar, VM limpa, Narrator
- [ ] certificado de assinatura de verdade — e então trocar o caminho padrão de
      instalação para o MSIX
