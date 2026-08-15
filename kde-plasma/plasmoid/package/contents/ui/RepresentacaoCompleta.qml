/* O popup do painel.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 *
 * Discreto de propósito: em que pé está, um botão para ditar, o que está em uso
 * e a porta para as configurações de verdade. A janela do Ditador não é
 * reproduzida aqui — modelo, microfone, idioma, colagem e download são dezenas
 * de controles que já existem, funcionam e não cabem num popup de painel. O que
 * aparece aqui deles é só leitura.
 *
 * Nada de cor escrita à mão, nada de tamanho em pixels: as cores saem do
 * `Kirigami.Theme` e as medidas de `Kirigami.Units`, que é o que faz o popup
 * acompanhar Breeze claro, Breeze escuro e escala fracionária sem uma linha a
 * mais.
 */

import QtQuick
import QtQuick.Layouts

import org.kde.ki18n
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.extras as PlasmaExtras

import io.github.danielfreitasdev.ditador

PlasmaExtras.Representation {
    id: completa

    /* O mesmo domínio do `main.qml`, e pelo mesmo motivo — veja lá. */
    KI18nContext {
        id: traducao

        translationDomain: "ditador"
    }

    required property string icone
    required property string resumo
    required property bool gravando
    required property bool podeDitar
    /* Milissegundos desde a época, como o Ditador os publica. Zero fora da
     * gravação. */
    required property double gravandoDesde
    required property string modelo
    required property string idioma
    /* Vale a pena escutar o nível do microfone agora? */
    required property bool ouvindoNivel
    required property DitadorBackend alvo
    /* O contêiner já desenha um cabeçalho para nós? Vem de fora, como tudo o
     * mais: o objeto anexado `Plasmoid` só é alcançável do arquivo raiz do
     * widget, e alcançá-lo daqui funcionaria em execução mas seria um acesso
     * não qualificado — o aviso que este arquivo existe para não ter. */
    required property bool cabecalhoDoContainer

    signal ditar
    signal parar
    signal configurar

    Layout.minimumWidth: Kirigami.Units.gridUnit * 14
    Layout.minimumHeight: Kirigami.Units.gridUnit * 13
    Layout.preferredWidth: Kirigami.Units.gridUnit * 16
    Layout.preferredHeight: Kirigami.Units.gridUnit * 14

    /* O pico mais recente do microfone, de 0 a 1. */
    property real pico: 0
    /* Segundos de gravação, contados a partir do que o Ditador publicou. */
    property int decorridos: 0

    /* O cronômetro existe só enquanto se grava, e bate uma vez por segundo — não
     * a cada quadro. O instante de início vem do backend e nunca daqui: contar
     * por conta própria daria um número que começa a divergir do que o resto do
     * programa mostra.
     *
     * `triggeredOnStart` para o "00:00" não ficar um segundo inteiro na tela
     * antes do primeiro tique. */
    Timer {
        running: completa.gravando && completa.gravandoDesde > 0
        interval: 1000
        repeat: true
        triggeredOnStart: true
        onTriggered: completa.decorridos =
            Math.max(0, Math.floor((Date.now() - completa.gravandoDesde) / 1000))
        onRunningChanged: if (!running) {
            completa.decorridos = 0;
        }
    }

    /* Quinze mensagens por segundo atravessam o barramento durante a gravação,
     * abram o popup ou não. O que este `enabled` evita é que elas virem
     * repintura quando ninguém está olhando. */
    Connections {
        target: completa.alvo
        enabled: completa.ouvindoNivel

        function onNivel(valor: double): void {
            /* A raiz quadrada é de quem desenha, não do contrato: o valor sai
             * cru do Ditador, e sem ela a barra quase não sai do lugar em voz
             * de conversa. */
            completa.pico = Math.sqrt(Math.max(0, Math.min(1, valor)));
        }
    }

    onOuvindoNivelChanged: if (!ouvindoNivel) {
        pico = 0;
    }

    header: PlasmaExtras.PlasmoidHeading {
        /* O contêiner às vezes desenha o próprio cabeçalho — na bandeja, por
         * exemplo. Dois títulos empilhados é o que esta condição evita. */
        visible: !completa.cabecalhoDoContainer

        contentItem: Kirigami.Heading {
            level: 1
            text: traducao.i18n("Ditador")
            elide: Text.ElideRight
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing

        /* ——— em que pé está ——— */
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
                source: completa.icone
                fallback: "audio-input-microphone-symbolic"
                isMask: true
                color: Kirigami.Theme.textColor
            }

            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: completa.resumo
                wrapMode: Text.WordWrap
                /* O estado já está dito no texto; o rótulo acessível repete o
                 * conjunto para quem chega neste ponto pela leitura de tela. */
                Accessible.name: traducao.i18n("Estado: %1", completa.resumo)
            }

            PlasmaComponents.Label {
                visible: completa.gravando
                text: {
                    const m = Math.floor(completa.decorridos / 60);
                    const s = completa.decorridos % 60;
                    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
                }
                font.features: {
                    "tnum": 1
                }
                Accessible.name: traducao.i18n("Gravando há %1 segundos", completa.decorridos)
            }
        }

        /* ——— o microfone está ouvindo? ——— */
        PlasmaComponents.ProgressBar {
            Layout.fillWidth: true
            visible: completa.gravando
            from: 0
            to: 1
            value: completa.pico
            Accessible.name: traducao.i18n("Nível do microfone")
        }

        /* ——— a ação ——— */
        PlasmaComponents.Button {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing

            text: completa.gravando
                ? traducao.i18n("Parar e transcrever")
                : traducao.i18n("Ditar agora")
            icon.name: completa.gravando
                ? "media-playback-stop-symbolic"
                : "ditador-gravando-symbolic"
            enabled: completa.podeDitar

            /* O rótulo e o que o botão faz saem da mesma pergunta — "o microfone
             * está aberto?" —, e por isso não podem se contradizer. É a mesma
             * razão de o backend expor começar e parar em vez de alternar. */
            onClicked: completa.gravando ? completa.parar() : completa.ditar()
        }

        Item {
            Layout.fillHeight: true
        }

        /* ——— o que está em uso, só leitura ——— */
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: 0
            visible: completa.modelo !== "" || completa.idioma !== ""

            PlasmaComponents.Label {
                text: traducao.i18n("Modelo")
                opacity: 0.7
                visible: completa.modelo !== ""
            }
            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: completa.modelo
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignRight
                visible: completa.modelo !== ""
            }

            PlasmaComponents.Label {
                text: traducao.i18n("Idioma")
                opacity: 0.7
                visible: completa.idioma !== ""
            }
            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: completa.idioma
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignRight
                visible: completa.idioma !== ""
            }
        }
    }

    /* O `position` do rodapé não é dito aqui: a `Page` o marca como `Footer`
     * sozinha ao recebê-lo, e é disso que o `PlasmoidHeading` tira as margens
     * invertidas que o encaixam na borda de baixo do popup. */
    footer: PlasmaExtras.PlasmoidHeading {
        contentItem: RowLayout {
            Item {
                Layout.fillWidth: true
            }

            PlasmaComponents.ToolButton {
                text: traducao.i18n("Configurações do Ditador…")
                icon.name: "configure"
                onClicked: completa.configurar()

                PlasmaComponents.ToolTip {
                    text: traducao.i18n("Abre a janela de configurações do Ditador")
                }
            }
        }
    }
}
