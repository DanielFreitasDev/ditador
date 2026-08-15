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

# O passeio vive dentro da interface, que só nasce quando esta execução assume o
# socket. Com uma instância viva — o caso normal, já que o app sobe com a sessão
# — ela responde "O Ditador já está rodando." e sai com zero: nenhuma janela,
# nenhuma captura, e o script anunciando sucesso.
if target/release/ditador --encerrar >/dev/null 2>&1; then
    echo "encerrei a instância que estava rodando"
    sleep 1
fi

for tema in claro escuro; do
    rm -rf "$tiros-$tema"
    mkdir -p "$tiros-$tema"
    echo "capturando o tema $tema…"
    # A saída do programa fica visível: era ela que explicava por que não saía
    # captura nenhuma, e ia toda para /dev/null.
    DITADOR_DEMO=1 \
    DITADOR_ZOOM="${DITADOR_ZOOM:-1.5}" \
    DITADOR_TEMA="$tema" \
    DITADOR_CAPTURA="$tiros-$tema" \
        target/release/ditador

    for tela in recording result settings; do
        if [ ! -f "$tiros-$tema/$tela.png" ]; then
            echo "!! o passeio não gravou $tela.png no tema $tema" >&2
            exit 1
        fi
    done
done

DITADOR_CAPTURA="$tiros" python3 assets/compor-capturas.py
