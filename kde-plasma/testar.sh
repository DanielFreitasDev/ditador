#!/usr/bin/env bash
# O laço de desenvolvimento do widget: confere, compila e abre numa janela.
#
# Nada aqui reinicia o plasmashell nem o KWin. O `plasmawindowed` roda o widget
# num processo separado, com o mesmo motor QML e as mesmas APIs do painel — se
# quebrar, quem morre é essa janela, e não a área de trabalho de quem está
# trabalhando. Derrubar o KWin numa sessão Wayland derruba a sessão.
#
# O plugin sai do diretório de build e não do sistema: o `QML_IMPORT_PATH` vale
# só para o processo que este script inicia, e é por isso que dá para iterar sem
# um `sudo make install` a cada mudança.
#
#   ./testar.sh              confere, compila, roda os testes e abre a janela
#   ./testar.sh --contrato   só confere o contrato contra o Ditador em execução
#   ./testar.sh --backend    só roda os testes do plugin contra o Ditador vivo
#   ./testar.sh --ci         compila e confere o QML; nada de sessão gráfica
#
# O `--ci` é a metade que roda numa máquina sem Plasma nenhum, e é o que a CI
# chama. Ele existe aqui, e não escrito no YAML do workflow, porque o filtro dos
# quatro avisos conhecidos do qmllint é regra deste projeto: mantida em dois
# lugares, um dia os dois discordam e o portão passa a valer menos do que
# aparenta.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAIZ="$(cd "$AQUI/.." && pwd)"
ID_DO_WIDGET="io.github.danielfreitasdev.ditador"
SERVICO="io.github.danielfreitasdev.Ditador"
CAMINHO="/io/github/danielfreitasdev/Ditador"
QT_BIN="/usr/lib/qt6/bin"

# ─── o contrato, contra o serviço vivo ───────────────────────────────────────
#
# O `cargo test` já compara `dbus/contrato.xml` com o que o Rust publica e com o
# XML embutido na extensão do GNOME. O que ele não alcança é o binário que está
# rodando agora, que pode ser de antes da mudança — e é justamente esse que o
# widget encontra pela frente.

contrato() {
    if ! gdbus introspect --session --dest "$SERVICO" --object-path "$CAMINHO" \
            >/dev/null 2>&1; then
        echo "--  O Ditador não está em execução; pulando a conferência do contrato."
        echo "    Para subir:  systemctl --user start ditador"
        return 0
    fi

    local vivo
    vivo="$(gdbus introspect --session --dest "$SERVICO" --object-path "$CAMINHO" --xml)"

    local faltando=()
    local membro
    # Os nomes saem do próprio XML canônico: acrescentar um método lá passa a
    # ser cobrado aqui sem editar este script.
    while read -r membro; do
        grep -q "name=\"$membro\"" <<<"$vivo" || faltando+=("$membro")
    done < <(grep -oE '<(method|property|signal) name="[A-Za-z]+"' "$RAIZ/dbus/contrato.xml" \
             | grep -oE '"[A-Za-z]+"' | tr -d '"')

    if [ ${#faltando[@]} -gt 0 ]; then
        echo "!!  O Ditador em execução não tem: ${faltando[*]}"
        echo "    Ele é anterior a esta versão do contrato. Reinstale-o:"
        echo "      cd $RAIZ && ./instalar.sh"
        return 1
    fi
    echo "ok  O contrato bate com o Ditador em execução."
}

# ─── o plugin, contra o Ditador vivo ─────────────────────────────────────────
#
# O `cargo test` prova que o Rust publica o contrato certo e o `qmllint` prova
# que o QML é válido; nenhum dos dois prova que os dois lados se entendem no
# barramento. Quem prova isso é este, e ele precisa do plugin compilado — daí
# rodar depois da compilação, e não antes.

backend() {
    if ! gdbus introspect --session --dest "$SERVICO" --object-path "$CAMINHO" \
            >/dev/null 2>&1; then
        echo "--  O Ditador não está em execução; pulando os testes do plugin."
        return 0
    fi
    "$QT_BIN/qmltestrunner" -import "$AQUI/build/qml" -input "$AQUI/plugin/tests"
}

# ─── conferir o QML ──────────────────────────────────────────────────────────

conferir_o_qml() {
    echo "==> qmllint"
    # O `-I build/qml` é o que permite conferir o QML contra o plugin recém-compilado
    # em vez do que estiver instalado no sistema.
    local saida restante
    saida="$("$QT_BIN/qmllint" -I "$AQUI/build/qml" -I /usr/lib/x86_64-linux-gnu/qt6/qml \
        "$AQUI"/plasmoid/package/contents/ui/*.qml 2>&1 || true)"

    # Quatro avisos são conhecidos e não são nossos: o `qmllint` não enxerga as
    # propriedades do objeto anexado `Plasmoid`, e os widgets do próprio Plasma 6.6
    # produzem exatamente os mesmos (confira com o org.kde.plasma.vault). Filtrá-los
    # aqui é o que faz o resto da saída significar alguma coisa.
    restante="$(grep -v 'Plasmoid.contextualActions' <<<"$saida" | grep -v '^\s*\^*$' | grep -v '^        PlasmaCore' || true)"
    if [ -n "$restante" ]; then
        echo "$restante"
        echo "!!  qmllint reclamou de algo novo."
        return 1
    fi
    echo "ok  qmllint limpo (fora os 4 avisos de Plasmoid.contextualActions, que o Plasma também tem)."
}

# ─── compilar o plugin ───────────────────────────────────────────────────────

compilar() {
    echo "==> Compilando o plugin"
    cmake -S "$AQUI" -B "$AQUI/build" -DCMAKE_BUILD_TYPE=RelWithDebInfo >/dev/null
    cmake --build "$AQUI/build" --parallel "$(nproc)" | tail -1
}

# ─── o widget, conferido sem Plasma ──────────────────────────────────────────
#
# O `metadata.json` só é lido pelo `kpackagetool6` na hora de instalar, e uma
# vírgula a mais ali derruba o widget na máquina de quem instalou, não na de
# quem escreveu. Conferir que é JSON válido custa um comando.

conferir_o_metadata() {
    echo "==> metadata.json do widget"
    local arquivo="$AQUI/plasmoid/package/metadata.json"
    python3 -c "
import json, sys
with open('$arquivo') as f:
    m = json.load(f)
assert m['KPackageStructure'] == 'Plasma/Applet', 'KPackageStructure errado'
assert m['KPlugin']['Id'] == '$ID_DO_WIDGET', 'o Id não é o do widget'
# 0.0.0 de propósito: quem preenche é o instalar.sh, lendo o Cargo.toml.
assert m['KPlugin']['Version'] == '0.0.0', 'a versão foi escrita à mão aqui'
"
    echo "ok  metadata.json válido, e a versão continua vindo do Cargo.toml."
}

case "${1:-}" in
    --contrato)
        contrato
        exit $?
        ;;
    --backend)
        backend
        exit $?
        ;;
    --ci)
        # A ordem é a inversa da do laço interativo: lá o qmllint roda antes,
        # contra o build da volta anterior, para dar resposta em um segundo;
        # aqui não existe volta anterior, então compila-se primeiro.
        compilar
        conferir_o_qml
        conferir_o_metadata
        echo
        echo "Pronto. O que falta — plasmawindowed, kpackagetool6 e os testes do"
        echo "plugin contra o Ditador vivo — precisa de sessão gráfica e roda em"
        echo "./kde-plasma/testar.sh, na máquina de quem mexe."
        exit 0
        ;;
esac

conferir_o_qml
compilar

# ─── pôr o widget no lugar ───────────────────────────────────────────────────

VERSAO="$(grep -m1 '^version = ' "$RAIZ/Cargo.toml" | cut -d'"' -f2)"
ESTAGIO="$(mktemp -d)"
trap 'rm -rf "$ESTAGIO"' EXIT
cp -r "$AQUI/plasmoid/package/." "$ESTAGIO/"
sed -i "s/\"Version\": \"0.0.0\"/\"Version\": \"$VERSAO\"/" "$ESTAGIO/metadata.json"

echo "==> Instalando o widget ($VERSAO)"
if kpackagetool6 --type Plasma/Applet --show "$ID_DO_WIDGET" >/dev/null 2>&1; then
    kpackagetool6 --type Plasma/Applet --upgrade "$ESTAGIO" | tail -1
else
    kpackagetool6 --type Plasma/Applet --install "$ESTAGIO" | tail -1
fi

contrato || true

echo "==> Testes do plugin contra o Ditador em execução"
backend

# ─── abrir ───────────────────────────────────────────────────────────────────

echo "==> plasmawindowed (feche a janela para terminar)"
echo
QML_IMPORT_PATH="$AQUI/build/qml" \
QT_LOGGING_RULES="ditador.plasma.debug=true" \
    plasmawindowed "$ID_DO_WIDGET"
