"""Monta as imagens do README.

As capturas que o `DITADOR_CAPTURA` grava têm fundo transparente — a janela do
Ditador é transparente mesmo, e o vidro conta com isso. Vê-las sozinhas engana:
metade do efeito é o que aparece por baixo. Então aqui elas são compostas sobre
o papel de parede, na posição exata em que a janela aparece na tela, que é o que
o usuário de fato vê.

    DITADOR_CAPTURA=/tmp/tiros cargo run --release   # gera os PNGs
    DITADOR_CAPTURA=/tmp/tiros python3 assets/compor-capturas.py

Variáveis: `DITADOR_CAPTURA` (pasta dos PNGs), `DITADOR_PAREDE` (imagem de
fundo; o padrão é o papel de parede do GNOME), `DITADOR_TELA` ("1920x1080").
"""

import os
import pathlib
import subprocess
import sys
import urllib.parse

from PIL import Image

# (arquivo, onde a janela fica, margem em volta no recorte, largura final)
CENAS = [
    ("recording", "baixo", 130, 900),
    ("result", "baixo", 110, 900),
    ("settings", "centro", 80, 760),
]

# A janela é posta acima da borda de baixo com esta folga — o mesmo número que
# `apply_window`, em ui.rs, usa.
FOLGA_INFERIOR = 130


def papel_de_parede() -> str:
    if caminho := os.environ.get("DITADOR_PAREDE"):
        return caminho
    for chave in ("picture-uri-dark", "picture-uri"):
        try:
            uri = subprocess.run(
                ["gsettings", "get", "org.gnome.desktop.background", chave],
                capture_output=True, text=True, check=True,
            ).stdout.strip().strip("'")
        except (OSError, subprocess.CalledProcessError):
            continue
        if uri.startswith("file://"):
            caminho = urllib.parse.unquote(uri[len("file://"):])
            if pathlib.Path(caminho).is_file():
                return caminho
    sys.exit("não achei o papel de parede; passe DITADOR_PAREDE=<imagem>")


def tela() -> tuple[int, int]:
    largura, _, altura = os.environ.get("DITADOR_TELA", "1920x1080").partition("x")
    return int(largura), int(altura)


def fundo(tamanho: tuple[int, int]) -> Image.Image:
    """O papel de parede cobrindo a tela, como o GNOME o desenha."""
    img = Image.open(papel_de_parede()).convert("RGB")
    escala = max(tamanho[0] / img.width, tamanho[1] / img.height)
    novo = (round(img.width * escala), round(img.height * escala))
    img = img.resize(novo, Image.LANCZOS)
    esq, topo = (novo[0] - tamanho[0]) // 2, (novo[1] - tamanho[1]) // 2
    return img.crop((esq, topo, esq + tamanho[0], topo + tamanho[1]))


def main() -> None:
    tam = tela()
    parede = fundo(tam)
    tiros = pathlib.Path(os.environ.get("DITADOR_CAPTURA", "/tmp/ditador-capturas"))
    saida = pathlib.Path(__file__).resolve().parent / "capturas"
    saida.mkdir(parents=True, exist_ok=True)

    for nome, onde, margem, largura_final in CENAS:
        origem = tiros / f"{nome}.png"
        if not origem.exists():
            print(f"faltando: {origem}", file=sys.stderr)
            continue

        janela = Image.open(origem).convert("RGBA")
        w, h = janela.size
        x = (tam[0] - w) // 2
        y = (tam[1] - h) // 2 if onde == "centro" else tam[1] - h - FOLGA_INFERIOR

        cena = parede.copy()
        cena.paste(janela, (x, y), janela)

        caixa = (
            max(x - margem, 0),
            max(y - margem, 0),
            min(x + w + margem, tam[0]),
            min(y + h + margem, tam[1]),
        )
        recorte = cena.crop(caixa)
        alvo = (largura_final, round(recorte.height * largura_final / recorte.width))
        recorte = recorte.resize(alvo, Image.LANCZOS)

        destino = saida / f"{nome}.jpg"
        recorte.save(destino, quality=94, optimize=True, progressive=True)
        print(f"{destino}  {alvo[0]}×{alvo[1]}  {destino.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
