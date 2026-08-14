#!/usr/bin/env bash
# Refaz as imagens do README.
#
# Roda o Ditador nos dois temas com o passeio de demonstração ligado — ele passa
# sozinho pelas três telas, com um texto de exemplo, e grava um PNG de cada uma —
# e depois pousa as capturas sobre um fundo liso. Não precisa de microfone, do
# modelo baixado nem de ninguém falar na hora certa; precisa só de uma sessão
# gráfica aberta.
set -euo pipefail

cd "$(dirname "$0")"
tiros="${DITADOR_CAPTURA:-/tmp/ditador-capturas}"

if [ ! -x target/release/ditador ]; then
    echo "compilando primeiro…"
    cargo build --release
fi

for tema in claro escuro; do
    rm -rf "$tiros-$tema"
    mkdir -p "$tiros-$tema"
    echo "capturando o tema $tema…"
    DITADOR_DEMO=1 \
    DITADOR_ZOOM="${DITADOR_ZOOM:-1.5}" \
    DITADOR_TEMA="$tema" \
    DITADOR_CAPTURA="$tiros-$tema" \
        target/release/ditador >/dev/null 2>&1
done

DITADOR_CAPTURA="$tiros" python3 assets/compor-capturas.py
