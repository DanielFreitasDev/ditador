"""Monta as imagens do README a partir das capturas do próprio programa.

As capturas que o `DITADOR_CAPTURA` grava têm fundo transparente — a janela do
Ditador não tem decoração e os cantos são arredondados —, então aqui elas são
pousadas sobre uma tela de fundo lisa, na cor do tema que aparece na imagem.

    ./gerar-imagens.sh          # roda o programa nos dois temas e chama isto

Ou à mão, se as capturas já estiverem prontas:

    DITADOR_CAPTURA=/tmp/tiros python3 assets/compor-capturas.py

Espera encontrar `<pasta>-claro/` e `<pasta>-escuro/`, cada uma com
`recording.png`, `result.png` e `settings.png`.
"""

import os
import pathlib
import sys

from PIL import Image

# Fundo de cada tema. São cores próximas das da janela, um degrau afastadas: a
# janela precisa de contraste para a borda e a sombra aparecerem, mas a imagem
# não pode virar uma moldura.
FUNDO = {"claro": (238, 238, 241), "escuro": (16, 16, 16)}

# (arquivo de saída, captura, temas lado a lado, folga em volta, largura final)
CENAS = [
    ("gravando", "recording", ["claro", "escuro"], 56, 1200),
    ("resultado", "result", ["claro"], 64, 900),
    ("configuracoes", "settings", ["escuro"], 56, 760),
]


def pousar(janela: Image.Image, tema: str, folga: int) -> Image.Image:
    """A janela centrada numa tela lisa da cor do tema."""
    tela = Image.new(
        "RGB", (janela.width + 2 * folga, janela.height + 2 * folga), FUNDO[tema]
    )
    tela.paste(janela, (folga, folga), janela)
    return tela


def main() -> None:
    base = os.environ.get("DITADOR_CAPTURA", "/tmp/ditador-capturas")
    saida = pathlib.Path(__file__).resolve().parent / "capturas"
    saida.mkdir(parents=True, exist_ok=True)

    for nome, captura, temas, folga, largura_final in CENAS:
        partes = []
        for tema in temas:
            origem = pathlib.Path(f"{base}-{tema}") / f"{captura}.png"
            if not origem.exists():
                print(f"faltando: {origem}", file=sys.stderr)
                break
            partes.append(pousar(Image.open(origem).convert("RGBA"), tema, folga))
        if len(partes) != len(temas):
            continue

        # Com dois temas as duas metades ficam encostadas, sem nada entre elas:
        # o corte seco é o que mostra que é a mesma janela nos dois desenhos.
        altura = max(p.height for p in partes)
        cena = Image.new("RGB", (sum(p.width for p in partes), altura))
        x = 0
        for parte in partes:
            cena.paste(parte, (x, (altura - parte.height) // 2))
            x += parte.width

        alvo = (largura_final, round(cena.height * largura_final / cena.width))
        cena = cena.resize(alvo, Image.LANCZOS)
        destino = saida / f"{nome}.png"
        cena.save(destino, optimize=True)
        print(f"{destino}  {alvo[0]}×{alvo[1]}  {destino.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
