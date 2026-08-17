#!/usr/bin/env bash
# A versão do projeto: onde ela mora, qual é a próxima e se todos os arquivos
# concordam sobre ela.
#
#   .github/scripts/versao.sh atual              # o que está no Cargo.toml
#   .github/scripts/versao.sh impacto            # correcao | funcionalidade | incompativel
#   .github/scripts/versao.sh proxima [auto|patch|minor|major]
#   .github/scripts/versao.sh gravar 0.6.0       # escreve nos arquivos que a guardam
#   .github/scripts/versao.sh conferir           # todos os arquivos batem?
#
# ## Por que um trailer, e não conventional commits
#
# O CLAUDE.md diz, em tantas palavras, que o assunto do commit é em português,
# sentence case, **sem prefixo e sem conventional commits** — ele descreve o
# efeito da mudança, não a categoria dela. Derivar a versão de um `feat:` ou de
# um `fix:` no assunto exigiria desfazer essa regra em todo commit futuro.
#
# O que o projeto já usa, e em todo commit, é **trailer**: o
# `Co-Authored-By:` do fim da mensagem. Então a categoria entra por ali:
#
#     Mais ar embaixo das fileiras de botões
#
#     <corpo longo em prosa, explicando causa e raciocínio>
#
#     Impacto: funcionalidade
#     Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
#
# Três valores, e nada mais:
#
#     correcao       → PATCH   conserto, ajuste, texto, documentação
#     funcionalidade → MINOR   coisa nova que não quebra quem já usa
#     incompativel   → MAJOR   quebra quem já usa (veja a ressalva do 0.x)
#
# **Sem o trailer, o commit vale PATCH.** É o padrão certo para este projeto:
# a maioria dos commits é conserto, e esquecer o trailer não pode publicar uma
# versão que promete mais do que mudou. O caminho do erro aponta para baixo.
#
# ## A ressalva do 0.x
#
# Enquanto o MAJOR for 0, `incompativel` sobe o **MINOR** (0.5.0 → 0.6.0), e não
# o MAJOR. É o que a própria SemVer manda para a linha 0.x: "qualquer coisa pode
# mudar a qualquer momento; a API pública não deve ser considerada estável".
# Chegar ao 1.0.0 é uma decisão, e não o efeito colateral do primeiro commit que
# renomeou alguma coisa — quem quiser dá `major` explícito no disparo do
# workflow, que ignora esta regra.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$RAIZ"

CARGO_TOML="Cargo.toml"
METADATA_GNOME="gnome-extension/metadata.json"
METADATA_KDE="kde-plasma/plasmoid/package/metadata.json"

# ─── os arquivos que guardam a versão ────────────────────────────────────────

atual() {
    grep -m1 '^version = ' "$CARGO_TOML" | cut -d'"' -f2
}

versao_do_lock() {
    # O bloco do próprio pacote no Cargo.lock: `name = "ditador"` e, na linha
    # seguinte, a versão. O `-A1` é o que evita pegar a versão de uma dependência
    # qualquer que tenha "ditador" no nome.
    grep -A1 '^name = "ditador"$' Cargo.lock | grep -m1 '^version = ' | cut -d'"' -f2
}

versao_da_extensao() {
    grep -m1 '"version-name"' "$METADATA_GNOME" | cut -d'"' -f4
}

versao_do_widget() {
    grep -m1 '"Version"' "$METADATA_KDE" | cut -d'"' -f4
}

# ─── o impacto, lido dos commits ─────────────────────────────────────────────

ultima_tag() {
    git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true
}

# Tira os acentos e baixa a caixa, para que "Correção", "correcao" e "CORREÇÃO"
# sejam a mesma coisa. Quem escreve a mensagem do commit não deve ter de lembrar
# de qual grafia o script entende.
#
# **A tabela vem antes do `tr`, e tem as maiúsculas acentuadas.** Era o
# contrário, e a promessa do parágrafo acima não se cumpria: o `tr
# '[:upper:]' '[:lower:]'` não baixa a caixa de caractere multibyte, então
# "CORREÇÃO" chegava à tabela como "correÇÃo" — com o Ç e o Ã ainda maiúsculos,
# que não estavam nela — e saía "correÇÃo", que não casa com nada. O `case` do
# `impacto` então caía no ramo do desconhecido e tratava a mudança como
# correção. Trocando a ordem e acrescentando as maiúsculas, os seis jeitos de
# escrever a palavra dão no mesmo.
normalizar() {
    sed 'y/ÁÀÂÃÉÊÍÓÔÕÚÜÇÑáàâãéêíóôõúüçñ/AAAAEEIOOOUUCNaaaaeeiooouucn/' \
        | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]'
}

# Os valores que o trailer aceita, já normalizados. Fora daqui é erro de
# digitação, e o `impacto` diz isso em vez de adivinhar.
impacto_de() {
    case "$(printf '%s' "$1" | normalizar)" in
        incompativel|quebra|major|breaking) printf 'incompativel\n' ;;
        funcionalidade|recurso|minor|feature) printf 'funcionalidade\n' ;;
        correcao|conserto|patch|fix) printf 'correcao\n' ;;
        *) return 1 ;;
    esac
}

# Os valores do trailer `Impacto:` de um commit — com uma rede embaixo.
#
# ## A rede, e por que ela existe
#
# O git só reconhece um trailer quando ele está no **último bloco** da mensagem,
# sem linha em branco separando-o dos outros trailers. Uma linha em branco entre
# o `Impacto:` e o `Co-Authored-By:` faz o `Impacto:` deixar de ser trailer e
# virar texto comum — e o `%(trailers:key=Impacto)` devolve vazio.
#
# Isso aconteceu de verdade, e caro: o commit dos dez recursos da 0.6.1 trazia
# `Impacto: funcionalidade` com uma linha em branco a mais, o script leu vazio,
# a regra "sem trailer vale PATCH" entrou em ação, e dez recursos novos foram
# publicados como uma correção. Em silêncio: nada na saída dizia que havia uma
# linha `Impacto:` ali que não estava sendo lida.
#
# Com o disparo automático — todo push no main publica — esse erro deixaria de
# ser um episódio e passaria a ser permanente. Então: quando o trailer de
# verdade não existe, o valor é procurado numa linha `Impacto:` no começo de
# qualquer linha da mensagem, e o script **avisa** que a formatação está errada.
# O aviso vai para a saída de erro, que a CI mostra no log e no resumo.
#
# Só valores conhecidos entram por esse caminho: um "Impacto:" citado no meio de
# uma frase em prosa — a documentação deste projeto fala dele o tempo todo — não
# pode virar decisão de versão.
valores_de_impacto() {
    local sha="$1" valores
    valores="$(git log -1 "$sha" --format='%(trailers:key=Impacto,valueonly,separator=%x0A)')"
    if [ -n "$valores" ]; then
        printf '%s\n' "$valores"
        return 0
    fi

    local solta
    while IFS= read -r solta; do
        [ -n "$solta" ] || continue
        if impacto_de "$solta" >/dev/null 2>&1; then
            echo "!! O commit $(git log -1 --format=%h "$sha") tem uma linha \"Impacto: $solta\"" >&2
            echo "   que o git NÃO lê como trailer — provavelmente há uma linha em branco" >&2
            echo "   entre ela e os outros trailers. O valor foi aproveitado assim mesmo," >&2
            echo "   mas conserte a mensagem: trailers ficam todos no último bloco, juntos." >&2
            printf '%s\n' "$solta"
        fi
    done < <(git log -1 "$sha" --format=%B | sed -n 's/^[Ii]mpacto:[[:space:]]*//p')
}

impacto() {
    local tag faixa
    tag="$(ultima_tag)"
    faixa="${tag:+$tag..}HEAD"

    local maior="correcao"
    local sha valor lido
    while read -r sha; do
        [ -n "$sha" ] || continue
        while read -r valor; do
            [ -n "$valor" ] || continue
            if ! lido="$(impacto_de "$valor")"; then
                echo "!! Impacto desconhecido num commit: \"$valor\" (tratado como correção)" >&2
                continue
            fi
            case "$lido" in
                incompativel) maior="incompativel" ;;
                funcionalidade)
                    [ "$maior" = "correcao" ] && maior="funcionalidade"
                    ;;
            esac
        done < <(valores_de_impacto "$sha")
    done < <(git log --no-merges "$faixa" --format=%H)

    printf '%s\n' "$maior"
}

# ─── a próxima versão ────────────────────────────────────────────────────────

proxima() {
    local pedido="${1:-auto}"
    local base
    base="$(atual)"

    local tipo
    case "$pedido" in
        auto)
            case "$(impacto)" in
                incompativel) tipo=major ;;
                funcionalidade) tipo=minor ;;
                *) tipo=patch ;;
            esac
            ;;
        patch|minor|major) tipo="$pedido" ;;
        *) echo "Incremento inválido: $pedido (use auto, patch, minor ou major)" >&2; exit 2 ;;
    esac

    local IFS=.
    read -r maior menor correcao <<<"$base"

    # A ressalva do 0.x, explicada no cabeçalho. Vale só para o `auto`: um
    # `major` digitado à mão é uma decisão de quem digitou, e o script não
    # discute com ela.
    if [ "$tipo" = major ] && [ "$maior" = 0 ] && [ "$pedido" = auto ]; then
        echo "-- Mudança incompatível na linha 0.x: subindo o MINOR, e não o MAJOR." >&2
        echo "   (para chegar ao 1.0.0, dispare o workflow com incremento=major)" >&2
        tipo=minor
    fi

    case "$tipo" in
        major) printf '%d.0.0\n' "$((maior + 1))" ;;
        minor) printf '%d.%d.0\n' "$maior" "$((menor + 1))" ;;
        patch) printf '%d.%d.%d\n' "$maior" "$menor" "$((correcao + 1))" ;;
    esac
}

# ─── gravar ──────────────────────────────────────────────────────────────────

gravar() {
    local nova="${1:?uso: versao.sh gravar X.Y.Z}"
    [[ "$nova" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
        echo "Versão inválida: $nova (esperado X.Y.Z)" >&2; exit 2; }

    # O Cargo.toml é a fonte da verdade; os outros são cópias que precisam
    # concordar com ele. A ordem aqui é essa mesma.
    sed -i "0,/^version = \".*\"/s//version = \"$nova\"/" "$CARGO_TOML"

    # A extensão do GNOME leva o número no `version-name`, que é o que aparece
    # em Extensões e no site extensions.gnome.org. O `version` numérico do
    # e.g.o não é nosso — quem o atribui é o site, na revisão.
    sed -i "s/\"version-name\": \".*\"/\"version-name\": \"$nova\"/" "$METADATA_GNOME"

    # O widget do Plasma **não** entra aqui: o `Version` dele é 0.0.0 no
    # repositório de propósito, e quem o preenche é o `kde-plasma/instalar.sh`
    # (e o CMakeLists), lendo o Cargo.toml. Está no CLAUDE.md.

    # O Cargo.lock é versionado e traz a versão do próprio pacote. Qualquer
    # comando do cargo o reescreve; o `metadata` é o mais barato que faz isso.
    if command -v cargo >/dev/null; then
        cargo metadata --format-version 1 >/dev/null
    else
        echo "!! cargo não encontrado: o Cargo.lock ficou com a versão antiga." >&2
        exit 1
    fi

    echo "$nova gravado em: $CARGO_TOML, Cargo.lock, $METADATA_GNOME"
}

# ─── conferir ────────────────────────────────────────────────────────────────

conferir() {
    local esperada erros=0
    esperada="$(atual)"
    echo "Versão do Cargo.toml: $esperada"

    conferir_um() {
        local nome="$1" achado="$2" queria="$3"
        if [ "$achado" = "$queria" ]; then
            printf 'ok  %-46s %s\n' "$nome" "$achado"
        else
            printf '!!  %-46s %s (esperado: %s)\n' "$nome" "$achado" "$queria"
            erros=$((erros + 1))
        fi
    }

    conferir_um "Cargo.lock" "$(versao_do_lock)" "$esperada"
    conferir_um "$METADATA_GNOME (version-name)" "$(versao_da_extensao)" "$esperada"
    # Este é o contrário dos outros: tem de continuar sendo 0.0.0. Quem escrever
    # a versão à mão aqui cria uma segunda fonte da verdade, que é justamente o
    # que o CLAUDE.md proíbe.
    conferir_um "$METADATA_KDE (Version, fixo em 0.0.0)" "$(versao_do_widget)" "0.0.0"

    if [ "$erros" -gt 0 ]; then
        echo
        echo "$erros arquivo(s) fora de sincronia. Para acertar:"
        echo "  .github/scripts/versao.sh gravar $esperada"
        return 1
    fi
    echo
    echo "Todos os arquivos concordam."
}

case "${1:-}" in
    atual)    atual ;;
    impacto)  impacto ;;
    proxima)  proxima "${2:-auto}" ;;
    gravar)   gravar "${2:-}" ;;
    conferir) conferir ;;
    *)
        sed -n '2,8p' "$0" | sed 's/^# \?//'
        exit 2
        ;;
esac
