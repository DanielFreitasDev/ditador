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
cargo clippy              # sem warnings
cargo build --release
```

`cargo test` com as features padrão compila o whisper.cpp com Vulkan. Para iterar rápido:
`cargo test --no-default-features --features cpu`.

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
2. `README.md` linhas 25, 47 e 48 — os nomes dos `.deb` estão escritos à mão

Depois: commit `Versão X.Y.Z` (ou `Versão X.Y.Z: <o bug corrigido>`), `git tag vX.Y.Z`, `./empacotar.sh`, e
GitHub Release com o `.deb` como asset (`gh release create`).

## Commits

Assunto em português, sentence case, sem prefixo e sem conventional commits — descreve o efeito, não o arquivo
(`Mais ar embaixo das fileiras de botões`). Corpo longo em prosa, explicando causa e raciocínio. Todo commit
termina com o trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

## Armadilhas — não "consertar"

- **`_exit(0)` em `src/main.rs:280`** pula os destrutores de propósito: desmontar os buffers do ggml/Vulkan dá
  SIGSEGV no driver NVIDIA, e o systemd trataria isso como falha e reiniciaria o app.
- **`clipboard::remember_environment()` é a primeira linha de `main()`** (`src/main.rs:35`). O modo X11
  (`force_x11: true` por padrão) remove `WAYLAND_DISPLAY` do ambiente em `src/main.rs:130`; sem o snapshot
  anterior o `wl-copy` para de funcionar. Não reordene.
- **Renderer glow com `multisampling: 0`** (`src/main.rs:242`): o wgpu recusou transparência e nenhuma config do
  glutin combina alpha com MSAA. Mudar qualquer um dos dois quebra os cantos arredondados ou a criação da janela.
- **Quem diz se o microfone está aberto é `recording_since`, nunca `state.view`** (`src/controller.rs`). Falar de
  novo enquanto a frase anterior é transcrita é o uso normal do programa, e nesse intervalo a janela do resultado
  anterior pode tomar a tela por cima de um ditado em andamento. Decidindo pela tela, o `stop_recording` desistia
  e o microfone ficava aberto para sempre. Pelo mesmo motivo os eventos do áudio carregam o número do ditado
  (`AudioCmd::Start { ditado }`): é ele que separa o que é da gravação de agora do que é de uma anterior que
  ainda estava a caminho.
- **Bandeja (`src/tray.rs`)**: o app sobe antes das extensões do GNOME Shell, então um StatusNotifierWatcher
  ausente significa "esperar", não "erro".
- **`Config` usa `#[serde(default)]`** e há testes garantindo que configs antigas continuam carregando. Ao mexer
  em `src/config.rs`, preserve isso.
- O atalho global lê `/dev/input/event*` via evdev: sem o usuário no grupo `input` ele silenciosamente não
  funciona. `instalar.sh` e o postinst do `.deb` apenas avisam.

## Variáveis de diagnóstico

Combináveis, lidas em `src/main.rs` e `src/ui.rs`:

| Variável | Efeito |
|---|---|
| `DITADOR_CAPTURA=<dir>` | grava um PNG de cada tela quando ela estabiliza |
| `DITADOR_DEMO=1` | percorre as três telas com texto de exemplo e sai |
| `DITADOR_TEMA=claro\|escuro` | ignora o tema configurado |
| `DITADOR_ZOOM=1.5` | fator de zoom, limitado entre 0.5 e 3.0 |
| `DITADOR_QUADROS=1` | desliga o vsync e loga FPS a cada 2 s |
| `RUST_LOG=debug` | inclui o texto transcrito no log |

## Instância única

Uma segunda execução não abre outro processo: `src/ipc.rs` tenta abrir `$XDG_RUNTIME_DIR/ditador.sock` e, se já
estiver ocupado, manda um comando para a instância viva. Subcomandos da CLI têm nome em português com alias em
inglês (`--alternar|--toggle`, `--encerrar|--quit`).
