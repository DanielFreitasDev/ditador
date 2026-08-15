#!/usr/bin/env bash
# Roda o teste de ciclo de vida num GNOME Shell aninhado e sem tela.
#
# A sessão de quem está desenvolvendo não é tocada: o `gnome-shell-test-tool`
# sobe outro Shell, com XDG_*_HOME próprios e um monitor virtual, instala nele o
# ZIP que acabou de ser empacotado e roda o script de automação lá dentro.
#
# O barramento também é só dele (`dbus-run-session`). Não é preciosismo: dois
# GNOME Shell não cabem no mesmo barramento, porque os dois querem o nome
# `org.gnome.Shell` — sem isto o Shell do teste morre antes de começar.
#
# A consequência é que o Ditador de verdade, que está no barramento de sessão,
# não é visto lá dentro: a extensão sobe dizendo "Indisponível". É o bastante
# para o que este teste prova — que nada duplica e nada sobra ao ligar e
# desligar —, e é também o cenário mais difícil, o de quem habilita a extensão
# antes de instalar o aplicativo.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="ditador@danielfreitasdev.github.io"
PACOTE="$AQUI/${UUID}.shell-extension.zip"

if ! command -v gnome-shell-test-tool >/dev/null; then
    echo "!! Falta o gnome-shell-test-tool (pacote gnome-shell no Ubuntu 26.04)." >&2
    exit 1
fi

echo "==> Empacotando"
cd "$AQUI"
gnome-extensions pack --force --extra-source=src --out-dir="$AQUI" .

echo "==> Subindo um GNOME Shell só para o teste"
exec dbus-run-session -- gnome-shell-test-tool \
    --headless \
    --disable-animations \
    --extension "$PACOTE" \
    "$AQUI/scripts/teste-de-ciclo.js"
