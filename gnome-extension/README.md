# Ditador — extensão do GNOME Shell

Integração do [Ditador](../README.md) com o GNOME Shell 50: indicador na barra
superior, controles nas Configurações rápidas e um aviso na tela enquanto se
dita.

A extensão é **opcional**. Sem ela o Ditador funciona exatamente como sempre —
com o ícone do StatusNotifierItem e a própria sobreposição. Ela não grava áudio,
não transcreve, não lê o teclado, não abre subprocessos e não acessa a rede: só
fala D-Bus com o processo Rust, que continua sendo o programa.

| | |
|---|---|
| UUID | `ditador@danielfreitasdev.github.io` |
| Alvo | GNOME Shell 50.x (`"shell-version": ["50"]`) |
| Testado em | Ubuntu 26.04, GNOME Shell 50.1, Wayland |

Só a geração 50. Não há código de compatibilidade com 45–49 nem com a 51 — o
que se ganharia em alcance se pagaria em ramos que ninguém consegue testar.

## Antes de instalar

```bash
gnome-shell --version          # precisa dizer 50.x
```

O aplicativo Ditador precisa estar instalado (a extensão sem ele fica dizendo
*Indisponível*, e nada mais). Da raiz do repositório:

```bash
./instalar.sh                  # ou: ./instalar.sh cpu | ./instalar.sh cuda
ditador --baixar-modelo
systemctl --user enable --now ditador
```

## Instalar, atualizar, remover

```bash
./gnome-extension/instalar.sh                                    # instala e habilita
gnome-extensions disable ditador@danielfreitasdev.github.io      # desliga
gnome-extensions enable  ditador@danielfreitasdev.github.io      # liga de novo
gnome-extensions uninstall ditador@danielfreitasdev.github.io    # remove
```

Atualizar é rodar o `instalar.sh` de novo — ele reempacota e sobrescreve.

**Quando é preciso sair da sessão.** Numa **primeira instalação**, sim: o GNOME
Shell procura extensões uma vez só, ao iniciar (`_loadExtensions`, em
`js/ui/extensionSystem.js`), e não há vigia de diretório para extensões novas.
Numa sessão Wayland não existe recarregar o Shell — o `Alt+F2` seguido de `r` é
coisa do X11, que o GNOME 50 não tem mais. O instalador já deixa a extensão
marcada para ligar sozinha; basta sair e entrar.

Depois disso, **habilitar e desabilitar valem na hora**, sem sair de nada. Para
uma extensão já conhecida do Shell, atualizar o código também pede um novo
login.

## Como funciona

```
        GNOME Shell 50.1
               │
   extensão (GJS, ESModules)
   ├── indicador na barra          gicon por estado
   ├── Configurações rápidas       QuickMenuToggle + menu
   └── aviso na tela               St.BoxLayout .osd-window
               │
             D-Bus  io.github.danielfreitasdev.Ditador
               │
          Ditador (Rust)
   evdev · CPAL · Whisper · Vulkan · clipboard · socket Unix
```

O estado só anda numa direção. O Rust publica cada mudança como
`PropertiesChanged`; a extensão reage e desenha. **Não há máquina de estados do
lado do JavaScript** e nada é consultado de tempos em tempos — o único
temporizador da extensão é o do cronômetro, um tique por segundo, criado quando
a gravação começa e removido quando ela termina.

### A interface D-Bus

Nome e interface `io.github.danielfreitasdev.Ditador`, objeto em
`/io/github/danielfreitasdev/Ditador`. O porquê do nome está no bloco que abre
[`src/dbus.rs`](../src/dbus.rs).

| Método | O que faz |
|---|---|
| `Alternar()` | grava se estiver parado, para se estiver gravando |
| `IniciarGravacao()` | começa a gravar; não faz nada se já estiver gravando |
| `PararGravacao()` | para e manda transcrever; não faz nada se não estiver |
| `AbrirConfiguracoes()` | abre a janela de configurações do aplicativo |
| `Encerrar()` | encerra o Ditador |

| Propriedade | Tipo | |
|---|---|---|
| `Estado` | `s` | `carregando`, `pronto`, `gravando`, `transcrevendo` ou `erro` |
| `Mensagem` | `s` | o erro ou aviso da vez; vazia quando não há |
| `GravandoDesde` | `t` | início da gravação em ms desde a época; `0` parado |
| `Modelo` | `s` | nome curto do modelo (`large-v3-turbo-q5_0`) |
| `Idioma` | `s` | idioma configurado, por extenso |
| `Atalho` | `s` | o atalho global, como se escreve numa frase |

| Sinal | |
|---|---|
| `Nivel(d)` | o pico do microfone, de 0 a 1, umas 15 vezes por segundo — **só enquanto se grava** |

`Nivel` é sinal e não propriedade porque não é estado: é um fio de água passando,
e nada disso precisa ser lembrado depois. Uma propriedade guardaria o último
valor para sempre, inclusive com o microfone já fechado, e faria o barramento
anunciar `PropertiesChanged` quinze vezes por segundo — o oposto do que ele
existe para dizer. O valor sai cru; a raiz quadrada que dá presença aos sons
baixos é escolha de quem desenha, e cada superfície faz a sua.

Não existe um estado `indisponivel` no barramento: esse é a *ausência* do nome
nele, e quem o percebe é a extensão (`INDISPONIVEL`, em `src/backend.js`). Pelo
mesmo motivo não há um `iniciando` separado de `carregando` — neste programa o
arranque *é* a carga do modelo.

A extensão usa `IniciarGravacao`/`PararGravacao`, e nunca `Alternar`, nos botões
que dizem o que vão fazer: entre desenhar "Ditar agora" e o clique chegar cabe
um ditado inteiro pelo atalho global, e um `Alternar` faria o botão parar a
gravação que ele prometia começar.

### Como o ícone não aparece duas vezes

Enquanto está habilitada, a extensão detém um segundo nome:

```
io.github.danielfreitasdev.Ditador.GnomeExtension
```

O Rust observa esse nome (`vigiar_a_extensao`, em `src/dbus.rs`). Enquanto ele
existir, o aplicativo **desregistra** o StatusNotifierItem e deixa de desenhar a
própria sobreposição de "gravando"/"transcrevendo" — quem diz essas duas coisas
passa a ser o Shell. Quando o nome some, os dois voltam.

Isso não depende de a extensão se despedir no `disable()`. Quem detém um nome no
D-Bus é a *conexão*, e o barramento a solta sozinho quando ela cai:

| O que aconteceu | O ícone do aplicativo volta? |
|---|---|
| `disable()` normal | sim |
| extensão desabilitada abruptamente | sim |
| GNOME Shell reiniciado ou encerrado | sim, a conexão caiu |
| extensão travou e o GJS morreu | sim, a conexão caiu |

As telas que têm ação — resultado, configurações, erro — continuam sendo do
aplicativo mesmo com a extensão ligada. Um OSD não tem onde pôr o texto para
copiar nem os botões que resolvem o problema.

### Reserva

| | Com a extensão | Sem ela | Outro desktop |
|---|---|---|---|
| Ícone | indicador do Shell | StatusNotifierItem | StatusNotifierItem |
| Menu | Configurações rápidas | menu do ícone | menu do ícone |
| Aviso ao ditar | OSD do Shell | sobreposição egui/XWayland | sobreposição egui/XWayland |
| Controle por comando | socket Unix | socket Unix | socket Unix |
| Atalho global | evdev | evdev | evdev |

O `evdev` continua sendo a implementação oficial do atalho, e a extensão não
tenta substituí-lo. O GNOME não entrega o evento de *soltar* a tecla a quem
registra um atalho, e sem ele "segurar para falar" não existe.

## Arquivos

```
extension.js      ciclo de vida: monta no enable(), desmonta no disable()
prefs.js          GTK4 + Libadwaita, num processo à parte
stylesheet.css    uma regra, e nenhuma cor
src/backend.js    D-Bus: proxy, o nome que segura, o sinal "mudou"
src/estado.js     o vocabulário dos estados (texto e símbolo), num lugar só
src/indicator.js  SystemIndicator: o ícone da barra e o dono do controle
src/quickSettings.js  o QuickMenuToggle e o menu dele
src/osd.js        o aviso na tela, o cronômetro e o medidor de voz
schemas/          as duas preferências da integração
scripts/          testes; não entram no pacote
```

### Ícones

A extensão **não** leva os ícones dentro dela: usa `ditador-symbolic` e os
outros três do tema, que o `instalar.sh` do aplicativo instala em
`~/.local/share/icons/hicolor/symbolic/apps/`. Vindos do tema, eles são
recoloridos conforme o tema do sistema — que é o que "symbolic" quer dizer. Se
não estiverem lá, o `Gio.ThemedIcon` com dois nomes cai sozinho num ícone padrão
do GNOME (ver `RESERVAS`, em `src/estado.js`).

### Preferências

Só o que é desta camada: **ícone na barra superior** e **aviso na tela ao
ditar**. O que configura o ditado — modelo, microfone, idioma, GPU, área de
transferência — continua na tela do próprio Ditador, e não é repetido aqui.

## Desenvolvimento

```bash
npm install                    # só o ESLint; nada disso vai para o ZIP
npm run lint
./scripts/testar.sh            # ciclo de vida, num GNOME Shell aninhado
gjs -m scripts/teste-do-backend.js   # conversa com o Ditador em execução
```

`./scripts/testar.sh` sobe outro GNOME Shell — sem tela, com monitor virtual e
barramento próprio — instala nele o ZIP e habilita/desabilita a extensão três
vezes, contando os atores a cada volta. É o que prova que nada duplica e nada
sobra. A sessão de quem está desenvolvendo não é tocada.

O barramento privado é obrigatório (dois Shell não dividem o nome
`org.gnome.Shell`), e o efeito colateral é que lá dentro o Ditador não existe: a
extensão sobe dizendo *Indisponível*. Quem cobre a outra metade é o
`teste-do-backend.js`, que roda no `gjs` comum, no barramento de verdade, e faz
um ditado de dois segundos de ponta a ponta.

## Diagnóstico

```bash
gnome-shell --version
gnome-extensions list
gnome-extensions info ditador@danielfreitasdev.github.io
journalctl --user -o cat /usr/bin/gnome-shell -f     # erros de JS aparecem aqui
```

Do lado do aplicativo:

```bash
ditador --diagnostico
busctl --user list | grep danielfreitasdev            # os dois nomes, se ambos estão no ar
gdbus introspect --session \
    --dest io.github.danielfreitasdev.Ditador \
    --object-path /io/github/danielfreitasdev/Ditador
gdbus monitor --session --dest io.github.danielfreitasdev.Ditador
journalctl --user -u ditador -f
```

O `gdbus monitor` é o que responde "o Rust está publicando as mudanças?" — a
cada ditado devem aparecer os `PropertiesChanged` de `Estado` e `GravandoDesde`.

| Sintoma | Onde olhar |
|---|---|
| A extensão não aparece na lista | primeira instalação: sair da sessão e entrar |
| Diz *Indisponível* | `systemctl --user status ditador` |
| Dois ícones na barra | `busctl --user list \| grep GnomeExtension` — se o nome não está lá, a extensão não subiu |
| O ícone sumiu de vez | desabilite a extensão; se não voltar, `journalctl --user -u ditador` |
| Nada acontece ao segurar a tecla | é o `evdev`, não a extensão: `ditador --diagnostico` |

## Publicação

O pacote sai com `gnome-extensions pack` (é o que o `instalar.sh` faz) e não
contém binário, biblioteca nativa, executável, modelo do Whisper nem
`node_modules` — o aplicativo Rust é um componente externo, instalado à parte.

A extensão ainda não está no extensions.gnome.org.
