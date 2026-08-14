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
# Para se o programa estiver rodando: não dá para sobrescrever um binário em uso.
if ditador --encerrar >/dev/null 2>&1; then sleep 1; fi
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

MODELO="${XDG_DATA_HOME:-$HOME/.local/share}/ditador/models/ggml-large-v3-turbo-q5_0.bin"
if [ ! -f "$MODELO" ]; then
    echo
    echo "!! O modelo ainda não foi baixado. Rode:"
    echo "   ./baixar-modelo.sh"
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

  Iniciar agora:            systemctl --user start ditador
  Iniciar junto com a sessão: systemctl --user enable --now ditador
  Ver o que está acontecendo: journalctl --user -u ditador -f
  Parar:                    systemctl --user stop ditador

FIM
