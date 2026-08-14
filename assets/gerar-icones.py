#!/usr/bin/env python3
"""Rasteriza os SVGs do Ditador.

Duas saídas, ambas versionadas no repositório para que quem clonar não precise
de ferramenta nenhuma:

  png/ditador-<tamanho>.png      o ícone colorido, para o tema hicolor
  png/bandeja-<estado>.png       os símbolos em branco, embutidos no binário
                                 (src/tray.rs) como reserva quando o tema do
                                 sistema não tem os nossos ícones instalados

Precisa do librsvg pelo GdkPixbuf, que já vem no GNOME:
    python3 assets/gerar-icones.py
"""

import pathlib
import sys
import tempfile

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

AQUI = pathlib.Path(__file__).resolve().parent
SAIDA = AQUI / "png"

# O tema hicolor procura estes tamanhos; o 512 é o que o GNOME usa na visão de
# atividades em telas grandes.
TAMANHOS = [16, 24, 32, 48, 64, 128, 256, 512]

# Estados da barra superior. O nome do arquivo vira o nome no `tray.rs`.
ESTADOS = {
    "pronto": "ditador-symbolic.svg",
    "gravando": "ditador-gravando-symbolic.svg",
    "carregando": "ditador-carregando-symbolic.svg",
    "falhou": "ditador-falhou-symbolic.svg",
}
# Duas resoluções: a barra do GNOME pede 22 px, e 44 cobre as telas em 2x.
TAMANHOS_BANDEJA = [22, 44]


def rasterizar(svg: pathlib.Path, destino: pathlib.Path, tamanho: int) -> None:
    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(str(svg), tamanho, tamanho)
    pixbuf.savev(str(destino), "png", [], [])


def main() -> int:
    SAIDA.mkdir(exist_ok=True)

    colorido = AQUI / "ditador.svg"
    for tamanho in TAMANHOS:
        destino = SAIDA / f"ditador-{tamanho}.png"
        rasterizar(colorido, destino, tamanho)
        print(destino.relative_to(AQUI.parent))

    for estado, arquivo in ESTADOS.items():
        original = (AQUI / "simbolicos" / arquivo).read_text()
        # Os símbolos são pintados de #2e3436 para o GTK recolorir; o pixmap que
        # vai embutido no binário não passa pelo GTK, então já sai branco — a
        # barra superior do GNOME é escura em qualquer tema.
        branco = original.replace('fill="#2e3436"', 'fill="#ffffff"')
        with tempfile.NamedTemporaryFile("w", suffix=".svg") as tmp:
            tmp.write(branco)
            tmp.flush()
            for tamanho in TAMANHOS_BANDEJA:
                destino = SAIDA / f"bandeja-{estado}-{tamanho}.png"
                rasterizar(pathlib.Path(tmp.name), destino, tamanho)
                print(destino.relative_to(AQUI.parent))

    return 0


if __name__ == "__main__":
    sys.exit(main())
