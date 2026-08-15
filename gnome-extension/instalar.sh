#!/usr/bin/env bash
# Empacota e instala a extensão do GNOME Shell, só para este usuário.
#
# É independente do ./instalar.sh da raiz de propósito: quem usa outra área de
# trabalho — ou o GNOME sem esta extensão — instala o Ditador e nunca passa por
# aqui. O caminho contrário não vale: sem o aplicativo instalado, a extensão
# fica no ar dizendo "Indisponível" e nada mais.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UUID="ditador@danielfreitasdev.github.io"
PACOTE="$AQUI/${UUID}.shell-extension.zip"

if ! command -v gnome-extensions >/dev/null; then
    echo "!! O comando 'gnome-extensions' não está aqui — isto precisa do GNOME Shell." >&2
    exit 1
fi

# A extensão declara `"shell-version": ["50"]`, e o GNOME recusa instalar uma
# extensão que não declare a versão dele. Avisar aqui é melhor do que deixar o
# erro aparecer como "extensão desatualizada" depois de tudo pronto.
VERSAO="$(gnome-shell --version | sed 's/[^0-9.]//g')"
case "$VERSAO" in
    50|50.*) ;;
    *)
        echo "!! Esta extensão é para o GNOME Shell 50; aqui está o $VERSAO."
        echo "   A instalação segue, mas o Shell vai recusá-la."
        ;;
esac

echo "==> Empacotando"
cd "$AQUI"
# `--extra-source=src` porque só a raiz entra sozinha. O `scripts/` fica de
# fora de propósito: é ferramenta de quem desenvolve, não da extensão.
gnome-extensions pack --force --extra-source=src --out-dir="$AQUI" .

echo "==> Instalando em ~/.local/share/gnome-shell/extensions"
# É o `install` que compila o schema (`gschemas.compiled`) na pasta de destino;
# o ZIP leva só o XML. Instalar à mão, copiando arquivos, deixaria a extensão
# sem as preferências dela.
gnome-extensions install --force "$PACOTE"

echo "==> Habilitando"
# O GNOME Shell varre a pasta de extensões uma vez, ao subir (`_loadExtensions`
# em `js/ui/extensionSystem.js`) — não há vigia de diretório para extensões
# novas. Numa sessão Wayland não existe recarregar o Shell, então uma extensão
# recém-instalada só passa a existir para ele no próximo login.
#
# O `enable` daqui pode falhar por isso, e não é problema: ele grava a
# preferência em `org.gnome.shell enabled-extensions`, e o Shell obedece a ela
# quando carregar. Por isso o `gsettings` como reserva.
if ! gnome-extensions enable "$UUID" 2>/dev/null; then
    python3 - "$UUID" <<'PY'
import subprocess, sys
uuid = sys.argv[1]
CHAVE = ['gsettings', 'get', 'org.gnome.shell', 'enabled-extensions']
atual = subprocess.run(CHAVE, capture_output=True, text=True).stdout.strip()
if uuid in atual:
    sys.exit(0)
lista = atual[1:-1].strip()
nova = f"[{lista + ', ' if lista else ''}'{uuid}']"
subprocess.run(['gsettings', 'set', 'org.gnome.shell', 'enabled-extensions', nova], check=True)
PY
    PRECISA_SAIR=1
fi

cat <<FIM

Instalada.

  Estado:        gnome-extensions info $UUID
  Preferências:  gnome-extensions prefs $UUID
  Desabilitar:   gnome-extensions disable $UUID
  Remover:       gnome-extensions uninstall $UUID
  Registros:     journalctl --user -o cat /usr/bin/gnome-shell -f

FIM

if [ -n "${PRECISA_SAIR:-}" ]; then
    cat <<'FIM'
!! Saia da sessão e entre de novo para a extensão subir.

   O GNOME Shell só procura extensões novas ao iniciar, e numa sessão Wayland
   não há como reiniciá-lo sem isso — o Alt+F2 seguido de "r" só existe no X11,
   que o GNOME 50 não tem mais. Ela já está marcada para ligar sozinha.

FIM
fi
