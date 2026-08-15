#!/usr/bin/env bash
# Baixa um modelo GGML do Whisper para ~/.local/share/ditador/models
set -euo pipefail

MODELO="${1:-large-v3-turbo-q5_0}"
DESTINO="${XDG_DATA_HOME:-$HOME/.local/share}/ditador/models"
ARQUIVO="$DESTINO/ggml-${MODELO}.bin"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${MODELO}.bin"

if [ "$MODELO" = "--lista" ] || [ "$MODELO" = "-l" ]; then
    cat <<'FIM'
Modelos úteis (nome para passar como argumento):

  large-v3-turbo-q5_0   ~574 MB   padrão: rápido e preciso, ideal para ditado
  large-v3-turbo        ~1.6 GB   turbo sem quantização
  large-v3-q5_0         ~1.1 GB   large-v3 quantizado
  large-v3              ~3.1 GB   máxima qualidade, bem mais lento
  medium-q5_0           ~539 MB   alternativa mais leve
  small-q5_1            ~190 MB   para máquinas fracas

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
if [ "$(head -c 4 "$PARCIAL")" != "ggml" ]; then
    echo "Erro: o arquivo baixado não é um modelo do Whisper." >&2
    echo "      A rede pode ter devolvido uma página no lugar dele." >&2
    exit 1
fi

mv "$PARCIAL" "$ARQUIVO"
echo
echo "Pronto: $ARQUIVO ($(du -h "$ARQUIVO" | cut -f1))"
