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

## Antes de começar: `git pull`

**Primeira coisa de toda tarefa, antes de ler ou editar qualquer arquivo:**

```
git pull
```

Este repositório recebe commits que não saem daqui. O workflow "Publicar versão" grava a versão nova no
`Cargo.toml`, no `Cargo.lock`, no `metadata.json` e no `CHANGELOG.md`, cria a tag e empurra sozinho — e
outros automatismos do GitHub também escrevem no `main`. Quer dizer que o `main` local pode estar atrás
sem que nada nesta máquina tenha mudado, e que ele pode ficar atrás **no meio de uma tarefa**, entre o
primeiro arquivo lido e o `git push`.

Sem o `pull`, o preço aparece só no fim: o push é recusado por não ser fast-forward e é preciso rebasear
com o trabalho já pronto — e, pior, o que se produziu pode estar errado por ter partido de um estado
velho. Já aconteceu com as capturas do README: elas foram geradas mostrando `v0.5.0` no cabeçalho da
tela de configurações enquanto a 0.6.0 já estava publicada no remoto, e tiveram de ser refeitas depois
do rebase.

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

No Windows, o mesmo, com o ambiente carregado antes (o `build.ps1` faz isso
sozinho):

```
.\windows-integration\scripts\build.ps1 -Testar        # fmt, testes e clippy no Rust
dotnet build windows-integration\Ditador.Windows.sln   # o frontend, aviso = erro
```

Isso tudo a CI também roda (veja abaixo). O que ela **não** roda continua sendo por sua conta: os
portões da extensão do GNOME (`./gnome-extension/scripts/testar.sh`) e do Plasma
(`./kde-plasma/testar.sh`) precisam de um Shell aninhado e de um `plasmashell`, e não existe agente
que os tenha. Mexeu na extensão ou no widget, rode o portão dele na sua máquina — e conte com o
`testar.sh` da extensão precisar de mais de uma tentativa, que ele já faz sozinho até três vezes.

## Memória técnica persistente (`docs/LEARNINGS.md`)

O projeto guarda em **`docs/LEARNINGS.md`** o que já foi investigado aqui: problemas cuja causa não era
óbvia, o sintoma exato que eles davam, a causa que se descobriu no fim, o que resolveu e a regra que
evita a próxima vez. Particularidades das bibliotecas, dos três sistemas de área de trabalho suportados,
do Whisper, do áudio, do empacotamento e da CI moram lá.

**Antes de investigar, procure lá.** Erro inesperado, erro de compilação, teste que falha, CI vermelha,
problema de ambiente, comportamento que muda entre Windows, GNOME e KDE, incompatibilidade, áudio que
não abre, modelo que não carrega, diferença entre CPU e GPU, pacote que não instala, release que não
sai — a pergunta pode já ter resposta. Pesquise por termos do erro, da mensagem, da biblioteca, do
módulo, do sistema ou do comportamento observado, e só comece do zero depois disso.

O que estiver lá **não é verdade absoluta**: confira se ainda vale antes de aplicar. Se um contorno
registrado no passado tiver hoje uma solução oficial ou melhor, use a de hoje, implemente-a e **atualize
a entrada**, marcando o contorno antigo como obsoleto. O arquivo deve representar o estado atual
conhecido do projeto, e não congelá-lo em decisões velhas.

**Depois de resolver, registre.** Sem perguntar se pode — faz parte de terminar a tarefa. O critério é:
*isto pouparia uma investigação, um erro ou um bom tempo de trabalho no futuro?* Registre sobretudo
quando a causa não era óbvia, quando foram precisas várias tentativas, quando a primeira hipótese estava
errada, quando a documentação oficial não bastou, ou quando há chance real de o problema voltar. Não
registre o que se lê no código, o que é trivial nem o que deu certo de primeira: **aquilo não é
aprendizado, é `git log`**. E antes de criar uma entrada nova, procure uma parecida e melhore aquela.

A divisão de responsabilidade é esta, e vale a pena mantê-la:

- **`CLAUDE.md` diz como trabalhar** — idioma, portões, arquitetura, e as armadilhas que já viraram
  regra vigente ("não 'conserte' isto").
- **`docs/LEARNINGS.md` registra o que se aprendeu trabalhando** — sintoma, causa, solução, prevenção.

Este arquivo não deve engordar acumulando problema técnico. Uma entrada do `LEARNINGS.md` que vire regra
permanente do projeto pode ganhar uma linha na seção "Armadilhas" abaixo; a explicação inteira continua
lá, que é onde ela cabe.

O formato de cada entrada e as regras de organização estão no topo do próprio `docs/LEARNINGS.md`.

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

## A CI (`.github/workflows/ci.yml`)

Existe desde que o projeto passou a ter dois sistemas, e é essa a razão dela: o porte para Windows foi
feito numa máquina Windows, e o lado Linux ficou meses sem ver um compilador. A cada push, em qualquer
ramo, ela confere os quatro lados do projeto, **encadeados nesta ordem**:

1. **Rust** — `cargo fmt --check`, testes, clippy e build de release nos dois sistemas
   (`ubuntu-latest` e `windows-latest`) com a feature `cpu`, a única que compila num agente sem GPU e
   sem o Vulkan SDK; mais um trabalho só para compilar o Vulkan no Linux (o backend que o `.deb` leva) e
   o `cargo audit`. Só no Linux, porque conferem arquivo e não código: `xmllint` no `dbus/contrato.xml`
   e `.github/scripts/versao.sh conferir`.
2. **Windows** — build e testes do frontend WinUI, e a compilação do script do Inno Setup.
3. **GNOME** — `npm run lint`, `--dry-run` dos schemas do GSettings e o `gnome-extensions pack`.
4. **KDE** — num contêiner `ubuntu:26.04` (o `ubuntu-latest` é 24.04 e traz Qt 6.4, abaixo do
   `QT_MINIMO 6.6` do `CMakeLists.txt`): `./kde-plasma/testar.sh --ci`, que compila o plugin C++, roda o
   `qmllint` e valida o `metadata.json`.

`RUSTFLAGS: -D warnings` estende ao rustc a régua que o `[lints.clippy]` do `Cargo.toml` já aplicava ao
clippy. O encadeamento é de propósito: em paralelo, um erro de digitação no Rust reprova os quatro ao
mesmo tempo e a página de resultados vira uma parede vermelha.

O que ela **não** cobre é tudo o que precisa de GPU, microfone, sessão gráfica ou barramento de sessão.
A medição de backends (`mede_o_backend`, em `src/stt.rs`) continua `#[ignore]` e local; o ciclo de vida
da extensão do GNOME e o portão inteiro do Plasma continuam sendo `./gnome-extension/scripts/testar.sh` e
`./kde-plasma/testar.sh` (sem `--ci`) na máquina de quem mexe; e do frontend WinUI só a leitura do
protocolo do canal de controle é testada — janela, ícone, menu e posição seguem nos roteiros manuais do
README. Verde na CI não substitui nenhum desses.

O `release.yml` **chama** este mesmo arquivo (`workflow_call`) antes de publicar qualquer coisa: não há
duas listas de conferências. Tudo sobre publicação está em `docs/CI-E-RELEASES.md`.

## Onde mora o que é de cada sistema (`src/plataforma/`)

Tudo o que muda de sistema operacional está em `src/plataforma/{linux,windows}/`, e nada fora dessa
pasta sabe em que sistema está rodando: a máquina de estados do ditado, o Whisper, a interface e a
configuração são domínio puro. Cada plataforma oferece **nove módulos, com estes nomes**. Não é um
`trait` porque nada aqui é escolhido em tempo de execução — a plataforma é decidida na compilação, e um
`trait` só acrescentaria despacho dinâmico e objetos vazios para representar uma escolha já feita. O
compilador cobra o contrato do mesmo jeito: falta um módulo, falta um símbolo, e o `cargo build` daquela
plataforma reclama por nome.

| Módulo | Linux | Windows |
|---|---|---|
| `teclado` | evdev (`/dev/input/event*`) | Raw Input (`WM_INPUT`) |
| `teclas` | tabela do evdev | tabela `VK_*` → código canônico |
| `ipc` | socket Unix em `$XDG_RUNTIME_DIR` | named pipe com DACL do usuário |
| `autostart` | serviço do systemd ou `.desktop` do XDG | `HKCU\…\Run` |
| `tray` | StatusNotifierItem (ksni) | quem mostra o ícone é o frontend |
| `integracoes` | nomes no barramento D-Bus | presença do frontend no pipe |
| `clipboard` | `wl-copy` / `ydotool` | `arboard` / `SendInput` |
| `registro` | o journal do systemd | arquivo em `LocalAppData` |
| `microfone` | nada a explicar | a recusa por privacidade do Windows |

Vários arquivos do topo de `src/` continuam existindo como **fachada** sobre a plataforma — `src/tray.rs`
e `src/ipc.rs` são de poucas linhas, `src/clipboard.rs` acrescenta o `arboard` como reserva. É neles que
a pergunta é escrita uma vez só ("mantenha um ícone em dia com o estado do programa"), para que as duas
respostas, radicalmente diferentes, não vazem para o `main.rs`.

Os nomes dos módulos vieram dos arquivos que já existiam no topo de `src/` e ficaram em inglês de
propósito: mover um arquivo era necessário para o Windows compilar, renomeá-lo não era, e um diff que
faz as duas coisas ao mesmo tempo é bem mais difícil de revisar do que dois. Arquivo novo daqui para
frente nasce em português, como manda a seção "Idioma".

O código de tecla que circula pelo programa inteiro é **o do evdev, inclusive no Windows**: é o que está
gravado no `hotkey` das configurações de quem já usa o Ditador, é o que a extensão do GNOME e o widget
do Plasma leem, e uma terceira numeração "neutra" obrigaria a traduzir dos dois lados e ainda deixaria
os arquivos antigos para trás. No Windows a tradução acontece na borda, em `teclas::do_windows`. O
raciocínio inteiro — incluindo por que nenhuma linha aqui tenta fazer o Windows parecer Linux — está no
bloco `//!` de `src/plataforma/mod.rs`.

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

## Integração com o Windows 11 (`windows-integration/`)

São **duas metades**, como no Plasma, mas a divisão é outra: o `ditador.exe` em
Rust faz tudo o que é trabalho (atalho por Raw Input, áudio, Whisper, área de
transferência) e o `Ditador.Windows.exe` (C#, WinUI 3, .NET 10, Windows App SDK
2.4) faz tudo o que é interface (ícone na área de notificação, aviso na tela,
painel de status, notificações). Eles conversam pelo **mesmo canal de controle**
do `ditador --status`, com um comando a mais: `assinar`.

```
.\windows-integration\scripts\instalar.ps1        # compila, instala, sobe. Sem admin.
.\windows-integration\scripts\desinstalar.ps1     # e -ApagarDados
.\windows-integration\scripts\build.ps1           # os dois lados
.\windows-integration\scripts\empacotar-exe.ps1   # o instalador .exe que vai na release
.\windows-integration\scripts\empacotar-msix.ps1  # o pacote, para o futuro
python windows-integration\scripts\gerar-icones.py  # depois de mexer no desenho dos .ico
```

**A fonte da verdade das APIs é a documentação da Microsoft**, e o que está em
`NativeMethods.txt` é gerado dela pelo CsWin32 — nenhum P/Invoke é escrito à mão.
Nada de API não documentada: o menu do ícone sai em tema claro mesmo no Windows
escuro porque escurecê-lo exigiria os ordinais 133/135 do `uxtheme`, e o preço de
usá-los é quebrar numa atualização sem aviso.

O `windows-integration/README.md` tem a arquitetura, o protocolo do `assinar`, a
ACL do pipe conferida na máquina, o que foi testado e o que não foi.

## O contrato D-Bus é um só

`dbus/contrato.xml` é a cópia canônica da interface. Não é ele que a cria — quem publica é o
`src/plataforma/linux/dbus.rs`, e o zbus a monta do código Rust —, mas é dele que os clientes saem: o proxy
Qt é **gerado** dele em tempo de compilação (`qt_add_dbus_interface`), e o XML embutido em
`gnome-extension/src/backend.js` é comparado com ele. O teste
`o_contrato_canonico_bate_com_os_tres_lados` (em `src/plataforma/linux/dbus.rs`) lê o XML canônico, pede ao
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

**Empurrar para o `main` publica uma versão.** É automático: o processo inteiro —
validar, numerar, commitar, taguear, empacotar e publicar — está no `.github/workflows/release.yml`, e
o passo a passo, com o que fazer quando der errado, está em **`docs/CI-E-RELEASES.md`**. Não faça à
mão o que ele faz; e se precisar mudar como uma versão é publicada, mude lá, não aqui.

**Um push que não deve virar versão leva o trailer `Publicar: nao`** no commit da ponta — documentação,
imagem do README, ajuste de comentário. O código é validado do mesmo jeito; o que não acontece é a
versão. O botão (*Actions → Publicar versão*) continua existindo para republicar ou forçar um
incremento, e ele ignora esse trailer.

Os três fatos que continuam valendo, e que o workflow respeita:

- `Cargo.toml` é a única fonte da verdade da versão (`empacotar.sh` faz grep nela; o binário usa
  `env!("CARGO_PKG_VERSION")`). As cópias que precisam concordar com ele — `Cargo.lock`, que é
  versionado, e o `version-name` do `gnome-extension/metadata.json` — são mantidas pelo
  `.github/scripts/versao.sh`, e a CI reprova quando elas se separam
  (`.github/scripts/versao.sh conferir`).
- O README não tem nenhuma versão escrita à mão: os nomes dos `.deb` são glob e o link aponta para
  `releases/latest`. Não reintroduza o número lá — era um passo que já foi esquecido.
- O modelo não vai como asset: são 574 MB que não mudam entre versões, o app baixa sozinho e o
  `--baixar-modelo` resolve por terminal. A release 0.2.0 leva uma cópia dele por motivos históricos.

O número sai do trailer `Impacto:` dos commits desde a última tag (veja "Commits", abaixo), com a
ressalva da linha 0.x: enquanto o MAJOR for 0, `incompatível` sobe o MINOR, porque chegar ao 1.0.0 é
uma decisão e não efeito colateral. Quem quiser mandar no número dispara o workflow com
`incremento: patch|minor|major`.

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

⚠️ O `git tag` **era** o passo mais fácil de esquecer, e já foi pulado em quatro versões seguidas: o
repositório chegou à 0.4.2 tendo `v0.2.0` como única tag, e o único release publicado ainda mostrava a
interface de vidro que o README já dizia ter removido. É por isso que quem cria a tag hoje é o workflow,
no mesmo passo em que grava a versão — não há mais como publicar sem ela. As versões puladas continuam
sem tag de propósito: uma tag inventada meses depois aponta para um commit que nunca foi empacotado nem
publicado, e mentir sobre isso é pior do que a lacuna.

## Commits

Assunto em português, sentence case, sem prefixo e sem conventional commits — descreve o efeito, não o arquivo
(`Mais ar embaixo das fileiras de botões`). Corpo longo em prosa, explicando causa e raciocínio. Todo commit
termina com o trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

Acima dele, quando a mudança for mais do que um conserto, vai o trailer **`Impacto:`** — é dele que sai o
número da próxima versão, e o assunto do commit é o que aparece no changelog:

| Trailer | Sobe | Quando |
|---|---|---|
| `Impacto: correção` | PATCH | conserto, ajuste, texto, documentação |
| `Impacto: funcionalidade` | MINOR | coisa nova que não quebra quem já usa |
| `Impacto: incompatível` | MAJOR | quebra quem já usa |

**Sem o trailer, o commit vale PATCH** — é o padrão certo aqui, porque a maioria dos commits deste
projeto é conserto e esquecer o trailer não pode publicar uma versão que promete mais do que mudou. Ele
é trailer, e não prefixo no assunto, justamente para não desfazer a regra do parágrafo acima: a
categoria da mudança não pertence à frase que descreve o efeito dela.

⚠️ **Os trailers ficam todos no mesmo bloco final, sem linha em branco entre eles.** O git só reconhece
um trailer no último parágrafo da mensagem; uma linha em branco entre o `Impacto:` e o
`Co-Authored-By:` faz o primeiro deixar de ser trailer e virar texto comum. Assim:

```
Impacto: funcionalidade
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

E **não** assim — que é o que já aconteceu em três commits deste repositório, um deles publicando dez
recursos novos como se fossem uma correção (a 0.6.1):

```
Impacto: funcionalidade
                                  ← esta linha em branco quebra tudo
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

Confira antes de empurrar — agora que todo push no `main` publica, o erro deixou de ser um episódio:

```
git log -1 --format='%(trailers:key=Impacto,valueonly)'   # vazio = está errado
```

O `.github/scripts/versao.sh` tem uma rede para isso desde a 0.7.0: não achando o trailer, ele procura
uma linha `Impacto:` solta, usa o valor e **avisa** na saída de erro. A rede acerta o número; a
mensagem continua errada, e é para consertar.

## Armadilhas — não "consertar"

Cada linha aqui é uma regra vigente: código que parece errado, não está, e o motivo vem junto. A
investigação que produziu o conhecimento — sintoma, diagnóstico, o que se tentou antes — mora em
`docs/LEARNINGS.md`; procure lá antes de investigar qualquer coisa que esta lista não responda.

- **`sair_sem_desmontar` (em `src/main.rs`) pula os destrutores de propósito**: desmontar os buffers do
  ggml/Vulkan dá SIGSEGV no driver NVIDIA, e o systemd trataria isso como falha e reiniciaria o app.
  Todo caminho de saída passa por ele, inclusive o de erro da interface — devolver o erro com `?` faria o
  processo terminar pelo runtime do Rust, que é exatamente o que se quer evitar. No Linux é o `_exit` da
  libc; no Windows, o `ExitProcess`, porque com o MSVC o `_exit` é detalhe interno do runtime C e não um
  símbolo estável para se ligar.
- **`clipboard::remember_environment()` é a primeira linha de `main()`.** O modo X11
  (`force_x11: true` por padrão, e lido só no Linux) remove `WAYLAND_DISPLAY` do ambiente logo antes de a
  janela subir; sem o retrato tirado antes disso o `wl-copy` para de funcionar. Quem tira o retrato hoje é
  `plataforma::clipboard::lembrar_o_ambiente`, mas a ordem no `main()` é a mesma. Não reordene.
- **Renderer glow com `multisampling: 0`** (na `NativeOptions`, em `src/main.rs`): o wgpu recusou
  transparência e nenhuma config do glutin combina alpha com MSAA. Mudar qualquer um dos dois quebra os
  cantos arredondados ou a criação da janela.
- **Quem diz se o microfone está aberto é `recording_since`, nunca `state.view`** — use `Shared::gravando()`
  (`src/state.rs`), que existe para não haver duas maneiras de perguntar. Falar de novo enquanto a frase
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
- **Bandeja (`src/plataforma/linux/tray.rs`; o `src/tray.rs` é só a fachada)**: o app sobe antes das
  extensões do GNOME Shell, então um StatusNotifierWatcher ausente significa "esperar", não "erro".
- **`Config` usa `#[serde(default)]`** e há testes garantindo que configs antigas continuam carregando. Ao mexer
  em `src/config.rs`, preserve isso. E só a **ausência** do arquivo autoriza gravar os padrões por cima: qualquer
  outro erro de leitura segue com os padrões em memória, sem tocar no que está no disco.
- No Linux o atalho global lê `/dev/input/event*` via evdev: sem o usuário no grupo `input` ele
  silenciosamente não funciona. `instalar.sh` e o postinst do `.deb` apenas avisam, e `ditador --diagnostico` diz na cara.
  O aviso disso mora em `Shared::aviso_atalho`, com campo próprio — dividindo o `message` com o aviso do
  modelo faltando, um dos dois sumia antes de ser lido.
- **`pressed`, em `src/hotkey.rs`, conta origens por dispositivo.** Guardando só o código da tecla, o teclado
  virtual que o `ydotool` cria para a colagem automática soltava a tecla que a pessoa ainda segurava.
- **`EstadoPublico` (`src/state.rs`) é a única tabela de "em que pé está o programa".** O ícone da bandeja
  (`icones::Estado::do_publico`) e o D-Bus saem dela. Já foram duas tabelas, e duas é uma a mais do que se
  consegue manter iguais. Os textos de `EstadoPublico::nome` (`pronto`, `gravando`, …) são **protocolo**: a
  extensão do GNOME os compara, e há um teste em `src/plataforma/linux/dbus.rs`
  (`os_estados_publicados_tem_nomes_estaveis`) para que mudá-los não passe batido.
- **Quem recolhe o ícone da bandeja é a presença de um nome no barramento, não um aviso da extensão.**
  O `src/plataforma/linux/dbus.rs` vigia `io.github.danielfreitasdev.Ditador.GnomeExtension` e escreve,
  pelo `anotar`, em `Shared::integracoes.gnome` (o widget do Plasma tem o nome dele e o campo `plasma`);
  `tray.rs` **desregistra** o item (não usa `Status::Passive`, que o hospedeiro pode ignorar) e `ui.rs`
  esconde as telas de gravação por `Shared::tela_visivel`. É assim porque o barramento solta o nome sozinho
  quando a conexão cai — Shell reiniciado, extensão morta no meio do `disable()`, tanto faz: o ícone volta.
  Um protocolo de "avise quando sair" perderia todos esses casos.
- **O nome no barramento é pedido com `allow_name_replacements(false)` e `replace_existing_names(false)`**
  (`pedir_o_nome`, em `src/plataforma/linux/dbus.rs`). Não são linhas supérfluas: o padrão do zbus é
  `AllowReplacement | ReplaceExisting | DoNotQueue`, e com ele um segundo Ditador **roubava** o nome do que
  já estava rodando — os dois escrevendo no journal a mesma linha de sucesso — e, ao sair, deixava o nome
  sem dono nenhum, com o legítimo de pé e invisível para a extensão. E, como o nome *da extensão* continuava
  no barramento, o ícone da bandeja também não voltava. Há um teste que sobe o próprio `dbus-daemon` para
  provar isso; a investigação inteira está em `docs/LEARNINGS.md`.
- **A vigília de uma integração que perde o fluxo anota a ausência** (`desistir_de_vigiar`, no mesmo
  arquivo). O fluxo só acaba com a conexão morta, e aí não há como saber quem está no ar: assumir que a
  integração saiu devolve o ícone e a tela de gravação. Errar para esse lado custa dois ícones; errar para o
  outro custa nenhum, que foi o que se observou.
- **`tela_visivel` esconde só `Recording` e `Processing`.** Resultado, configurações e erro continuam do
  aplicativo mesmo com a extensão ligada: são telas com texto para copiar e com os botões que resolvem o
  problema, e um OSD não tem onde pôr isso.
- **`GravandoDesde` não é recalculado enquanto a gravação é a mesma** (`Retrato::tirar`, em
  `src/retrato.rs`). O `Instant` é monotônico e vira hora de parede por subtração, então recalculá-lo daria
  um número um pouco diferente a cada vez — e o cronômetro do OSD, que é desenhado a partir dele, voltaria
  para zero no meio da frase.
- **O sinal `Nivel` só é emitido durante a gravação**, a 15 Hz (`INTERVALO_DO_NIVEL`, em
  `src/plataforma/linux/dbus.rs`, e o gêmeo dele em `src/assinatura.rs` para o fluxo do Windows).
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
- **O registro do Raw Input é do processo, e a interface do egui rouba o nosso.**
  Quem chamar `RegisterRawInputDevices` por último para a mesma página/uso HID
  leva as mensagens, e o registro anterior para de valer **sem erro nenhum**.
  Neste processo há dois candidatos: a escuta de teclado
  (`plataforma/windows/teclado.rs`) e o winit, que registra entrada bruta quando
  cria a janela do eframe — depois da escuta, portanto por cima dela. O sintoma é
  o pior possível: o log diz "observando o teclado por Raw Input", o
  `RegisterRawInputDevices` devolveu sucesso, e **nenhum `WM_INPUT` chega
  jamais**; o atalho global simplesmente não faz nada, enquanto o ícone e o menu
  funcionam. A defesa é a `vigiar_o_registro`, que confere com
  `GetRegisteredRawInputDevices` para qual janela o teclado está registrado e o
  retoma quando não for a nossa. Não a remova achando que é redundante: sem ela o
  atalho para de funcionar no arranque, toda vez.
- **`ConnectNamedPipe` que falha precisa de um `DisconnectNamedPipe` antes da
  próxima volta** (`src/plataforma/windows/ipc.rs`). Um cliente que abre o pipe e
  vai embora sem dizer nada — o `Get-Acl` do PowerShell faz exatamente isso —
  deixa a instância devolvendo `ERROR_NO_DATA` para sempre. Com um `continue`
  seco, o laço girava nesse erro consumindo um núcleo inteiro e **nenhum
  `ditador --status` era atendido nunca mais**, num Ditador que continuava
  gravando e transcrevendo normalmente. Há teste de regressão
  (`um_cliente_que_some_nao_derruba_o_canal`).
- **A janela que recebe os cliques do ícone é de nível superior, e não
  `HWND_MESSAGE`.** Janelas *message-only* não recebem mensagens de difusão, e
  `TaskbarCreated` — o aviso de que o Explorer voltou — é uma difusão. Com ela, o
  ícone sumiria no primeiro reinício do Explorer e não voltaria nunca.
- **O `Retrato` mora em `src/retrato.rs`, e não no `plataforma/linux/dbus.rs`.** Ele nasceu lá,
  quando o D-Bus era o único jeito de o mundo de fora enxergar o Ditador; hoje o
  named pipe publica o mesmo estado para o `Ditador.Windows`, e duas cópias da
  mesma tabela é o que este arquivo proíbe em tantas palavras.
- **`ipc::Fluxo` carrega um `Sender` que nunca envia nada.** É o que avisa a
  thread da assinatura de que o cliente foi embora: sem ele, ela dormia à espera
  da próxima mudança de estado — que num Ditador parado nunca vem — e
  `Integracoes::frontend` ficava ligado num programa sem frontend nenhum, com a
  janela do egui escondida e o usuário sem ícone e sem aviso.
- **Os ícones da área de notificação vêm em dois conjuntos, claro e escuro.** O
  Windows não recolore ícone de bandeja: um glifo branco some na barra clara. A
  troca é feita no `WM_SETTINGCHANGE` com `ImmersiveColorSet` — e mudar só a
  chave do registro não a dispara, é preciso a difusão (foi assim que o teste
  passou a valer).
- **A colagem automática do Windows (`SendInput`) solta o que atrapalha antes do
  Ctrl+V, e devolve depois.** Shift/Alt/Win segurados na hora da colagem virariam
  Ctrl+Shift+V (que é "colar sem formatação" em metade dos programas) — e quem
  grava por alternar com um atalho de modificador está com a mão nele justamente
  nesse instante. Um Ctrl já segurado é **aproveitado**, não reapertado: apertar
  por cima e soltar no fim deixa o sistema achando que a pessoa soltou uma tecla
  que ela ainda segura. E as teclas da direita e as Win precisam de
  `KEYEVENTF_EXTENDEDKEY`, senão soltar o Alt direito solta o esquerdo e o direito
  fica preso. Tudo isso é `montar_sequencia`, com testes; não a simplifique para
  "Ctrl↓ V↓ V↑ Ctrl↑".
- **`plataforma::integracoes::start` vem antes de `tray::start` em `main.rs`.** (No Linux essa é a fachada
  do `dbus::start` de sempre; só o nome mudou com a mudança de casa dos módulos.) É o D-Bus que descobre se
  alguma integração já está no ar; descobrindo primeiro, a bandeja nasce sabendo e o ícone não pisca na
  barra no login.
  No Plasma isto é ainda mais apertado do que no GNOME, e é o que faz o widget não piscar: o
  `plasmashell` só carrega o widget **depois** de o Ditador aparecer no barramento
  (`X-Plasma-DBusActivationService`), então a corrida é entre ele carregar o pacote e o `integracoes::start`
  terminar de montar as assinaturas. Medido num arranque real, o widget assume 40 ms depois do
  `systemd` iniciar o Ditador, e não há linha de "serviço do ícone da barra superior no ar" no journal —
  o StatusNotifierItem não é registrado e recolhido, é nunca registrado. Invertendo a ordem, ele passa a
  aparecer e sumir a cada login.
- **O comando `assinar` do canal de controle é só do Windows**, e o `cfg!(target_os = "windows")` no braço
  do `match` em `src/main.rs` é o que garante isso. Ele liga `Integracoes::frontend`, que recolhe o ícone
  da bandeja e passa o aviso de gravação para quem assinou — e no Linux quem faz esse papel é um nome no
  barramento D-Bus, não uma palavra pelo socket; aberto dos dois lados, bastaria mandar `assinar` à mão
  pelo `ditador.sock` para deixar o Ditador sem ícone e sem aviso numa área de trabalho que não tem nada
  para substituí-los.
- **`RIDEV_DEVNOTIFY` no registro do Raw Input não é opcional** (`plataforma/windows/teclado.rs`): sem ele
  o processo nunca recebe `WM_INPUT_DEVICE_CHANGE` e nunca fica sabendo que um teclado sumiu. Arrancar o
  teclado USB com a tecla do atalho segurada deixaria a origem dela em `pressed` para sempre — e o
  microfone aberto para sempre junto.
- **O cão de guarda do named pipe guarda o valor do `HANDLE` sem a posse dele**, de propósito
  (`atender`, em `src/plataforma/windows/ipc.rs`): sendo dono, a instância só voltaria para o rodízio
  depois dos dois segundos de paciência e quatro `--status` seguidos esgotariam as quatro. O preço é que
  existe **um** caminho em que o handle fecha sem ninguém marcar `respondido` — o `spawn` da thread que
  atende o cliente falhar, devolvendo a closure dentro do `Err` para ser destruída —, e por isso esse
  `Err` é tratado em vez de virar `let _ =`: sem a marcação, dois segundos depois o cão de guarda
  desconectaria um valor que o Windows já pode ter reentregue a outro objeto deste processo.
- **`IconeDaBandeja.CarregarIcone` (C#) precisa do `SetHandleAsInvalid()` logo depois do
  `DangerousGetHandle()`.** Sem ele o `SafeHandle` continua dono do ícone, e o finalizador o destrói horas
  depois — com o ícone ainda na bandeja, sendo desenhado pelo Shell —, fazendo o `DestroyIcon` do
  `Redesenhar` ou do `Dispose` cair sobre um handle já liberado. Comentário dizendo "quem destrói é o
  `Redesenhar`" não basta: é essa linha que transfere a posse de verdade.
- **No `osd.js` da extensão, "visível" não quer dizer "não está saindo".** Quem chama `hide()` é o
  `esconder()`, e só depois do `await` do esmaecimento — então durante a saída o ator ainda é
  `this.visible`. É para isso que existe o `_saindo`: voltando cedo por `this.visible`, um
  `transcrevendo → pronto` seguido de um Pause dentro dos 100 ms deixava o ditado inteiro sem aviso, sem
  cronômetro e sem medidor, com o temporizador do relógio girando num ator invisível.
- **O `./gnome-extension/scripts/testar.sh` trava de vez em quando, e não é a extensão.** O Shell aninhado
  depende do `gnome-shell-perf-helper` pegar `org.gnome.Shell.PerfHelper` no barramento privado para que o
  `runPerfScript` chame o roteiro; quando ele não aparece, nada acontece — nenhuma linha, nenhum erro, e o
  comando fica pendurado. É intermitente e é ambiente. Uma das causas está tratada: o ajudante **não**
  morre com a sessão aninhada derrubada por tempo, e um que sobra atrapalha a volta seguinte — o script
  mata o que sobrou antes de cada tentativa e ao sair. Não resolve sozinho, então continuam o teto de
  tempo (`DITADOR_LIMITE_DO_TESTE`, 120 s) e as três tentativas, e só o travamento merece nova tentativa —
  teste que falhou, falhou. E não vá procurar o defeito no JS da extensão por causa dele: o
  `npm run lint`, o `gjs -m scripts/teste-do-backend.js` e o `--dry-run` dos schemas continuam valendo e
  cobrem o resto.
- **Toda conta de altura da interface precisa de piso** — use `ui::altura_util`. A janela é
  redimensionada por comando (`ViewportCommand::InnerSize`), e o comando é atendido no quadro
  **seguinte**: existe sempre um desenho feito com o tamanho da tela anterior. Vindo da gravação (178)
  para o resultado (372), a sobra daquele quadro dá 92 e a conta dava −6 — e o `set_min_height` do egui
  entra em pânico com altura negativa, derrubando o programa. Já derrubava na 0.6.0; só ninguém rodava
  o passeio numa build de depuração. Investigação em `docs/LEARNINGS.md`.
- **A correção de termos do `src/dicionario.rs` não pode voltar a ser gulosa.** Ela mede *todas* as
  janelas de *todas* as posições antes de decidir, e aceita por semelhança decrescente. Experimentando
  a janela maior primeiro — que é o que parece certo — "usei o kubernetes" virava "usei Kubernetes": a
  chave `okubernetes` está a uma edição de `kubernetes`, porque acrescentar uma letra é uma edição
  mesmo quando a letra é uma palavra inteira.
- **`portatil::init()` é a primeira linha do `main`, antes do logger.** No Windows o destino do arquivo
  de log sai de `data_dir()`, que depende dessa decisão. É por isso que ela não escreve no log — quem
  conta o que ela descobriu é o `portatil::relatar()`, logo depois de o logger subir. Não a mova para
  depois, e não ponha `log::` dentro dela.
- **`memoria::pinar_o_alocador()` também é do arranque, e não é supérflua.** Sem ela a glibc retém ~29 MB
  de RSS para sempre depois dos primeiros ditados (medido; a curva estaciona, não cresce sem limite). O
  teste `os_buffers_de_um_ditado_voltam_para_o_sistema` falha se alguém a remover.
- **Em `audio::Captura::comecar`, o anel de pré-gravação é despejado *antes* de a bandeira `gravando`
  subir.** Na ordem contrária, as amostras que chegarem durante o despejo entram no buffer à frente das
  que já estavam no anel — e o ditado começa com um pedacinho do futuro antes do passado.
- **O teto de duração é `AtomicUsize` e tem o `ajustar_o_teto`, porque ele muda sem o microfone ser
  reaberto.** `pede_reabertura` não inclui o `max_secs` de propósito (reabrir por causa de um deslizante
  fecharia o microfone de quem só arrastou um controle), e no modo sempre aberto — o padrão — o `abrir()`
  acontece uma vez por execução: lido só ali, o "Gravação máxima" das configurações não valia até
  reiniciar o programa. Pelo mesmo motivo o `comecar()` **reserva o buffer de novo a cada ditado**: o
  `terminar()` leva a alocação embora junto com as amostras, e sem a reserva o buffer voltava a crescer
  dentro do callback de áudio, que é o que o `with_capacity` do construtor existia para impedir.
- **O medidor de nível só é alimentado durante a gravação**, mesmo com o microfone aberto o tempo todo
  (`build`, em `src/audio.rs`). É a mesma regra do sinal `Nivel` do D-Bus, e agora ela precisa ser dita
  no callback: no modo sempre aberto o callback roda sem parar, e alimentar o medidor fora do ditado
  faria a thread do D-Bus emitir quinze vezes por segundo com o microfone parado.
- **`AudioCmd::Cancel` é um comando próprio, e não um `Stop` cujo resultado se joga fora.** O áudio
  descartado não atravessa o canal — são megabytes que não são copiados — e o controlador não precisa
  lembrar, quando um `Captured` chegasse, que aquele ditado foi cancelado. Um estado a menos para
  manter em dia.
- **O atalho de cancelar é conferido antes do de ditar** (`conferir_cancelar`, em `src/hotkey.rs`), e
  dispara só na transição de incompleto para completo. Cancelar é instantâneo: não tem par de soltar, e
  um evento por tecla apertada enquanto a combinação estivesse embaixo mandaria uma enxurrada.
- **O histórico é gravado antes de o texto ser entregue** (`on_transcription`). Se a colagem cair na
  janela errada ou a área de transferência recusar, o texto já está a salvo — que é a razão de o módulo
  existir. Inverter a ordem desfaz o recurso sem quebrar nenhum teste.
- **O SHA-256 de um modelo da Hugging Face é o `x-linked-etag`, não o `etag`.** O primeiro vem no
  redirecionamento e é o `lfs.oid`; o segundo vem na resposta final do CDN e é o hash do Xet. Conferir
  contra o `etag` reprova **todo** download bom. Detalhes em `docs/LEARNINGS.md`.
- **Nenhum argumento de uma chamada que trave o estado pode vir de `lock(&shared)`.** O `MutexGuard`
  temporário vive até o fim da expressão, então `on_stt(Evento { ditado: b.estado().ditado_atual, .. })`
  trava o teste para sempre — sem falhar e sem mensagem. Tire o valor antes da chamada.

## Variáveis de diagnóstico

Combináveis, lidas em `src/main.rs` e `src/ui.rs`:

| Variável | Efeito |
|---|---|
| `DITADOR_CAPTURA=<dir>` | grava um PNG de cada tela quando ela estabiliza |
| `DITADOR_DEMO=1` | percorre as quatro telas com conteúdo de exemplo e sai |
| `DITADOR_TEMA=claro\|escuro` | ignora o tema configurado |
| `DITADOR_ZOOM=1.5` | fator de zoom, limitado entre 0.5 e 3.0 |
| `DITADOR_QUADROS=1` | desliga o vsync e loga FPS a cada 2 s |
| `RUST_LOG=ditador=debug` | inclui o texto transcrito no log |

`RUST_LOG=debug` seco também funciona, mas traz junto o aperto de mão do zbus e o C do
ggml — que o filtro padrão (`FILTRO_PADRAO`, em `src/main.rs`) mantém em `warn` justamente
porque ocupavam três quartos do journal.

`ditador --diagnostico` confere de uma vez a leitura do teclado (no Linux, o grupo `input`), o modelo,
o microfone, a área de transferência e a colagem, o `curl`, a integração de área de trabalho no ar e a
instância em execução — e, onde o log é arquivo nosso, onde ele está. A linha do teclado é montada pela
plataforma (`plataforma::teclado::diagnostico`), porque a pergunta é a mesma nos dois sistemas mas o
motivo de falhar e o conselho não têm nada em comum. É a primeira coisa a rodar quando alguém disser que
"não acontece nada" — ou que "o ícone do Ditador sumiu da barra", que é a pergunta que a linha da
integração responde.

## Instância única

Uma segunda execução não abre outro processo: `src/ipc.rs` tenta tomar o canal de controle e, se já
estiver ocupado, manda um comando para a instância viva. O transporte muda de sistema para sistema e o
protocolo não — socket Unix em `$XDG_RUNTIME_DIR/ditador.sock` no Linux, named pipe
`\\.\pipe\Ditador-<SID>` (DACL só do usuário) no Windows —, e os dois carregam uma linha de comando e uma
linha de resposta, terminadas por `\n`. Subcomandos da CLI têm nome em português com alias em inglês
(`--alternar|--toggle`, `--encerrar|--quit`, `--cancelar|--cancel`, `--historico|--history`). Vale aqui a
mesma regra do contrato D-Bus: **acrescentar, nunca renomear** — um comando renomeado quebra o atalho que
alguém configurou no painel do sistema para chamar `ditador --alternar`.

Os comandos do canal hoje: `toggle`, `iniciar`, `parar`, `cancelar`, `settings`, `historico`,
`integracoes`, `status`, `quit` e — só no Windows — `assinar`. O `cancelar` e o `historico` **não** têm
método equivalente no D-Bus: a extensão do GNOME e o widget do Plasma não os conhecem, e acrescentá-los
lá exigiria mexer nos três lados do contrato de uma vez. Quando alguém precisar deles numa integração de
área de trabalho, é o `dbus/contrato.xml` que ganha o método — acrescentando, nunca renomeando.
