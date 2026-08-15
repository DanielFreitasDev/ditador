/* O Ditador na bandeja do sistema do Plasma.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 *
 * Este arquivo não guarda estado. Ele lê o que o `DitadorBackend` diz, traduz
 * para ícone e frase, e devolve cliques. A fonte da verdade é o processo Rust
 * do outro lado do barramento — uma segunda máquina de estados aqui só teria
 * como discordar dela.
 *
 * As duas representações moram em arquivos próprios e recebem tudo por
 * propriedade, em vez de alcançarem `raiz` pelo escopo. Custa as linhas de
 * declaração e paga em `qmllint` limpo: acesso não qualificado é aviso, e um
 * aviso que se aprende a ignorar é o mesmo que não ter aviso nenhum.
 */

pragma ComponentBehavior: Bound

import QtQuick

import org.kde.ki18n
import org.kde.kirigami as Kirigami
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid

import io.github.danielfreitasdev.ditador

PlasmoidItem {
    id: raiz

    /* O `i18nd()` solto que os widgets do Plasma costumam usar vem de um
     * contexto que o motor instala por fora, e que o `qmllint` não tem como
     * conhecer: cada chamada vira um aviso de acesso não qualificado. Este
     * objeto é a forma que o KF6 6.8 em diante oferece para o mesmo — o domínio
     * fica dito uma vez, e as chamadas passam a ser qualificadas.
     *
     * O domínio existe mesmo sem catálogo nenhum: as frases estão em português
     * porque o projeto inteiro está, e sem tradução carregada o `i18n` devolve o
     * texto de origem. O que ele garante é que o dia em que houver um catálogo
     * nada mais precise mudar aqui. */
    KI18nContext {
        id: traducao

        translationDomain: "ditador"
    }

    /* O Ditador não está no ar. Não é um estado que venha pelo barramento: é a
     * ausência do nome nele, e por isso quem o percebe é este lado.
     *
     * Na prática ele quase não aparece — o widget só é carregado porque o
     * serviço apareceu (`X-Plasma-DBusActivationService`, no `metadata.json`) —,
     * mas existe o instante entre o Ditador encerrar e o `plasmashell` descarregar
     * o widget, e é feio mostrar "Pronto" nele. */
    readonly property string indisponivel: "indisponivel"

    readonly property string situacao: backend.disponivel ? backend.estado : indisponivel
    readonly property bool gravando: situacao === "gravando"

    /* O modelo carregou, então dá para ditar. A mesma regra do `tray.rs`
     * (`pronto_para_ditar`), pelo mesmo motivo: um botão "Ditar agora" ativo
     * enquanto o modelo carrega promete o que o outro lado ainda não entrega. */
    readonly property bool podeDitar: situacao !== "carregando"
        && situacao !== "erro"
        && situacao !== indisponivel

    /* Os ícones do projeto, os mesmos do ícone da bandeja e do indicador do
     * GNOME — o Ditador tem a mesma cara nas três áreas de trabalho. O tema
     * Breeze não traz nada equivalente para "transcrevendo", e usar um
     * microfone genérico para dois estados diferentes seria trocar informação
     * por familiaridade.
     *
     * As formas são distintas entre si (microfone, ponto de gravação, ampulheta,
     * triângulo de aviso), e não só a cor: quem não distingue cores continua
     * lendo o estado. */
    readonly property string icone: {
        switch (situacao) {
        case "gravando":
            return "ditador-gravando-symbolic";
        case "carregando":
        case "transcrevendo":
            return "ditador-carregando-symbolic";
        case "erro":
            return "ditador-falhou-symbolic";
        default:
            return "ditador-symbolic";
        }
    }

    /* Uma linha dizendo em que pé está — a mesma que a bandeja mostra na dica
     * (`Retrato::resumo`, em `tray.rs`). */
    readonly property string resumo: {
        switch (situacao) {
        case "carregando":
            return traducao.i18n("Carregando o modelo…");
        case "erro":
            return backend.mensagem !== ""
                ? backend.mensagem
                : traducao.i18n("O modelo não carregou");
        case "gravando":
            return traducao.i18n("Ouvindo…");
        case "transcrevendo":
            return traducao.i18n("Transcrevendo…");
        case "pronto":
            return backend.atalho !== ""
                ? traducao.i18n("Pronto · segure %1", backend.atalho)
                : traducao.i18n("Pronto");
        default:
            return traducao.i18n("O Ditador não está em execução");
        }
    }

    DitadorBackend {
        id: backend
    }

    Plasmoid.icon: raiz.icone

    /* O rodinha de "ocupado" só na carga do modelo, que acontece uma vez no
     * arranque e demora. A transcrição também é trabalho, mas dura um ou dois
     * segundos a cada frase ditada — um giro por frase seria pisca-pisca, e o
     * ícone próprio já diz o que está havendo. */
    Plasmoid.busy: raiz.situacao === "carregando"

    /* Ativo enquanto o Ditador estiver de pé.
     *
     * A tentação é marcar `PassiveStatus` quando não há nada acontecendo, para
     * o "Mostrar quando relevante" da bandeja o recolher. Só que este ícone é a
     * única presença do programa no painel: recolhido, quem acabou de instalar
     * o widget não encontra nem o botão de ditar nem o de configurações. É
     * também o que o StatusNotifierItem que ele substitui sempre fez
     * (`Status::Active`, em `tray.rs`), e trocar esse comportamento junto com a
     * instalação do widget seria uma surpresa.
     *
     * Quem quiser o ícone escondido tem, na própria bandeja do Plasma, a opção
     * "Oculto" — que continua valendo. */
    Plasmoid.status: raiz.situacao === raiz.indisponivel
        ? PlasmaCore.Types.PassiveStatus
        : PlasmaCore.Types.ActiveStatus

    toolTipMainText: traducao.i18n("Ditador")
    toolTipSubText: raiz.resumo

    /* O popup é pequeno o bastante para nunca valer a pena virar janela. */
    switchWidth: Kirigami.Units.gridUnit * 12
    switchHeight: Kirigami.Units.gridUnit * 12

    compactRepresentation: RepresentacaoCompacta {
        icone: raiz.icone
        nome: traducao.i18n("Ditador")
        descricao: raiz.resumo
        onAcionada: raiz.expanded = !raiz.expanded
    }

    fullRepresentation: RepresentacaoCompleta {
        icone: raiz.icone
        resumo: raiz.resumo
        gravando: raiz.gravando
        podeDitar: raiz.podeDitar
        gravandoDesde: backend.gravandoDesde
        modelo: backend.modelo
        idioma: backend.idioma
        /* O nível só interessa com o popup aberto e o microfone ligado. Fora
         * disso o `enabled` do `Connections` lá dentro desliga o tratador, e as
         * quinze mensagens por segundo passam sem virar repintura. */
        ouvindoNivel: raiz.expanded && raiz.gravando
        alvo: backend
        cabecalhoDoContainer:
            (Plasmoid.containmentDisplayHints & PlasmaCore.Types.ContainmentDrawsPlasmoidHeading) !== 0

        onDitar: backend.iniciarGravacao()
        onParar: backend.pararGravacao()
        onConfigurar: {
            backend.abrirConfiguracoes();
            /* A janela do Ditador vai aparecer; o popup por cima dela só
             * atrapalharia. */
            raiz.expanded = false;
        }
    }

    /* Começar e parar são ações separadas, e não uma "Alternar", pelo motivo
     * escrito em `ditadorbackend.h`: entre desenhar o rótulo e o clique cabe um
     * ditado inteiro pela tecla de atalho, e um item escrito "Ditar agora"
     * jamais pode parar uma gravação. */
    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: traducao.i18n("Ditar agora")
            icon.name: "ditador-gravando-symbolic"
            visible: !raiz.gravando
            enabled: raiz.podeDitar
            onTriggered: backend.iniciarGravacao()
        },
        PlasmaCore.Action {
            text: traducao.i18n("Parar e transcrever")
            icon.name: "media-playback-stop-symbolic"
            visible: raiz.gravando
            onTriggered: backend.pararGravacao()
        },
        PlasmaCore.Action {
            text: traducao.i18n("Configurações do Ditador…")
            icon.name: "configure"
            enabled: backend.disponivel
            onTriggered: backend.abrirConfiguracoes()
        },
        PlasmaCore.Action {
            text: traducao.i18n("Encerrar o Ditador")
            icon.name: "application-exit"
            enabled: backend.disponivel
            onTriggered: backend.encerrar()
        }
    ]

    Component.onCompleted: {
        /* Não há tela de configurações deste widget, e um item de menu que abre
         * um diálogo vazio é pior do que item nenhum. O que se configuraria aqui
         * ou é do Ditador (modelo, microfone, idioma — e mora na janela dele) ou
         * é da bandeja do Plasma (mostrar, esconder). Veja o README. */
        Plasmoid.removeInternalAction("configure");
    }
}
