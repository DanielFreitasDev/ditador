# A CI e as versões

Como o projeto é conferido a cada push e como uma versão nova chega às mãos de
quem usa. Este documento é para quem vai mexer no projeto — as instruções de
instalação, que são para quem vai usá-lo, estão em
[`INSTALACAO.md`](INSTALACAO.md).

São **dois** workflows, e não mais:

| Arquivo | Quando roda | O que faz |
|---|---|---|
| [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | todo push, todo PR | confere os quatro lados do projeto |
| [`.github/workflows/release.yml`](../.github/workflows/release.yml) | à mão, no botão | valida, numera, marca a tag, empacota e publica |

O segundo **chama** o primeiro (`workflow_call`). Não existem duas listas de
conferências: existe a do `ci.yml`, e uma publicação passa por ela inteira antes
de qualquer coisa ser gravada.

---

## O portão (`ci.yml`)

Quatro estágios, encadeados nesta ordem:

```
1. Rust ──→ 2. Windows ──→ 3. GNOME ──→ 4. KDE
```

| # | Trabalho | Onde roda | O que confere |
|---|---|---|---|
| 1 | `Rust · Linux` e `Rust · Windows` | ubuntu-latest, windows-latest | `cargo fmt --check`, `cargo test`, `cargo clippy`, `cargo build --release` (feature `cpu`) |
| 1 | `Rust · Linux · Vulkan` | ubuntu-latest | compila com o backend que o `.deb` leva |
| 1 | `Rust · Auditoria` | ubuntu-latest | `cargo audit` |
| 2 | `Windows · Frontend WinUI` | windows-latest | `dotnet build` e `dotnet test`; o script do Inno Setup compila |
| 3 | `GNOME · Extensão do Shell` | ubuntu-latest | ESLint, schemas do GSettings, `gnome-extensions pack` |
| 4 | `KDE Plasma · Widget e plugin` | contêiner `ubuntu:26.04` | compila o plugin C++, `qmllint` no QML, valida o `metadata.json` |

Só no estágio 1 e só no Linux, porque conferem arquivo e não código:
`xmllint` no `dbus/contrato.xml` (um DOCTYPE quebrado ali derruba a compilação
do plugin do Plasma, três estágios adiante) e `versao.sh conferir`.

**Por que encadeado, e não em paralelo.** Rodando em paralelo, um erro de
digitação no Rust reprova os quatro ao mesmo tempo e a página de resultados vira
uma parede vermelha em que não se sabe por onde começar. Encadeado, o primeiro
que quebra é o que interessa, e os de baixo nem gastam agente. O preço é relógio
de parede, e quem o paga é o cache: o whisper.cpp, que é o que demora, só
recompila quando muda de verdade. Para inverter isso um dia, é trocar os
`needs:` — o resto continua igual.

**Por que o KDE roda num contêiner.** O `CMakeLists.txt` do plugin exige Qt 6.6
e ECM 6.0; o `ubuntu-latest` do GitHub ainda é o 24.04, que traz Qt 6.4. O
`ubuntu:26.04` é o alvo declarado do widget (Qt 6.10, KF6 6.24, Plasma 6.6) — a
régua roda no sistema para o qual o código foi escrito. É de lá que sai o
`plasma-desktoptheme`, o pacote que traz os módulos QML `org.kde.plasma.*` sem
os quais o `qmllint` não resolveria import nenhum e passaria a reclamar de tudo,
que é o mesmo que não conferir nada.

### O que a CI **não** confere

Continua valendo o de sempre: nada que precise de GPU, microfone, sessão gráfica
ou barramento de sessão. Verde aqui não substitui nenhum destes, que são da
máquina de quem mexe:

```bash
./gnome-extension/scripts/testar.sh    # ciclo de vida num GNOME Shell aninhado
gjs -m gnome-extension/scripts/teste-do-backend.js   # a extensão contra o Ditador vivo
./kde-plasma/testar.sh                 # plasmawindowed, kpackagetool6 e o plugin no barramento
cargo test -- --ignored                # a medição de backends (mede_o_backend)
```

Do frontend WinUI, só a leitura do protocolo do canal de controle é testada;
janela, ícone, menu e posição seguem nos roteiros manuais do
`windows-integration/README.md`.

---

## Publicar uma versão

**Actions → "Publicar versão" → Run workflow.** Duas perguntas:

- **incremento**: `auto` (o normal), ou `patch`/`minor`/`major` para mandar no
  número à mão.
- **rascunho**: publica sem deixar visível, para você revisar antes.

E é isso. O que acontece a partir do botão:

```
validar    o ci.yml inteiro (Rust → Windows → GNOME → KDE)
   ↓       reprovou? acaba aqui. Nada foi gravado, nenhuma tag existe.
versao     decide o número, grava nos arquivos, atualiza o CHANGELOG.md,
   ↓       commita "Versão X.Y.Z" no main e cria a tag vX.Y.Z
artefatos  da tag: os dois .deb, os dois instaladores .exe, o ZIP da extensão
   ↓
publicar   SHA256SUMS, notas, e a release no GitHub com tudo anexado
```

A validação vem **antes** de o número existir, e é isso que faz não haver versão
pela metade para desfazer: se o portão reprovar, o `main` não foi tocado.

### Por que o disparo é manual

Publicar a cada push no `main` transformaria todo commit numa versão — e versão
é uma promessa a quem instalou: "vale a pena baixar isto". Quem decide que um
conjunto de mudanças virou uma versão é gente.

O que **não** é decisão de gente, e por isso está automatizado, é o resto: qual
número vem a seguir, se a tag foi criada, se o `.deb` saiu dos dois jeitos, se a
release ganhou instruções. Era exatamente aí que este projeto errava — o
repositório chegou à 0.4.2 tendo `v0.2.0` como única tag, e o único release
publicado ainda mostrava uma interface que o README já dizia ter removido.

---

## O número da versão

### Como ele é decidido

Pelo trailer `Impacto:` dos commits desde a última tag. O maior deles ganha:

| Trailer | Sobe | Quando usar |
|---|---|---|
| `Impacto: correção` | PATCH | conserto, ajuste, texto, documentação |
| `Impacto: funcionalidade` | MINOR | coisa nova que não quebra quem já usa |
| `Impacto: incompatível` | MAJOR | quebra quem já usa (veja a ressalva do 0.x) |

Exemplo de commit completo:

```
Mais ar embaixo das fileiras de botões

O corpo em prosa, explicando causa e raciocínio, como sempre.

Impacto: funcionalidade
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

**Sem o trailer, o commit vale PATCH.** É o padrão certo aqui: a maioria dos
commits deste projeto é conserto, e esquecer o trailer não pode publicar uma
versão que promete mais do que mudou. O caminho do erro aponta para baixo.

**Por que um trailer, e não `feat:`/`fix:` no assunto.** O CLAUDE.md diz, em
tantas palavras, que o assunto é em português, sentence case, sem prefixo e sem
conventional commits — ele descreve o efeito da mudança, não a categoria dela.
Trailer é o que o projeto **já** usa em todo commit (o `Co-Authored-By:`), então
a categoria entra por ali sem desfazer nenhuma regra e sem mudar como as
mensagens se leem.

### A ressalva do 0.x

Enquanto o MAJOR for 0, um `Impacto: incompatível` sobe o **MINOR** (0.5.0 →
0.6.0). É o que a própria SemVer manda para a linha 0.x: "qualquer coisa pode
mudar a qualquer momento; a API pública não deve ser considerada estável".
Chegar ao 1.0.0 é uma decisão, e não o efeito colateral do primeiro commit que
renomeou alguma coisa — quem quiser dispara o workflow com `incremento: major`,
que ignora a regra.

### Onde a versão mora

O `Cargo.toml` é a única fonte da verdade. Os outros arquivos são cópias que
precisam concordar com ele, e há um teste na CI para isso:

| Arquivo | O que guarda |
|---|---|
| `Cargo.toml` | **a** versão |
| `Cargo.lock` | a mesma, no bloco do próprio pacote (é versionado) |
| `gnome-extension/metadata.json` | `version-name`, que é o que aparece em Extensões |
| `kde-plasma/…/metadata.json` | `0.0.0` **de propósito** — quem o preenche na instalação é o `instalar.sh`, lendo o Cargo.toml |

Quem mantém isso em dia é o [`.github/scripts/versao.sh`](../.github/scripts/versao.sh),
que roda igual na sua máquina:

```bash
.github/scripts/versao.sh atual       # 0.5.0
.github/scripts/versao.sh impacto     # correcao | funcionalidade | incompativel
.github/scripts/versao.sh proxima     # qual seria a próxima, sem gravar nada
.github/scripts/versao.sh conferir    # os arquivos concordam? (é o que a CI chama)
.github/scripts/versao.sh gravar 0.6.0
```

---

## Os anexos da release

| Anexo | Sai de |
|---|---|
| `ditador-vX.Y.Z-linux-amd64-gpu.deb` | `./empacotar.sh vulkan` |
| `ditador-vX.Y.Z-linux-amd64-cpu.deb` | `./empacotar.sh cpu` |
| `ditador-vX.Y.Z-windows-x64-gpu.exe` | `empacotar-exe.ps1 -Backend vulkan` |
| `ditador-vX.Y.Z-windows-x64-cpu.exe` | `empacotar-exe.ps1 -Backend cpu` |
| `ditador-gnome-extension-vX.Y.Z.zip` | `gnome-extensions pack` |
| `SHA256SUMS` | `sha256sum` de todos os acima |
| `Source code (zip)` e `(tar.gz)` | o GitHub, sozinho, a partir da tag |

Nenhum desses comandos é exclusivo da CI: **o workflow chama os mesmos scripts
que você chama**, e por isso dá para reproduzir qualquer anexo na sua máquina
antes de publicar. A única coisa que o workflow faz por fora é **renomear** os
`.deb`: o `empacotar.sh` os nomeia à moda Debian (`ditador_0.6.0_amd64.deb`),
que é o certo para um pacote, e o nome do anexo é outra coisa — ele precisa
dizer, na página de download, a versão, o sistema, a arquitetura e a variante.

O que **não** sai pronto, e por quê:

- **O widget do Plasma.** Tem uma metade em C++ que precisa ser compilada contra
  o Qt da máquina de destino. Vai pelo código-fonte.
- **O `.deb` com CUDA.** Exige o toolkit da NVIDIA para compilar, que não cabe
  num agente. Quem quer, compila (`./instalar.sh cuda`).
- **O modelo do Whisper.** São 574 MB que não mudam entre versões; o programa o
  baixa sozinho.
- **O MSIX.** O `empacotar-msix.ps1` continua sendo protótipo local: sem um
  certificado em que o Windows confie, o pacote não instala. O caminho de
  distribuição no Windows é o `.exe`.

### O instalador do Windows

É Inno Setup, e o script está em
[`windows-integration/instalador/ditador.iss`](../windows-integration/instalador/ditador.iss).
Ele instala em `%LOCALAPPDATA%\Programs\Ditador` sem pedir administrador, faz o
atalho do menu Iniciar, oferece a chave `Run`, confere (e baixa, se faltarem) o
.NET 10 Desktop Runtime e o Windows App Runtime, e deixa um desinstalador que
pergunta se deve levar junto a configuração e os modelos.

O que ele faz é **o mesmo** que o `instalar.ps1`, de propósito: mesma pasta,
mesmo atalho, mesma chave, mesmas dependências. Instalar por um e desinstalar
pelo outro funciona. A diferença é o público — o `.ps1` é para quem tem o
código-fonte e a caixa de ferramentas; o `.exe` é para quem só quer usar.

Na sua máquina:

```powershell
.\windows-integration\scripts\empacotar-exe.ps1 -Backend cpu
```

O `AppId` (o GUID no topo do `.iss`) é a identidade da instalação: é por ele que
o Windows sabe que a versão nova substitui a velha. **Não mude esse GUID** —
mudá-lo faria cada versão aparecer como um programa diferente na lista de
aplicativos instalados.

---

## As notas da release

O corpo de cada release tem três partes, montadas pelo
[`.github/scripts/notas.sh`](../.github/scripts/notas.sh):

1. **O que mudou** — os assuntos dos commits desde a tag anterior, agrupados
   pelo trailer `Impacto:`. É por isso que o assunto do commit importa tanto:
   ele é o que aparece ali, para quem nunca vai abrir o repositório.
2. **Como instalar, atualizar e remover** — o [`INSTALACAO.md`](INSTALACAO.md)
   inteiro, com o `vX.Y.Z` dos nomes de arquivo trocado pela versão de verdade.
   Não há segunda cópia das instruções: o que está na release é o que está no
   repositório, e corrigir uma corrige a outra.
3. **Somas de verificação** — o `SHA256SUMS`, também anexado como arquivo.

A mesma lista da parte 1 é escrita no [`CHANGELOG.md`](../CHANGELOG.md), no
commit da versão.

Para ver como ficaria antes de publicar:

```bash
.github/scripts/notas.sh 0.6.0 --so-mudancas    # só o changelog
.github/scripts/notas.sh 0.6.0 | less           # o corpo inteiro da release
```

---

## Quando der errado

**"A tag vX.Y.Z já existe."** Alguém publicou essa versão antes, ou uma
publicação anterior morreu depois de criar a tag. Confira com `git tag -l` e a
página de releases; se a tag existe e a release não, apague a tag
(`git push --delete origin vX.Y.Z`) antes de tentar de novo.

**O push do commit da versão foi recusado.** É proteção de ramo no `main`. Ou o
`github-actions[bot]` entra na lista de quem pode empurrar, ou a proteção passa
a permitir push de workflow. Sem isso, este workflow não tem como funcionar.

**O commit da versão disparou a CI de novo.** Não deveria: ele leva o trailer
`skip-checks: true`, que é o mecanismo documentado do GitHub para isso — e não
um `[skip ci]` enfiado no assunto, que o CLAUDE.md não permitiria.

**O trabalho do Windows demorou demais / falhou no Vulkan SDK.** A variante GPU
compila o gerador de shaders do ggml como um sub-projeto CMake inteiro; sem
cache, passa de uma hora (daí o `timeout-minutes: 150`). O SDK vem do instalador
da própria LunarG, em `sdk.lunarg.com`; se o endereço mudar, é essa a linha a
corrigir. A variante CPU não depende dele e continua saindo.

**O `.deb` saiu com dependências de menos.** O `empacotar.sh` imprime o que o
`dpkg-shlibdeps` reclamou em vez de engolir — leia o log do trabalho `Pacotes
.deb` antes de suspeitar da máquina de quem instalou.

**A publicação foi cancelada no meio.** A tag pode ter ficado criada e a release
não. Apague a tag e dispare de novo; nada mais fica pendurado — os artefatos são
do próprio run e somem com ele.

---

## Onde mexer

| Quero… | Mexo em |
|---|---|
| acrescentar uma conferência a todo push | `.github/workflows/ci.yml` |
| mudar como o número da versão é decidido | `.github/scripts/versao.sh` |
| mudar o texto que vai na release | `docs/INSTALACAO.md` (instruções) ou `.github/scripts/notas.sh` (estrutura) |
| acrescentar um anexo à release | `.github/workflows/release.yml`, no trabalho que o produz |
| mexer no instalador do Windows | `windows-integration/instalador/ditador.iss` |
| mudar o que vai no `.deb` | `empacotar.sh` — a CI só o chama |
