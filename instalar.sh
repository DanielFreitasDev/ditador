#!/usr/bin/env bash
# Compila, instala em ~/.local/bin e registra o serviço de usuário + ícone.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
SYSTEMD_DIR="$HOME/.config/systemd/user"

# Backend: vulkan (padrão), cuda ou cpu
BACKEND="${1:-vulkan}"

case "$BACKEND" in
    vulkan) FEATURES=(--features vulkan) ;;
    cuda)   FEATURES=(--no-default-features --features cuda) ;;
    cpu)    FEATURES=(--no-default-features --features cpu) ;;
    *) echo "Backend inválido: $BACKEND (use vulkan, cuda ou cpu)" >&2; exit 2 ;;
esac

echo "==> Compilando (backend: $BACKEND)"
cd "$AQUI"
cargo build --release "${FEATURES[@]}"

echo "==> Instalando o binário em $BIN_DIR"
mkdir -p "$BIN_DIR"
# O serviço estava de pé? A pergunta tem de ser feita **antes** de o `--encerrar`
# logo abaixo derrubá-lo.
#
# Sem ela, reinstalar com o Ditador rodando o deixava parado: o `--encerrar` sai
# com código zero e a unidade é `Restart=on-failure`, então o systemd entende
# que o programa terminou de propósito e não sobe de novo. Para quem instalou, o
# ícone some da barra, o atalho para de responder e nada volta até o próximo
# login — com o script anunciando "Instalado" no meio disso. É o mesmo cuidado
# que o `postinst` do `.deb` já toma, lá com um bilhete em /run.
ESTAVA_ATIVO=no
if systemctl --user is-active --quiet ditador 2>/dev/null; then
    ESTAVA_ATIVO=sim
fi
# Para se o programa estiver rodando: não dá para sobrescrever um binário em uso.
#
# O caminho absoluto vem primeiro porque o PATH pode ainda não ter o $BIN_DIR —
# no Ubuntu o ~/.profile só o acrescenta se a pasta já existir no login, e é
# este script que a cria. Sem isto, na primeira instalação o `ditador` do PATH
# não resolvia, a versão velha continuava rodando, e o script anunciava
# "Instalado" mesmo assim.
if "$BIN_DIR/ditador" --encerrar >/dev/null 2>&1 || ditador --encerrar >/dev/null 2>&1; then
    sleep 1
fi
install -m 755 target/release/ditador "$BIN_DIR/ditador"

echo "==> Instalando os ícones"
mkdir -p "$ICON_DIR/scalable/apps" "$ICON_DIR/symbolic/apps"
# O colorido, para a lista de aplicativos e o alternador de janelas. Vai como
# SVG (que o GNOME usa em qualquer tamanho) e também nos tamanhos fixos, que o
# tema prefere quando existem — sai mais nítido no dock.
install -m 644 assets/ditador.svg "$ICON_DIR/scalable/apps/ditador.svg"
for png in assets/png/ditador-*.png; do
    tamanho="${png##*-}"; tamanho="${tamanho%.png}"
    mkdir -p "$ICON_DIR/${tamanho}x${tamanho}/apps"
    install -m 644 "$png" "$ICON_DIR/${tamanho}x${tamanho}/apps/ditador.png"
done
# Os símbolos da barra superior. Ficam em symbolic/apps para o GTK recolori-los
# conforme o tema em vez de deixá-los cinza-chumbo.
install -m 644 assets/simbolicos/*.svg "$ICON_DIR/symbolic/apps/"
gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true

echo "==> Instalando o atalho do aplicativo"
mkdir -p "$DESKTOP_DIR"
install -m 644 assets/ditador.desktop "$DESKTOP_DIR/ditador.desktop"
update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true

echo "==> Instalando o serviço de usuário"
mkdir -p "$SYSTEMD_DIR"
install -m 644 assets/ditador.service "$SYSTEMD_DIR/ditador.service"
systemctl --user daemon-reload

# Religa o que estava rodando, e só isso — quem não tinha o serviço de pé
# continua sem ele. O `daemon-reload` vem antes de propósito: o arquivo da
# unidade acabou de ser reescrito.
if [ "$ESTAVA_ATIVO" = sim ]; then
    echo "==> Religando o serviço, que estava rodando antes desta instalação"
    systemctl --user restart ditador || \
        echo "!! não consegui religar; rode: systemctl --user restart ditador" >&2
fi

MODELO="${XDG_DATA_HOME:-$HOME/.local/share}/ditador/models/ggml-large-v3-turbo-q5_0.bin"
if [ ! -f "$MODELO" ]; then
    echo
    echo "!! O modelo ainda não foi baixado. Rode:"
    echo "   ditador --baixar-modelo"
    echo "   (ou abra o programa: a primeira tela oferece baixá-lo)"
fi

if ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
    echo
    echo "!! Seu usuário não está no grupo 'input' — o atalho global não vai funcionar."
    echo "   sudo usermod -aG input $USER   (depois saia e entre na sessão de novo)"
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo; echo "!! $BIN_DIR não está no PATH." ;;
esac

cat <<FIM

Instalado.

  Conferir o que falta:       ditador --diagnostico
  Iniciar agora:              systemctl --user start ditador
  Iniciar junto com a sessão: systemctl --user enable --now ditador
  Ver o que está acontecendo: journalctl --user -u ditador -f
  Parar:                      systemctl --user stop ditador

FIM
