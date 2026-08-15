#!/usr/bin/env python3
"""Gera os ícones .ico que o Ditador.Windows usa.

Roda com Pillow e mais nada:

    python windows-integration/scripts/gerar-icones.py

Os arquivos saem em `windows-integration/src/Ditador.Windows/Assets/` e são
**commitados**, como os PNGs de `assets/png/`: quem compila o frontend não
precisa de Python, e o binário nunca fica com o ícone de ontem.

## Por que não reaproveitar os PNGs que já existem

Os de `assets/png/bandeja-*.png` são para o GNOME, que os recolore sozinho
conforme o tema da barra — eles são brancos com alfa e nada mais. O Windows não
recolore ícone nenhum: o que o `Shell_NotifyIcon` recebe é o que aparece. Um
ícone branco some na barra de tarefas clara, e um preto some na escura.

Então aqui saem **dois** conjuntos, e o frontend troca de um para o outro quando
o tema do sistema muda (`WM_SETTINGCHANGE`). É o que os aplicativos nativos do
Windows fazem, e é a razão de existir o par `-claro`/`-escuro` no nome:

    bandeja-pronto-claro.ico   → glifo escuro, para a barra clara
    bandeja-pronto-escuro.ico  → glifo claro, para a barra escura

## Por que quatro estados e não cinco

São os mesmos quatro que a barra do GNOME distingue (`icones::Estado`, no Rust):
carregar o modelo e transcrever viram os dois "trabalhando", porque para quem
olha uma imagem de 16 pixels os dois querem dizer "espere". A distinção fina
continua existindo no texto da dica de ferramenta, que é onde ela cabe — e é de
lá que o Narrator a lê.
"""

from pathlib import Path

from PIL import Image, ImageDraw

# O desenho é feito neste tamanho e reduzido para cada um dos que o .ico leva.
# Quatro vezes o maior ícone da lista dá suavização de sobra sem custo nenhum:
# isto roda uma vez por alteração do desenho, não por build.
BASE = 512

# Os tamanhos que um .ico do Windows precisa ter para atravessar as escalas de
# 100% a 200% sem ninguém esticar bitmap. O Shell escolhe o mais próximo.
TAMANHOS = [16, 20, 24, 32, 40, 48, 64, 256]

# As cores do glifo. Não são preto e branco puros: na barra de tarefas clara o
# preto absoluto fica mais pesado que os ícones do sistema ao lado, e o #171717
# é justamente o da pastilha do `assets/ditador.svg` — o mesmo desenho, a mesma
# tinta.
ESCURO = (23, 23, 23, 255)  # glifo para barra clara
CLARO = (255, 255, 255, 255)  # glifo para barra escura

# O vermelho do "gravando" é o mesmo da interface do egui (`src/tema.rs`), e o
# âmbar do "trabalhando" acompanha. Eles não são a única diferença entre os
# estados — veja as formas abaixo —, mas ajudam quem enxerga cor.
VERMELHO = (229, 72, 77, 255)
AMBAR = (245, 165, 36, 255)


def microfone(tela: ImageDraw.ImageDraw, cor: tuple[int, int, int, int]) -> None:
    """A cápsula, o berço e o pé, nas mesmas coordenadas do `assets/ditador.svg`.

    Repetidas aqui em vez de lidas de lá porque o Pillow não lê SVG e trazer um
    rasterizador para o projeto por três formas seria caro; a garantia de que
    elas continuam iguais é o olho de quem mexer no SVG, e por isso o comentário
    está nos dois arquivos.
    """
    # Cápsula: retângulo 116×190 com raio 58 (ou seja, um bastão de topos
    # redondos).
    tela.rounded_rectangle((198, 103, 314, 293), radius=58, fill=cor)

    # Berço: a faixa entre os raios 82 e 106, na metade de baixo. Desenhada como
    # um arco grosso, que é como o Pillow faz uma faixa circular sem precisar de
    # dois caminhos.
    largura = 24
    raio = 94  # o meio entre 82 e 106
    tela.arc(
        (256 - raio, 252 - raio, 256 + raio, 252 + raio),
        start=0,
        end=180,
        fill=cor,
        width=largura,
    )

    # Pé.
    tela.rounded_rectangle((245, 358, 267, 412), radius=11, fill=cor)


def emblema(tela: ImageDraw.ImageDraw, estado: str, cor_de_fundo: tuple) -> None:
    """A marca de estado no canto inferior direito, quando há uma.

    Cada estado tem **forma** própria, e não só cor: um ponto cheio, um anel e um
    triângulo se distinguem em 16 pixels e em tela monocromática, e quem não
    enxerga cor continua tendo o que ler. A cor é o reforço, nunca a informação.

    O contorno na cor do fundo é o que separa o emblema do microfone quando os
    dois se tocam — sem ele, em 16 px, o ponto vermelho vira um borrão colado na
    cápsula.
    """
    if estado == "pronto":
        return

    # O canto de baixo à direita, com folga para a borda do ícone.
    cx, cy, r = 386, 386, 104

    if estado == "gravando":
        # Ponto cheio: o mesmo símbolo universal de "gravando".
        tela.ellipse(
            (cx - r, cy - r, cx + r, cy + r),
            fill=VERMELHO,
            outline=cor_de_fundo,
            width=22,
        )
    elif estado == "trabalhando":
        # Anel: cheio por fora, vazado por dentro. Lê-se como "em curso" e não se
        # confunde com o ponto cheio nem em tamanho pequeno.
        tela.ellipse(
            (cx - r, cy - r, cx + r, cy + r),
            fill=AMBAR,
            outline=cor_de_fundo,
            width=22,
        )
        interno = r - 46
        tela.ellipse(
            (cx - interno, cy - interno, cx + interno, cy + interno),
            fill=(0, 0, 0, 0),
        )
    elif estado == "falhou":
        # Triângulo: a forma que o Windows inteiro usa para alerta.
        altura = r * 1.9
        lado = r * 2.1
        pontos = [
            (cx, cy - altura / 2),
            (cx - lado / 2, cy + altura / 2),
            (cx + lado / 2, cy + altura / 2),
        ]
        tela.polygon(pontos, fill=VERMELHO, outline=cor_de_fundo, width=22)
        # A barra do "!", em vazado.
        tela.rounded_rectangle(
            (cx - 13, cy - 34, cx + 13, cy + 22), radius=13, fill=(0, 0, 0, 0)
        )
        tela.ellipse((cx - 13, cy + 34, cx + 13, cy + 60), fill=(0, 0, 0, 0))
    else:
        raise ValueError(f"estado desconhecido: {estado}")


# Quanto o microfone é ampliado no ícone da área de notificação.
#
# No ícone do aplicativo ele mora dentro de uma pastilha e a margem é o desenho;
# solto na barra de tarefas, a mesma margem vira desperdício — em 16 pixels o
# glifo ocupava pouco mais da metade da altura e ficava franzino ao lado dos
# ícones do sistema, que encostam nas bordas. Um terço a mais resolve sem cortar:
# 309 × 1,33 = 411 pontos de altura em 512.
AMPLIACAO = 1.33


def bandeja(estado: str, tema: str) -> Image.Image:
    """Um ícone da área de notificação, em `BASE`×`BASE` e com fundo transparente."""
    cor = ESCURO if tema == "claro" else CLARO
    # O "fundo" para efeito de contorno é a cor da barra por trás do ícone. Ela é
    # transparente de verdade, mas o contorno precisa de uma cor sólida para
    # abrir espaço em volta do emblema; a da barra é a que some.
    fundo = (255, 255, 255, 255) if tema == "claro" else (32, 32, 32, 255)

    # O microfone é desenhado à parte para poder ser ampliado inteiro; o emblema
    # vem depois, em cima, nas coordenadas de sempre — ele já nasce no tamanho
    # certo para o canto.
    camada = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    microfone(ImageDraw.Draw(camada), cor)
    esticada = round(BASE * AMPLIACAO)
    camada = camada.resize((esticada, esticada), Image.LANCZOS)
    canto = (BASE - esticada) // 2

    imagem = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    imagem.paste(camada, (canto, canto), camada)
    emblema(ImageDraw.Draw(imagem), estado, fundo)
    return imagem


def aplicativo() -> Image.Image:
    """O ícone do próprio aplicativo: a pastilha escura com o microfone branco.

    É o `assets/ditador.svg` redesenhado — o mesmo que a janela do egui já usa no
    Linux e no Windows. Aqui ele serve à janela do popup, ao Alt+Tab e ao atalho
    do menu Iniciar.
    """
    imagem = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    tela = ImageDraw.Draw(imagem)
    # A pastilha. O SVG usa uma superelipse de expoente 5; um retângulo
    # arredondado de raio 116 fica a menos de dois pixels dela em 512, e a
    # diferença some no tamanho em que este ícone é visto.
    tela.rounded_rectangle((24, 24, 488, 488), radius=116, fill=(23, 23, 23, 255))
    microfone(tela, CLARO)
    return imagem


def gravar(imagem: Image.Image, destino: Path) -> None:
    destino.parent.mkdir(parents=True, exist_ok=True)
    # O Pillow monta o .ico multitamanho sozinho a partir da imagem grande,
    # reduzindo com LANCZOS.
    imagem.save(destino, format="ICO", sizes=[(t, t) for t in TAMANHOS])
    print(f"  {destino.name}")


def main() -> None:
    raiz = Path(__file__).resolve().parents[1]
    destino = raiz / "src" / "Ditador.Windows" / "Assets"

    print("Ícones do Ditador para Windows")
    for estado in ("pronto", "gravando", "trabalhando", "falhou"):
        for tema in ("claro", "escuro"):
            gravar(bandeja(estado, tema), destino / f"bandeja-{estado}-{tema}.ico")
    gravar(aplicativo(), destino / "ditador.ico")

    # O PNG grande serve à janela do popup, onde o XAML quer um bitmap e não um
    # HICON.
    logo = aplicativo().resize((256, 256), Image.LANCZOS)
    logo.save(destino / "ditador-256.png")
    print("  ditador-256.png")


if __name__ == "__main__":
    main()
