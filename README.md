<p align="center">
  <img src="assets/png/ditador-256.png" alt="Ditador" width="128" height="128">
</p>

<h1 align="center">Ditador</h1>

<p align="center">
  Ditado por voz <b>offline</b> para Ubuntu/GNOME, em Rust, com o Whisper na GPU.<br>
  Segure <b>Pause/Break</b>, fale, solte. Nada sai da sua máquina.
</p>

<p align="center">
  <img src="assets/capturas/gravando.png" alt="A janela de gravação, no tema claro e no escuro" width="860">
</p>

<p align="center">
  <i>Tema claro e tema escuro, acompanhando o do sistema.</i>
</p>

## Como começar

**Com o pacote pronto** (Ubuntu 24.04 ou mais novo):

```bash
sudo apt install ./ditador_0.4.1_amd64.deb      # ou ditador-cpu_… se a máquina não tem GPU
sudo usermod -aG input $USER                    # para o atalho global ler o teclado
```

Saia da sessão e entre de novo. Abra o **Ditador** pelo menu de aplicativos: na
primeira vez ele oferece baixar o modelo de transcrição (~574 MB) ali mesmo, com
barra de progresso. Depois disso, tudo roda sem internet.

Em *Configurações → Sistema* está o interruptor **Iniciar junto com a sessão** —
ligue e o Ditador sobe sozinho toda vez que você entrar, já em segundo plano.

**Compilando você mesmo:**

```bash
sudo apt install -y cmake libasound2-dev libvulkan-dev glslc wl-clipboard
./instalar.sh                 # ou: ./instalar.sh cpu   |   ./instalar.sh cuda
ditador --baixar-modelo
```

**Gerando o pacote para outra máquina:**

```bash
./empacotar.sh                # target/deb/ditador_0.4.1_amd64.deb  (Vulkan)
./empacotar.sh cpu            # target/deb/ditador-cpu_0.4.1_amd64.deb
```

O pacote leva o programa, os ícones, o atalho do menu e o serviço de usuário do
systemd. Não leva o modelo: são centenas de megabytes, e a própria janela o
baixa na primeira vez.

<p align="center">
  <img src="assets/capturas/resultado.png" alt="O texto transcrito, pronto para copiar" width="760">
</p>

## Como funciona

```
evdev (/dev/input/event*)  ──►  controlador  ──►  cpal (microfone)
   segurar/soltar a tecla         │                    │ 16 kHz mono
                                  │                    ▼
   interface egui  ◄──────────────┤             whisper.cpp (Vulkan)
   gravando / resultado / config  │                    │
                                  │  ◄─────────────────┘
   ícone da barra  ◄──────────────┘        texto
   StatusNotifierItem
```

Seis threads conversando por canais: leitura de teclado, áudio, inferência,
controlador, interface e ícone. O estado compartilhado fica num `Mutex` só, e
o `Sinal` avisa a interface (repintando) e o ícone (que relê o estado) a cada
mudança — um canal de capacidade 1 por observador, para que avisos em rajada
se fundam num só.

## Uso

| Ação | Como |
|---|---|
| Ditar | Segure **Pause/Break**, fale, solte |
| Copiar | Botão **Copiar** (ou automático, já vem ligado) |
| Configurar | Ícone da barra → *Configurações*, ou `ditador --configuracoes` |
| Alternar gravação sem segurar tecla | Ícone da barra → *Ditar agora*, ou `ditador --alternar` |
| Baixar outro modelo | `ditador --baixar-modelo medium-q5_0` |
| Ver estado | `ditador --status` |
| Encerrar | Ícone da barra → *Encerrar*, ou `ditador --encerrar` |

`./baixar-modelo.sh --lista` mostra os modelos disponíveis e o tamanho de cada um.

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
tecla ou combinação), o idioma, o microfone, o modelo, ligar a colagem
automática, mandar o programa subir junto com a sessão e escolher o tema.

Com a cópia automática ligada dá para **desligar a janela de resultado** (*Área
de transferência → Mostrar a janela com o texto transcrito*): aí é falar, soltar
e colar, sem nada aparecer na frente. A janela volta a aparecer sozinha se o
texto não tiver chegado à área de transferência — a transcrição não se perde por
causa de uma preferência.

<p align="center">
  <img src="assets/capturas/configuracoes.png" alt="A tela de configurações" width="660">
</p>

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

**Como a interface é desenhada.** Em cores sólidas, e é só isso: cada tela é um
retângulo arredondado preenchido com a cor de fundo do tema, uma borda de um
pixel e uma sombra por baixo. Nada de transparência, refração ou desfoque.

A referência é o ChatGPT — o preto é preto, o branco é branco, e a hierarquia se
faz com três tons e uma linha, não com camadas translúcidas. Daí vem também o
botão principal de cada tela, em cor cheia e invertida em relação ao fundo:
preto sobre claro, branco sobre escuro. É o único elemento de contraste máximo
em qualquer tela, e por isso o olho sempre sabe qual é a ação.

A paleta inteira mora em `src/tema.rs` e tem treze cores por tema — fundo, duas
superfícies, duas bordas, dois níveis de texto, o botão principal e as cores de
gravando, concluído e erro. Os controles (`src/widgets.rs`) são todos feitos
dessas cores: botões em cápsula, interruptores que deslizam, cartões agrupando
as configurações, o seletor de tema em três abas. Sob o cursor a superfície troca
de tom numa animação de 120 ms — é a única coisa que se move, e custa uma
interpolação de cor por quadro.

**Tipografia.** A [Plus Jakarta
Sans](https://fonts.google.com/specimen/Plus+Jakarta+Sans), do Google Fonts (SIL
OFL 1.1). É uma grotesca geométrica com personalidade — o `a` e o `g` de andar
único, o corte diagonal do `t` — que ainda assim não atrapalha numa tela feita de
rótulo curto, e que segura bem os acentos do português.

O peso faz o trabalho que o tamanho faria: só três corpos aparecem na tela — 20
nos títulos (SemiBold), 14,5 nos rótulos (Medium) e no texto corrido (Regular), e
12,5 nas explicações, em cinza. A [JetBrains
Mono](https://fonts.google.com/specimen/JetBrains+Mono) entra só onde largura
fixa *é* a informação: o cronômetro, que senão dança a cada segundo, o valor de
um controle deslizante, a tecla do atalho e a versão.

As duas vão embutidas no binário, em instâncias estáticas: nenhuma máquina as tem
instaladas por padrão, e o rasterizador do egui não interpola eixos de fonte
variável — com uma variável, todo peso sairia no padrão.

**Por que o vidro saiu.** As versões 0.2 e 0.3 tinham um painel de vidro líquido
feito num shader GLSL: refração pela lei de Snell, borda especular com direção,
relevo com altura de verdade, e uma captura da tela por baixo para o vidro ter o
que refratar. Funcionava, e custava caro em código — 2.156 linhas entre o shader,
o desenho vetorial de reserva e a conversa com o `xdg-desktop-portal`, mais um
bloco de configuração com quase trinta parâmetros ópticos. Para uma
janela que fica na tela três segundos por vez, é muito aparato para pouca
entrega: a legibilidade dependia do papel de parede de cada um, e a imagem
refratada nem sequer acompanhava o que estava atrás em tempo real. O visual
sólido cabe em `tema.rs` + `widgets.rs`, não pede nada da GPU além do que o egui
já faz e é legível sobre qualquer coisa.

**Por que o programa sai com `_exit`.** Liberar os buffers da GPU enquanto a
thread principal desmonta o contexto gráfico derruba o driver da NVIDIA
(SIGSEGV dentro de `ggml_backend_vk_buffer_free_buffer`). Como o systemd leria
isso como falha e reiniciaria o serviço, o encerramento pula os destrutores
globais — o sistema recupera a memória de qualquer jeito.

**Como o "iniciar com a sessão" funciona.** Quando o serviço de usuário do
systemd está instalado (é o caso pelo pacote e pelo `instalar.sh`), o
interruptor faz `systemctl --user enable ditador`. Sem ele — quem só compilou e
rodou —, escreve um atalho em `~/.config/autostart`, que qualquer ambiente
gráfico entende. Desligar limpa os dois, para não sobrar resquício de uma
instalação anterior mandando o programa subir.

## Configuração

`~/.config/ditador/config.json`. Quase tudo aparece na tela de configurações; os
campos menos usados ficam em *Avançado* e em *Aparência*.

Um campo que vale conhecer: `initial_prompt`. O texto que você puser ali vai
como contexto para o modelo — útil para nomes próprios, jargão da sua área ou
para induzir um estilo de pontuação.

O bloco `appearance` é curto — o visual sólido não tem o que regular:

| Campo | Padrão | O que faz |
|---|---|---|
| `theme` | `"sistema"` | `"sistema"`, `"claro"` ou `"escuro"`. No modo sistema segue o que estiver escolhido em *Configurações → Aparência* do GNOME |
| `animation` / `animation_ms` | `true` / `150` | a janela entrar subindo um fio, e a duração disso |

Quem vinha da versão do vidro não precisa fazer nada: o `appearance` antigo, com
os parâmetros ópticos que não existem mais, continua sendo lido sem derrubar o
resto do arquivo — o tema volta ao padrão e as preferências que sobreviveram
ficam como estavam.

## Desenvolvimento

```bash
cargo test                       # config, reamostragem, texto, controlador
RUST_LOG=debug cargo run         # inclui o texto transcrito no log
./gerar-imagens.sh               # refaz as imagens deste README
```

E as variáveis de diagnóstico da interface, que se combinam:

| Variável | O que faz |
|---|---|
| `DITADOR_CAPTURA=<pasta>` | grava um PNG de cada tela assim que ela estabiliza |
| `DITADOR_DEMO=1` | passa sozinho pelas três telas, com texto de exemplo, e sai |
| `DITADOR_TEMA=claro\|escuro` | ignora a configuração e força um dos temas |
| `DITADOR_ZOOM=1.5` | desenha tudo maior, como numa tela densa |
| `DITADOR_QUADROS=1` | relata quadros/s, sem sincronia vertical |

A captura existe porque o GNOME nega a API de screenshot a aplicativos comuns,
e sem ela não há como conferir o desenho da interface. O `gerar-imagens.sh` junta
as quatro primeiras: roda o programa nos dois temas, deixa que ele mesmo passe
pelas telas e pousa as capturas sobre um fundo liso. Sai igual em qualquer
clone, sem microfone, sem o modelo baixado e sem ninguém falar na hora certa.

**Ícones.** `assets/ditador.svg` é o ícone do aplicativo: uma pastilha quase
preta, em silhueta de squircle, com o microfone branco dentro — quatro formas
sólidas, nenhum degradê. `assets/simbolicos/` traz os quatro estados
da barra superior (pronto, gravando, trabalhando, falhou), só com formas
preenchidas, que é o que o GTK consegue recolorir. Depois de mexer neles:

```bash
python3 assets/gerar-icones.py    # rasteriza assets/png/ (usa o librsvg do GNOME)
```

Os PNGs ficam versionados porque o binário os embute: a janela precisa do ícone
antes de qualquer instalação, e a bandeja usa os símbolos em branco como reserva
quando o tema do sistema ainda não tem os nossos.

## Limitações conhecidas

- **Colagem automática** (desligada por padrão) depende do `ydotool` e de a
  janela em foco continuar sendo a sua — a sobreposição pode roubar o foco em
  alguns gerenciadores de janela. A cópia automática não tem esse problema.
- O microfone é aberto no momento em que você pressiona a tecla; em máquinas
  lentas isso pode cortar a primeira sílaba.
- Trocar de modelo com o programa aberto libera o contexto anterior da GPU; se
  isso se mostrar instável no seu driver, reinicie o serviço depois de trocar.

## Licença

[MIT](LICENSE). Os modelos do Whisper têm licença própria (MIT também, da
OpenAI) e são baixados à parte, não distribuídos aqui.
