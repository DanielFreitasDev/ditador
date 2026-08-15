# Ditador no Windows 11

Documentação técnica do suporte a Windows. Para o uso normal, veja o `README.md`
da raiz.

> **Estado em 15/08/2026.** O núcleo em Rust compila, roda e transcreve no
> Windows, com o atalho global verificado em hardware real. O frontend em
> WinUI 3 — ícone na área de notificação, aviso de gravação na tela e popup de
> status — **ainda não existe**. Sem ele o Ditador funciona por atalho,
> transcrição e área de transferência, mas não aparece na barra.
>
> **O lado Linux não foi recompilado depois desta portabilidade.** Ela mexeu em
> código compartilhado, e a máquina onde foi feita é Windows. É o primeiro item
> da lista no fim deste arquivo, e deve ser feito antes de qualquer coisa nova.

## O que muda entre Linux e Windows

O núcleo é o mesmo código: máquina de estados do ditado, Whisper, configuração,
interface do egui, regras de transcrição. O que muda mora todo em
`src/plataforma/`, com um contrato de sete módulos que o compilador cobra.

| | Linux | Windows |
|---|---|---|
| Atalho global | evdev (`/dev/input/event*`) | Raw Input (`WM_INPUT`, `RIDEV_INPUTSINK`) |
| Nomes de tecla | tabela do evdev | tabela `VK_*` → código canônico |
| Canal de controle | socket Unix em `$XDG_RUNTIME_DIR` | named pipe `\\.\pipe\Ditador-<SID>` |
| Instância única | `connect()` no socket | `FILE_FLAG_FIRST_PIPE_INSTANCE` |
| Ícone na barra | StatusNotifierItem (ksni) | do frontend WinUI, não deste processo |
| Integração de desktop | nomes no barramento D-Bus | presença do frontend no pipe |
| Início automático | systemd `--user` ou `.desktop` do XDG | `HKCU\…\CurrentVersion\Run` |
| Colagem automática | `ydotool` (opcional) | não existe, por decisão |
| Área de transferência | `wl-copy`, com `arboard` de reserva | `arboard` |
| Configuração | `~/.config/ditador/` | `%APPDATA%\ditador\` |
| Modelos | `~/.local/share/ditador/models/` | `%LOCALAPPDATA%\ditador\models\` |

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

## Requisitos de compilação

```powershell
winget install --id Rustlang.Rustup            # alvo stable-x86_64-pc-windows-msvc
winget install --id Microsoft.VisualStudio.2026.BuildTools   # workload C++
winget install --id Kitware.CMake
winget install --id LLVM.LLVM                  # libclang, para o bindgen
winget install --id KhronosGroup.VulkanSDK     # só para a feature vulkan
```

O CUDA Toolkit é opcional e só serve à feature `cuda`; baixe-o pela NVIDIA.

## Compilar

```powershell
.\windows-integration\scripts\build.ps1                 # Vulkan, o padrão
.\windows-integration\scripts\build.ps1 -Backend cpu
.\windows-integration\scripts\build.ps1 -Backend cuda
.\windows-integration\scripts\build.ps1 -Testar         # fmt, testes e clippy antes
```

O script carrega o ambiente do Visual Studio, confere a caixa de ferramentas e
ajusta as quatro variáveis que o build exige. Para trabalhar à mão numa sessão:

```powershell
. .\windows-integration\scripts\ambiente.ps1
```

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
chave já ligada.

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

### Instância única sai de graça

`FILE_FLAG_FIRST_PIPE_INSTANCE` faz o `CreateNamedPipeW` falhar se o nome já
existir. Isso é exatamente a pergunta "já há um Ditador nesta sessão?",
respondida pelo próprio sistema — sem mutex separado e sem arquivo de trava que
sobreviva a um travamento. Não há corrida entre conferir e criar, porque as duas
coisas são a mesma chamada. (O socket Unix precisa de um segundo `connect` depois
do erro justamente por não ter isso.)

Detalhe contraintuitivo: o erro devolvido quando o nome já existe é
`ERROR_ACCESS_DENIED`, que parece problema de permissão.

### Raw Input, e não `WH_KEYBOARD_LL` nem `RegisterHotKey`

`RegisterHotKey` entrega um evento só, sem o "soltou" — não serve a
segurar-para-falar, que é a semântica inteira deste programa.

`WH_KEYBOARD_LL` funciona, mas é um gancho global: toda tecla digitada em
qualquer aplicativo passaria pelo nosso processo antes de chegar ao destino. A
própria documentação da Microsoft recomenda Raw Input para monitoramento e avisa
que um gancho lento é **removido em silêncio** pelo sistema — falha que aparece
como "o atalho parou de funcionar depois de um tempo". Fica como reserva.

Usamos `RIDEV_INPUTSINK` e **nada além**. Em particular, nada de
`RIDEV_NOLEGACY`: o Ditador observa o teclado, não o consome.

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

## O que está pronto e o que falta

Pronto e verificado nesta máquina:

- [x] o núcleo em Rust compila no MSVC, com os três backends
- [x] `cargo fmt`, `cargo test` (78) e `cargo clippy` limpos nos três
- [x] áudio pelo cpal/WASAPI — lista e escolhe dispositivos
- [x] Whisper transcrevendo, com o modelo baixado pelo próprio programa
- [x] caminhos de configuração e de modelo corretos para Windows
- [x] named pipe com DACL restrita e instância única; `--status` e `--encerrar`
      atravessando o pipe
- [x] **o atalho global em hardware real**: segurar `Pause` com outra janela em
      foco abre o microfone, soltar transcreve, e o texto chega à área de
      transferência
- [x] início automático pela chave `Run`

Duas coisas que só o teste em hardware revelou:

* **A tecla `Pause` faz auto-repetição.** Segurá-la produz um par de mensagens
  (`VKey=0x13` com E1, depois `VKey=0xFF`) a cada repique — treze delas em três
  segundos. A máquina de teclas absorve isso porque só reage à transição; se
  algum dia ela passar a contar apertos, o microfone vai abrir e fechar treze
  vezes por ditado.
* **A primeira transcrição da máquina leva ~22 s**, contra 0,4 s nas seguintes.
  É o driver compilando os pipelines de shader do Vulkan, e o cache é por
  executável — depois disso nem o reinício do programa paga de novo. Vale um
  aviso na tela algum dia; não vale uma mudança de arquitetura.

Falta — e o primeiro item é o mais urgente:

- [ ] **Compilar e testar no Linux.** A portabilidade mexeu em código Linux:
      `dbus.rs` e `tray.rs` mudaram de lugar, `hotkey.rs` foi partido entre a
      máquina de teclas e a leitura do evdev, e `keys.rs`, `ipc.rs`,
      `clipboard.rs`, `autostart.rs`, `icones.rs`, `ui.rs` e `main.rs` foram
      editados. **Nada disso foi compilado no Linux** — a máquina onde o porte
      foi feito é Windows e não tem WSL. Houve revisão estática e mais nada.

      O que confere de uma vez: `cargo fmt --check`, `cargo test`,
      `cargo clippy`, `cargo build --release` e o
      `o_contrato_canonico_bate_com_os_tres_lados`, que só existe no build
      Linux e é quem garante que o XML, o zbus e os dois clientes continuam
      dizendo a mesma coisa. Depois, `./gnome-extension/scripts/testar.sh` e
      `./kde-plasma/testar.sh`.

      As pastas `gnome-extension/`, `kde-plasma/` e o `dbus/contrato.xml` não
      foram tocados — verificado por `git diff --name-only`. Mas o Rust que
      conversa com elas, sim.

- [ ] o fluxo de eventos no pipe (o comando `assinar`), para o frontend receber
      mudanças de estado sem perguntar
- [ ] o frontend `Ditador.Windows` em WinUI 3: ícone, popup, OSD, notificações,
      e a instância única pelo `AppInstance` do Windows App SDK
- [ ] um `AppUserModelID` estável, para o Shell associar notificações e ícone ao
      aplicativo certo
- [ ] empacotamento (MSIX ou instalador) e `install.ps1` / `uninstall.ps1`;
      com MSIX, trocar a chave `Run` pelo `StartupTask`
- [ ] documentar a assinatura de código (o `.pfx` já está no `.gitignore`;
      falta o procedimento)
- [ ] os roteiros de teste: multimonitor, DPI misto, reinício do Explorer,
      suspender e retomar, ACL vista de outra conta, e a instalação limpa numa
      VM do zero
- [ ] medir e registrar memória e CPU em repouso, do backend e do frontend
- [ ] avisar na tela que a primeira transcrição da máquina demora (~22 s
      compilando shaders) em vez de deixar parecer que travou
- [ ] considerar uma matriz de CI (`ubuntu-latest` + `windows-latest`), que hoje
      não existe — o projeto nunca teve CI, e é ela que pegaria justamente o
      primeiro item desta lista
