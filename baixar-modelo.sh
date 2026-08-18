#!/usr/bin/env bash
# Baixa um modelo GGML do Whisper para ~/.local/share/ditador/models
set -euo pipefail

MODELO="${1:-large-v3-turbo-q5_0}"
DESTINO="${XDG_DATA_HOME:-$HOME/.local/share}/ditador/models"
ARQUIVO="$DESTINO/ggml-${MODELO}.bin"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${MODELO}.bin"

if [ "$MODELO" = "--lista" ] || [ "$MODELO" = "-l" ]; then
    cat <<'FIM'
Modelos (nome para passar como argumento). Os dois sugeridos vêm marcados:

  large-v3              ~3.1 GB   máxima qualidade, bem mais lento
  large-v3-turbo        ~1.6 GB   turbo sem quantização
  medium                ~1.5 GB   a geração anterior do porte grande
  large-v3-q5_0         ~1.1 GB   o mais preciso que cabe numa GPU comum
* large-v3-turbo-q5_0   ~574 MB   padrão para quem tem GPU
  medium-q5_0           ~539 MB   do tamanho do padrão, e mais lento
  small                 ~488 MB   o small sem quantização
* small-q5_1            ~190 MB   sugerido para transcrever na CPU
  base                  ~148 MB   rápido em qualquer máquina; erra mais
  tiny                  ~78 MB    último recurso; em português, erra bastante
  base-q5_1             ~60 MB    para máquina fraca ou conexão limitada
  tiny-q5_1             ~32 MB    o menor de todos; serve para testar

A lista é a mesma do CATALOGO de src/modelo.rs, e há um teste conferindo que as
duas não se separaram.

Uso: ./baixar-modelo.sh [nome-do-modelo]
FIM
    exit 0
fi

mkdir -p "$DESTINO"

if [ -f "$ARQUIVO" ]; then
    echo "Modelo já existe: $ARQUIVO"
    exit 0
fi

echo "Baixando ggml-${MODELO}.bin"
echo "  de   $URL"
echo "  para $ARQUIVO"
echo

# O temporário leva o PID: com um nome fixo, este script e o botão da janela
# apontavam para o mesmo arquivo e um sobrescrevia o download do outro.
PARCIAL="$ARQUIVO.$$.parcial"
# Sai limpo em qualquer caminho — sem isto, um Ctrl-C no meio deixava centenas
# de megabytes esquecidos na pasta dos modelos.
trap 'rm -f "$PARCIAL"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -L --fail --progress-bar --connect-timeout 20 \
        --speed-limit 1024 --speed-time 60 --retry 2 -o "$PARCIAL" "$URL"
elif command -v wget >/dev/null 2>&1; then
    wget --show-progress --timeout=20 --read-timeout=60 --tries=3 -O "$PARCIAL" "$URL"
else
    echo "Erro: preciso de curl ou wget." >&2
    exit 1
fi

# Confere antes de dar o arquivo por bom. Um modelo truncado, ou a página de
# erro de um proxy, trancava a instalação inteira: a partir daí tudo respondia
# "já existe" e o único caminho de volta era apagar o arquivo à mão.
#
# **A assinatura no disco é `6c 6d 67 67`, e não os caracteres "ggml".** O
# whisper.cpp grava o inteiro 0x67676d6c na ordem nativa da máquina, que em x86 e
# ARM é little-endian: lido como texto, o começo do arquivo dá "lmgg". Esta linha
# já esteve escrita como `!= "ggml"`, e nessa forma ela **reprovava todo download
# bem-sucedido** — o script baixava os 574 MB, acusava a rede de ter devolvido
# uma página e apagava o arquivo pelo `trap` da linha acima. O mesmo erro esteve
# no `src/modelo.rs`, onde foi corrigido primeiro; aqui ficou. Hoje há um teste
# (`o_script_confere_a_assinatura_na_ordem_em_que_ela_esta_no_disco`) lendo esta
# linha, para que os dois lados não se separem de novo.
#
# A comparação é em hexadecimal, e não com os bytes crus: assim ela não depende
# de a assinatura ser texto imprimível nem do locale de quem roda o script.
if [ "$(head -c 4 "$PARCIAL" | od -An -tx1 | tr -d ' \n')" != "6c6d6767" ]; then
    echo "Erro: o arquivo baixado não é um modelo do Whisper." >&2
    echo "      A rede pode ter devolvido uma página no lugar dele." >&2
    exit 1
fi

mv "$PARCIAL" "$ARQUIVO"
echo
echo "Pronto: $ARQUIVO ($(du -h "$ARQUIVO" | cut -f1))"
