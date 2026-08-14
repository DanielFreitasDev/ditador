# Ditador

Ditado por voz **offline** para Ubuntu/GNOME, em Rust, usando o Whisper
(whisper.cpp) na GPU. Nada é enviado para a internet.

Segure **Pause/Break**, fale, solte. O texto aparece numa caixinha de vidro e já
vai para a área de transferência.

## Como funciona

```
evdev (/dev/input/event*)  ──►  controlador  ──►  cpal (microfone)
   segurar/soltar a tecla         │                    │ 16 kHz mono
                                  │                    ▼
   interface egui  ◄──────────────┤             whisper.cpp (Vulkan)
   vidro / animação / resultado   │                    │
                                  │  ◄─────────────────┘
   ícone da barra  ◄──────────────┘        texto
   StatusNotifierItem
```

Seis threads conversando por canais: leitura de teclado, áudio, inferência,
controlador, interface e ícone. O estado compartilhado fica num `Mutex` só, e
o `Sinal` avisa a interface (repintando) e o ícone (que relê o estado) a cada
mudança — um canal de capacidade 1 por observador, para que avisos em rajada
se fundam num só.

## Instalação

```bash
sudo apt install -y cmake libasound2-dev libvulkan-dev glslc wl-clipboard
```

```bash
./baixar-modelo.sh && ./instalar.sh && systemctl --user enable --now ditador
```

O `instalar.sh` compila, põe o binário em `~/.local/bin`, registra o ícone e o
serviço de usuário. Rode `./baixar-modelo.sh --lista` para ver outros modelos.

Seu usuário precisa estar no grupo `input` (para ler o teclado):

```bash
sudo usermod -aG input $USER
```

Depois disso, saia e entre de novo na sessão.

## Uso

| Ação | Como |
|---|---|
| Ditar | Segure **Pause/Break**, fale, solte |
| Copiar | Botão **Copiar** (ou automático, já vem ligado) |
| Configurar | Ícone da barra → *Configurações*, ou `ditador --configuracoes` |
| Alternar gravação sem segurar tecla | Ícone da barra → *Ditar agora*, ou `ditador --alternar` |
| Ver estado | `ditador --status` |
| Encerrar | Ícone da barra → *Encerrar*, ou `ditador --encerrar` |

### Ícone na barra superior

Enquanto o Ditador está no ar, um ícone fica na barra de cima e mostra o estado
sem precisar abrir nada:

| Ícone | Significa |
|---|---|
| microfone | pronto — segure o atalho e fale |
| ponto de gravação | ouvindo |
| carregando | transcrevendo, ou carregando o modelo |
| triângulo de aviso | o modelo não carregou |

Clicar abre o menu: *Ditar agora*, *Configurações*, *Encerrar*.

O GNOME não tem bandeja nativa; o ícone é um **StatusNotifierItem**, exibido
pela extensão *Ubuntu AppIndicators*, que vem habilitada no Ubuntu. Se ela
estiver desligada, o Ditador funciona igual — só avisa no log que ficou sem
ícone.

Nas configurações dá para trocar o atalho (clique no botão e pressione a nova
tecla ou combinação), o idioma, o microfone, o modelo, e ligar a colagem
automática.

## Decisões que valem explicar

**Por que ler `/dev/input` em vez de usar o atalho do GNOME.** No Wayland o
GNOME não entrega o evento de *soltar* a tecla para aplicativos comuns, então
"segurar para falar" seria impossível. A leitura do evdev é passiva: as teclas
continuam chegando normalmente ao programa em foco. Por isso o atalho padrão é
o Pause/Break — uma tecla sem função própria em lugar nenhum.

Se preferir não usar o grupo `input`, dá para criar um atalho do GNOME apontando
para `ditador --alternar`: aí é apertar uma vez para começar e outra para parar.

**Por que Vulkan e não CUDA.** O CUDA 12.4, única versão nos repositórios do
Ubuntu 26.04, não compila contra a glibc 2.43 do sistema. O Vulkan usa a mesma
GPU, já vinha instalado com o driver e roda o `large-v3-turbo` em ~0,6 s por
frase. Para compilar com CUDA (exige o toolkit da NVIDIA):

```bash
./instalar.sh cuda    # ou: ./instalar.sh cpu
```

**Por que a janela vai pelo XWayland.** No Wayland um aplicativo comum não
escolhe onde sua janela aparece nem consegue ficar por cima das outras — as duas
coisas que uma sobreposição de ditado precisa. Pelo X11 funciona. Desligue em
*Configurações → Avançado* se preferir Wayland nativo.

**Por que o vidro não borra o fundo.** Nenhum compositor do Linux expõe desfoque
de fundo para aplicativos. Então o vidro vem das outras pistas que o olho usa
para reconhecê-lo, todas em `src/glass.rs`:

* silhueta de **squircle** — os cantos são quartos de superelipse
  (`|x/r|⁴·² + |y/r|⁴·² = 1`), não arcos de círculo, que é o que dá a curva
  contínua dos cantos da Apple. Cápsulas e círculos voltam ao arco;
* **borda especular com direção**: a normal da silhueta é calculada ponto a
  ponto e comparada com a direção da luz, então a beirada acende no topo/à
  esquerda, esfria e azula embaixo — e ganha um brilho extra por onde o cursor
  passa, como vidro polido sob a mão;
* **faixa de refração**: quatro linhas concêntricas cada vez mais fracas
  imitando a luz que a borda concentra;
* brilho de topo e retorno de base em malha, com as linhas de corte adensadas
  na altura dos cantos para o degradê não cortar a curva;
* duas bordas separadas por 2 pt, para sugerir espessura.

Os controles (`src/widgets.rs`) são feitos das mesmas peças: botões em cápsula,
interruptores que deslizam, cartões agrupando as configurações — todos acendem
sob o cursor e afundam ao serem pressionados.

**Por que o programa sai com `_exit`.** Liberar os buffers da GPU enquanto a
thread principal desmonta o contexto gráfico derruba o driver da NVIDIA
(SIGSEGV dentro de `ggml_backend_vk_buffer_free_buffer`). Como o systemd leria
isso como falha e reiniciaria o serviço, o encerramento pula os destrutores
globais — o sistema recupera a memória de qualquer jeito.

## Configuração

`~/.config/ditador/config.json`. Tudo que está lá aparece na tela de
configurações; os campos menos usados ficam em *Avançado*.

Um campo que vale conhecer: `initial_prompt`. O texto que você puser ali vai
como contexto para o modelo — útil para nomes próprios, jargão da sua área ou
para induzir um estilo de pontuação.

## Desenvolvimento

```bash
cargo test                       # reamostragem e limpeza de texto
RUST_LOG=debug cargo run         # inclui o texto transcrito no log
DITADOR_CAPTURA=/tmp/shots cargo run --release   # grava um PNG de cada tela
```

A captura existe porque o GNOME nega a API de screenshot a aplicativos comuns,
e sem ela não há como conferir o desenho da interface.

## Limitações conhecidas

- **Colagem automática** (desligada por padrão) depende do `ydotool` e de a
  janela em foco continuar sendo a sua — a sobreposição pode roubar o foco em
  alguns gerenciadores de janela. A cópia automática não tem esse problema.
- O microfone é aberto no momento em que você pressiona a tecla; em máquinas
  lentas isso pode cortar a primeira sílaba.
- Trocar de modelo com o programa aberto libera o contexto anterior da GPU; se
  isso se mostrar instável no seu driver, reinicie o serviço depois de trocar.
