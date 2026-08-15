# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Idioma

Tudo neste projeto é em português (pt-BR): comentários, doc comments (`//!`, `///`), mensagens de commit,
strings de interface, logs, ajuda da CLI e README. Só o `LICENSE` é em inglês.

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
`wl-copy`, o `ydotool`, o `curl` e a instância em execução. É a primeira coisa a rodar
quando alguém disser que "não acontece nada".

## Instância única

Uma segunda execução não abre outro processo: `src/ipc.rs` tenta abrir `$XDG_RUNTIME_DIR/ditador.sock` e, se já
estiver ocupado, manda um comando para a instância viva. Subcomandos da CLI têm nome em português com alias em
inglês (`--alternar|--toggle`, `--encerrar|--quit`).
