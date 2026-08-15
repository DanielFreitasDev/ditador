# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Idioma

Tudo neste projeto é em português (pt-BR): comentários, doc comments (`//!`, `///`), mensagens de commit,
strings de interface, logs, ajuda da CLI e README. Só o `LICENSE` é em inglês. Vale também para o
JavaScript de `gnome-extension/` — os nomes dos arquivos seguem a convenção do GNOME (`extension.js`,
`prefs.js`, `backend.js`), mas comentários, strings e identificadores nossos são em português. E para o
C++/QML de `kde-plasma/`: os nomes que o Qt e o KPackage exigem ficam como são (`metadata.json`,
`contents/ui/main.qml`, `Q_PROPERTY`, `QML_ELEMENT`), e tudo o que é nosso — classes, propriedades,
sinais, arquivos QML — é em português (`DitadorBackend`, `RepresentacaoCompleta.qml`, `gravandoDesde`).

**Identificadores novos também são em português** — módulos, funções, variáveis, structs, campos e nomes de
teste. Campos serde e enums de estado que já existem em inglês (`hotkey`, `auto_copy`, `View::Recording`,
`ModelState::Ready`) ficam como estão por compatibilidade com os arquivos de config já gravados; não renomeie.

Nomes de teste são frases inteiras: `fn o_desktop_de_autostart_aponta_para_o_binario()`.

Os comentários explicam *por quê*, não *o quê*, e cada módulo abre com um bloco `//!` justificando decisões.
Acompanhe esse registro — travessões (`—`) e reticências (`…`) tipográficas inclusive.

## Antes de considerar pronto

Nesta ordem, e todos precisam passar:

```
cargo fmt                 # rustfmt padrão, sem config própria
cargo test
cargo clippy              # sem warnings — o [lints.clippy] do Cargo.toml os trata como erro
cargo build --release
```

`cargo test` com as features padrão compila o whisper.cpp com Vulkan. Para iterar rápido:
`cargo test --no-default-features --features cpu`.

As três features de backend são mutuamente exclusivas e há `compile_error!` no topo do
`src/main.rs` garantindo isso — esquecer o `--no-default-features` falha em segundos, com a
receita certa na mensagem, em vez de compilar o Vulkan junto em silêncio.

## Build e empacotamento

Features de GPU são mutuamente exclusivas; `vulkan` é o padrão.

```
cargo build --release                                          # Vulkan
cargo build --release --no-default-features --features cpu     # só CPU
cargo build --release --no-default-features --features cuda     # exige nvcc
./instalar.sh [vulkan|cuda|cpu]   # compila, encerra a instância rodando e instala em ~/.local/bin
./empacotar.sh [vulkan|cpu|cuda]  # .deb em target/deb/ — o nome do pacote muda por backend
```

Deps de sistema: `cmake libasound2-dev libvulkan-dev glslc wl-clipboard` (e `dpkg-deb`, `fakeroot` para empacotar).

Não há CI. Nada roda automaticamente — verifique localmente.

## Extensão do GNOME Shell (`gnome-extension/`)

Opcional, alvo **só** GNOME Shell 50.x, e independente do `.deb`: o `empacotar.sh` não a leva, e o
`instalar.sh` da raiz não a instala. O portão dela é próprio:

```
cd gnome-extension
npm install && npm run lint     # ESLint 9; node_modules nunca entra no ZIP
./scripts/testar.sh             # ciclo de vida num GNOME Shell aninhado (3 voltas)
gjs -m scripts/teste-do-backend.js   # conversa com o Ditador que estiver rodando
./instalar.sh
```

`testar.sh` roda sob `dbus-run-session` porque dois GNOME Shell não dividem o nome `org.gnome.Shell` —
e por isso, lá dentro, o Ditador não existe e a extensão sobe dizendo "Indisponível". Quem cobre a outra
metade é o `teste-do-backend.js`, no barramento de verdade.

**A fonte da verdade das APIs do Shell é o próprio Shell instalado**, não a internet:

```
gresource list /usr/lib/gnome-shell/libshell-18.so
gresource extract /usr/lib/gnome-shell/libshell-18.so /org/gnome/shell/ui/quickSettings.js
```

São os 160 arquivos JS do GNOME 50.1 exatamente como esta máquina os roda. Tutoriais de GNOME 42/43/44
descrevem APIs que não existem mais.

Numa **primeira instalação** é preciso sair da sessão e entrar: o Shell varre a pasta de extensões uma vez
só, em `_loadExtensions`, e não há vigia de diretório. Habilitar/desabilitar depois disso vale na hora.
Não existe `Alt+F2` + `r` no Wayland.

Documentação técnica: `gnome-extension/README.md`.

## Integração com o KDE Plasma (`kde-plasma/`)

Opcional, alvo **só** Plasma 6.6 / Qt 6 / KF6 / Wayland, independente do `.deb` e do `instalar.sh` da
raiz. Sem código de Plasma 5, KF5, Qt 5 nem `metadata.desktop`. Portão próprio:

```
./kde-plasma/testar.sh              # qmllint + compila + testes + plasmawindowed
./kde-plasma/testar.sh --contrato   # o XML canônico contra o Ditador em execução
./kde-plasma/testar.sh --backend    # o plugin conversando com o Ditador de verdade
./kde-plasma/instalar.sh            # pede sudo uma vez, para o plugin C++
```

São **duas metades**: o widget (QML + JSON, instalado pelo `kpackagetool6` no escopo do usuário, sem
senha) e o plugin C++ (módulo QML, precisa ir para `qmake6 -query QT_INSTALL_QML`, que é do sistema — o
Qt 6 não tem diretório de módulos QML por usuário). O C++ existe porque o QML do Plasma 6 não fala D-Bus
sozinho, e o atalho seria carregar o `org.kde.plasma.plasma5support`, que é a camada de compatibilidade
do Plasma 5.

**A fonte da verdade das APIs é o Plasma instalado**, como no GNOME:

```
ls /usr/share/plasma/plasmoids/                                    # widgets de verdade, para copiar o idioma
cat /usr/share/plasma/plasmoids/org.kde.plasma.vault/contents/ui/main.qml
ls /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/plasma/components/    # a API que existe mesmo
grep -A40 'name: "PlasmoidItem"' /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/plasma/plasmoid/plasmoidplugin.qmltypes
```

Documentação técnica, incluindo a pesquisa sobre OSD nativo e por que não há um: `kde-plasma/README.md`.

## O contrato D-Bus é um só

`dbus/contrato.xml` é a cópia canônica da interface. Não é ele que a cria — quem publica é o `src/dbus.rs`,
e o zbus a monta do código Rust —, mas é dele que os clientes saem: o proxy Qt é **gerado** dele em tempo
de compilação (`qt_add_dbus_interface`), e o XML embutido em `gnome-extension/src/backend.js` é comparado
com ele. O teste `o_contrato_canonico_bate_com_os_tres_lados` (em `src/dbus.rs`) lê o XML canônico, pede ao
próprio zbus a introspecção do `Servico` (`Interface::introspect_to_writer`, que não precisa de barramento)
e confere os três. Mexer num sem mexer nos outros falha o `cargo test`.

**Acrescentar, nunca renomear.** Um método a mais é invisível para quem não o conhece; um renomeado quebra
a extensão do GNOME já instalada na máquina de alguém, que não é atualizada junto com o aplicativo.

## Assets

Os PNGs de `assets/png/` e as fontes de `assets/fontes/` entram no binário via `include_bytes!`. Depois de
editar `assets/ditador.svg` ou `assets/simbolicos/*.svg`, rode `python3 assets/gerar-icones.py` (PyGObject +
librsvg) e commite os PNGs — senão o binário continua com os ícones antigos.

`./gerar-imagens.sh` regera as capturas do README; precisa de sessão gráfica ativa e Pillow.

## Lançar uma versão

`Cargo.toml` é a única fonte da verdade da versão (`empacotar.sh` faz grep nela; o binário usa
`env!("CARGO_PKG_VERSION")`). Ao subir a versão, atualize também:

1. `Cargo.lock` — é versionado; qualquer comando cargo atualiza, mas precisa ser commitado

O README não tem mais nenhuma versão escrita à mão: os nomes dos `.deb` viraram `ditador_*_amd64.deb` e o
link aponta para `releases/latest`. Não reintroduza o número lá — era um passo que já foi esquecido.

Depois: `cargo audit`, commit `Versão X.Y.Z` (ou `Versão X.Y.Z: <o bug corrigido>`),
**`git tag vX.Y.Z`**, `./empacotar.sh && ./empacotar.sh cpu`, `sha256sum *.deb > SHA256SUMS` dentro de
`target/deb/`, e GitHub Release com os dois `.deb` e o `SHA256SUMS` como assets (`gh release create`).

O modelo não vai como asset: são 574 MB que não mudam entre versões, o app baixa sozinho e o
`--baixar-modelo` resolve por terminal. A release 0.2.0 leva uma cópia dele por motivos históricos.

### O que o `cargo audit` costuma dizer

Na 0.5.0: **zero vulnerabilidades**, e um aviso de `ttf-parser` sem manutenção (RUSTSEC-2026-0192).
Esse aviso não é acionável aqui e é esperado — a cadeia é
`eframe → winit → sctk-adwaita → ab_glyph → owned_ttf_parser → ttf-parser`, cinco níveis abaixo, e o
`sctk-adwaita` desenha a decoração de janela do Wayland, que este programa nem usa
(`with_decorations(false)`, e o padrão é XWayland). Trocar pelo `skrifa` que o aviso sugere é trabalho do
`ab_glyph`. Some sozinho quando o eframe atualizar.

**Não silencie com `--ignore`.** O que interessa é a linha `vulnerabilities: 0` continuar zero; um aviso
conhecido aparecendo duas ou três vezes por ano é o comportamento certo, e a lista de exceções é o que
esconderia o dia em que ele virar problema de verdade.

⚠️ O `git tag` é o passo mais fácil de esquecer, e já foi pulado em quatro versões seguidas: o repositório
chegou à 0.4.2 tendo `v0.2.0` como única tag, e o único release publicado ainda mostrava a interface de
vidro que o README já dizia ter removido. Confira com `git tag -l` **antes** de fechar. As versões puladas
não foram tagueadas depois de propósito — uma tag inventada meses depois aponta para um commit que nunca
foi empacotado nem publicado, e mentir sobre isso é pior do que a lacuna.

## Commits

Assunto em português, sentence case, sem prefixo e sem conventional commits — descreve o efeito, não o arquivo
(`Mais ar embaixo das fileiras de botões`). Corpo longo em prosa, explicando causa e raciocínio. Todo commit
termina com o trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

## Armadilhas — não "consertar"

- **`_exit(codigo)` em `src/main.rs:369`** pula os destrutores de propósito: desmontar os buffers do
  ggml/Vulkan dá SIGSEGV no driver NVIDIA, e o systemd trataria isso como falha e reiniciaria o app.
  Todo caminho de saída passa por ele, inclusive o de erro da interface — devolver o erro com `?` faria o
  processo terminar pelo runtime do Rust, que é exatamente o que se quer evitar.
- **`clipboard::remember_environment()` é a primeira linha de `main()`** (`src/main.rs:78`). O modo X11
  (`force_x11: true` por padrão) remove `WAYLAND_DISPLAY` do ambiente em `src/main.rs:196`; sem o snapshot
  anterior o `wl-copy` para de funcionar. Não reordene.
- **Renderer glow com `multisampling: 0`** (`src/main.rs:323`): o wgpu recusou transparência e nenhuma config do
  glutin combina alpha com MSAA. Mudar qualquer um dos dois quebra os cantos arredondados ou a criação da janela.
- **Quem diz se o microfone está aberto é `recording_since`, nunca `state.view`** — use `Shared::gravando()`
  (`src/state.rs:133`), que existe para não haver duas maneiras de perguntar. Falar de novo enquanto a frase
  anterior é transcrita é o uso normal do programa, e nesse intervalo a janela do resultado anterior pode tomar
  a tela por cima de um ditado em andamento. Decidindo pela tela, o `stop_recording` desistia e o microfone
  ficava aberto para sempre; a bandeja, que decidia igual, oferecia "Ditar agora" num item que parava a
  gravação. Pelo mesmo motivo os eventos do áudio e da transcrição carregam o número do ditado
  (`AudioCmd::Start { ditado }`, `SttCmd::Transcribe { ditado, .. }`): é ele que separa o que é da gravação de
  agora do que é de uma anterior que ainda estava a caminho.
- **Texto de interface não guarda estado.** O fechamento da tela de erro já foi decidido comparando
  `state.message.starts_with("Carregando")`, e o caminho do download escrevia outra frase — a janela ficava
  presa com o emblema de erro ao lado de uma mensagem de sucesso. Quem decide agora é `erro_e_so_espera`
  (`src/state.rs`). Não volte a ler mensagem para tomar decisão.
- **A reamostragem mora em `src/stt.rs`, não em `src/audio.rs`.** São ~100 multiplicações por amostra de
  saída, e fazê-las na thread de áudio prendia a mesma thread que precisa estar livre para abrir o microfone
  do ditado seguinte. `AudioEvent::Captured` entrega o áudio na taxa do dispositivo, de propósito.
- **Bandeja (`src/tray.rs`)**: o app sobe antes das extensões do GNOME Shell, então um StatusNotifierWatcher
  ausente significa "esperar", não "erro".
- **`Config` usa `#[serde(default)]`** e há testes garantindo que configs antigas continuam carregando. Ao mexer
  em `src/config.rs`, preserve isso. E só a **ausência** do arquivo autoriza gravar os padrões por cima: qualquer
  outro erro de leitura segue com os padrões em memória, sem tocar no que está no disco.
- O atalho global lê `/dev/input/event*` via evdev: sem o usuário no grupo `input` ele silenciosamente não
  funciona. `instalar.sh` e o postinst do `.deb` apenas avisam, e `ditador --diagnostico` diz na cara.
  O aviso disso mora em `Shared::aviso_atalho`, com campo próprio — dividindo o `message` com o aviso do
  modelo faltando, um dos dois sumia antes de ser lido.
- **`pressed`, em `src/hotkey.rs`, conta origens por dispositivo.** Guardando só o código da tecla, o teclado
  virtual que o `ydotool` cria para a colagem automática soltava a tecla que a pessoa ainda segurava.
- **`EstadoPublico` (`src/state.rs`) é a única tabela de "em que pé está o programa".** O ícone da bandeja
  (`icones::Estado::do_publico`) e o D-Bus saem dela. Já foram duas tabelas, e duas é uma a mais do que se
  consegue manter iguais. Os textos de `EstadoPublico::nome` (`pronto`, `gravando`, …) são **protocolo**: a
  extensão do GNOME os compara, e há um teste em `src/dbus.rs` para que mudá-los não passe batido.
- **Quem recolhe o ícone da bandeja é a presença de um nome no barramento, não um aviso da extensão.**
  `dbus.rs` vigia `io.github.danielfreitasdev.Ditador.GnomeExtension` e escreve em `Shared::extensao_gnome`;
  `tray.rs` **desregistra** o item (não usa `Status::Passive`, que o hospedeiro pode ignorar) e `ui.rs`
  esconde as telas de gravação por `Shared::tela_visivel`. É assim porque o barramento solta o nome sozinho
  quando a conexão cai — Shell reiniciado, extensão morta no meio do `disable()`, tanto faz: o ícone volta.
  Um protocolo de "avise quando sair" perderia todos esses casos.
- **`tela_visivel` esconde só `Recording` e `Processing`.** Resultado, configurações e erro continuam do
  aplicativo mesmo com a extensão ligada: são telas com texto para copiar e com os botões que resolvem o
  problema, e um OSD não tem onde pôr isso.
- **`GravandoDesde` não é recalculado enquanto a gravação é a mesma** (`Retrato::tirar`, em `src/dbus.rs`).
  O `Instant` é monotônico e vira hora de parede por subtração, então recalculá-lo daria um número um pouco
  diferente a cada vez — e o cronômetro do OSD, que é desenhado a partir dele, voltaria para zero no meio
  da frase.
- **O sinal `Nivel` só é emitido durante a gravação**, a 15 Hz (`INTERVALO_DO_NIVEL`, em `src/dbus.rs`).
  Fora dela a thread fica parada num `recv` do `Sinal` — não há laço acordando para perguntar se já é hora.
  É a única coisa periódica do projeto inteiro, e é por isso que ela é fechada dos dois lados: nada de
  propriedade (que guardaria o último valor para sempre e faria `PropertiesChanged` quinze vezes por
  segundo) e nada de emitir com o microfone fechado.
- **`state::Integracoes` responde a duas perguntas, e não a uma.** `mostram_o_icone()` (quem já mostra o
  Ditador na barra — GNOME **ou** Plasma) e `mostram_o_aviso()` (quem já avisa na tela que se está
  gravando — **só** o GNOME). Juntar as duas num booleano só, como já foi, apaga o aviso de gravação de
  quem usa o Plasma: lá não há nada que o substitua. Há testes cobrindo as duas separadamente.
- **Não tente de novo o OSD nativo no KWin.** O `SceneEffect` — único caminho declarativo — é o
  `ScriptedQuickSceneEffect`, que herda o `paintScreen` do `QuickSceneEffect`; ele **não** encadeia
  `effects->paintScreen()` e portanto substitui a cena inteira enquanto ativo. Uma caixinha de "Gravando"
  viria com todas as janelas sumindo. O único efeito que faz o certo (`OutputLocatorEffect`) é C++ dentro
  do KWin, contra ABI interna. E o `org.kde.osdService` é interno do `plasmashell`: sem XML em
  `/usr/share/dbus-1/interfaces/` e sem promessa a terceiros. Está tudo apurado em `kde-plasma/README.md`.
- **No `kde-plasma/CMakeLists.txt` entra só o `KDEInstallDirs` do ECM.** O `KDECMakeSettings` zera o
  prefixo das bibliotecas MODULE (convenção dos plugins do KDE, que são `nome.so`), e um módulo QML precisa
  de `libnome.so`: com ele o módulo era achado e o plugin não, com a mensagem
  `module "…" plugin "ditadorplasma" not found`.
- **`dbus/contrato.xml`: DOCTYPE numa linha só, e nada de `--` nos comentários.** O `qdbusxml2cpp` recusa o
  DOCTYPE quebrado em duas linhas (o `gdbus introspect` o imprime assim — não copie a saída dele por cima),
  e XML proíbe dois hifens seguidos dentro de comentário, o que derruba qualquer linha de comando com
  opções longas ali dentro. `xmllint --noout dbus/contrato.xml` diz na hora.
- **O `Version` do `plasmoid/package/metadata.json` é `0.0.0` de propósito.** Quem o preenche é o
  `instalar.sh`, lendo o `Cargo.toml` — que continua sendo a única fonte da verdade da versão. O
  `CMakeLists.txt` faz o mesmo. Não escreva a versão à mão em nenhum dos dois.
- **Depois de mexer no widget, reinicie o `plasmashell`** (`systemctl --user restart plasma-plasmashell`)
  — inclusive para mudanças só de QML. O `kpackagetool6 --upgrade` troca os arquivos no disco, mas o
  `plasmashell` fica com a compilação anterior na memória: já foi observado ele repetir no journal um erro
  de sintaxe que não existia mais no arquivo instalado, no exato segundo do `--upgrade`. Sem saber disso
  a pessoa corrige o QML, vê o mesmo erro e vai procurar o problema no lugar errado. Para iterar sem
  reiniciar nada, use o `./kde-plasma/testar.sh`, que abre o widget num processo à parte.
- **Os 4 avisos `Plasmoid.contextualActions` do `qmllint` não são nossos.** A propriedade existe (é do
  `Plasma::Applet`); o `qmllint` não enxerga propriedades de objeto anexado, e os widgets do próprio
  Plasma 6.6 produzem os mesmos — confira rodando-o no `org.kde.plasma.vault`. O `testar.sh` filtra
  exatamente esses quatro e falha em qualquer outro. Os demais foram **resolvidos**, não silenciados
  (`KI18nContext` no lugar do `i18nd` solto; `Plasmoid` alcançado só do arquivo raiz).
- **`dbus::start` vem antes de `tray::start` em `main.rs`.** É o D-Bus que descobre se a extensão já está no
  ar; descobrindo primeiro, a bandeja nasce sabendo e o ícone não pisca na barra no login.

## Variáveis de diagnóstico

Combináveis, lidas em `src/main.rs` e `src/ui.rs`:

| Variável | Efeito |
|---|---|
| `DITADOR_CAPTURA=<dir>` | grava um PNG de cada tela quando ela estabiliza |
| `DITADOR_DEMO=1` | percorre as três telas com texto de exemplo e sai |
| `DITADOR_TEMA=claro\|escuro` | ignora o tema configurado |
| `DITADOR_ZOOM=1.5` | fator de zoom, limitado entre 0.5 e 3.0 |
| `DITADOR_QUADROS=1` | desliga o vsync e loga FPS a cada 2 s |
| `RUST_LOG=ditador=debug` | inclui o texto transcrito no log |

`RUST_LOG=debug` seco também funciona, mas traz junto o aperto de mão do zbus e o C do
ggml — que o filtro padrão (`FILTRO_PADRAO`, em `src/main.rs`) mantém em `warn` justamente
porque ocupavam três quartos do journal.

`ditador --diagnostico` confere de uma vez o grupo `input`, o modelo, o microfone, o
`wl-copy`, o `ydotool`, o `curl`, a integração de área de trabalho no ar e a instância em
execução. É a primeira coisa a rodar quando alguém disser que "não acontece nada" — ou que
"o ícone do Ditador sumiu da barra", que é a pergunta que a linha da integração responde.

## Instância única

Uma segunda execução não abre outro processo: `src/ipc.rs` tenta abrir `$XDG_RUNTIME_DIR/ditador.sock` e, se já
estiver ocupado, manda um comando para a instância viva. Subcomandos da CLI têm nome em português com alias em
inglês (`--alternar|--toggle`, `--encerrar|--quit`).
