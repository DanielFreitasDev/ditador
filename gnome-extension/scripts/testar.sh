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

# O `gnome-extensions pack` compila os schemas e falharia num XML inválido, mas
# falharia no meio de outra tarefa e com outra mensagem. Conferir aqui, primeiro
# e sozinho, faz o erro de schema dizer que é erro de schema.
echo "==> Conferindo os schemas do GSettings"
glib-compile-schemas --strict --dry-run "$AQUI/schemas"

echo "==> Empacotando"
cd "$AQUI"
gnome-extensions pack --force --extra-source=src --out-dir="$AQUI" .

# O teto de tempo não é zelo: o Shell aninhado depende do
# `gnome-shell-perf-helper` aparecer no barramento privado para que o
# `runPerfScript` chame o nosso roteiro, e quando ele não aparece nada acontece
# — nenhuma linha, nenhum erro, e o comando fica pendurado até alguém notar.
# Melhor um portão que falha dizendo o que houve do que um que nunca volta.
LIMITE=${DITADOR_LIMITE_DO_TESTE:-120}

# Uma das causas do travamento é um processo que sobra.
#
# Quem chama o nosso roteiro é o `Scripting.runPerfScript` do Shell, e ele só o
# chama depois que o `gnome-shell-perf-helper` aparece no barramento. O Shell
# aninhado sobe esse ajudante toda vez — e o ajudante **não** morre junto com a
# sessão aninhada quando ela é derrubada por tempo. Ficando um vivo de uma volta
# anterior, a seguinte tem uma razão a mais para não começar.
#
# Matar o que sobrou antes de cada tentativa é barato e seguro: este ajudante só
# existe para rodar roteiros de automação do Shell, nunca numa sessão de uso.
# Não é a cura completa — mesmo limpo, o arranque da sessão aninhada ainda falha
# de vez em quando —, e é por isso que as três tentativas continuam aqui.
limpar_o_ajudante() {
    pkill -u "$(id -u)" -f 'libexec/gnome-shell-perf-helper' 2>/dev/null || true
}

rodar() {
    limpar_o_ajudante
    timeout --signal=TERM --kill-after=10 "$LIMITE" \
        dbus-run-session -- gnome-shell-test-tool \
        --headless \
        --disable-animations \
        --extension "$PACOTE" \
        "$AQUI/scripts/teste-de-ciclo.js"
}

# E a limpeza também na saída, para não deixar a próxima volta — ou a próxima
# pessoa — com o mesmo problema herdado.
trap limpar_o_ajudante EXIT

echo "==> Subindo um GNOME Shell só para o teste (limite de ${LIMITE}s)"
set +e
CODIGO=0
for tentativa in 1 2 3; do
    [ "$tentativa" -gt 1 ] &&
        echo "==> A sessão aninhada não rodou o roteiro; tentativa $tentativa de 3" >&2
    rodar
    CODIGO=$?
    # Só o travamento merece outra tentativa. Teste que falhou, falhou.
    [ "$CODIGO" -eq 124 ] || [ "$CODIGO" -eq 137 ] || break
done
set -e

if [ "$CODIGO" -eq 124 ] || [ "$CODIGO" -eq 137 ]; then
    cat >&2 <<'FIM'

!! O Shell aninhado subiu e o roteiro do ciclo de vida nunca começou — duas
   vezes seguidas.

   O sintoma conhecido é o `gnome-shell-perf-helper` não pegar o nome
   `org.gnome.Shell.PerfHelper` no barramento privado; sem ele, o
   `Scripting.runPerfScript` do Shell espera para sempre e este teste fica
   pendurado. É intermitente: a mesma máquina que trava agora passa daqui a
   pouco. Confira com o Shell aninhado no ar:

       busctl --user list | grep PerfHelper
       ls -l /usr/libexec/gnome-shell-perf-helper

   Isto é ambiente, não a extensão: o `npm run lint`, o
   `gjs -m scripts/teste-do-backend.js` e o `--dry-run` dos schemas continuam
   valendo e cobrem o resto.
FIM
    exit 1
fi

exit "$CODIGO"
