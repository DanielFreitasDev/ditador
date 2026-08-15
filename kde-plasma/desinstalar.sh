#!/usr/bin/env bash
# Remove **só** a integração com o Plasma.
#
# O Ditador, o modelo de transcrição, a configuração do usuário e a extensão do
# GNOME ficam onde estão. Depois disto o programa continua inteiro: o ícone da
# bandeja volta sozinho assim que o widget solta o nome dele no barramento, sem
# reiniciar nada.
set -euo pipefail

ID_DO_WIDGET="io.github.danielfreitasdev.ditador"
QMLDIR="$(qmake6 -query QT_INSTALL_QML 2>/dev/null || echo /usr/lib/x86_64-linux-gnu/qt6/qml)"
MODULO="$QMLDIR/io/github/danielfreitasdev/ditador"

# ─── o widget ────────────────────────────────────────────────────────────────

if kpackagetool6 --type Plasma/Applet --show "$ID_DO_WIDGET" >/dev/null 2>&1; then
    echo "==> Removendo o widget"
    kpackagetool6 --type Plasma/Applet --remove "$ID_DO_WIDGET"
else
    echo "--  O widget não está instalado."
fi

# ─── o plugin QML ────────────────────────────────────────────────────────────

if [ -d "$MODULO" ]; then
    echo "==> Removendo o plugin de $MODULO"
    echo "    (pede senha: o diretório de módulos QML é do sistema)"
    sudo rm -rf "$MODULO"
    # As pastas do caminho são nossas e ficariam vazias. O `|| true` é porque
    # `io/` pode ser de outro projeto um dia — aí o rmdir recusa, que é o certo.
    sudo rmdir -p --ignore-fail-on-non-empty \
        "$QMLDIR/io/github/danielfreitasdev" 2>/dev/null || true
else
    echo "--  O plugin não está instalado."
fi

cat <<'FIM'

Pronto. A integração com o Plasma saiu.

O Ditador continua instalado e funcionando; o ícone dele volta para a bandeja do
sistema assim que o plasmashell descarregar o widget. Se você o tinha no painel,
pode ser preciso removê-lo de lá — ele vira um espaço vazio até o plasmashell
reiniciar.

Para remover o Ditador inteiro, veja o README na raiz do repositório.

FIM
