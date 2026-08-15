# Ditador no KDE Plasma 6

Integração **opcional**. O Ditador funciona inteiro sem ela — no Plasma, no
GNOME, no Hyprland, no COSMIC — pelo ícone da bandeja e pela janela do próprio
programa. O que esta pasta acrescenta é um widget nativo do Plasma no lugar
daquele ícone.

Ela não substitui, não reescreve e não move nada do Ditador. Áudio, Whisper,
Vulkan, área de transferência, evdev, configuração e o socket Unix continuam
todos em Rust, exatamente onde estavam.

```
                            Ditador (Rust)
                     ┌─────────────────────────┐
                     │ áudio · Whisper · GPU   │
                     │ evdev · clipboard       │
                     │ controller · config     │
                     │ socket Unix · D-Bus     │
                     └────────────┬────────────┘
                                  │
                        dbus/contrato.xml
                     (um contrato, vários clientes)
                                  │
                ┌─────────────────┴─────────────────┐
                ▼                                   ▼
      extensão do GNOME Shell              widget do Plasma
      GNOME Shell 50.x · GJS               Plasma 6.6 · Qt 6 · QML
```

## Onde isto foi feito e testado

| | |
|---|---|
| Sistema | Ubuntu/Kubuntu 26.04 LTS (Resolute Raccoon) |
| Plasma | 6.6.6 |
| KWin | 6.6.6 (Wayland) |
| Qt | 6.10.2 |
| KDE Frameworks | 6.24.0 |
| Sessão | Wayland |

Nada aqui tem código de compatibilidade com Plasma 5, KF5 ou Qt 5, e nem com
X11 como alvo. Confira a sua máquina antes de abrir um problema:

```bash
plasmashell --version
kwin_wayland --version
qmake6 -query QT_VERSION
kpackagetool6 --list-types | grep Applet
```

## As duas metades

```
kde-plasma/
├── CMakeLists.txt        ─┐
├── plugin/                │ C++ compilado: a ponte QML ↔ D-Bus
│   ├── ditadorbackend.*   │
│   └── presenca.*        ─┘
│
├── plasmoid/package/     ─┐ QML e JSON: o widget. Não compila.
│   ├── metadata.json      │
│   └── contents/ui/*.qml ─┘
│
├── instalar.sh · desinstalar.sh · testar.sh
└── README.md
```

Elas se instalam de jeitos diferentes de propósito:

| | onde vai | pede senha? |
|---|---|---|
| widget | `~/.local/share/plasma/plasmoids/`, pelo `kpackagetool6` | não |
| plugin | diretório de módulos QML do Qt (`qmake6 -query QT_INSTALL_QML`) | **sim**, uma vez |

O widget é do usuário porque pode ser: é texto, o Plasma o lê de `~`, e trocá-lo
custa um segundo. O plugin não pode: o motor QML só procura módulos em
`QT_INSTALL_QML`, que é do sistema. Não há caminho de módulo QML por usuário no
Qt 6, e a alternativa seria mexer no `QML_IMPORT_PATH` da sessão inteira — um
efeito colateral global para resolver um problema local. O `sudo` acontece uma
vez, na instalação, e nunca em execução.

Os arquivos instalados ficam todos sob `io/github/danielfreitasdev/ditador/`,
que é namespace nosso: a desinstalação apaga essa pasta e não toca em mais nada.

## Por que existe C++ aqui

Porque o widget precisa falar D-Bus, e no Plasma 6 o QML sozinho não fala.

O atalho conhecido é `org.kde.plasma.plasma5support`, que traz um `DataSource`
de D-Bus para dentro do QML. É, pelo nome, a camada de compatibilidade com o
Plasma 5, e carregá-la numa integração nova feita para o Plasma 6 seria começar
com dívida. O caminho atual é uma classe `QObject` pequena, exposta ao QML como
um módulo próprio.

**Consequência**, que é preciso dizer com todas as letras:

- este widget **não é um pacote QML puro**;
- ele não pode ser distribuído pela KDE Store como um `.zip` de widget, porque
  metade dele é um `.so`;
- quem o instala precisa das ferramentas de compilação (o `instalar.sh` diz
  quais, e recusa a instalação sem elas);
- a distribuição natural dele é junto do Ditador, por pacote da distribuição ou
  pelo repositório.

Foi uma troca consciente: arquitetura correta e robusta acima de um ZIP fácil de
publicar.

### O que o C++ **não** faz

Nada de captura de áudio, nada de Whisper, nada de GPU, nada de modelo, nada de
`/dev/input`, nada de shell, nada de download, nada de `sudo`. Ele converte
QML ↔ D-Bus e mais nada. Toda regra de negócio continua no Rust — inclusive a
de o que é "gravando", que este lado nem tenta reproduzir.

## O contrato D-Bus

`dbus/contrato.xml`, na raiz do repositório, é a cópia canônica. Não é ele que
cria a interface — quem a publica é o `src/dbus.rs`, e o zbus a monta do código
Rust —, mas é dele que os clientes saem.

O proxy C++ é **gerado** dele em tempo de compilação (`qt_add_dbus_interface`),
e nenhum nome de método é escrito à mão aqui. Um método renomeado no contrato
vira erro de compilação em vez de um clique que não faz nada.

Três lados falam esta língua, e um teste em `src/dbus.rs`
(`o_contrato_canonico_bate_com_os_tres_lados`) confere que continuam iguais —
comparando o XML canônico, a introspecção que o zbus produz do próprio código, e
o XML embutido no JavaScript da extensão do GNOME. Mexer num sem mexer nos
outros falha o `cargo test`.

A API não mudou por causa do Qt. Nenhum método, propriedade, sinal, caminho ou
nome de barramento foi renomeado; nada foi removido. A extensão do GNOME
instalada continua funcionando sem ser atualizada.

### Nada bloqueia

Este código roda dentro do `plasmashell`, o processo que desenha a área de
trabalho inteira. Uma chamada síncrona ao Ditador congelaria o painel pelo tempo
que o outro lado levasse — e o outro lado às vezes está carregando 574 MB de
modelo na GPU.

- **`QDBusInterface` não é usado.** Ele introspecta o serviço dentro do
  construtor, e isso bloqueia. O proxy vem do XML, com os tipos resolvidos em
  tempo de compilação.
- **As propriedades não são lidas pelo proxy gerado.** Os getters que o
  `qdbusxml2cpp` produz fazem um `Get` síncrono a cada leitura, e o QML lê
  propriedade a cada repintura. O que se usa é um `GetAll` assíncrono quando o
  serviço aparece, mais o `PropertiesChanged` que o Ditador emite a cada mudança.
- **Os métodos vão por `asyncCall`**, com `QDBusPendingCallWatcher` só para
  registrar a falha.
- **Nada pergunta de tempos em tempos.** Quem avisa que o Ditador subiu ou caiu é
  o `QDBusServiceWatcher`. Não há temporizador de sondagem em lugar nenhum.

O único temporizador do widget é o cronômetro da gravação: bate uma vez por
segundo, existe só enquanto se grava e para junto. O instante de início vem do
backend (`GravandoDesde`), e o lado de cá nunca conta o tempo por conta própria.

## Presença: um ícone, nunca dois

O widget segura `io.github.danielfreitasdev.Ditador.PlasmaIntegration` no
barramento enquanto está carregado. O Rust observa esse nome e recolhe o
StatusNotifierItem dele.

É um nome, e não um aviso, de propósito: quem detém um nome no barramento é uma
*conexão*, e o barramento a solta sozinho quando ela cai. Vale para
`plasmashell` que travou, widget removido do painel, plugin que não carregou,
sessão encerrada, integração desinstalada — em todos, o ícone da bandeja volta
sem que ninguém tenha se despedido. Um protocolo de "avise quando sair" perderia
justamente os casos em que não há quem avise.

Duas instâncias do widget não são problema: a contagem em `presenca.cpp` faz o
nome ser adquirido pelo primeiro e largado pelo último. Entre processos (o
`plasmawindowed` aberto durante o desenvolvimento, por exemplo), quem não
conseguiu o nome continua funcionando inteiro e fica vigiando — se ele vagar,
assume.

Vale a pena dizer o que **não** foi feito: nada aqui olha `KDE_FULL_SESSION` nem
`XDG_CURRENT_DESKTOP` para decidir esconder o ícone. Estar no KDE não quer dizer
ter o widget instalado, e a detecção precisa provar que a integração está
carregada — não que ela poderia estar.

### O ícone pisca no arranque?

Um pouco, e é inerente. O widget declara
`X-Plasma-DBusActivationService`: o `plasmashell` só o carrega quando o serviço
do Ditador aparece no barramento. Então a ordem é sempre

```
Ditador sobe → nome no barramento → plasmashell carrega o widget →
widget segura a presença → Ditador recolhe o ícone da bandeja
```

e existe uma fresta, de alguns milissegundos, em que os dois estão na barra. A
alternativa seria o widget segurar o nome mesmo com o Ditador fora do ar, o que
o faria mentir sobre o que está disponível. Preferimos a fresta.

## OSD nativo: por que não existe

O prompt desta integração pedia o equivalente ao OSD que a extensão do GNOME
desenha, e pedia para não forçar. A resposta técnica é **não dá, com API
pública**, e vale registrar por quê para ninguém reinvestigar.

**`SceneEffect` do KWin não serve.** É o único caminho declarativo (pacote
`KWin/Effect`, `contents/ui/main.qml`, `import org.kde.kwin`), e é o
`ScriptedQuickSceneEffect`, que herda de `QuickSceneEffect`. O `paintScreen`
dele, no Plasma 6.6, é:

```cpp
void QuickSceneEffect::paintScreen(...)
{
    const auto it = d->views.find(screen);
    if (it != d->views.end()) { ...
        effects->renderOffscreenQuickView(renderTarget, viewport, screenView.get());
    }
}
```

Ele **não** encadeia `effects->paintScreen()`. Enquanto ativo, substitui a cena:
a caixinha de "Gravando" viria acompanhada de todas as janelas sumindo. Não é
uma questão de escrever o QML com cuidado — é o que a classe faz. É por isso que
o Overview e o WindowView desenham as próprias miniaturas de janela: eles
precisam, porque a cena real não está mais lá.

**O que faz o que se quer é C++ interno.** O `OutputLocatorEffect` — o
retângulo que aparece em cada monitor ao mudar a configuração de telas — é
exatamente um overlay pequeno e passivo, e ele consegue porque chama
`effects->paintScreen()` **primeiro** e só depois compõe a própria vista por
cima. Esse caminho exige um plugin binário ligado à API de efeitos do KWin, que
é interna, não tem promessa de ABI e muda a cada versão. Não é algo para um
projeto de fora carregar.

**`org.kde.osdService` é interno.** Ele existe, tem um `showText(icon, text)`
tentador, e é o que o Plasma usa para volume e brilho. Mas é implementado dentro
do `plasmashell` (`shell/osd.h`), não tem XML publicado em
`/usr/share/dbus-1/interfaces/`, não aparece na documentação do
`develop.kde.org` e não promete nada a terceiros. A regra era não usar API
privada só porque ela foi encontrada no código interno.

**Então o aviso de gravação continua sendo a janela do Ditador.** E funciona bem
no Plasma: ela sobe pelo XWayland e o KWin honra o "sempre por cima" que o
GNOME/Wayland recusa. Do lado do Rust isso está escrito em
`state::Integracoes::mostram_o_aviso`, que é a razão de existirem duas perguntas
em vez de uma:

| | recolhe o ícone da bandeja | recolhe o aviso de gravação |
|---|---|---|
| extensão do GNOME | sim | sim (o OSD do Shell o substitui) |
| widget do Plasma | sim | **não** (nada o substituiria) |

Se um dia o KWin ganhar um efeito declarativo que preserve a cena, ou o Plasma
publicar o `osdService`, o lugar a mexer é aquele método e nada mais.

## Decisões de interface

**Sem página de configuração.** Os três exemplos que se costuma pensar não
sobrevivem ao exame: "mostrar OSD nativo" não tem OSD para mostrar; "mostrar o
indicador quando pronto" é literalmente a opção *Sempre visível / Visível quando
relevante / Oculto* que a própria bandeja do Plasma já oferece; "notificar
erros" contraria a HIG, que reserva notificação para o que exige ação, e o ícone
com o popup já dizem o que houve. Tudo o mais que alguém configuraria — modelo,
microfone, idioma, colagem, backend — é do Ditador, e mora na janela dele, a um
clique do popup. Uma página que duplicasse qualquer um dos dois lados seria uma
segunda verdade. Por isso o `Plasmoid.removeInternalAction("configure")` no
`main.qml`: um item de menu que abre um diálogo vazio é pior do que item nenhum.

**Ícones do projeto, tratados como máscara.** São os mesmos
`ditador-*-symbolic` da bandeja e do indicador do GNOME — o programa tem a mesma
cara nas três áreas de trabalho, e o Breeze não tem nada equivalente a
"transcrevendo" (usar um microfone genérico para dois estados diferentes seria
trocar informação por familiaridade). Eles têm `fill="#2e3436"` fixo, que é a
convenção do GTK, onde o tema recolore ícones `-symbolic` à força; o Qt não faz
isso. Daí o `isMask: true` no `Kirigami.Icon`: a silhueta é desenhada na cor de
texto do tema, e o ícone acompanha Breeze claro, Breeze escuro e qualquer
esquema de cores. As quatro formas são distintas entre si — microfone, ponto de
gravação, ampulheta, triângulo — então o estado não depende de cor para ser
lido. O `fallback` é `audio-input-microphone-symbolic`, do Breeze, para o caso
de os ícones do Ditador não estarem instalados.

**Categoria `ApplicationStatus`.** O Ditador é um aplicativo do usuário rodando
em segundo plano, não um serviço do sistema — `SystemServices` é o que o cofre e
o daemon de backup usam. É também o que o StatusNotifierItem do Ditador sempre
declarou (`Category::ApplicationStatus`, em `src/tray.rs`), e as duas superfícies
concordarem é o ponto.

**Começar e parar, nunca alternar.** O contrato tem `Alternar` — é o que a tecla
de atalho e o `ditador --alternar` usam —, e ele fica de fora deste lado por
escolha. Entre desenhar um botão escrito "Ditar agora" e o clique nele cabe um
ditado inteiro pela tecla, que é o uso normal do programa; com `Alternar`, esse
botão pararia a gravação que começou nesse meio-tempo. Pedindo o resultado
desejado em vez da troca, o rótulo nunca mente. É a mesma decisão que a extensão
do GNOME tomou, pela mesma razão.

**Nenhum `QProcess`, nenhum shell.** "Ditar agora", "Configurações do Ditador" e
"Encerrar" são chamadas D-Bus. Nada aqui executa `ditador --alternar`.

## Instalar

Com o Ditador já instalado (`./instalar.sh` na raiz):

```bash
./kde-plasma/instalar.sh
```

Ele confere as ferramentas, compila o plugin, pede a senha uma vez para
instalá-lo, e põe o widget no lugar. As dependências, se faltarem, saem na tela
com o nome exato do pacote — todos conferidos com `dpkg -S` num Kubuntu 26.04:

```bash
sudo apt install cmake build-essential qmake6 qt6-base-dev qt6-declarative-dev \
                 extra-cmake-modules kpackagetool6 qml6-module-org-kde-ki18n
```

Para as ferramentas de desenvolvimento (`qmllint`, `qmlformat`):

```bash
sudo apt install qt6-declarative-dev-tools
```

Depois, para o widget aparecer:

1. o Ditador precisa estar **em execução** — é a presença dele no barramento que
   faz o `plasmashell` carregar o widget;
2. botão direito na bandeja → **Configurar a Bandeja do Sistema** → **Entradas**,
   e ponha "Ditador" em **Mostrado**.

O ícone antigo da bandeja some sozinho quando o widget assume.

Se o widget não aparecer na lista de entradas, o `plasmashell` ainda não releu a
pasta de widgets:

```bash
systemctl --user restart plasma-plasmashell
```

## Atualizar

Depois de um `git pull`:

```bash
./kde-plasma/instalar.sh
```

O mesmo script serve: ele detecta que o widget já está instalado e usa
`kpackagetool6 --upgrade`, e recompila e reinstala o plugin.

**Depois de atualizar, reinicie o `plasmashell`** — vale para o C++ e também para
o QML:

```bash
systemctl --user restart plasma-plasmashell
```

Não é "por via das dúvidas". O `--upgrade` troca os arquivos no disco, mas o
`plasmashell` continua com a compilação anterior do QML na memória: numa
atualização observada aqui, ele repetiu no `journal` um erro de sintaxe que já
não existia no arquivo instalado, no exato segundo do `--upgrade`. Quem for
depurar sem saber disso vai corrigir o QML, ver o mesmo erro e procurar o
problema no lugar errado. Para o `.so` do plugin é ainda mais direto: ele já está
carregado no processo, e um processo não troca uma biblioteca carregada.

Não é preciso sair da sessão — nem na primeira instalação, ao contrário do GNOME
Shell. O Ditador continua rodando durante tudo isso: é outro processo, e o ícone
dele volta para a bandeja enquanto o painel se reconstrói.

## Remover

```bash
./kde-plasma/desinstalar.sh
```

Tira **só** a integração com o Plasma: o widget e o plugin. O Ditador, o modelo
de transcrição (574 MB), a configuração do usuário e a extensão do GNOME ficam
onde estão. O ícone da bandeja volta sozinho assim que o `plasmashell`
descarregar o widget.

## Desenvolver

```bash
./kde-plasma/testar.sh
```

Confere o QML, compila o plugin, roda os testes, atualiza o widget e o abre numa
janela pelo `plasmawindowed` — mesmo motor QML e mesmas APIs do painel, em outro
processo. O plugin sai do diretório de build, e não do sistema, então dá para
iterar sem um `sudo` a cada mudança.

Nada nesse ciclo reinicia o `plasmashell` nem o KWin. **Derrubar o KWin numa
sessão Wayland derruba a sessão inteira** — nunca use `kwin_wayland --replace`
nem `killall kwin_wayland` para testar nada.

As duas metades também rodam sozinhas:

```bash
./kde-plasma/testar.sh --contrato   # o XML canônico contra o Ditador em execução
./kde-plasma/testar.sh --backend    # o plugin conversando com o Ditador de verdade
```

O `--backend` é o par do `teste-do-backend.js` da extensão do GNOME, e existe
pelo mesmo motivo: o `cargo test` prova que o Rust publica o contrato certo, o
`qmllint` prova que o QML é válido, e nenhum dos dois prova que os dois lados se
entendem no barramento. Ele instancia o plugin de verdade, espera o `GetAll`
responder, confere o retrato inteiro, **abre o microfone por um segundo**, exige
que o estado vire `gravando`, que `GravandoDesde` deixe de ser zero e que o sinal
`Nivel` chegue — e fecha o que abriu, inclusive se falhar no meio.

```
PASS   : DitadorBackend::test_01_o_ditador_responde()
PASS   : DitadorBackend::test_02_o_retrato_chega_inteiro()
PASS   : DitadorBackend::test_03_gravar_e_parar()
PASS   : DitadorBackend::test_04_a_presenca_esta_no_barramento()
```

Enquanto ele roda, ele *é* uma integração do Plasma: segura o nome de presença, e
o Ditador recolhe o ícone da bandeja. Dá para ver os dois lados anunciando isso
no `journalctl --user -u ditador -f`.

### Sobre os avisos do `qmllint`

Sobram quatro, todos `Could not find property "Plasmoid.contextualActions"`. A
propriedade existe (é do `Plasma::Applet`); o `qmllint` é que não enxerga as
propriedades do objeto anexado. Os widgets do próprio Plasma 6.6 produzem os
mesmos — confira:

```bash
/usr/lib/qt6/bin/qmllint -I /usr/lib/x86_64-linux-gnu/qt6/qml \
  /usr/share/plasma/plasmoids/org.kde.plasma.vault/contents/ui/main.qml
```

O `testar.sh` filtra exatamente esses quatro e falha em qualquer outro. Os
demais foram resolvidos, e não silenciados: o acesso não qualificado ao
`i18nd()` sumiu com o `KI18nContext` do KF6 (que deixa o domínio dito uma vez e
as chamadas qualificadas), e o acesso ao objeto `Plasmoid` de dentro de
`RepresentacaoCompleta.qml` virou uma propriedade passada de fora.

### A fonte da verdade das APIs

O Plasma instalado nesta máquina, não a internet. Tutoriais de Plasma 5
descrevem `metadata.desktop`, `PlasmaComponents 2` e `X-Plasma-API:
declarativeappletscript` — coisas que ou não existem mais ou não são mais
necessárias.

```bash
# Os widgets de verdade, para copiar o idioma atual:
ls /usr/share/plasma/plasmoids/
cat /usr/share/plasma/plasmoids/org.kde.plasma.vault/contents/ui/main.qml

# A API dos componentes:
ls /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/plasma/components/
cat /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/plasma/extras/Representation.qml

# Os tipos que o PlasmoidItem expõe:
grep -A40 'name: "PlasmoidItem"' \
  /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/plasma/plasmoid/plasmoidplugin.qmltypes

# Os nomes de ícone que existem mesmo:
find /usr/share/icons/breeze -name "*microphone*"
```

## Diagnóstico

```bash
# O Ditador enxerga a integração?
ditador --diagnostico

# O serviço está no ar, e com que cara?
gdbus introspect --session --dest io.github.danielfreitasdev.Ditador \
  --object-path /io/github/danielfreitasdev/Ditador --xml

# O estado agora:
qdbus6 io.github.danielfreitasdev.Ditador /io/github/danielfreitasdev/Ditador \
  org.freedesktop.DBus.Properties.Get io.github.danielfreitasdev.Ditador Estado

# O widget está segurando a presença?
qdbus6 | grep PlasmaIntegration

# O widget está instalado?
kpackagetool6 --type Plasma/Applet --list | grep ditador
kpackagetool6 --type Plasma/Applet --show io.github.danielfreitasdev.ditador

# O plugin QML está no lugar?
ls "$(qmake6 -query QT_INSTALL_QML)/io/github/danielfreitasdev/ditador"

# Os registros:
journalctl --user -f -t plasmashell
journalctl --user -u ditador -f
```

Para o widget falar mais durante um teste:

```bash
QT_LOGGING_RULES="ditador.plasma.debug=true" plasmawindowed io.github.danielfreitasdev.ditador
```

A categoria é `ditador.plasma`, e ela diz o essencial e só: backend subiu,
backend caiu, presença adquirida, chamada que falhou. Nada por quadro, nada por
tique de temporizador.

Essas ferramentas (`qdbus6`, `gdbus`, `kpackagetool6`) são de diagnóstico. Nada
em execução depende delas.

## Limitações conhecidas

- **Sem OSD nativo.** Explicado acima. O aviso de gravação é a janela do
  Ditador, pelo XWayland.
- **O plugin precisa de `sudo` para instalar.** O Qt 6 não tem diretório de
  módulos QML por usuário.
- **Não distribuível pela KDE Store** como widget puro, por causa do `.so`.
- **Não há catálogo de traduções.** As frases estão em português, que é a língua
  do projeto; o código já passa por `i18n()` com domínio próprio, então acrescentar
  um catálogo depois não exige mexer no QML.
- **O ícone pisca por alguns milissegundos** no arranque do Ditador, pelo motivo
  descrito acima.
- **Alt+Tab, Overview, tela cheia, tela de bloqueio e multimonitor** não são
  assunto desta integração: não há efeito de KWin, nada desenha por cima da
  cena, e o widget vive dentro do painel. A tela de bloqueio não vê nada nosso —
  o `plasmashell` não roda widgets nela.

## Licença

MIT, a mesma do Ditador (veja `LICENSE`, na raiz). Não houve relicenciamento de
nada: os arquivos novos desta pasta nascem com o cabeçalho
`SPDX-License-Identifier: MIT`.

MIT é compatível com o ecossistema KDE — é permissiva e o KDE a aceita para
componentes de terceiros. Widgets do próprio Plasma costumam ser GPL ou
LGPL/"KDE Accepted GPL"; isso importaria se este código fosse ser incorporado ao
`plasma-workspace`, o que não é o caso: ele mora neste repositório e acompanha o
Ditador.
