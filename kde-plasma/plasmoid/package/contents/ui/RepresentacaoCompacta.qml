/* O ícone no painel.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 *
 * Um ícone e nada mais: sem texto fixo, sem tamanho escrito em pixels, sem
 * suposição de que o painel é horizontal. O Plasma dá o retângulo — em cima,
 * embaixo, à esquerda ou à direita, na espessura que o usuário escolheu — e o
 * ícone o preenche. É por isso que não há `Layout` nenhum aqui.
 */

import QtQuick

import org.kde.kirigami as Kirigami

MouseArea {
    id: compacta

    required property string icone
    /* O nome do widget, para quem lê a tela em vez de olhar. */
    required property string nome
    /* Em que pé está — a mesma frase da dica. */
    required property string descricao

    /* Clique, Espaço ou Enter: abre e fecha o popup. */
    signal acionada

    hoverEnabled: true
    activeFocusOnTab: true

    Accessible.role: Accessible.Button
    Accessible.name: compacta.nome
    Accessible.description: compacta.descricao
    Accessible.onPressAction: compacta.acionada()

    onClicked: compacta.acionada()
    Keys.onSpacePressed: event => {
        compacta.acionada();
        event.accepted = true;
    }
    Keys.onReturnPressed: event => {
        compacta.acionada();
        event.accepted = true;
    }
    Keys.onEnterPressed: event => {
        compacta.acionada();
        event.accepted = true;
    }

    Kirigami.Icon {
        anchors.fill: parent

        source: compacta.icone
        /* Se os ícones do Ditador não estiverem instalados no tema, um microfone
         * do Breeze é melhor do que o quadrado de "faltando". */
        fallback: "audio-input-microphone-symbolic"

        /* Sem isto o ícone some no Breeze escuro.
         *
         * Os SVGs de `assets/simbolicos/` têm `fill="#2e3436"` fixo, que é a
         * convenção do GTK: lá o tema recolore ícones "-symbolic" à força,
         * sobrescrevendo o `fill`. O Qt não faz isso sozinho. Como máscara, o
         * Kirigami desenha a silhueta na cor de texto do tema, e aí o ícone
         * acompanha painel claro, painel escuro e qualquer esquema de cores —
         * que é o que o `fill` fixo prometia e não cumpria aqui. */
        isMask: true
        color: Kirigami.Theme.textColor

        active: compacta.containsMouse || compacta.activeFocus
    }
}
