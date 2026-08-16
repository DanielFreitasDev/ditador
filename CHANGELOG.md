# Histórico de versões

Este arquivo é escrito pelo workflow de publicação, e não à mão: a cada versão
lançada ele ganha uma seção nova, montada dos assuntos dos commits desde a tag
anterior e agrupada pelo trailer `Impacto:` de cada um. É a mesma lista que vai
para o corpo da release no GitHub.

Por isso o assunto do commit importa tanto: ele é o que aparece aqui, para quem
nunca vai abrir o repositório. O CLAUDE.md pede que ele descreva o **efeito** da
mudança — este arquivo é o motivo.

Como publicar uma versão: [`docs/CI-E-RELEASES.md`](docs/CI-E-RELEASES.md).

<!-- as versões novas entram logo abaixo desta linha -->

## Antes da automação

O histórico até a **0.5.0** não está aqui, e não vai ser reconstruído: escrever
hoje um changelog de versões que foram publicadas sem ele daria um texto
plausível e não verificado, que é pior do que a lacuna. Ele está no `git log`,
que é onde sempre esteve:

```bash
git log --oneline v0.2.0..v0.5.0
```

As tags também têm buracos — o repositório chegou à 0.4.2 tendo `v0.2.0` como
única tag, e as versões puladas não foram tagueadas depois de propósito: uma tag
inventada meses depois aponta para um commit que nunca foi empacotado nem
publicado. É justamente esse esquecimento que a automação existe para tornar
impossível daqui para frente.
