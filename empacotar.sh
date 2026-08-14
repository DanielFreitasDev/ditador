#!/usr/bin/env bash
# Gera um pacote .deb do Ditador, pronto para instalar em outra máquina Ubuntu.
#
#   ./empacotar.sh            # backend Vulkan (padrão: GPU quando houver)
#   ./empacotar.sh cpu        # só CPU, sem nenhuma dependência de GPU
#   ./empacotar.sh cuda       # CUDA (exige o toolkit da NVIDIA para compilar)
#
# O pacote leva o programa, os ícones, o atalho do menu e o serviço de usuário
# do systemd. Não leva o modelo de transcrição: são centenas de megabytes, e a
# própria janela oferece baixá-lo na primeira vez (ou `ditador --baixar-modelo`).
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$AQUI"

BACKEND="${1:-vulkan}"
case "$BACKEND" in
    vulkan)
        FEATURES=(--features vulkan)
        PACOTE="ditador"
        RESUMO="Ditado por voz offline com Whisper (GPU via Vulkan)"
        DEPS_BACKEND="libvulkan1"
        SUGERE="mesa-vulkan-drivers"
        ;;
    cpu)
        FEATURES=(--no-default-features --features cpu)
        PACOTE="ditador-cpu"
        RESUMO="Ditado por voz offline com Whisper (só CPU)"
        DEPS_BACKEND=""
        SUGERE=""
        ;;
    cuda)
        FEATURES=(--no-default-features --features cuda)
        PACOTE="ditador-cuda"
        RESUMO="Ditado por voz offline com Whisper (GPU via CUDA)"
        DEPS_BACKEND=""
        SUGERE="nvidia-cuda-toolkit"
        ;;
    *) echo "Backend inválido: $BACKEND (use vulkan, cpu ou cuda)" >&2; exit 2 ;;
esac

for ferramenta in dpkg-deb fakeroot cargo; do
    command -v "$ferramenta" >/dev/null || {
        echo "Falta o $ferramenta. sudo apt install dpkg-dev fakeroot" >&2; exit 1; }
done

VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARQ="$(dpkg --print-architecture)"
RAIZ="target/deb/$PACOTE"

echo "==> Compilando (backend: $BACKEND)"
cargo build --release "${FEATURES[@]}"

echo "==> Montando a árvore do pacote"
rm -rf "$RAIZ"
install -Dm755 target/release/ditador                "$RAIZ/usr/bin/ditador"
install -Dm644 assets/ditador.desktop                "$RAIZ/usr/share/applications/ditador.desktop"
install -Dm644 assets/ditador.service                "$RAIZ/usr/lib/systemd/user/ditador.service"
install -Dm644 assets/ditador.svg                    "$RAIZ/usr/share/icons/hicolor/scalable/apps/ditador.svg"
install -Dm644 README.md                             "$RAIZ/usr/share/doc/$PACOTE/README.md"

for png in assets/png/ditador-*.png; do
    tamanho="${png##*-}"; tamanho="${tamanho%.png}"
    install -Dm644 "$png" "$RAIZ/usr/share/icons/hicolor/${tamanho}x${tamanho}/apps/ditador.png"
done
for svg in assets/simbolicos/*.svg; do
    install -Dm644 "$svg" "$RAIZ/usr/share/icons/hicolor/symbolic/apps/$(basename "$svg")"
done

# O serviço vem do repositório apontando para ~/.local/bin (a instalação
# manual). No pacote, o binário fica em /usr/bin.
sed -i 's|ExecStart=%h/.local/bin/ditador|ExecStart=/usr/bin/ditador|' \
    "$RAIZ/usr/lib/systemd/user/ditador.service"

strip --strip-unneeded "$RAIZ/usr/bin/ditador" 2>/dev/null || true

echo "==> Descobrindo as dependências"
# As bibliotecas ligadas de verdade saem do dpkg-shlibdeps; as que o winit e o
# glutin abrem em tempo de execução (X11, Wayland, EGL) não aparecem no ELF e
# precisam ser listadas à mão.
DLABERTAS="libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon0, libwayland-client0, libegl1, libgl1"
LIGADAS=""
if command -v dpkg-shlibdeps >/dev/null; then
    mkdir -p "$RAIZ/debian"
    printf 'Source: %s\nPackage: %s\nArchitecture: %s\n' "$PACOTE" "$PACOTE" "$ARQ" \
        > "$RAIZ/debian/control"
    if (cd "$RAIZ" && dpkg-shlibdeps -O --ignore-missing-info usr/bin/ditador 2>/dev/null) \
        > "$RAIZ/deps.txt"; then
        LIGADAS="$(sed 's/^shlibs:Depends=//' "$RAIZ/deps.txt")"
    fi
    rm -rf "$RAIZ/debian" "$RAIZ/deps.txt"
fi
if [ -z "$LIGADAS" ]; then
    echo "    (dpkg-shlibdeps não respondeu; usando a lista de reserva)"
    LIGADAS="libc6, libgcc-s1, libstdc++6, libasound2t64 | libasound2"
fi

DEPS="$LIGADAS, $DLABERTAS"
[ -n "$DEPS_BACKEND" ] && DEPS="$DEPS, $DEPS_BACKEND"
# O dpkg-shlibdeps pode já ter encontrado a biblioteca do backend; repetir o
# nome no Depends não quebra nada, mas suja a saída do `apt show`.
DEPS="$(printf '%s' "$DEPS" | tr ',' '\n' | sed 's/^ *//; s/ *$//' \
        | awk 'NF && !vistos[$1]++' | paste -sd, - | sed 's/,/, /g')"

echo "==> Escrevendo os metadados"
mkdir -p "$RAIZ/DEBIAN"
cat > "$RAIZ/DEBIAN/control" <<FIM
Package: $PACOTE
Version: $VERSAO
Section: utils
Priority: optional
Architecture: $ARQ
Maintainer: Daniel Freitas <danielsfreitas97@gmail.com>
Depends: $DEPS
Recommends: curl | wget${SUGERE:+, $SUGERE}
Provides: ditador-backend
Conflicts: ditador-backend
Replaces: ditador-backend
Description: $RESUMO
 Segure uma tecla, fale, solte: o texto transcrito aparece numa janela de vidro
 e vai para a área de transferência. Tudo acontece na sua máquina — o áudio não
 sai dela, e o programa funciona sem internet depois que o modelo é baixado.
 .
 Roda em segundo plano com um ícone na barra superior. O modelo de transcrição
 (cerca de 574 MB) não vem no pacote: a própria janela oferece baixá-lo na
 primeira vez, ou rode "ditador --baixar-modelo".
FIM

cat > "$RAIZ/DEBIAN/postinst" <<'FIM'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
    update-desktop-database /usr/share/applications 2>/dev/null || true

    # O atalho global lê o teclado direto do /dev/input, o que exige estar no
    # grupo "input". Só o dono da sessão pode decidir isso, então aqui vai só o
    # aviso — mexer no grupo de um usuário por conta própria seria demais.
    QUEM="${SUDO_USER:-${PKEXEC_UID:+$(id -nu "$PKEXEC_UID" 2>/dev/null)}}"
    if [ -n "$QUEM" ] && ! id -nG "$QUEM" 2>/dev/null | tr ' ' '\n' | grep -qx input; then
        echo ""
        echo "Ditador: falta um passo para o atalho global funcionar:"
        echo "  sudo usermod -aG input $QUEM"
        echo "  (depois saia da sessão e entre de novo)"
        echo ""
    fi
    echo "Ditador instalado. Abra pelo menu de aplicativos, ou:"
    echo "  systemctl --user enable --now ditador   # subir junto com a sessão"
fi
exit 0
FIM

cat > "$RAIZ/DEBIAN/prerm" <<'FIM'
#!/bin/sh
set -e
# Para a instância de quem está removendo, senão o binário some por baixo dela.
if [ "$1" = remove ] || [ "$1" = upgrade ]; then
    [ -n "${SUDO_USER:-}" ] && su - "$SUDO_USER" -c 'ditador --encerrar' >/dev/null 2>&1 || true
fi
exit 0
FIM

cat > "$RAIZ/DEBIAN/postrm" <<'FIM'
#!/bin/sh
set -e
if [ "$1" = remove ] || [ "$1" = purge ]; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
exit 0
FIM

chmod 755 "$RAIZ/DEBIAN/postinst" "$RAIZ/DEBIAN/prerm" "$RAIZ/DEBIAN/postrm"

install -Dm644 /dev/stdin "$RAIZ/usr/share/doc/$PACOTE/copyright" <<FIM
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: ditador

Files: *
Copyright: Daniel Freitas
License: MIT
FIM

printf '%s (%s) unstable; urgency=low\n\n  * Versão %s.\n\n -- Daniel Freitas <danielsfreitas97@gmail.com>  %s\n' \
    "$PACOTE" "$VERSAO" "$VERSAO" "$(date -R)" \
    | gzip -9n > "$RAIZ/usr/share/doc/$PACOTE/changelog.Debian.gz"

echo "==> Empacotando"
SAIDA="target/deb/${PACOTE}_${VERSAO}_${ARQ}.deb"
fakeroot dpkg-deb --build --root-owner-group "$RAIZ" "$SAIDA" >/dev/null

echo
echo "Pronto: $SAIDA  ($(du -h "$SAIDA" | cut -f1))"
echo
echo "Para instalar (aqui ou em outra máquina Ubuntu):"
echo "  sudo apt install ./$SAIDA"
echo
