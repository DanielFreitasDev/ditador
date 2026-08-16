# Memória técnica do Ditador

O que já foi investigado neste projeto, com a causa que se descobriu no fim.

Existe para uma situação só: alguém — pessoa ou agente — topa com um erro,
gasta horas atrás dele, encontra a causa, conserta. Semanas depois o mesmo erro
aparece, e a investigação inteira recomeça do zero porque não ficou registro
nenhum do que se aprendeu. **Este arquivo é o registro.**

## Antes de investigar, procure aqui

Erro de compilação, teste que falha, CI vermelha, comportamento que muda de um
sistema para outro, áudio que não abre, modelo que não carrega, pacote que não
instala: **procure aqui primeiro**, por termos do erro, da mensagem, da
biblioteca, do módulo ou do sistema operacional. É por isso que os títulos são
escritos com as palavras que alguém digitaria na busca, e não com o nome bonito
do problema.

Achou uma entrada? Confira antes se ela ainda vale. Este arquivo descreve o
estado conhecido do projeto, não uma verdade eterna — biblioteca atualiza,
sistema muda, e um contorno de ontem pode ter virado solução oficial hoje. Nesse
caso, use a solução de hoje e **atualize a entrada**.

## Depois de resolver, registre

Sem pedir autorização, como parte de terminar a tarefa. O critério é um só:

> Isto pouparia uma investigação, um erro ou um bom tempo de trabalho no futuro?

Vale registrar quando a causa não era óbvia, quando foram precisas várias
tentativas, quando a primeira hipótese estava errada, quando a documentação
oficial não bastou, quando o comportamento muda entre Windows, GNOME e KDE, ou
quando há chance real de o problema voltar.

Não vale registrar o que se lê no código, o que é trivial ("criei o arquivo X",
"renomeei a variável Y") nem o que deu certo de primeira. **Isto não é diário de
bordo** — o `git log` já é.

**Antes de criar uma entrada nova, procure uma parecida e melhore aquela.** Cinco
entradas descrevendo variações do mesmo problema valem menos do que uma boa.

## Este arquivo e os outros dois

| Arquivo | Responde |
|---|---|
| `CLAUDE.md` | **como** trabalhar aqui: idioma, portões, arquitetura, e as armadilhas que são regra vigente ("não 'conserte' isto") |
| `docs/LEARNINGS.md` | **o que** se aprendeu trabalhando: sintoma → causa → solução → prevenção |
| `git log` | o que foi feito, e quando |

A fronteira com as "Armadilhas" do `CLAUDE.md` é essa: lá está o que **não se
deve mexer** e por quê, em forma de regra permanente; aqui está a investigação
que produziu o conhecimento, em forma de sintoma e diagnóstico. Uma entrada
daqui que vire regra permanente do projeto pode ganhar uma linha lá — sem apagar
esta, que é onde o "por quê" cabe por inteiro.

## Como escrever uma entrada

Título com as palavras que se pesquisaria: **onde** — **o que aconteceu**.
Depois, só os campos que fizerem sentido; nenhum é obrigatório.

```markdown
## Área/Sistema — o sintoma em poucas palavras

**Contexto** — onde e em que situação aconteceu.
**Sintoma** — a mensagem ou o comportamento, colado como ele aparece.
**Causa** — o que era de verdade.
**Solução** — o que resolveu.
**Prevenção** — a regra que evita a próxima vez.
**Arquivos** — o que está envolvido.
**Ambiente** — quando muda o caso: Windows, GNOME, KDE, Wayland, CPU, GPU, CI, release.
**Comandos** — só os que servem para diagnosticar de novo.
```

Cada entrada precisa se explicar sozinha, para quem não viu nada da investigação.
Nada de "como vimos antes", "aquele erro" ou "a solução que testamos".

As categorias abaixo nascem conforme o conhecimento chega. Não crie seção vazia
esperando enchê-la depois.

---

# CI/CD e GitHub Actions

## CI/KDE — `CMAKE_CXX_COMPILER not set` num contêiner onde o g++ está instalado

**Contexto** — o trabalho do KDE na CI roda dentro do contêiner `ubuntu:26.04`, e
instala as dependências com `apt-get` antes de compilar o plugin C++ do widget.

**Sintoma** — na primeira linha do `cmake -S`, duas mensagens em sequência:

```
CMake Error: CMake was unable to find a build program corresponding to
"Unix Makefiles". CMAKE_MAKE_PROGRAM is not set. You probably need to select
a different build tool.
CMake Error: CMAKE_CXX_COMPILER not set, after EnableLanguage
```

**Causa** — faltava o **`make`**, e não o compilador. O pacote `g++` do Ubuntu traz
o compilador e não traz o make, que vem pelo `build-essential`; a imagem oficial
do Ubuntu é crua e não tem nenhum dos dois. A segunda mensagem é consequência da
primeira — sem programa de build, o CMake aborta o `EnableLanguage` e relata o
compilador como não definido.

**Solução** — `make` explícito na lista do `apt-get install`.

**Prevenção** — em contêiner cru, `g++` não implica `make`. Ao ler
`CMAKE_CXX_COMPILER not set`, leia a mensagem **de cima** primeiro: quando ela
fala do gerador ("unable to find a build program"), o compilador não é o
problema.

**Arquivos** — `.github/workflows/ci.yml` (trabalho `kde`), `kde-plasma/CMakeLists.txt`.
**Ambiente** — CI, contêiner `ubuntu:26.04`.

## CI/KDE — o aviso de locale do Qt reprova o portão do `qmllint`

**Contexto** — o `./kde-plasma/testar.sh --ci` roda o `qmllint` e falha em
qualquer saída que não seja um dos quatro avisos conhecidos de
`Plasmoid.contextualActions`. Ele lê a saída padrão e a de erro juntas
(`2>&1`), de propósito: o `qmllint` fala pela saída de erro.

**Sintoma** — o portão reprova com `!! qmllint reclamou de algo novo.`, e o que
ele imprime não é reclamação nenhuma:

```
Detected locale "C" with character encoding "ANSI_X3.4-1968", which is not UTF-8.
Qt depends on a UTF-8 locale, and has switched to "C.UTF-8" instead.
```

**Causa** — a imagem crua do Ubuntu não tem locale configurado, e **toda**
ferramenta do Qt imprime esse aviso na saída de erro — o `qmllint`, o `moc`, o
`cmake` durante a compilação. Na máquina de quem desenvolve isso nunca aparece,
porque a sessão gráfica já tem um locale UTF-8.

**Solução** — `LANG: C.UTF-8` e `LC_ALL: C.UTF-8` no `env:` do trabalho.

**Prevenção** — não filtre o aviso no script. O problema de ambiente é real: o
QML deste widget tem acento dentro, e uma ferramenta rodando em ASCII é a
ferramenta errada para conferi-lo. Filtrar deixaria o portão verde conferindo o
arquivo com meia leitura. Vale para qualquer ferramenta do Qt em contêiner.

**Arquivos** — `.github/workflows/ci.yml` (trabalho `kde`), `kde-plasma/testar.sh`.
**Ambiente** — CI, contêiner sem locale.

## CI — os pacotes do Ubuntu que dão as ferramentas de GNOME, Plasma e Qt num agente sem tela

**Contexto** — montar portões de CI para a extensão do GNOME e o widget do Plasma
sem sessão gráfica. Achar o nome do pacote de cada ferramenta é o trabalho chato,
e errar dá "command not found" ou import de QML que não resolve.

**Causa/mapa** — apurado com `dpkg -S` numa Kubuntu 26.04, e confirmado rodando
na CI:

| Ferramenta / módulo | Pacote |
|---|---|
| `gnome-extensions` (o `pack`, que é o build da extensão) | `gnome-shell` |
| `glib-compile-schemas` | `libglib2.0-bin` |
| `qmllint`, `qmltestrunner` | `qt6-declarative-dev-tools` |
| `ECMConfig.cmake` (o `find_package(ECM)`) | `extra-cmake-modules` |
| `org.kde.plasma.components`, `.core`, `.extras`, `.plasmoid` | `plasma-desktoptheme` |
| `org.kde.kirigami` | `qml6-module-org-kde-kirigami` (vem por dependência do anterior) |
| `org.kde.ki18n` | `qml6-module-org-kde-ki18n` |
| `QtTest` (para o `qmltestrunner`) | `qml6-module-qttest` |

O `gnome-shell` é pesado, mas com `--no-install-recommends` não puxa a sessão
inteira e o `pack` funciona sem tela. O `plasma-desktoptheme` é o que faz o
`qmllint` resolver os imports do widget — sem ele, ele reclama de tudo, que é o
mesmo que não conferir nada.

**Prevenção** — o `ubuntu-latest` do GitHub é **uma versão atrás** (hoje, 24.04,
com Qt 6.4). O `CMakeLists.txt` do plugin exige Qt 6.6, então o trabalho do KDE
roda em contêiner `ubuntu:26.04`, que é o alvo declarado do widget. Antes de
supor que um pacote não existe, confira em qual versão do Ubuntu você está
procurando.

**Ambiente** — CI, Ubuntu.
**Comandos** — `dpkg -S $(command -v <ferramenta>)` e
`dpkg -S /usr/lib/x86_64-linux-gnu/qt6/qml/<caminho>/qmldir` respondem isso numa
máquina que já tenha a ferramenta.

## CI/Windows — o script do Inno Setup dá para compilar sem os binários do programa

**Contexto** — o instalador `.exe` do Windows é gerado pelo Inno Setup só na hora
de publicar uma versão. Um erro de sintaxe no `.iss` apareceria, portanto, no
pior momento possível: no meio de um lançamento.

**Solução** — o `.iss` tem um símbolo `SemArquivos` que exclui a seção `[Files]`
por `#ifdef`, e a CI compila o script com ele a cada push:

```powershell
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /Qp /DSemArquivos /DMyAppVersion=0.0.0 ...
```

**Verificado na CI** — o Inno Setup 6 **já vem** na imagem `windows-latest`
(há um `choco install innosetup` de reserva, que não chegou a ser usado), e o
ISCC compila um script com `[Files]` vazio sem reclamar. São as duas suposições
em que esse portão se apoia, e as duas se confirmaram.

**Prevenção** — todo arquivo que só é usado na hora de publicar merece um modo
"compila mas não produz nada" chamado a cada push. Vale para o `.iss` e valeria
para qualquer gerador de pacote.

**Arquivos** — `windows-integration/instalador/ditador.iss`, `.github/workflows/ci.yml`.
**Ambiente** — CI, `windows-latest`.

---

# Interface e capturas do README

## Capturas — `recording.png` não sai, e as outras duas saem no mesmo segundo

**Contexto** — `./gerar-imagens.sh` refaz as imagens do README rodando o passeio
de demonstração (`DITADOR_DEMO=1` + `DITADOR_CAPTURA=<dir>`), que passa sozinho
pelas telas de gravação, resultado e configurações e fotografa cada uma.

**Sintoma** — o script para com `!! o passeio não gravou recording.png no tema
claro`, e no log as capturas de `result` e `settings` aparecem com o mesmo
carimbo de segundo, cerca de dez segundos depois do arranque — quando as fases
do passeio duram 4 s, 5 s e 6 s.

**Causa** — a espera que deixava a tela assentar antes da foto contava **quadros**
(`frames_restantes = 12`), o que só equivale aos ~200 ms pretendidos se a janela
receber os 60 quadros por segundo que se supõe. Ela não recebe: recém-criada e
com sincronia vertical, sob XWayland esta janela roda a cerca de **2 quadros por
segundo**. Os doze quadros viraram cinco segundos — mais do que os quatro da
primeira fase, que por isso nunca chegava a ser fotografada, e o suficiente para
atrasar as outras duas até se encontrarem. Não é lentidão da interface: com
`DITADOR_QUADROS=1`, que desliga a sincronia, a mesma tela faz ~1800 quadros por
segundo. A primeira hipótese — arranque lento comendo a primeira fase — estava
errada, mas apontou um defeito de verdade ao lado (veja "Prevenção").

**Solução** — a espera passou a ser de relógio (`ASSENTAR`, 1200 ms, acima do
teto de `animation_ms`), que é do que ela precisava desde o começo: as animações
que ela existe para deixar terminar são cronometradas, não contadas em quadros.

**Prevenção** — nada que espere animação deve contar quadros. E o passeio agora
marca o instante inicial no **primeiro quadro**, não na construção do `App`: os
segundos que o eframe, o glow e o driver Vulkan levam para pôr uma janela na
tela saíam do orçamento da primeira fase.

**Arquivos** — `src/ui.rs` (`Captura`, `ASSENTAR`, `Demo`, `demonstrar`),
`gerar-imagens.sh`.
**Ambiente** — Linux, Wayland com a janela via XWayland (o padrão do Ditador).
**Comandos** — `DITADOR_DEMO=1 DITADOR_QUADROS=1 DITADOR_CAPTURA=/tmp/x
target/release/ditador` mostra a taxa real de quadros por tela.

## Capturas — o aviso de modelo ausente saiu impresso por cima dos botões no README

**Contexto** — a `assets/capturas/resultado.png` publicada no README mostrava,
em vermelho e por cima dos botões "Copiar" e "Copiar e colar", a frase "O modelo
de transcrição ainda não está aqui (…/ggml-large-v3-turbo-q5_0.bin)".

**Causa** — duas, somadas. O passeio de demonstração força `ModelState::Ready`
mas não limpava `state.message`, e a thread do Whisper, que roda em paralelo e
não sabe de passeio nenhum, escreve ali quando o arquivo do modelo falta — a
captura foi tirada no minuto em que o download ainda não tinha terminado. E na
tela de resultado essa mensagem era desenhada sem limite de largura, à direita
dos botões, então uma frase comprida passava por cima deles.

**Solução** — o passeio limpa `message`, zera `aviso_atalho` e ignora
`state.integracoes` (com a extensão do GNOME instalada, `tela_visivel` esconde a
tela de gravação e o script falhava sem dizer por quê). E a mensagem da tela de
resultado é um `egui::Label::truncate()` com o texto inteiro no `hover`: o que
sobra ali depende dos botões à esquerda, que mudam com o zoom e com a presença
do "Copiar e colar", então não cabe número fixo.

**Prevenção** — modo de diagnóstico que promete funcionar "sem microfone e sem
modelo baixado" precisa neutralizar **tudo** o que o ambiente escreve na tela,
não só o campo principal. E antes de commitar imagem do README, abra a imagem.

**Arquivos** — `src/ui.rs` (`demonstrar`, tela de resultado), `assets/capturas/`.
**Ambiente** — Linux; o efeito da extensão do GNOME só aparece em quem a tem
instalada.
