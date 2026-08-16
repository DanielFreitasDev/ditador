#!/usr/bin/env bash
# Monta o texto da versão: o que mudou, e como instalar.
#
#   .github/scripts/notas.sh 0.6.0                # o corpo inteiro da release
#   .github/scripts/notas.sh 0.6.0 --so-mudancas  # só a seção "O que mudou"
#   .github/scripts/notas.sh 0.6.0 --desde v0.5.0 # de onde contar (padrão: última tag)
#
# ## De onde sai cada metade
#
# **O que mudou** sai do `git log` desde a última tag, agrupado pelo trailer
# `Impacto:` de cada commit — o mesmo que decide o número da versão
# (`versao.sh`). Um commit sem trailer aparece em "Correções", que é o padrão.
# O assunto do commit vira a linha da lista sem nenhuma edição, e é por isso que
# o CLAUDE.md pede que ele descreva o **efeito** da mudança: é este arquivo que
# o publica para quem nunca vai abrir o repositório.
#
# **Como instalar** sai do `docs/INSTALACAO.md`, inteiro, com o `vX.Y.Z` dos
# nomes de arquivo trocado pela versão de verdade. Não há segunda cópia das
# instruções: o que está na release é o que está no repositório, e corrigir uma
# corrige a outra.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$RAIZ"

REPO="DanielFreitasDev/ditador"
INSTALACAO="docs/INSTALACAO.md"

VERSAO="${1:?uso: notas.sh X.Y.Z [--so-mudancas] [--desde <tag>]}"
shift

SO_MUDANCAS=0
DESDE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --so-mudancas) SO_MUDANCAS=1 ;;
        --desde) DESDE="${2:?--desde precisa de uma tag}"; shift ;;
        *) echo "Opção desconhecida: $1" >&2; exit 2 ;;
    esac
    shift
done

if [ -z "$DESDE" ]; then
    DESDE="$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)"
fi
FAIXA="${DESDE:+$DESDE..}HEAD"

# ─── o que mudou ─────────────────────────────────────────────────────────────

incompativeis=""
funcionalidades=""
correcoes=""

normalizar() {
    tr '[:upper:]' '[:lower:]' | sed 'y/áàâãéêíóôõúüç/aaaaeeiooouuc/' | tr -d '[:space:]'
}

# `%x1f` separa os campos e `%x1e` separa os registros: o assunto de um commit
# pode ter qualquer coisa dentro, inclusive os caracteres que se usaria como
# separador ingênuo. Estes dois não aparecem em texto escrito por gente.
while IFS=$'\x1f' read -r -d $'\x1e' hash assunto trailer; do
    hash="${hash#$'\n'}"   # o \n que sobra do registro anterior
    [ -n "$hash" ] || continue

    # O commit que sobe a versão é ruído numa lista de mudanças: ele não é uma
    # mudança, é a consequência de todas as outras.
    case "$assunto" in
        Versão\ [0-9]*) continue ;;
    esac

    linha="- $assunto ([\`${hash:0:7}\`](https://github.com/$REPO/commit/$hash))"

    case "$(printf '%s' "$trailer" | normalizar)" in
        incompativel|quebra|major|breaking) incompativeis+="$linha"$'\n' ;;
        funcionalidade|recurso|minor|feature) funcionalidades+="$linha"$'\n' ;;
        *) correcoes+="$linha"$'\n' ;;
    esac
done < <(git log --no-merges --reverse "$FAIXA" \
            --format="%H%x1f%s%x1f%(trailers:key=Impacto,valueonly,separator=%x2C)%x1e")

echo "## O que mudou"
echo

if [ -n "$incompativeis" ]; then
    echo "### Mudanças incompatíveis"
    echo
    printf '%s\n' "$incompativeis"
fi
if [ -n "$funcionalidades" ]; then
    echo "### Novidades"
    echo
    printf '%s\n' "$funcionalidades"
fi
if [ -n "$correcoes" ]; then
    echo "### Correções e ajustes"
    echo
    printf '%s\n' "$correcoes"
fi
if [ -z "$incompativeis$funcionalidades$correcoes" ]; then
    echo "_Nenhum commit desde \`$DESDE\`._"
    echo
fi

if [ -n "$DESDE" ]; then
    echo "[Todas as mudanças desde a \`$DESDE\`](https://github.com/$REPO/compare/$DESDE...v$VERSAO)"
    echo
fi

[ "$SO_MUDANCAS" = 1 ] && exit 0

# ─── como instalar ───────────────────────────────────────────────────────────

if [ ! -f "$INSTALACAO" ]; then
    echo "!! $INSTALACAO não existe — a release sairia sem instruções." >&2
    exit 1
fi

echo "---"
echo
# O `vX.Y.Z` literal do documento vira a versão de verdade, para que os nomes
# dos arquivos na release possam ser copiados e colados como estão.
sed "s/vX\.Y\.Z/v$VERSAO/g" "$INSTALACAO"
