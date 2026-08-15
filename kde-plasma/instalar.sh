#!/usr/bin/env bash
# Instala a integração do Ditador com o KDE Plasma 6.
#
# São duas metades, e elas se instalam de jeitos diferentes de propósito:
#
#   o widget   — QML e JSON, sem compilar, vai para a pasta do usuário pelo
#                kpackagetool6. Não pede senha.
#   o plugin   — C++ compilado, precisa morar no diretório de módulos QML do Qt
#                para o motor do plasmashell achá-lo. Esse diretório é do
#                sistema, e é aí que entra o sudo — uma vez, na instalação.
#
# O Ditador em si não passa por aqui. Instale-o antes, com o ./instalar.sh da
# raiz do repositório.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAIZ="$(cd "$AQUI/.." && pwd)"
ID_DO_WIDGET="io.github.danielfreitasdev.ditador"

# ─── o que precisa existir ───────────────────────────────────────────────────

falta() {
    echo
    echo "!! Faltam ferramentas para compilar a integração. Instale:"
    echo
    echo "   sudo apt install $*"
    echo
    exit 1
}

# Os nomes de pacote abaixo foram conferidos com `dpkg -S` num Kubuntu 26.04, e
# não deduzidos: cada um é o pacote que de fato traz o arquivo testado na linha.
pendentes=()
command -v cmake         >/dev/null || pendentes+=(cmake)
command -v g++           >/dev/null || pendentes+=(build-essential)
command -v qmake6        >/dev/null || pendentes+=(qmake6)
command -v kpackagetool6 >/dev/null || pendentes+=(kpackagetool6)
[ -f /usr/share/ECM/kde-modules/KDEInstallDirs.cmake ] || pendentes+=(extra-cmake-modules)
[ -d /usr/lib/x86_64-linux-gnu/cmake/Qt6DBus ]         || pendentes+=(qt6-base-dev)
[ -d /usr/lib/x86_64-linux-gnu/cmake/Qt6Qml ]          || pendentes+=(qt6-declarative-dev)
# Este não é de compilação: é o módulo QML que o widget importa em execução.
[ -f /usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/ki18n/qmldir ] \
    || pendentes+=(qml6-module-org-kde-ki18n)

if [ ${#pendentes[@]} -gt 0 ]; then
    falta "$(printf '%s\n' "${pendentes[@]}" | sort -u | tr '\n' ' ')"
fi

case "${XDG_CURRENT_DESKTOP:-}" in
    *KDE*) ;;
    *)
        echo "!! Você não parece estar numa sessão do Plasma"
        echo "   (XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-vazio})."
        echo "   A instalação segue — o widget só aparece numa sessão KDE."
        echo
        ;;
esac

# ─── o plugin QML em C++ ─────────────────────────────────────────────────────

# O prefixo é o do próprio Qt desta máquina, e não um /usr/local chutado: o
# motor QML só procura módulos em QT_INSTALL_QML, e instalar fora dali daria uma
# biblioteca que ninguém carrega. Dá para mandar outro pelo ambiente.
PREFIXO="${PREFIXO:-$(qmake6 -query QT_INSTALL_PREFIX)}"
QMLDIR="$(qmake6 -query QT_INSTALL_QML)"

echo "==> Compilando o plugin QML (Qt $(qmake6 -query QT_VERSION))"
cmake -S "$AQUI" -B "$AQUI/build" \
    -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    -DCMAKE_INSTALL_PREFIX="$PREFIXO" >/dev/null
cmake --build "$AQUI/build" --parallel "$(nproc)"

echo "==> Instalando o plugin em $QMLDIR/io/github/danielfreitasdev/ditador"
echo "    (é o único passo que pede senha — o diretório de módulos QML é do sistema)"
sudo cmake --install "$AQUI/build" >/dev/null

# ─── o widget ────────────────────────────────────────────────────────────────

# A versão vem do Cargo.toml, que é a única fonte da verdade dela neste projeto.
# O metadata.json versionado guarda 0.0.0 justamente para ninguém precisar
# lembrar de atualizá-lo à mão — quem o preenche é este script.
VERSAO="$(grep -m1 '^version = ' "$RAIZ/Cargo.toml" | cut -d'"' -f2)"

ESTAGIO="$(mktemp -d)"
trap 'rm -rf "$ESTAGIO"' EXIT
cp -r "$AQUI/plasmoid/package/." "$ESTAGIO/"
sed -i "s/\"Version\": \"0.0.0\"/\"Version\": \"$VERSAO\"/" "$ESTAGIO/metadata.json"

if kpackagetool6 --type Plasma/Applet --show "$ID_DO_WIDGET" >/dev/null 2>&1; then
    echo "==> Atualizando o widget para a versão $VERSAO"
    kpackagetool6 --type Plasma/Applet --upgrade "$ESTAGIO"
else
    echo "==> Instalando o widget (versão $VERSAO)"
    kpackagetool6 --type Plasma/Applet --install "$ESTAGIO"
fi

# ─── o que fazer agora ───────────────────────────────────────────────────────

cat <<'FIM'

Pronto.

Para o widget aparecer:

  1. o Ditador precisa estar em execução — ele é quem o traz para a bandeja.
     Confira com:  ditador --diagnostico

  2. clique com o botão direito na bandeja do sistema → Configurar a Bandeja do
     Sistema → Entradas, e ponha "Ditador" em "Mostrado".

O ícone do Ditador na bandeja (o antigo) some sozinho quando o widget assume:
são o mesmo recado, e não faria sentido dar os dois.

Reinicie o plasmashell agora:

  systemctl --user restart plasma-plasmashell

Numa primeira instalação é o que faz ele achar o widget; numa atualização é o que
faz a versão nova valer — ele fica com a compilação anterior do QML na memória, e
sem isso você depura um erro que já corrigiu. O painel some por um segundo e
volta; o Ditador não é afetado, é outro processo.

FIM
