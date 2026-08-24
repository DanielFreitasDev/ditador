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

## CI/Release — um artefato interno do `ci.yml` virou anexo da release (`ditador.exe` solto)

**Contexto** — a versão portátil pôs o `ci.yml` a passar o `ditador.exe` do
estágio 1 (Rust no Windows) para o estágio 2 (que empacota o portátil) por
`upload-artifact`/`download-artifact`, com o nome `backend-windows-cpu`. O
`release.yml` **chama** o `ci.yml` por `workflow_call`, então a validação de uma
publicação roda dentro do mesmo run que os trabalhos de release.

**Sintoma** — a release v0.9.0 saiu com um anexo `ditador.exe` (19 MB) que
nenhum trabalho de release produziu, listado também no `SHA256SUMS` e no corpo
das notas. Um binário cru, sem versão no nome, compilado da validação — não do
código da tag — e confundível com um instalador.

**Causa** — o trabalho `publicar` baixava **todos** os artefatos do run
(`download-artifact` sem `name` nem `pattern`, com `merge-multiple`). Enquanto
os únicos artefatos eram os dos trabalhos de release, "todos" era o filtro
certo; no instante em que o `ci.yml` chamado criou um artefato para uso interno,
"todos" passou a incluí-lo. O vazamento não aparece em push de ramo (o `ci.yml`
sozinho não publica nada) — só na release, que é onde ninguém quer estreia de
defeito.

**Solução** — na v0.9.0, à mão: `gh release delete-asset`, `SHA256SUMS`
regravado sem a linha e o corpo das notas editado. No `release.yml`, os
artefatos de release ganharam o prefixo `anexos-` e o `publicar` baixa
`pattern: anexos-*` — o que o `ci.yml` criar para si nunca casa com o filtro.

**Prevenção** — artefato de release e artefato de uso interno dividem o mesmo
espaço de nomes do run quando um workflow chama o outro. Quem publica não pode
baixar "tudo": baixa um prefixo reservado, e todo trabalho novo de release
nomeia o artefato com ele. E depois de mexer em artefatos de qualquer um dos
dois workflows, confira a lista de anexos da release seguinte contra a tabela
do `docs/CI-E-RELEASES.md`.

**Arquivos** — `.github/workflows/release.yml` (nomes `anexos-*` e o `pattern`
do `publicar`), `.github/workflows/ci.yml` (o artefato `backend-windows-cpu`).
**Ambiente** — CI, release.
**Comandos** — `gh release view vX.Y.Z --json assets --jq '[.assets[].name]'`

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

## Interface — `Negative height makes no sense, but got: -6` ao trocar de tela

**Contexto** — o passeio de demonstração (`DITADOR_DEMO=1`) derrubava o programa
na transição da tela de gravação para a de resultado. Aconteceu com o código da
0.6.0 também: é um defeito antigo, que só nunca tinha sido acionado porque
ninguém rodava o passeio numa build de depuração.

**Sintoma**

```
thread 'main' panicked at egui-0.36.1/src/ui.rs:749:
Negative height makes no sense, but got: -6
   3: ditador::ui::App::result::{{closure}}
   4: ditador::widgets::cartao::{{closure}}
```

**Causa** — a janela deste programa não tem tamanho próprio por tela: cada troca
manda um `ViewportCommand::InnerSize`, e o comando é **atendido no quadro
seguinte**. Existe portanto sempre um desenho feito com o tamanho da tela
anterior. Vindo da gravação (178 pontos de altura) para o resultado (372), a
sobra daquele quadro é de 92 pontos, e a conta da tela de resultado —
`available_height() - (10 + ALTURA + 12 + RESPIRO)`, menos os 24 da margem do
cartão — dá −6. O `set_min_height` do egui entra em pânico com altura negativa.

Medido com um `eprintln!` na própria função: `available=92 altura_texto=18
max_rect=[[40.0 40.0] - [444.0 182.0]]` — 182 é a altura da janela de gravação,
não a de resultado.

**Solução** — `ui::altura_util(ui, rodape)`, que subtrai o rodapé e nunca devolve
menos de 24. As quatro contas de altura da interface (resultado, configurações,
histórico e a margem interna do cartão do resultado) passam por lá.

**Prevenção** — **toda** conta de altura desta interface precisa de piso. Não é
zelo: a janela é redimensionada por comando, então o quadro com o tamanho errado
não é um caso raro, é um por troca de tela. O piso vale por aquele quadro; o
seguinte já vem com a janela certa.

**Arquivos** — `src/ui.rs` (`altura_util`, `result`, `settings`, `historico`).
**Ambiente** — Linux/XWayland, build de depuração. Numa build de release a
janela às vezes chega a tempo, o que é justamente o que fez o defeito atravessar
várias versões sem aparecer.
**Comandos** — `DITADOR_DEMO=1 DITADOR_CAPTURA=/tmp/x RUST_BACKTRACE=1
target/debug/ditador`

## Interface — a quarta e a quinta vez em que uma resposta atrasada tomou a janela de quem já estava noutra coisa

**Contexto** — o padrão de defeito mais repetido deste projeto, encontrado agora
em mais dois lugares. Todo caminho que **chega tarde** — porque esperou o
Whisper, porque dormiu antes de colar, porque o microfone só falhou depois —
volta a mexer na janela num instante em que ela já pode pertencer a outra coisa:
a um ditado novo em andamento, ou à tela de configurações com um rascunho
digitado dentro.

**Sintoma** — dois casos novos, os dois com a janela sempre-no-topo aparecendo
por cima de quem estava falando:

* **a colagem que falha** (`colar_depois`, um quarto de segundo depois de o texto
  ficar pronto): a tela de resultado subia com "Copiei, mas não consegui colar"
  e o campo de texto **vazio** — porque o `start_recording` do ditado novo já
  tinha limpado o `text`. Uma janela de erro sem o erro dentro;
* **o microfone que falha** (`AudioEvent::Failed`): trocava a tela de
  configurações pela de erro, e o rascunho digitado se perdia ao reabrir. O
  comentário do `SttEvent::Failed`, logo ao lado, afirmava em tantas palavras
  que "o mesmo critério do áudio" já valia — e não valia: o braço do áudio
  conferia só o número do ditado.

**Causa** — a guarda existe e tem nome (`resultado_pode_aparecer`, `ocupada`),
mas ela mora dentro do `on_transcription`. Quem não passa por ali não a herda, e
os dois caminhos acima não passam.

**Solução** — a mesma pergunta nos dois: `state.gravando() || state.view ==
View::Settings`. Ocupada a janela, sobra a linha do journal, que já existia.

**Prevenção** — a regra, que vale para o próximo caminho tardio que alguém
escrever: **antes de escrever em `state.view`, pergunte de quem é a janela
agora.** E desconfie de comentário que afirma que outro braço do `match` já faz
alguma coisa — dois dos defeitos desta rodada estavam exatamente aí, num
comentário que descrevia um comportamento que o código não tinha.

Vale a mesma desconfiança para o **estado de captura de atalho**: sair da tela de
configurações pelo botão "Ver as transcrições" deixava `capturando` de pé e o
ouvinte de teclas em modo de captura fora da tela que o explica — o atalho de
ditar parava de ditar, e o aperto seguinte virava um rascunho que ninguém ia
salvar. Toda saída da tela de configurações precisa desfazer a captura, e não só
o "Cancelar" dela.

**Arquivos** — `src/controller.rs` (`contar_a_falha_na_colagem`, `on_audio`,
`abrir_historico`), `src/hotkey.rs` (`sair_da_captura`).

## Interface — na lista de transcrições o "há 5 min" congelava e o aviso de cópia não saía mais

**Contexto** — a tela de transcrições (`View::Historico`), aberta pelo ícone da
barra, pelo botão das configurações ou por `ditador --historico --janela`.

**Sintoma** — dois, e os dois passam despercebidos numa conferência rápida:

* o tempo relativo ao lado de cada frase ("agora", "há 5 min", "ontem") ficava
  parado no valor que tinha no instante em que a lista foi aberta. Com a janela
  meia hora aberta, uma frase ditada naquele momento continuava dizendo "agora";
* o aviso verde "na área de transferência", que a tela de resultado apaga depois
  de três segundos, **ficava na tela para sempre** depois de um clique em
  "Copiar" — inclusive por cima de uma cópia que já não era a última.

**Causa** — o `logic()` da interface decidia a cadência de repintura num `match`
sobre a tela, e o histórico caía no braço `_ => {}`. Quer dizer: a janela só era
redesenhada quando o `Sinal` avisava que o estado tinha mudado — e nem o relógio
de parede nem o `copied_at` de três segundos são estado que mude.

A tela de resultado, que mostra exatamente o mesmo aviso de três segundos, já
tinha `request_repaint_after(250 ms)` por este motivo. O histórico nasceu depois
e não entrou na conta.

**Solução** — a decisão saiu de dentro do `logic()` para uma função própria,
`cadencia_de_repintura(view, modelo_carregando)`, com o histórico ao lado do
resultado. `None` é "só quando o estado mudar", `Some(Duration::ZERO)` é "todo
quadro" (gravação, transcrição, o anel da carga do modelo) e
`Some(250 ms)` é "isto conta tempo mas não anima".

**Prevenção** — a função existe separada justamente porque **a resposta errada
aqui não derruba nada, não aparece em teste de tela nenhum e não deixa rastro no
log**: ela só faz um número parar de andar. Separada, ela cabe num teste de
unidade — e há um (`as_telas_que_contam_tempo_se_redesenham_sozinhas`), que
percorre as telas uma a uma. Tela nova que mostre tempo decorrido, prazo ou
aviso que expira entra lá.

**Arquivos** — `src/ui.rs` (`cadencia_de_repintura`, `logic`).

## Interface — o deslizante de porcentagem perdia um ponto em 53 e em 59

**Contexto** — o volume dos avisos sonoros e a exigência do dicionário são
guardados como fração (`f32`, de 0 a 1) e mostrados como inteiro de 0 a 100. A
conversão acontece nos dois sentidos toda vez que a tela de configurações abre.

**Sintoma** — escolher 53 % de volume, salvar, reabrir as configurações e ver
52 %. O valor gravado mudava junto no Salvar seguinte. Só com esses dois números.

**Causa** — `(fracao * 100.0) as i64` **trunca**. Em `f32`,
`0.53 * 100.0 = 52,999998` e `0.59 * 100.0 = 58,999996`; os outros 99 valores da
faixa arredondam para o inteiro exato e voltam certos, o que é o pior tipo de
defeito — parece funcionar em quase todo lugar.

**Solução** — `por_cento` e `de_por_cento` em `src/ui.rs`, com `.round()`, usadas
pelos dois deslizantes.

**Prevenção** — conversão de `f32` para inteiro que representa um valor escolhido
por alguém arredonda, nunca trunca. E o teste varre a faixa inteira: um caso
isolado teria passado, porque 98 % dos valores estavam certos.

**Arquivos** — `src/ui.rs` (`por_cento`, `de_por_cento`).
**Comandos** — a varredura que encontrou os dois:

```rust
(0..=100).filter(|v| ((*v as f32 / 100.0) * 100.0) as i64 != *v)
```

## Dicionário — a correção de termos comia o artigo antes do termo

**Contexto** — a primeira versão do `src/dicionario.rs` transformava "usei o
kubernetes ontem" em "usei Kubernetes ontem". O artigo desaparecia.

**Causa** — a varredura era gulosa da esquerda para a direita e experimentava as
janelas **maiores primeiro**, aceitando a primeira que casasse. A chave da janela
de duas palavras "o kubernetes" é `okubernetes`, que está a **uma** edição de
`kubernetes`: acrescentar uma letra é uma edição, mesmo quando a letra é uma
palavra inteira. Aquela janela casava com 0,91 de semelhança, era testada antes
por ser maior, e engolia o artigo — sem a varredura nunca chegar a experimentar a
palavra seguinte sozinha, que casa exato.

O mesmo valia para qualquer palavra curta antes de um termo longo: "e
kubernetes", "do saopaulo", "com chargebee".

**Solução** — medir **todas** as janelas de **todas** as posições antes de
decidir, ordenar por semelhança (decrescente), depois por tamanho da janela, e
aceitar de cima para baixo descartando as que se sobrepõem a uma já aceita. O
casamento exato (1,0) passa a ganhar do aproximado (0,91) mesmo estando à direita
dele. O tamanho da janela só desempata: com "Charge" e "ChargeBee" cadastrados,
as duas janelas de "charge bee" casam exato e a maior é a certa.

**Prevenção** — casamento aproximado com janela variável não pode ser guloso da
esquerda para a direita. Uma janela maior *sempre* tem mais chance de estar a
poucas edições de um termo longo, então "maior primeiro" é o oposto do certo.

**Arquivos** — `src/dicionario.rs` (`corrigir`, `Candidato`, `melhor_termo`).
**Comandos** — `cargo test --no-default-features --features cpu dicionario`

## Linux/memória — o RSS crescia 29 MB e não voltava, sem haver vazamento

**Contexto** — investigação de quanto de memória o Ditador retém por ditado,
antes de decidir se valia mexer no alocador.

**Sintoma** — reproduzindo o padrão de alocação de um ditado quarenta vezes
seguidas (buffer do microfone de 23 MB, vetor reamostrado, alocações pequenas
sobrevivendo no meio do ciclo), o RSS do processo sobe para ~30 MB acima do
inicial e **fica lá**. Com `mallopt(M_MMAP_THRESHOLD, 128 kB)` no arranque, o
mesmo teste retém 84 kB.

| | RSS retido depois de 40 ditados |
|---|---|
| como estava | 29,4 MB |
| com o limiar pinado | 0,1 MB |

**Causa** — a glibc serve alocações acima do "limiar de mmap" com um `mmap`
privado, que volta ao sistema no `free`. O limiar é **dinâmico**: ao liberar um
bloco mapeado, a glibc o eleva até o tamanho daquele bloco (teto de 32 MB),
supondo que virá outro igual. A partir daí os blocos grandes saem das arenas do
malloc, e memória de arena liberada fica em cache para reúso — com as alocações
pequenas e vivas do programa fixando aquelas páginas.

**Vale registrar a forma da curva**, porque ela não é a de um vazamento: o
consumo sobe nos primeiros ditados e depois **estaciona**. Não é um programa que
engorda sem limite; é um programa que fica com trinta megabytes a mais para
sempre. Num aplicativo que sobe com a sessão e passa o dia na bandeja, é
justamente a memória que não deveria estar ocupada — mas quem procurar aqui
esperando ver o número crescer sem parar não vai ver.

**Solução** — `src/memoria.rs`: `mallopt(M_MMAP_THRESHOLD, 128 kB)` na primeira
linha do `main` e `malloc_trim(0)` ao fim de cada transcrição, na thread do
Whisper. Dois `extern "C"` escritos à mão, sem dependência nova, atrás de
`cfg(all(target_os = "linux", target_env = "gnu"))` — na musl as duas funções não
existem e o binário não linkaria.

**Prevenção** — o teste `os_buffers_de_um_ditado_voltam_para_o_sistema` reproduz
o padrão e falha se o RSS retido passar de 8 MB. Quem remover a chamada do `main`
por parecer supérflua descobre ali, e não pelo relato de uma máquina lenta no fim
do dia.

**Arquivos** — `src/memoria.rs`, `src/main.rs` (primeira linha), `src/stt.rs`
(fim do laço de trabalho).
**Ambiente** — Linux com glibc (medido na 2.43, Ubuntu). Não se aplica ao
Windows, ao macOS nem à musl: a heurística de limiar dinâmico é da glibc.

## Hugging Face — qual cabeçalho carrega o SHA-256 de um modelo

**Contexto** — implementar a conferência de soma do download do modelo, para o
caso do arquivo que chega com o tamanho certo e os bytes errados.

**O que se descobriu** — a resposta de `resolve/main/<arquivo>` traz **dois**
cabeçalhos parecidos e com valores diferentes:

* `x-linked-etag`, na resposta do **redirecionamento** (302), é o SHA-256 do
  arquivo do Git LFS — o mesmo valor que a API publica em `lfs.oid`;
* `etag`, na resposta **final** do CDN (200), é o hash do Xet, que é outra coisa.

Conferir contra o segundo reprova **todo** download bom. E há um terceiro caso:
para arquivos que não são LFS, o `x-linked-etag` carrega o SHA-1 do Git, de 40
caracteres — daí a conferência de tamanho e de alfabeto antes de aceitar o valor.

**Como pedir a soma pela API**, que é de onde a tabela `SOMAS` saiu:

```bash
curl -s -X POST https://huggingface.co/api/models/ggerganov/whisper.cpp/paths-info/main \
  -H 'Content-Type: application/json' \
  -d '{"paths":["ggml-large-v3-turbo-q5_0.bin"]}' | jq -r '.[].lfs.oid'
```

**Arquivos** — `src/modelo.rs` (`SOMAS`, `linked_etag`, `conferir`, `somar`).

## Testes — um teste apagou as configurações da máquina, e nenhum teste reprovou

**Contexto** — ao acrescentar uma opção nova (soltar o modelo da memória por
ociosidade), foi escrito um teste conferindo que salvar as configurações leva a
opção até a thread do Whisper. Ele chamava `Controller::apply_draft`, que é o que
o botão *Salvar* chama.

**Sintoma** — nenhum. Os 200 testes passaram, o clippy passou, a compilação
passou. O estrago apareceu por acaso, horas depois, numa captura de tela feita
para conferir o desenho da interface: a tela de configurações mostrava a opção
nova **ligada com 5 minutos**, que era exatamente o que o teste tinha escolhido.
Conferindo o `~/.config/ditador/config.json`, ele estava com a configuração da
`Bancada` de testes — cópia automática desligada, sons desligados, histórico
desligado —, por cima das escolhas de quem usa a máquina.

**Causa** — `apply_draft` grava a configuração em disco (`Config::save()`), e
`Config::save()` usa `config_path()`, que é global: num teste, ele aponta para o
`~/.config/ditador` de quem rodou `cargo test`. A `Bancada` já tinha o cuidado de
desligar cópia automática, sons e histórico justamente para não encostar na
máquina de quem testa; a configuração era o quarto efeito colateral, e o único
que **apaga** alguma coisa. Não há cópia de onde voltar: o `salvar_em` grava de
forma atômica (arquivo temporário + `rename`) justamente para não deixar arquivo
pela metade, e o `rename` substitui o anterior.

**Solução** — o teste passou a chamar `apply_audio_settings`, que é a parte que
aplica as configurações às threads sem tocar no disco; e uma trava passou a ler o
próprio `src/controller.rs` (`include_str!`) e reprovar se o módulo de testes
mencionar `apply_draft` ou `ApplyDraft`. Os nomes procurados são montados em
pedaços (`format!("apply_{}(", "draft")`) porque, escritos por extenso, a própria
trava casaria com a busca.

O que dava para restaurar foi restaurado pelo que se podia provar: os padrões
documentados no README para as três chaves que a bancada mexe, e o
`start_with_session` pelo `systemctl --user is-enabled ditador`, que é a fonte de
verdade daquele campo. O que era personalização e não deixou rastro — atalho,
idioma, prompt inicial, termos próprios — não tem como voltar.

**Prevenção** — antes de chamar qualquer coisa do programa dentro de um teste,
pergunte **onde aquilo escreve**. Neste projeto as portas para o disco são duas
(`config_dir()` e `data_dir()`, ambas em `src/config.rs`), e qualquer caminho que
chegue a uma delas num teste está escrevendo na máquina de alguém. Um teste verde
não é prova de que nada foi destruído — ele só olha o que você mandou ele olhar.

**Arquivos** — `src/controller.rs` (`Bancada`, `apply_draft`,
`apply_audio_settings`), `src/config.rs` (`salvar_em`, `config_path`).

## Testes — um `MutexGuard` nos argumentos de uma chamada trava o próprio teste

**Contexto** — um teste novo do controlador pendurava para sempre, sem falhar e
sem mensagem; o `cargo test` ficava rodando até o tempo limite.

**Sintoma** — nenhum. O teste simplesmente não termina.

**Causa** — isto:

```rust
b.controlador.on_stt(SttEvent::Done {
    ditado: b.estado().ditado_atual,   // <- o guard vive até o `;`
    ...
});
```

O `b.estado()` devolve um `MutexGuard`, e um temporário criado dentro de uma
expressão vive até o fim dela — ou seja, o mutex continua travado enquanto
`on_stt` roda, e a primeira coisa que `on_stt` faz é travá-lo.

**Solução** — tirar o valor antes da chamada:

```rust
let ditado = b.estado().ditado_atual;
b.controlador.on_stt(SttEvent::Done { ditado, ... });
```

**Prevenção** — nenhuma chamada que trave o estado compartilhado pode receber, em
qualquer argumento, algo que venha de `lock(&shared)`. O teste
`o_fim_da_gravacao_nao_atropela_a_tela_de_configuracoes` já traz o comentário; a
armadilha voltou de todo modo, o que é a razão desta entrada.

**Arquivos** — `src/controller.rs` (módulo de testes).

# Integrações de área de trabalho

## D-Bus/zbus — o Ditador some da barra e a extensão diz "Indisponível", com o programa rodando

**Contexto** — o Ditador estava de pé havia duas horas, ditando e transcrevendo
normalmente. A extensão do GNOME Shell mostrava "Indisponível" e **não havia
ícone nenhum** na barra — nem o da extensão, nem o StatusNotifierItem de reserva.
`ditador --status` respondia na hora, pelo socket, dizendo "modelo: pronto".

**Sintoma** — o nome bem-conhecido não tem dono, num processo que o pegou e
continua vivo:

```
$ gdbus call --session --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetNameOwner io.github.danielfreitasdev.Ditador
Erro: …NameHasNoOwner: Could not get owner of name '…Ditador': no such name
```

No journal, **nada**: a única linha sobre D-Bus continuava sendo a de sucesso,
escrita no arranque.

**Causa** — o padrão do zbus para as bandeiras do `RequestName`.
`BitFlags::<RequestNameFlags>::default()` vale
`AllowReplacement | ReplaceExisting | DoNotQueue`, e o `connection::Builder` usa
esse padrão quando ninguém diz nada. Quer dizer que o Ditador pedia o nome
**oferecendo-o** a quem viesse depois e **tomando-o** de quem já estava lá.

Uma segunda instância — a instância única só barra quem consegue tomar o socket
de controle, e o caminho `Bind::SemSocket` existe justamente para quando ele não
dá — roubava o nome da que estava rodando. Os dois processos escreviam a mesma
linha dizendo que a interface tinha subido. Quando o intruso saía, o
`DoNotQueue` impedia o legítimo de voltar para a fila, e o nome ficava **sem dono
nenhum** para sempre.

O ícone não voltar é a segunda metade, e é o que torna o defeito invisível: quem
recolhe o StatusNotifierItem é `Integracoes::gnome`, que é anotado pela vigília
do nome *da extensão* — e a extensão continuava lá. Então o Ditador seguia
achando que alguém mostrava o ícone por ele, enquanto esse alguém já não
conseguia enxergá-lo.

**Solução** — pedir o nome com as duas bandeiras desligadas
(`allow_name_replacements(false)`, `replace_existing_names(false)`). A segunda
instância passa a receber `Error::NameTaken` e registra "sem a interface D-Bus
(name already taken on the bus)", que é o degrau abaixo certo — e a primeira
nunca perde o nome.

Em separado, a vigília de cada integração agora **anota a ausência** quando o
fluxo de avisos acaba (`desistir_de_vigiar`). O fluxo só acaba com a conexão
morta, e daí em diante não há como saber quem está no ar: assumir "não há
integração" traz o ícone de volta e devolve a tela de gravação. Errar para esse
lado custa dois ícones; errar para o outro custa nenhum.

**Prevenção** — nunca aceitar o padrão do `RequestName` de uma biblioteca de
D-Bus sem olhar quais bandeiras ele traz. Para um programa de instância única a
resposta certa é sempre "quem chegou primeiro fica"; `ReplaceExisting` só faz
sentido para quem substitui um serviço de propósito, e `AllowReplacement` só para
quem quer ser substituído.

E o padrão **muda de biblioteca para biblioteca**, o que é o que torna a
armadilha traiçoeira: as outras duas integrações deste mesmo projeto acertam sem
pedir nada. O `Gio.bus_own_name` da extensão do GNOME usa
`BusNameOwnerFlags.NONE`, e o `registerService()` do widget do Plasma usa
`DontAllowReplacement | DontQueueService`. Só o zbus traz as duas bandeiras
perigosas no `Default`, e foi por isso que o defeito existiu só do lado Rust.

E, do lado do diagnóstico: uma conexão de barramento que morre **não avisa
ninguém**. Todo caminho que dependa dela precisa dizer no log quando falha —
o `publicar` reprovado era um `log::debug!`, invisível no filtro padrão.

**Arquivos** — `src/plataforma/linux/dbus.rs` (`PODE_SER_SUBSTITUIDO`,
`SUBSTITUIMOS_QUEM_JA_TEM`, `pedir_o_nome`, `desistir_de_vigiar`),
`src/plataforma/linux/tray.rs`, `src/state.rs` (`Integracoes`).
**Ambiente** — Linux, qualquer área de trabalho com barramento de sessão.
zbus 5.19.
**Comandos** — para reproduzir sem tocar na sessão de ninguém, o teste
`uma_segunda_instancia_nao_rouba_o_nome_da_que_ja_esta_no_ar` sobe o próprio
`dbus-daemon`. À mão:

```
gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetNameOwner io.github.danielfreitasdev.Ditador
```

# Áudio

## Áudio — a "Gravação máxima" das configurações não valia até reiniciar o programa

**Contexto** — modo **sempre aberto** (`microfone_sempre_aberto`, o padrão desde
a 0.7), em que o stream do cpal fica de pé entre os ditados.

**Sintoma** — arrastar o deslizante "Gravação máxima" de 120 s para 600 s,
salvar, e as gravações continuarem sendo cortadas em 120 s. Nada na tela nem no
log dizia por quê — e a linha do journal ao encerrar pelo teto dizia **600 s**,
porque ela lê a configuração nova enquanto o corte usava a antiga.

**Causa** — `AudioSettings::pede_reabertura` não inclui `max_secs`, de propósito:
reabrir o dispositivo por causa de um deslizante fecharia o microfone de quem só
arrastou um controle. Só que o teto era calculado uma vez, dentro de `abrir()`, e
guardado no `Captura`. No modo sob demanda isso não aparecia — o dispositivo é
reaberto a cada ditado —, e no modo sempre aberto `abrir()` acontece **uma vez
por execução do programa**.

O comentário do `pede_reabertura` chegava a afirmar que o teto "é lido a cada
gravação". Não era, e foi essa frase que fez o defeito passar despercebido em
duas leituras do arquivo.

**Solução** — `max_samples` virou `AtomicUsize` e ganhou o `ajustar_o_teto`, que
o `Configure` chama no dispositivo que ficou aberto. A conta saiu para
`teto_em_amostras`, usada pelos dois lugares.

No mesmo passo apareceu um vizinho: o `terminar()` leva a alocação do buffer
embora junto com as amostras (é o que evita copiar megabytes para entregá-las),
então do **segundo** ditado em diante o buffer nascia com capacidade zero e
crescia dentro do callback de áudio — um `realloc` com cópia de tudo em tempo
real, que é exatamente o que o `Vec::with_capacity` do construtor existia para
impedir. O `comecar()` agora reserva de novo, nesta thread, que pode esperar o
alocador.

**Prevenção** — valor de configuração guardado dentro de um recurso de vida longa
precisa de um caminho de atualização, ou de reabertura. E comentário que afirma
"é lido a cada X" merece um teste: aqui os dois modos de microfone divergiam
justamente nesse ponto.

**Arquivos** — `src/audio.rs` (`Captura::max_samples`, `ajustar_o_teto`,
`teto_em_amostras`, o braço `Configure` do `run`).
**Ambiente** — os dois sistemas; só aparece com `microfone_sempre_aberto`.

## VAD/testes — o detector de fala recusava o seno puro do teste, e estava certo

**Contexto** — ao escrever o `src/vad.rs` (aparar o silêncio das pontas antes de
transcrever), o teste que verifica o caso "fala do começo ao fim, sem silêncio de
onde tirar o piso de ruído" reprovou logo na primeira execução.

**Sintoma** —

```
thread 'vad::tests::a_fala_do_comeco_ao_fim_volta_inteira' panicked:
há fala aqui
```

O detector devolvia `None` (nenhuma fala) para um áudio que o teste montava com
um seno de 220 Hz do primeiro ao último quadro, em amplitude alta.

**Causa** — não era defeito do detector: era defeito do sinal de teste. Um dos
dois critérios absolutos do módulo é a **dinâmica** — a diferença entre o quadro
mais forte e o percentil 10 dos quadros. Ruído de máquina (chiado do microfone,
ventilador, zumbido de rede elétrica) tem pico e piso quase colados; fala tem
sílaba, sobe e desce umas quatro vezes por segundo, e abre facilmente 15 dB entre
um e outro. Um seno contínuo é, por essa régua, exatamente ruído de máquina — e
recusá-lo é a resposta certa.

**Solução** — o gerador de fala dos testes ganhou envelope silábico
(`0,15 + 0,85·|sen(2π·4t)|`), que é o que separa voz de tom constante. O vale não
chega a zero de propósito: nem entre duas sílabas a voz some por completo.

**Prevenção** — sinal sintético para testar detecção de voz precisa ter envelope.
Um seno puro não é fala e nenhum detector honesto vai dizer que é; quem "consertar"
o detector para aceitá-lo estará ensinando o programa a transcrever o zumbido do
ventilador. O mesmo vale para o ruído: use ruído de amplitude constante quando
quiser justamente o caso que **não** é fala.

**Arquivos** — `src/vad.rs` (a constante `DINAMICA_MINIMA_DB`, os auxiliares
`fala` e `chiado` do módulo de testes).

**Comandos** — `cargo test --no-default-features --features cpu vad::`

## Vulkan/NVIDIA — soltar o contexto do ggml com o programa vivo é seguro; no encerramento, não

**Contexto** — a funcionalidade de descarregar o modelo por ociosidade libera o
`WhisperContext` (e com ele os buffers da GPU) enquanto o programa continua de
pé, com a janela do egui aberta e o contexto gráfico do glow vivo. O `CLAUDE.md`
tem uma regra antiga que assusta exatamente aqui: **desmontar os buffers do
ggml/Vulkan dá SIGSEGV no driver da NVIDIA**, e é por isso que o encerramento do
programa pula os destrutores (`sair_sem_desmontar`, em `src/main.rs`).

**Sintoma** — nenhum, e essa é a informação. A pergunta era se a regra valia
para os dois casos ou só para um.

**Causa** — a regra vale para o **encerramento**, e o motivo está na segunda
metade dela: o SIGSEGV acontece quando a thread do Whisper libera buffers da GPU
*enquanto a thread principal desmonta o contexto gráfico*. São duas threads
mexendo no driver ao mesmo tempo. Fora do encerramento não há a segunda: a
thread principal está desenhando normalmente ou parada, e ninguém está
destruindo contexto nenhum. É o mesmo caminho que a **troca de modelo** já usava
desde sempre, e que nunca deu problema.

**Solução** — nada a fazer no código; o `Saida::Ocioso` libera o contexto
normalmente e o `Saida::Fim` continua usando `std::mem::forget`. O que faltava
era a verificação, e ela foi feita:

- o ensaio `stt::ensaio` inteiro rodando com `--features vulkan` e
  `use_gpu: GPU_CAPABLE` numa RTX 3060 — carrega, descarrega por ociosidade,
  recarrega e transcreve, cinco vezes seguidas, sem uma falha;
- e o aplicativo completo, com janela, bandeja e D-Bus no ar, em modo portátil
  com prazo de 1 minuto:

```text
13:31:46  modelo carregado (backend Vulkan, gpu=true)
13:32:46  modelo descarregado por ociosidade
13:33:13  modelo de volta (backend Vulkan, gpu=true)
13:33:17  transcrição 1: 4.3 s de áudio
```

O processo seguiu vivo e a GPU continuou respondendo.

**Prevenção** — não confunda os dois casos ao ler a regra do `CLAUDE.md`. "Não
libere buffers da GPU" **no encerramento** é regra; fora dele, liberar é o
comportamento normal e testado. Mexendo nisso de novo, o portão é o ensaio acima
com a feature `vulkan`, numa máquina com placa — a CI não tem GPU e nunca vai
reprovar isto por você.

**Arquivos** — `src/stt.rs` (`Saida::Ocioso` e `Saida::Fim`, `mod ensaio`),
`src/main.rs` (`sair_sem_desmontar`).

**Ambiente** — NVIDIA + Vulkan. Numa máquina sem GPU o caso não existe.

**Comandos** —

```bash
DITADOR_AUDIO_DE_TESTE=/tmp/jfk.wav \
  cargo test --release ensaio -- --ignored --test-threads=1
```

## Whisper — o que ele inventa não é silêncio digital, é **ruído de sala**

**Contexto** — ao ligar o aparo de silêncio (`src/vad.rs`), a primeira conferência
foi a óbvia: mandar três segundos de zeros ao modelo e ver o que ele responde.

**Sintoma** — nada. O texto saiu vazio **mesmo sem o aparo**, o que parecia
indicar que a funcionalidade nova não resolvia problema nenhum.

**Causa** — silêncio digital (amostras todas em zero) já era coberto pelas duas
defesas que o `transcribe` tinha: o `no_speech_probability > 0.85` e o
`is_non_speech_marker`. Só que silêncio digital **não é o caso de verdade**: um
microfone de verdade nunca entrega zeros. Ele entrega o piso dele — chiado, o
ventilador da máquina, o zumbido da rede elétrica —, e é diante *disso* que o
modelo inventa.

Repetido o ensaio com ruído a -55 dBFS (amplitude 0,004), quatro segundos, modelo
`small-q5_1`:

```
    ruído de sala SEM o aparo:  "ស\u{17d2}\u{17d2}\u{17d2}\u{17d2}"
    ruído de sala COM o aparo:  ""
```

Cinco caracteres de khmer. Não são marcador (não estão entre colchetes nem
parênteses), têm letra (então o filtro de "só pontuação" não pega) e a
probabilidade de silêncio do segmento ficou abaixo do corte — ou seja,
**atravessavam as duas defesas** e caíam na área de transferência de quem
esbarrou na tecla.

E, com fala presente, o ruído nas pontas muda a decodificação: a mesma frase
cercada de dois segundos de ruído de cada lado perdeu uma vírgula em relação à
frase nua ("And so my fellow Americans" contra "And so, my fellow Americans").

**Solução** — o `vad::achar_a_fala` corta as pontas antes de o áudio chegar ao
modelo, e descarta a gravação inteira quando não há fala.

**Prevenção** — ao testar qualquer coisa relacionada a "o modelo diante de
silêncio", **use ruído de sala, não zeros**. Zeros são um caso mais fácil que
qualquer defesa pega, e testar com eles produz a conclusão errada: a de que não
havia problema. O auxiliar `ensaio::ruido_de_sala`, em `src/stt.rs`, existe para
isso.

**Arquivos** — `src/vad.rs`, `src/stt.rs` (`transcribe` e o `mod ensaio`).

**Comandos** —

```bash
DITADOR_AUDIO_DE_TESTE=/tmp/jfk.wav \
  cargo test --release --no-default-features --features cpu ensaio \
  -- --ignored --nocapture
```

## Whisper/CPU — o modelo padrão transcreve mais devagar do que se fala, e o número

**Contexto** — a escolha de qual modelo sugerir a quem não tem GPU, ao montar o
`CATALOGO` do `src/modelo.rs`.

**Sintoma** — nada quebra: simplesmente ditar numa máquina sem GPU é uma
experiência ruim, e não havia número que dissesse **quanto** ruim nem qual modelo
resolveria.

**Causa** — o `large-v3-turbo-q5_0` tem 809 M de parâmetros. Ele é o padrão
porque com Vulkan roda a 42× o tempo real (medido no `Cargo.toml`); na CPU, o
mesmo modelo cai para menos de 1× — ou seja, a transcrição demora mais do que a
fala durou.

**Solução** — medido nesta máquina (Ryzen 5 4600G, 12 threads, binário
`--features cpu`), os mesmos 11,0 s de fala, três passadas cada:

```
    large-v3-turbo-q5_0    18,1 s     0,6× o tempo real
    small-q5_1              3,5 s     3,2× o tempo real
```

Daí o `modelo::PADRAO_CPU = "small-q5_1"`, que é o que a tela marca com estrela e
o `--baixar-modelo` sem argumento escolhe num binário só-CPU.

**Prevenção** — ao comparar modelos, o que interessa não é a razão entre eles e
sim **de que lado do 1× cada um cai**: abaixo disso o programa perde a razão de
existir naquela máquina. E meça com o mesmo áudio, na mesma máquina, na mesma
tarde — comparar com número lembrado de outra medição não vale.

**Arquivos** — `src/modelo.rs` (`PADRAO_CPU`, `CATALOGO`), `src/stt.rs` (o teste
`medicao::mede_o_backend`).

**Ambiente** — CPU. Com GPU a pergunta não se coloca.

**Comandos** — o arnês aceita escolher o modelo sem mexer na configuração de quem
roda o teste:

```bash
curl -sL -o /tmp/jfk.wav \
  https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav
DITADOR_AUDIO_DE_TESTE=/tmp/jfk.wav DITADOR_MODELO_DE_TESTE=small-q5_1 \
  cargo test --release --no-default-features --features cpu mede_o_backend \
  -- --ignored --nocapture
```

## Whisper/Vulkan/Intel — a GPU integrada é **mais lenta** que a CPU, e o padrão do `.deb` a usa

**Contexto** — Dell OptiPlex Micro Plus 7010, i7-13700T (8 P + 8 E, 24 threads,
35 W de PL1) com UHD 770 e sem placa dedicada, Kubuntu 24.04, Mesa 25.2.8. O
`.deb` de GPU instalado e `use_gpu: true`, que é o padrão dele.

**Sintoma** — ditar era lento sem nada estar quebrado: 0,9 s de fala viravam
texto em 9,7 s, 1,9 s em 13,5 s. Nenhum erro no journal, o `--diagnostico` dizia
"Tudo o que o Ditador precisa está no lugar", e a linha de carga confirmava
`backend Vulkan, gpu=true` — que é justamente o que fazia parecer que estava
tudo certo.

**Causa** — a UHD 770 não tem com que rodar o ggml depressa, e o próprio
ggml-vulkan diz isso na linha de descoberta do dispositivo:

```
ggml_vulkan: 0 = Intel(R) Graphics (RPL-S) (Intel open-source Mesa driver)
  | uma: 1 | fp16: 1 | bf16: 0 | warp size: 32 | shared memory: 65536
  | int dot: 0 | matrix cores: none
```

`int dot: 0` e `matrix cores: none` querem dizer que os dois caminhos rápidos do
ggml-vulkan não existem ali. Medido com o `whisper-bench` do whisper.cpp 1.8.3,
o mesmo `large-v3-turbo-q5_0`, o mesmo encoder de 30 s:

```
    Vulkan (UHD 770)     36,1 s
    CPU (16 threads)     15,2 s
```

Ou seja: a GPU integrada custa **2,4× o tempo da CPU**. Não é defeito de
configuração nem de driver — é a peça. Some a isso que a iGPU divide os mesmos
35 W e a mesma memória com a CPU, e não sobra nada a favor dela.

**Solução** — `use_gpu: false`. O binário continua sendo o do `.deb` de Vulkan;
o que muda é ele não pedir o dispositivo, e a linha de carga passa a dizer
`(backend Vulkan, gpu=false)`.

**Prevenção** — "tem GPU" não é a pergunta certa; a pergunta é **qual**. Antes de
recomendar o `.deb` de GPU para uma máquina de vídeo integrado, olhe a linha
`ggml_vulkan:` do log: sem `int dot` e sem `matrix cores`, a CPU ganha. E note
que o rótulo do `--diagnostico` (`backend Vulkan`) é o do **binário**, não o do
que está em uso — quem responde isso é a linha `gpu=` do `ditador::stt`.

**Arquivos** — `src/stt.rs` (`GPU_CAPABLE`, `BACKEND`, `params.use_gpu`),
`src/config.rs` (`use_gpu`).

**Ambiente** — Linux, vídeo integrado Intel (Xe-LP / UHD 7xx). Placa dedicada é
outro caso: numa RTX 3060 o Vulkan roda a 42× o tempo real.

**Comandos** — o que revela o dispositivo e o que ele tem:

```bash
RUST_LOG=debug ditador 2>&1 | grep -E "ggml_vulkan|using .* backend|gpu="
```

## Whisper — o encoder cobra 30 s por qualquer frase, e quem corta isso é o `audio_ctx`

**Contexto** — procurando por que ditar duas palavras custava o mesmo que ditar
meio minuto, numa máquina que transcreve na CPU.

**Sintoma** — o tempo de transcrição praticamente não depende do tamanho da
fala. No journal: 0,9 s de áudio em 9,7 s; 1,9 s em 13,5 s. E o `whisper-bench`,
que só roda o encoder, dava o mesmo número dos ditados de verdade.

**Causa** — o encoder do Whisper trabalha numa janela fixa de 1500 quadros, que
são os 30 s do modelo. O que falta é preenchido com silêncio e **computado do
mesmo jeito**. Não há redução automática para áudio curto no whisper.cpp 1.8.3:
`exp_n_audio_ctx` só sai de zero quando alguém escreve nele. O `whisper-rs` 0.16
expõe isso como `FullParams::set_audio_ctx`, e o Ditador não o chamava.

**Solução** — `janela_do_encoder`, em `src/stt.rs`: a janela passou a acompanhar
o tamanho do áudio que o modelo vai receber, com o dobro de folga e um piso de
768 quadros. Medido nesta máquina, `large-v3-turbo-q5_0` na CPU com 16 threads,
os mesmos 7,0 s de fala em português, pelo `medicao::mede_o_backend`:

```
    janela cheia (1500)     15,1 s     (14,2 · 15,1 · 16,1 · 15,2, só o encoder)
    janela adaptativa (768)  6,5 s     (6,37 · 7,45 · 6,75 · 6,36)
```

De ponta a ponta, pelo `medicao::mede_o_backend` — que é o `whisper_full`
inteiro, com reamostragem e normalização —, o mesmo áudio passou a sair em
**5,84 s** (5,84 · 5,86 · 5,84), com o texto idêntico ao da janela cheia. E há
um segundo ganho de graça: o aparo do silêncio, que já existia, agora encurta a
janela junto com o áudio.

⚠️ **Meça intercalando as duas configurações, não uma bateria de cada.** Este
i7-13700T tem PL1 de 35 W e PL2 de 106 W: partindo frio, o mesmo encoder de
janela cheia mediu 9,7 s; morno, 15 s; depois de uma bateria seguida, 31,7 s.
São 3× de diferença sem nada ter mudado no programa, e é exatamente a armadilha
que faz uma medição isolada "provar" o que se quiser.

**Prevenção** — **encurtar demais não degrada o texto, faz o modelo repetir**, que
num programa de ditado é bem pior do que demorar. Foi isso que fixou a folga em
dois e o piso em 768; os números que a sustentam, com o mesmo modelo:

```
     fala    janela   texto
      7 s      512    certo, igual ao da janela cheia
      5 s      512    certo — e este é o caso apertado: fala cortada no
                      meio de uma palavra, que é o que acontece quando
                      alguém solta o atalho cedo (margem de 2,05×)
      5 s      384    "Ask not. Ask not. Ask not." (margem de 1,54×)
     18 s      900    certo
     18 s      768    "para daniel.empresa.empresa. Daniel.empresa."
     33 s      900    "O Rafael ficou respiro." três vezes
```

Quem for baixar a `FOLGA` precisa refazer esta medição — inclusive o caso da
fala cortada no meio, que é o que reprova antes dos outros. Há teste trancando
isso (`a_janela_cobre_o_dobro_do_que_a_pessoa_falou`).

**Arquivos** — `src/stt.rs` (`janela_do_encoder`, `JANELA_CHEIA`,
`JANELA_MINIMA`, `FOLGA`, `QUADROS_POR_SEGUNDO`).

**Ambiente** — vale nos dois sistemas e nos três backends; só se **nota** onde o
encoder é o custo do ditado, que é a CPU.

**Comandos** — a varredura que produziu a tabela, com o `whisper-cli` do
whisper.cpp (o `-ac` é o mesmo `audio_ctx`):

```bash
for ac in 0 900 768 640 512; do
  whisper-cli -m ggml-large-v3-turbo-q5_0.bin -f fala.wav \
    -l pt -bo 1 -bs 1 -mc 0 -nt -t 16 -ac $ac
done
```


# Histórico

## Histórico — a entrada mostrava o áudio de outra frase

**Contexto** — histórico com "Guardar também o áudio" ligado, ditando duas vezes
seguidas — o uso normal deste programa, que aceita falar de novo enquanto a frase
anterior é transcrita.

**Sintoma** — duas entradas do `historico.jsonl` com o mesmo valor no campo
`audio`, e um WAV só no disco. Quem abrisse o áudio da primeira ouvia a segunda.

**Causa** — o nome do arquivo era `{instante}-{pid}.wav`. O instante tem
resolução de **segundo** e o `pid` é o mesmo dentro de um processo: duas
transcrições que terminassem no mesmo segundo produziam o mesmo nome, e a segunda
gravava por cima da primeira. O comentário no código dizia que o nome cobria
"dois ditados no mesmo segundo" — cobria dois *Ditadores*, que é outra coisa.

**Solução** — um contador atômico do processo entra no nome
(`{instante}-{pid}-{n}.wav`), que é a mesma receita que o `config.rs` já usa para
o arquivo temporário do `salvar_em`.

**Prevenção** — nome de arquivo montado com relógio precisa de um desempate que
não dependa do relógio. E o teste de um nome único confere **o conteúdo**, não só
que os nomes diferem: nomes distintos com o conteúdo trocado seria o mesmo
defeito.

**Arquivos** — `src/historico.rs` (`registrar_em`).

## Histórico — a entrada ficava sem duração e sem áudio quando um toque na tecla vinha logo depois

**Contexto** — falar de novo enquanto a frase anterior é transcrita, que é o uso
normal deste programa. Aqui com um detalhe a mais: o segundo "ditado" é um toque
sem querer, curto demais para valer.

**Sintoma** — a frase de verdade aparecia na lista com `"duracao_ms": 0` e sem o
campo `audio`, mesmo com "Guardar também o áudio" ligado. Nada no log dizia por
quê, e repetindo o ditado sozinho tudo funcionava.

**Causa** — o controlador guarda **um** ditado por vez em `para_o_historico`, à
espera de a transcrição terminar: quem chega depois toma o lugar de quem estava
lá. Entre dois ditados de verdade essa é a decisão certa, e está documentada — a
alternativa seria uma fila de buffers de megabytes sem teto.

O que estava errado era **a ordem**: o `AudioEvent::Captured` guardava antes de
conferir a duração mínima. Um ditado descartado por ser curto demais nunca chega
a ser transcrito, então ele tomava a vaga e não a usava para nada — e ainda
pagava a cópia das amostras, de megabytes, para um áudio que ia direto para o
lixo.

**Solução** — guardar **depois** do descarte por duração mínima, no mesmo
`Captured`. Duas linhas trocadas de lugar.

**Prevenção** — a regra geral: um recurso que guarda "o último X" tem de guardar
depois de saber que aquele X vai existir, e não no instante em que ele aparece.
Há teste
(`um_ditado_curto_demais_nao_rouba_o_historico_do_que_ainda_esta_sendo_transcrito`).

**Ambiente** — com o microfone sempre aberto (o padrão desde a 0.7) e a duração
mínima nos 300 ms de fábrica isto quase não acontece: os 300 ms de pré-gravação
entram na conta da duração e praticamente nenhum toque fica abaixo do piso. Para
reproduzir, suba a "Gravação mínima" nas configurações.

**Arquivos** — `src/controller.rs` (`on_audio`, braço `Captured`).

# Empacotamento e distribuição

## Modelo — o `./baixar-modelo.sh` reprovava **todo** download bem-sucedido, e apagava o arquivo

**Contexto** — `./baixar-modelo.sh`, o caminho que o README manda usar para
baixar o modelo sem sessão gráfica (por SSH, num servidor, numa instalação
scriptada).

**Sintoma** — o download anda até o fim, os 574 MB chegam, e então:

```
Erro: o arquivo baixado não é um modelo do Whisper.
      A rede pode ter devolvido uma página no lugar dele.
```

O `trap … EXIT` do script apaga o `.parcial` em seguida, então não sobra nem o
arquivo para conferir. Baixando o mesmo endereço à mão com `curl`, o arquivo é
perfeito e o Ditador o carrega sem reclamar — o que joga a suspeita na rede, que
é justamente o que a mensagem sugere e o lugar errado de procurar.

**Causa** — a conferência da assinatura comparava com a **string** `"ggml"`:

```sh
if [ "$(head -c 4 "$PARCIAL")" != "ggml" ]; then
```

O whisper.cpp grava a assinatura como o **inteiro** `0x67676d6c`, na ordem
nativa da máquina. Em x86 e ARM isso é little-endian, então no disco os quatro
bytes saem invertidos — `6c 6d 67 67` —, que lidos como texto dão `lmgg`. A
comparação nunca podia dar certo: o script reprovava cem por cento dos downloads
bons e aprovava zero.

E o mais instrutivo: **este erro já tinha sido encontrado e corrigido**, do lado
Rust, em `src/modelo.rs` — o comentário lá conta a história inteira, inclusive
que ele passou despercebido por tanto tempo porque só roda no fim de um download
e quem programa já tem o modelo no disco. A correção não atravessou para o
script, que tem a mesma conferência escrita à mão e ninguém releu.

**Solução** — comparar em hexadecimal, que não depende de a assinatura ser texto
imprimível nem do locale de quem roda:

```sh
if [ "$(head -c 4 "$PARCIAL" | od -An -tx1 | tr -d ' \n')" != "6c6d6767" ]; then
```

**Prevenção** — duas, e a segunda é a que importa:

* a ordem certa está escrita numa constante de teste
  (`COMECO_DE_UM_MODELO`, em `src/modelo.rs`), conferida contra o próprio
  servidor da Hugging Face com `Range: 0-15` e **não** deduzida da constante do
  código, que é onde o erro morava;
* há um teste em Rust que **lê a linha do script** e a compara com essa
  constante (`o_script_confere_a_assinatura_na_ordem_em_que_ela_esta_no_disco`).
  O projeto já fazia isso para a lista de modelos do script; agora faz para a
  assinatura também. Foi o único jeito de os dois lados pararem de se separar em
  silêncio — o `cargo test` não sabia que aquele arquivo existia.

A regra geral: **conferência duplicada em duas linguagens precisa de um teste
que leia as duas.** Comentário dizendo "igual ao do outro lado" não é conferência.

**Comandos** — a pergunta, para qualquer modelo no disco:

```
head -c 4 modelo.bin | od -An -tx1     # 6c 6d 67 67
```

**Arquivos** — `baixar-modelo.sh`, `src/modelo.rs` (`parece_um_modelo`).

## Modelo — `ditador --baixar-modelo` respondia "já está aqui" para um arquivo que não presta

**Contexto** — o modelo no destino existe mas está quebrado: a página de um
portal cativo gravada com status 200, um download interrompido pelo disco cheio,
uma cópia truncada de outra máquina.

**Sintoma** — o Whisper recusa carregar, a janela mostra "Não consegui carregar
o modelo", e no terminal:

```
$ ditador --baixar-modelo
O modelo já está aqui: ~/.local/share/ditador/models/ggml-large-v3-turbo-q5_0.bin
```

Sem saída nenhuma pela linha de comando. A pessoa precisa descobrir sozinha que
tem de apagar o arquivo à mão.

**Causa** — a decisão era `destino.exists()`, e só. A janela já tinha o conserto
para isto — o `oferta_de_download` reconhece o "arquivo ruim" (modelo em
`Failed` + arquivo presente) e oferece "Baixar o modelo de novo" —, mas o
terminal ficou com a conferência velha. Quem está numa sessão por SSH, que é
justamente quem usa este comando, ficava sem saída.

**Solução** — `modelo::parece_um_modelo(&destino)`: quatro bytes lidos do começo
do arquivo, a mesma assinatura que o `conferir` do fim do download já
verificava, agora numa função pública e com uma cópia só. Não passando, o
comando avisa e baixa por cima (o `rename` do fim substitui o destino).

**Prevenção** — a conferência é de quatro bytes de propósito: ela roda em todo
`--baixar-modelo`, e ler os 574 MB para somar SHA-256 a cada execução seria
pagar caro por um caso raro. O arquivo trocado no meio continua sendo pego pela
soma, no fim do download, e pelo botão da janela.

**Arquivos** — `src/main.rs` (`baixar_modelo`), `src/modelo.rs`
(`parece_um_modelo`).

## Instalação — `./instalar.sh` deixava o Ditador **parado** em quem já o usava

**Contexto** — reinstalar (`./instalar.sh`) com o serviço de usuário rodando, que
é o que qualquer pessoa faz ao atualizar uma cópia compilada à mão.

**Sintoma** — o script termina dizendo "Instalado.", e a partir dali o ícone some
da barra, o atalho global não faz mais nada e `ditador --status` responde
"parado". Nada volta até `systemctl --user start ditador` ou o próximo login. O
`journalctl` não tem erro nenhum: a unidade simplesmente está inativa.

**Causa** — o script chama `ditador --encerrar` antes de sobrescrever o binário
(não dá para trocar um executável em uso). O encerramento pelo canal de controle
é uma saída **limpa**, código zero — e a unidade é `Restart=on-failure`. Para o
systemd o programa terminou de propósito, então ele não sobe de novo. Quem
parou tem de religar.

Este mesmo mecanismo já estava resolvido no `.deb`: o `prerm` deixa um bilhete em
`/run/ditador.estava-ativo` e o `postinst` religa. O `instalar.sh` tinha o mesmo
problema e nenhuma das duas metades.

**Solução** — perguntar `systemctl --user is-active --quiet ditador` **antes** do
`--encerrar`, guardar a resposta, e dar `restart` no fim (depois do
`daemon-reload`, porque o arquivo da unidade acabou de ser reescrito). Quem não
tinha o serviço de pé continua sem ele.

**Prevenção** — todo caminho que encerra o Ditador para mexer nos arquivos dele
precisa lembrar de religá-lo. `Restart=on-failure` não cobre saída limpa, e é
assim de propósito: `Restart=always` faria o `--encerrar` do usuário não valer
nada.

**Arquivos** — `instalar.sh`, `assets/ditador.service`, `empacotar.sh` (o
`prerm`/`postinst`, para comparar).

## Empacotamento — o `.deb` saía com um arquivo gravável pelo grupo, conforme a umask de quem empacotou

**Contexto** — `./empacotar.sh` rodado numa máquina com `umask 002`, que é o
padrão do Ubuntu para contas com grupo próprio.

**Sintoma** — dentro do pacote, `usr/share/doc/<pacote>/changelog.Debian.gz` com
permissão `664` enquanto todo o resto ia `644`. Não quebra a instalação; é o tipo
de coisa que o `lintian` acusa (`non-standard-file-perm`) e que muda conforme a
máquina — num agente do GitHub, com `umask 022`, o mesmo script produz `644` e o
problema não existe.

**Causa** — quase todos os arquivos entram na árvore por `install -Dm644`, que
grava o modo explicitamente. O changelog não: ele é gerado por um `printf … |
gzip > arquivo`, e um `>` do shell cria o arquivo com `666` menos a umask. O
`copyright`, que nasce do mesmo jeito, já tinha um `chmod 644` logo abaixo — a
linha existia por este exato motivo e não foi repetida para o vizinho.

**Solução** — o `chmod 644` que faltava, mais uma **conferência do resultado**
antes do `dpkg-deb`: nada dentro da árvore pode ser gravável por grupo ou por
outros (`find "$RAIZ" -perm /022`). É a mesma ideia da conferência do `objdump`
que já existia ali — olhar o pacote pronto, e não confiar em ter feito tudo
certo pelo caminho.

**Prevenção** — modo de arquivo que sai de um redirecionamento do shell depende
da umask de quem roda; modo que sai de `install -m` não. Numa árvore que vira
pacote, tudo o que nasce por `>` precisa de `chmod` explícito — e a rede que
confere o conjunto é mais barata do que lembrar disso.

**Comandos** — a pergunta, para qualquer `.deb`:

```
dpkg-deb -c pacote.deb | grep -E '^.{2}.{3}w|^.{2}.{6}w'
```

**Arquivos** — `empacotar.sh`.

## Empacotamento — o `.deb` publicado morre com `Illegal instruction` ao carregar o modelo

**Contexto** — o `.deb` da 0.7.1, recém-publicado pelo workflow, baixado e
rodado num AMD Ryzen 5 4600G (Zen 2: tem AVX2, não tem AVX-512). O da 0.7.0,
publicado pelo mesmo workflow semanas antes, rodava na mesma máquina.

**Sintoma** — o programa sobe, registra os teclados, publica o ícone da barra e
some. O log termina na linha do microfone e **nunca** chega em "modelo
carregado". Quem o lançou de um terminal vê:

```
Illegal instruction     (core dumped)
```

O `--versao`, o `--microfones` e o `--diagnostico` funcionam — nenhum deles
carrega o modelo, e é por isso que uma conferência rápida diz que está tudo bem.

**Causa** — `-march=native` no ggml. O `GGML_NATIVE` do whisper.cpp vem **ligado
por padrão** (`GGML_NATIVE_DEFAULT` é ON fora de compilação cruzada), e ligado
ele acrescenta `-march=native`: o binário sai com as instruções do processador
que o compilou. Quem compila é um agente do GitHub, e qual máquina a Azure
empresta muda de execução para execução.

Medido nos dois pacotes, com `objdump -d … | grep -c '%zmm'`:

| | registradores `%zmm` (AVX-512) | roda no Zen 2 |
|---|---|---|
| `.deb` da 0.7.0 | 0 | sim |
| `.deb` da 0.7.1 | 7 231 | **não** |

Nada no repositório tinha mudado entre um e outro. O pacote funcionar ou não era
sorteio, e a mesma versão podia estar boa para umas máquinas e quebrada para
outras.

**Solução** — `GGML_NATIVE = "OFF"` no `[env]` do `.cargo/config.toml`, que vale
para toda compilação feita da raiz do repositório. O whisper-rs-sys repassa ao
CMake qualquer variável de ambiente que comece com `GGML_`, `WHISPER_` ou
`CMAKE_`.

Com ele o `-march=native` sai e as opções de conjunto de instruções passam a
valer por si — no binário recompilado aqui, nenhuma piora onde importa:

```
%zmm (AVX-512)   0        (era o que matava)
%ymm (AVX2)      14 493   (os kernels que importam continuam lá)
vfmadd (FMA)     843
vcvtph2ps (F16C) 49
```

O piso passa a ser AVX2, que existe em todo Intel desde 2013 (Haswell) e em todo
AMD Zen.

**Prevenção** — a variável é a correção; a rede é o `empacotar.sh`, que agora
**olha o binário pronto** antes de fechar o `.deb` e reprova se achar `%zmm`.
Ela continua valendo se alguém apagar a linha do `.cargo/config.toml`, se o
padrão do whisper.cpp mudar de novo ou se o `-march` entrar por um caminho que
ninguém previu — porque ela não confere a intenção, confere o resultado.

Duas armadilhas de método, que valem mais do que a correção em si:

* **uma variável de ambiente nova não recompila o C++ sozinha.** O
  `whisper-rs-sys` não declara `cargo:rerun-if-env-changed=GGML_NATIVE`, então o
  `cargo build` seguinte recompila só o nosso crate e o binário continua o
  antigo. Para conferir de verdade: `cargo clean -p whisper-rs-sys` antes. Foi
  o que quase fez esta correção passar por conferida sem estar;
* **o portão precisa ser exercido nos dois sentidos.** O `empacotar.sh` foi
  rodado com o binário bom (aprova) e com o binário quebrado da 0.7.1 no lugar
  (reprova, com código 1). Um portão que só se viu aprovar não é um portão.

**Arquivos** — `.cargo/config.toml`, `empacotar.sh` (o passo "Conferindo se o
binário roda fora desta máquina"), `.github/workflows/release.yml` (o `binutils`
das dependências).
**Ambiente** — vale para os dois sistemas: o instalador `.exe` do Windows sai do
mesmo `cargo build` e tinha o mesmo padrão. Só o `.deb` foi medido.
**Comandos** — a pergunta inteira cabe numa linha, e serve para qualquer binário
baixado de uma release:

```
objdump -d ./ditador | grep -c '%zmm'    # 0 = roda em qualquer x86-64 com AVX2
```

