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
