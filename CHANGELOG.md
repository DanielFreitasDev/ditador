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

## 0.8.0 — 2026-08-18

### Novidades

- Aparar o silêncio, soltar o modelo parado, avisar de versão nova e escolher modelo por lista ([`a4eae9d`](https://github.com/DanielFreitasDev/ditador/commit/a4eae9d80714e6626dd52a90d468b0af4838323c))

### Correções e ajustes

- Publicar uma versão passa a terminar com os comandos de atualizar a máquina ([`dde889e`](https://github.com/DanielFreitasDev/ditador/commit/dde889eca907add111b81c7583681c4aab53588b))

[Todas as mudanças desde a `v0.7.3`](https://github.com/DanielFreitasDev/ditador/compare/v0.7.3...v0.8.0)

## 0.7.3 — 2026-08-18

### Correções e ajustes

- O baixar-modelo.sh volta a aceitar o modelo que ele mesmo acabou de baixar ([`ef9e58e`](https://github.com/DanielFreitasDev/ditador/commit/ef9e58e2588137fa1ed234b26cd1d6cdeafbc398))
- Baixar o modelo pelo terminal deixa de ser um beco sem saída quando o arquivo não presta ([`2d95f6e`](https://github.com/DanielFreitasDev/ditador/commit/2d95f6edf8bb9e19f2d74bbcca0f8742198144ad))
- Um toque na tecla deixa de levar embora o áudio da frase que ainda está no Whisper ([`cb75008`](https://github.com/DanielFreitasDev/ditador/commit/cb75008d84e3601883b1d85dfb6fb0002eb64b5a))
- Nada que chegue tarde toma mais a janela de quem já está falando ou salvando configurações ([`fcff1f3`](https://github.com/DanielFreitasDev/ditador/commit/fcff1f3794df4f6965b35ef2a37c155c31af340f))
- A captura de atalho não sobrevive mais à tela que a explica ([`cb90e39`](https://github.com/DanielFreitasDev/ditador/commit/cb90e39fe3f5da61b2c97aee6dc9a39ed2a12288))
- O "há 5 min" da lista de transcrições volta a andar ([`89e77d4`](https://github.com/DanielFreitasDev/ditador/commit/89e77d4bcd46cc70f7c1dd1abc384537e6f3ce1d))
- Reinstalar com o Ditador rodando deixa de desligá-lo ([`caf7a4d`](https://github.com/DanielFreitasDev/ditador/commit/caf7a4db16f855709d31248a4a568c5bae131428))
- O .deb não sai mais com um arquivo gravável pelo grupo ([`bf0177c`](https://github.com/DanielFreitasDev/ditador/commit/bf0177ce980bffac371da9d8470ac030656a6458))
- Registrar as seis investigações desta rodada de auditoria ([`439bb86`](https://github.com/DanielFreitasDev/ditador/commit/439bb86afae6851ba830442a11124ed4ccc92202))

[Todas as mudanças desde a `v0.7.2`](https://github.com/DanielFreitasDev/ditador/compare/v0.7.2...v0.7.3)

## 0.7.2 — 2026-08-18

### Correções e ajustes

- O barramento do teste do nome se encerra mesmo quando o teste reprova ([`fe52f72`](https://github.com/DanielFreitasDev/ditador/commit/fe52f724a6bdadb784483d782ed389eaf89aa644))
- O pacote publicado volta a rodar em processador sem AVX-512 ([`b729917`](https://github.com/DanielFreitasDev/ditador/commit/b729917545b9c19aaffdb9218d5f63b9b82e50b5))

[Todas as mudanças desde a `v0.7.1`](https://github.com/DanielFreitasDev/ditador/compare/v0.7.1...v0.7.2)

## 0.7.1 — 2026-08-18

### Correções e ajustes

- Um segundo Ditador deixa de roubar o nome do que já está rodando ([`536fb85`](https://github.com/DanielFreitasDev/ditador/commit/536fb8527023b883b926e2166510851ed7f5bff5))
- A gravação máxima escolhida nas configurações passa a valer na hora ([`dec9422`](https://github.com/DanielFreitasDev/ditador/commit/dec9422b793efd8fd72f6db40694b1e368e491f6))
- Duas frases ditadas no mesmo segundo deixam de dividir o mesmo áudio ([`a6e8122`](https://github.com/DanielFreitasDev/ditador/commit/a6e8122d8554f0e6db6bee1e996ddddde1259d3f))
- O volume escolhido em 53% não vira 52% ao reabrir as configurações ([`303161d`](https://github.com/DanielFreitasDev/ditador/commit/303161d7e9f83cf415e219875239421a3e1aea27))
- Registrar as quatro investigações desta rodada de auditoria ([`375a89e`](https://github.com/DanielFreitasDev/ditador/commit/375a89e82530c6533523d5113b19fdffae2434fc))

[Todas as mudanças desde a `v0.7.0`](https://github.com/DanielFreitasDev/ditador/compare/v0.7.0...v0.7.1)

## 0.7.0 — 2026-08-17

### Novidades

- Empurrar para o main passa a publicar a versão sozinho ([`3f58c86`](https://github.com/DanielFreitasDev/ditador/commit/3f58c86b05535595ae8c4c19be136ac6edea0fcd))

[Todas as mudanças desde a `v0.6.1`](https://github.com/DanielFreitasDev/ditador/compare/v0.6.1...v0.7.0)

## 0.6.1 — 2026-08-17

### Correções e ajustes

- A captura do README mostrava o aviso de modelo faltando por cima dos botões ([`b602aaf`](https://github.com/DanielFreitasDev/ditador/commit/b602aaf1d4a9635126489ed5dc3ab743bba78f5f))
- Toda tarefa começa com um git pull, porque o repositório se mexe sozinho ([`73070a8`](https://github.com/DanielFreitasDev/ditador/commit/73070a8d887799d4f58af02f77f344ed0979cbf1))
- O texto ditado deixa de existir só na área de transferência ([`499c8aa`](https://github.com/DanielFreitasDev/ditador/commit/499c8aa606325b856a79f3082ea6e975c24f7067))

[Todas as mudanças desde a `v0.6.0`](https://github.com/DanielFreitasDev/ditador/compare/v0.6.0...v0.6.1)

## 0.6.0 — 2026-08-16

### Novidades

- O número da próxima versão sai dos commits, e não da memória de quem lança ([`3fd5799`](https://github.com/DanielFreitasDev/ditador/commit/3fd579995c199934e5057baacce488ae60e4e11e))
- A régua da CI passa a olhar o KDE, que era o único lado sem portão nenhum ([`442b3a4`](https://github.com/DanielFreitasDev/ditador/commit/442b3a4fcebf223112ccfbd7204ed348bb3352f6))
- Publicar uma versão passa a ser um botão, com instalador de Windows junto ([`a10fccd`](https://github.com/DanielFreitasDev/ditador/commit/a10fccd177de70d60ea110008be8cfd71b41a041))
- O que se descobre investigando passa a sobreviver à sessão em que se descobriu ([`4ba5443`](https://github.com/DanielFreitasDev/ditador/commit/4ba54436437cdaee01dd860862f038329f0a1345))

### Correções e ajustes

- Registrar o que o cargo audit diz, para não reinvestigar a mesma cadeia ([`5481944`](https://github.com/DanielFreitasDev/ditador/commit/5481944975a9be809fff77b0ab9efb919f4b783a))
- O Ditador diz pelo D-Bus em que pé está ([`8aa22f8`](https://github.com/DanielFreitasDev/ditador/commit/8aa22f808b9fdeb010d14d484e2389165b8028f2))
- O GNOME Shell mostra o Ditador na barra, nas Configurações rápidas e na tela ([`36ee4fc`](https://github.com/DanielFreitasDev/ditador/commit/36ee4fcf04b2521effa130bddea982a04867611e))
- As barras do microfone sobem e descem no aviso do GNOME ([`708f843`](https://github.com/DanielFreitasDev/ditador/commit/708f843e54ec53d1d222e95ee5a845bfe40659ad))
- O empacotar.sh avisa que o apt não troca um binário de mesma versão ([`9ff781a`](https://github.com/DanielFreitasDev/ditador/commit/9ff781ad2cbcb00e2ceb28452725f41ade407f0b))
- O Plasma mostra o Ditador como um widget seu, e não como um ícone hospedado ([`11d2d6d`](https://github.com/DanielFreitasDev/ditador/commit/11d2d6d0afc0fc78605c9d35c03eff221157eac7))
- O teste de ciclo de vida da extensão passou a rodar, e a falhar quando falha ([`1d0ff30`](https://github.com/DanielFreitasDev/ditador/commit/1d0ff300d0f59856923fc0946a2775b4b592ed4c))
- O ícone da bandeja não pisca ao lado do widget do Plasma — medido, não suposto ([`a8d4a99`](https://github.com/DanielFreitasDev/ditador/commit/a8d4a990f204f3da2fd754ee482123c5fa5ad29c))
- Os arquivos criados no Windows não chegam ao Linux com fim de linha trocado ([`e9cef8d`](https://github.com/DanielFreitasDev/ditador/commit/e9cef8d4f955b956468f3a7c62a0a3779b6e2de3))
- A assinatura do modelo era conferida ao contrário, e recusava todo download ([`34cd2fb`](https://github.com/DanielFreitasDev/ditador/commit/34cd2fba8497f74b2f6b54bee28491072f3337f1))
- O Ditador roda no Windows, e o Linux não perdeu nada no caminho ([`e9ef55d`](https://github.com/DanielFreitasDev/ditador/commit/e9ef55d52189a9cdf15f0418384c324164ccac77))
- Dá para medir qual backend é mais rápido, em vez de supor ([`ee4893a`](https://github.com/DanielFreitasDev/ditador/commit/ee4893a800bf8d4b3e3ff786c79c714832fdd32a))
- Compilar no Windows deixa de ser um quebra-cabeça de cinco peças ([`e1e3d40`](https://github.com/DanielFreitasDev/ditador/commit/e1e3d4093f306d40e5180cd6ea3fb2af9950a52e))
- O atalho global funciona no Windows, e a janela deixou de ter uma caixa atrás ([`903bdd2`](https://github.com/DanielFreitasDev/ditador/commit/903bdd2e58ce6ee6ba25540891d76655e327e077))
- A lista do que falta estava faltando o item mais importante ([`33cb041`](https://github.com/DanielFreitasDev/ditador/commit/33cb04137a93cd4314c082642a277773021b31e4))
- Quem quiser saber o estado do Ditador não precisa mais ficar perguntando ([`8bd9b5a`](https://github.com/DanielFreitasDev/ditador/commit/8bd9b5aad4f31887332c2a23f275879051e53313))
- O Ditador aparece na área de notificação do Windows, e avisa na tela quando ouve ([`69a15c6`](https://github.com/DanielFreitasDev/ditador/commit/69a15c6f37a68628f0449110f78883afa7f5ad09))
- O Linux deixa de ficar meses sem ver um compilador ([`307f7a6`](https://github.com/DanielFreitasDev/ditador/commit/307f7a663ebf9fce8176864b5b52b378fac6f4eb))
- A espera de vinte segundos deixa de parecer um programa travado ([`29832ee`](https://github.com/DanielFreitasDev/ditador/commit/29832ee65a4dc93bfed1d51ccf38791366bd97cc))
- O frontend do Windows ganha os testes que ele não tinha ([`03cdec4`](https://github.com/DanielFreitasDev/ditador/commit/03cdec4111ff766ec8a258d4e056ad3751b1f751))
- A lista de testes dizia "desinstalação limpa" antes de alguém desinstalar ([`d71e1fe`](https://github.com/DanielFreitasDev/ditador/commit/d71e1fe12c86e50170acf077e47604287e0810cb))
- O domínio do frontend sai de dentro da interface, e o teste deixa de pendurar ([`ba22b3e`](https://github.com/DanielFreitasDev/ditador/commit/ba22b3e53bbb15ed20adc979e37d2da71f2c5a42))
- O atalho global voltou a funcionar: a interface roubava o teclado do próprio programa ([`833a434`](https://github.com/DanielFreitasDev/ditador/commit/833a4343b0b4e743add35c5197d91ea9b989a3bf))
- O Windows também cola sozinho, como o Ubuntu já fazia ([`11288b5`](https://github.com/DanielFreitasDev/ditador/commit/11288b5ce676dc254be8366a4482bce721257e64))
- O socket do Ubuntu deixa de atender uma palavra que só o Windows fala ([`f8f82f9`](https://github.com/DanielFreitasDev/ditador/commit/f8f82f904cf348c4cadfa08530eb9378e29044a3))
- O Ditador do Windows deixa de ser mudo: log em arquivo, e três defeitos de posse ([`1d8f3fd`](https://github.com/DanielFreitasDev/ditador/commit/1d8f3fd08e4be45920109c84cdf44e770c2de0cb))
- O ícone da bandeja do Windows tinha dois donos, e o painel não fechava ([`8997c23`](https://github.com/DanielFreitasDev/ditador/commit/8997c233e72bfd6973b5749396c55ef4b65dbffe))
- Desinstalar duas vezes deixava o Ditador nas notificações do Windows para sempre ([`2b7b317`](https://github.com/DanielFreitasDev/ditador/commit/2b7b31711342c6f49c218c0f3e1daf8266b4b9c0))
- O aviso do GNOME sumia se a pessoa voltasse a falar durante o esmaecimento ([`7d5ecca`](https://github.com/DanielFreitasDev/ditador/commit/7d5ecca1d0fc461f3ccfd715647eb84a7a1bb527))
- A CI passa a compilar o backend que o .deb leva, e a olhar a extensão ([`79dcef7`](https://github.com/DanielFreitasDev/ditador/commit/79dcef7f62812ec680533760ec096d29f7618470))
- Os dois módulos que o Linux "não usa" ganham o teste que prova que é de propósito ([`d41b9fb`](https://github.com/DanielFreitasDev/ditador/commit/d41b9fbdb8240de022760533fd36e55a011c6987))
- As mensagens que o teste do contrato imprime apontavam para um arquivo que sumiu ([`2466544`](https://github.com/DanielFreitasDev/ditador/commit/24665443ee6db9e1b434b0dfee6df3a20fdb74bd))
- A documentação volta a descrever o programa que existe ([`e9172fe`](https://github.com/DanielFreitasDev/ditador/commit/e9172fe07f738b875fc0a5e27d0997ee415178f4))
- Ligar "iniciar com o Windows" pela tela de configurações apagava a interface ([`49df90f`](https://github.com/DanielFreitasDev/ditador/commit/49df90ff5096e7b54141a857131296593fd2609c))
- Metade do travamento do portão da extensão era um processo que sobrava ([`399a6da`](https://github.com/DanielFreitasDev/ditador/commit/399a6da221c679ec1d13cffb750901b691cc28f2))
- A CI achou o que nenhuma máquina de desenvolvimento acharia: a chave Run ausente ([`3ff54fb`](https://github.com/DanielFreitasDev/ditador/commit/3ff54fbc4a7c51533e555a3ba0d7fcb6e2d4ca77))
- O contêiner do KDE não tinha make, e o CMake culpou o compilador ([`4ed1368`](https://github.com/DanielFreitasDev/ditador/commit/4ed136883addef8d59483543fc40051b8f9e7a7b))
- O aviso de locale do Qt virava reclamação do qmllint no contêiner do KDE ([`6f6ce7f`](https://github.com/DanielFreitasDev/ditador/commit/6f6ce7fc4fe6f84916a77644ee67e2cd6df1cd00))

[Todas as mudanças desde a `v0.5.0`](https://github.com/DanielFreitasDev/ditador/compare/v0.5.0...v0.6.0)

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
