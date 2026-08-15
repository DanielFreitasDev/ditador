/* Conversa com o Ditador que estiver rodando, pelo plugin de verdade.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 *
 * É o par do `gnome-extension/scripts/teste-do-backend.js`, e existe pelo mesmo
 * motivo: o `cargo test` prova que o Rust publica o contrato certo, o `qmllint`
 * prova que o QML é válido, e nenhum dos dois prova que os dois lados se
 * entendem no barramento. Só quem atravessa a ponte descobre que ela está lá.
 *
 * Roda contra o processo real, e não contra um dublê. Não há dublê possível: o
 * que se quer testar é justamente o que o outro lado faz.
 *
 * Como rodar:
 *
 *   ./testar.sh --backend
 *
 * O terceiro teste abre o microfone por um segundo. É de propósito — parar uma
 * gravação que não começou não prova nada — e ele fecha o que abriu, inclusive
 * quando falha no meio.
 */

import QtQuick
import QtTest

import io.github.danielfreitasdev.ditador

TestCase {
    id: bancada

    name: "DitadorBackend"

    /* Os cinco nomes do contrato. São protocolo, não rótulo: `EstadoPublico::nome`
     * do lado do Rust, e há um teste lá guardando cada um deles. */
    readonly property var estadosConhecidos: ["carregando", "pronto", "gravando", "transcrevendo", "erro"]

    DitadorBackend {
        id: backend
    }

    SignalSpy {
        id: niveis

        target: backend
        signalName: "nivel"
    }

    function test_01_o_ditador_responde() {
        /* O `GetAll` sai assíncrono no construtor; `disponivel` só fica
         * verdadeiro quando a resposta chega. Cinco segundos é folga larga para
         * uma chamada que costuma levar milissegundos — o que se está testando é
         * "responde", não "responde rápido". */
        tryVerify(() => backend.disponivel, 5000,
                  "O Ditador não respondeu. Ele está rodando? systemctl --user start ditador");

        verify(bancada.estadosConhecidos.includes(backend.estado),
               `estado fora do contrato: "${backend.estado}"`);
    }

    function test_02_o_retrato_chega_inteiro() {
        verify(backend.disponivel, "sem Ditador não há retrato a conferir");

        /* Estes três vêm do mesmo `GetAll`. Se um chegasse vazio, o popup
         * mostraria o campo em branco e ninguém saberia por quê — é o defeito
         * que a conferência de nomes em `ditadorbackend.cpp` procura evitar, e
         * este teste é a outra metade dela. */
        verify(backend.modelo !== "", "Modelo veio vazio");
        verify(backend.idioma !== "", "Idioma veio vazio");
        verify(backend.atalho !== "", "Atalho veio vazio");

        /* Parado, o cronômetro tem de estar zerado — é o que faz a interface não
         * desenhar um contador correndo sem gravação nenhuma. */
        if (backend.estado !== "gravando") {
            compare(backend.gravandoDesde, 0, "GravandoDesde não zerou fora da gravação");
        }
    }

    function test_03_gravar_e_parar() {
        if (backend.estado !== "pronto") {
            skip(`o Ditador está "${backend.estado}", e este teste precisa dele pronto`);
        }

        niveis.clear();
        backend.iniciarGravacao();

        try {
            tryVerify(() => backend.estado === "gravando", 3000,
                      "pedi para gravar e o estado não mudou");

            /* O cronômetro da interface é desenhado a partir deste número. Zero
             * aqui significa contador começando do nada. */
            verify(backend.gravandoDesde > 0, "GravandoDesde continuou zerado durante a gravação");

            /* O sinal do nível é a única coisa periódica do contrato, e só sai
             * durante a gravação. Chegando aqui, a ponte inteira está de pé:
             * método daqui para lá, propriedade e sinal de lá para cá. */
            tryVerify(() => niveis.count > 0, 2000, "o sinal Nivel não chegou durante a gravação");
        } finally {
            /* Deixar o microfone aberto porque um `verify` falhou seria pior do
             * que o defeito que ele achou. */
            backend.pararGravacao();
        }

        /* Depois de parar, ou já transcreveu ou está transcrevendo — as duas
         * são respostas certas, e qual delas é depende de quanto o Whisper
         * demorou. O que não pode é continuar gravando. */
        tryVerify(() => backend.estado !== "gravando", 3000,
                  "pedi para parar e o microfone continuou aberto");
        compare(backend.gravandoDesde, 0, "GravandoDesde não zerou depois de parar");
    }

    function test_04_a_presenca_esta_no_barramento() {
        /* Enquanto este teste roda, ele *é* uma integração do Plasma: o plugin
         * segurou o nome no construtor do backend. O Ditador, do outro lado,
         * está com o ícone da bandeja recolhido por causa disto — e é assim que
         * se sabe que o mecanismo funciona fora do painel também. */
        verify(backend.disponivel);
    }
}
