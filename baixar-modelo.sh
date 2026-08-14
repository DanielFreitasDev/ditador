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

if command -v curl >/dev/null 2>&1; then
    curl -L --fail --progress-bar -o "$ARQUIVO.parcial" "$URL"
elif command -v wget >/dev/null 2>&1; then
    wget --show-progress -O "$ARQUIVO.parcial" "$URL"
else
    echo "Erro: preciso de curl ou wget." >&2
    exit 1
fi

mv "$ARQUIVO.parcial" "$ARQUIVO"
echo
echo "Pronto: $ARQUIVO ($(du -h "$ARQUIVO" | cut -f1))"
