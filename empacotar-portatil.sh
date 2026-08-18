#!/usr/bin/env bash
# Gera a versão portátil do Ditador para Linux: um .tar.gz que se descompacta e
# roda, sem instalação, sem sudo e sem tocar nas pastas do sistema.
#
#   ./empacotar-portatil.sh                     # backend Vulkan (padrão)
#   ./empacotar-portatil.sh cpu                 # só CPU, roda em qualquer máquina
#   ./empacotar-portatil.sh cuda                # CUDA (exige o toolkit para compilar)
#   ./empacotar-portatil.sh [backend] --com-modelo [nome]   # leva o modelo dentro
#
# ## Por que isto existe, se já há o .deb
#
# O .deb pressupõe poder instalar. A versão portátil é para o resto: máquina de
# trabalho onde não se instala nada, pendrive, conta sem sudo. Ela usa o modo
# portátil que o programa já tem (src/portatil.rs): o arquivo `portatil` ao lado
# do executável faz a configuração, os modelos e o histórico morarem na pasta
# `Dados/` vizinha — nada vai para ~/.config nem ~/.local/share.
#
# ## O modelo dentro do pacote
#
# O pacote da release **não** leva o modelo — a regra é a mesma do .deb: são
# centenas de megabytes que não mudam entre versões, e o programa baixa sozinho.
# O `--com-modelo` existe para o caso em que essa regra não serve: a máquina de
# destino não tem internet, ou tem a rede filtrada. Gera-se o pacote gordo numa
# máquina com internet e leva-se o arquivo inteiro no pendrive.
#
# ## A pasta de cima chama `ditador-portatil`, sem versão, de propósito
#
# Atualizar é descompactar a versão nova por cima da pasta antiga: o binário e o
# LEIA-ME são substituídos, e a `Dados/` — que o pacote não carrega — fica como
# está, com a configuração e o modelo de quem usa. Com a versão no nome da
# pasta, cada atualização criaria uma pasta nova e a pessoa teria de migrar a
# `Dados/` à mão.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$AQUI"

BACKEND="${1:-vulkan}"
case "$BACKEND" in
    vulkan)
        FEATURES=(--features vulkan)
        ROTULO="gpu"
        # O sugerido para quem tem GPU — o mesmo modelo::PADRAO do src/modelo.rs,
        # e há um teste em Rust lendo esta linha para que os dois não se separem.
        MODELO_SUGERIDO="large-v3-turbo-q5_0"
        ;;
    cpu)
        FEATURES=(--no-default-features --features cpu)
        ROTULO="cpu"
        # O modelo::PADRAO_CPU: na CPU, o grande transcreve mais devagar do que
        # se fala, e embutir 574 MB do modelo errado seria pior do que não
        # embutir nada.
        MODELO_SUGERIDO="small-q5_1"
        ;;
    cuda)
        FEATURES=(--no-default-features --features cuda)
        ROTULO="cuda"
        MODELO_SUGERIDO="large-v3-turbo-q5_0"
        ;;
    *) echo "Backend inválido: $BACKEND (use vulkan, cpu ou cuda)" >&2; exit 2 ;;
esac

COM_MODELO=0
MODELO="$MODELO_SUGERIDO"
if [ "${2:-}" = "--com-modelo" ]; then
    COM_MODELO=1
    [ -n "${3:-}" ] && MODELO="$3"
elif [ -n "${2:-}" ]; then
    echo "Opção desconhecida: $2 (a única depois do backend é --com-modelo [nome])" >&2
    exit 2
fi

for ferramenta in cargo objdump tar; do
    command -v "$ferramenta" >/dev/null || {
        echo "Falta o $ferramenta. sudo apt install binutils tar" >&2; exit 1; }
done

VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARQ="$(dpkg --print-architecture 2>/dev/null || uname -m)"
PASTA="ditador-portatil"
RAIZ="target/portatil/$PASTA"

echo "==> Compilando (backend: $BACKEND)"
cargo build --release "${FEATURES[@]}"

# A mesma conferência do empacotar.sh, pelo mesmo motivo — este pacote vai para
# máquinas que não são esta, e o preço de um -march=native que voltasse já foi
# pago: o .deb da 0.7.1 morria com "Illegal instruction" num Ryzen 5 4600G. O
# raciocínio inteiro está no .cargo/config.toml e no empacotar.sh; aqui fica só
# a rede: o piso é AVX2, e registradores %zmm não existem sem AVX-512.
echo "==> Conferindo se o binário roda fora desta máquina"
ACIMA_DO_PISO="$(objdump -d target/release/ditador | grep -c '%zmm' || true)"
if [ "$ACIMA_DO_PISO" -gt 0 ]; then
    cat >&2 <<AVISO
O binário saiu com $ACIMA_DO_PISO instruções AVX-512 (registradores %zmm) e não
vai rodar em processador sem AVX-512. Quase sempre isto quer dizer que o
-march=native voltou: confira o GGML_NATIVE=OFF do .cargo/config.toml e se o
cargo está sendo chamado da raiz do repositório, que é de onde ele lê esse
arquivo.
AVISO
    exit 1
fi
echo "ok  sem instruções acima do piso de AVX2."

echo "==> Montando a pasta portátil"
rm -rf "$RAIZ"
install -Dm755 target/release/ditador "$RAIZ/ditador"
strip --strip-unneeded "$RAIZ/ditador" 2>/dev/null || true
install -Dm644 LICENSE "$RAIZ/LICENSE"

# O marcador é o interruptor do modo portátil (src/portatil.rs). O conteúdo é
# livre — o programa só olha se o arquivo existe —, então ele carrega a própria
# explicação, para quem abrir a pasta sem ter lido nada.
cat > "$RAIZ/portatil" <<'FIM'
Este arquivo liga o modo portátil do Ditador: a configuração, os modelos e o
histórico ficam na pasta Dados/, aqui ao lado, em vez de ~/.config e
~/.local/share. Apague-o para o programa voltar às pastas do sistema.
FIM
chmod 644 "$RAIZ/portatil"

if [ "$ROTULO" = gpu ]; then
    DEPENDE_DE_GPU="Esta variante usa a GPU via Vulkan: a máquina precisa da libvulkan1 e de um
driver de vídeo com Vulkan (em Ubuntu, mesa-vulkan-drivers), que todo desktop
com GPU costuma ter. Sem isso, use a variante \"cpu\", que roda em qualquer
máquina."
elif [ "$ROTULO" = cuda ]; then
    DEPENDE_DE_GPU="Esta variante usa CUDA: a máquina precisa do driver da NVIDIA e das
bibliotecas de runtime do CUDA."
else
    DEPENDE_DE_GPU="Esta variante roda só na CPU e não depende de placa de vídeo nenhuma. O piso
de processador é AVX2 (todo Intel desde 2013, todo AMD Ryzen)."
fi

if [ "$COM_MODELO" = 1 ]; then
    SOBRE_O_MODELO="O modelo de transcrição (ggml-$MODELO.bin) já vem dentro, em
Dados/dados/models/ — o programa funciona sem internet desde a primeira vez."
else
    SOBRE_O_MODELO="O modelo de transcrição não vem no pacote: a primeira janela oferece
baixá-lo, com barra de progresso, ou rode  ./ditador --baixar-modelo  no
terminal. Para máquina sem internet, gere o pacote com o modelo dentro
(./empacotar-portatil.sh $BACKEND --com-modelo) numa máquina que tenha."
fi

cat > "$RAIZ/LEIA-ME.txt" <<FIM
Ditador $VERSAO — versão portátil ($ROTULO)
========================================

Ditado por voz offline com Whisper: segure uma tecla, fale, solte, e o texto
aparece e vai para a área de transferência. O áudio não sai da máquina.

Para usar
---------
Num terminal, dentro desta pasta:

    ./ditador

Tudo o que o programa grava — configuração, modelos, histórico — fica na pasta
Dados/, aqui dentro. Nada vai para as pastas do sistema, e mover ou copiar esta
pasta inteira (para um pendrive, para outra máquina) leva tudo junto.

$SOBRE_O_MODELO

$DEPENDE_DE_GPU

O atalho global
---------------
O Ditador lê a tecla de atalho direto do teclado (/dev/input), o que exige o
usuário no grupo "input":

    sudo usermod -aG input \$USER      # e saia da sessão e entre de novo

Numa máquina onde você não tem sudo, o atalho global não funciona — mas o
ditado funciona do mesmo jeito, por dois caminhos que não pedem permissão
nenhuma:

  - o ícone do Ditador na barra ("Ditar agora"); ou
  - um atalho de teclado do próprio sistema (GNOME: Configurações → Teclado →
    Atalhos personalizados; KDE: Atalhos personalizados) chamando:

        /caminho/desta/pasta/ditador --alternar

    Um aperto começa a gravar, outro para e transcreve.

Se algo não acontecer
---------------------
    ./ditador --diagnostico

confere item por item tudo de que o programa depende — teclado, modelo,
microfone, área de transferência — e diz o que está faltando e como resolver.

Para atualizar
--------------
Baixe a versão nova e descompacte por cima desta pasta: o programa é
substituído e a Dados/ — que é sua — fica como está.

Ubuntu 24.04 ou mais novo. Licença MIT (arquivo LICENSE).
Código e versões novas: https://github.com/DanielFreitasDev/ditador
FIM
chmod 644 "$RAIZ/LEIA-ME.txt"

SUFIXO=""
if [ "$COM_MODELO" = 1 ]; then
    echo "==> Levando o modelo ggml-$MODELO.bin"
    ORIGEM="${XDG_DATA_HOME:-$HOME/.local/share}/ditador/models/ggml-$MODELO.bin"
    if [ ! -f "$ORIGEM" ]; then
        # O download é do baixar-modelo.sh, que já confere assinatura e limpa o
        # parcial — reimplementar aqui seria a segunda cópia de um caminho que
        # já deu trabalho para acertar. O efeito colateral de o modelo ficar na
        # pasta de quem empacota é desejado: o próximo pacote sai sem download.
        ./baixar-modelo.sh "$MODELO"
    fi
    # A conferência vale também para o arquivo que já estava aqui: embutir um
    # modelo truncado num pacote feito para máquina sem internet é o pior lugar
    # possível para se descobrir o problema. A assinatura no disco é little-
    # endian — head -c 4 dá 6c6d6767, e não "ggml"; o mesmo teste em Rust que
    # lê a linha do baixar-modelo.sh lê esta.
    if [ "$(head -c 4 "$ORIGEM" | od -An -tx1 | tr -d ' \n')" != "6c6d6767" ]; then
        echo "O arquivo em $ORIGEM não é um modelo do Whisper." >&2
        echo "Apague-o e rode de novo, que o download refaz." >&2
        exit 1
    fi
    # Exatamente onde o modo portátil procura: data_dir() = Dados/dados, e os
    # modelos em models/ dentro dela (src/config.rs).
    install -Dm644 "$ORIGEM" "$RAIZ/Dados/dados/models/ggml-$MODELO.bin"
    SUFIXO="-com-modelo"
fi

echo "==> Empacotando"
SAIDA="target/portatil/ditador-v${VERSAO}-linux-${ARQ}-${ROTULO}-portatil${SUFIXO}.tar.gz"
# Dono root fixo: um tar carrega o uid de quem empacotou, e não há razão para o
# pacote contar quem foi.
tar -C target/portatil --owner=root:0 --group=root:0 -czf "$SAIDA" "$PASTA"

echo
echo "Pronto: $SAIDA  ($(du -h "$SAIDA" | cut -f1))"
echo
echo "Para usar em outra máquina:"
echo "  tar -xzf $(basename "$SAIDA")"
echo "  cd $PASTA && ./ditador"
echo
