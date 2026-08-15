/* Teste do ciclo de vida da extensão, dentro de um GNOME Shell de verdade.
 *
 * Roda assim, num Shell aninhado e sem tela — a sessão de quem está
 * desenvolvendo fica intacta:
 *
 *     ./scripts/testar.sh
 *
 * O que se quer provar é o que a revisão do extensions.gnome.org cobra e o que
 * a prática quebra: habilitar, desabilitar e habilitar de novo não pode deixar
 * ícone duplicado, item de menu duplicado, ator órfão nem temporizador solto.
 * Por isso o teste conta os atores em vez de olhar a tela — contando, "aparece
 * duas vezes" e "não some" viram números diferentes de um.
 *
 * Este arquivo não vai dentro do pacote da extensão: ele não é passado ao
 * `gnome-extensions pack` como fonte extra, e nada em `extension.js` o importa.
 *
 * Ele varre a árvore de atores do Shell e olha o gerenciador de extensões por
 * dentro, o que a extensão em si nunca faz. Aqui é o lugar em que isso é
 * aceitável: um teste existe justamente para espiar por trás do que o código
 * público mostra.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const UUID = 'ditador@danielfreitasdev.github.io';
const NOME_DA_EXTENSAO = 'io.github.danielfreitasdev.Ditador.GnomeExtension';

/* O Shell chama `init()` assim que carrega o script de automação, e só chama
 * `run()` depois que o auxiliar de desempenho aparecer no barramento — coisa
 * que não acontece num barramento privado como o deste teste. Como aqui não se
 * mede desempenho nenhum, o trabalho todo vai no `init()`, e somos nós que
 * encerramos o Shell no fim. */
export const METRICS = {};

export function init() {
    executar().catch(e => {
        printerr(`o teste explodiu: ${e}`);
        falhas++;
    }).finally(() => {
        printerr(falhas === 0 ? 'TESTE OK' : `TESTE COM ${falhas} FALHA(S)`);
        global.context.terminate();
    });
}

let falhas = 0;

function confere(condicao, descricao) {
    const marca = condicao ? 'ok  ' : 'FALHOU';
    if (!condicao)
        falhas++;
    printerr(`${marca}  ${descricao}`);
}

function* descendentes(ator) {
    for (const filho of ator) {
        yield filho;
        yield* descendentes(filho);
    }
}

/* Quantos pedaços da extensão existem na árvore de atores agora. */
function censo() {
    let indicadores = 0;
    let controles = 0;
    let avisos = 0;

    for (const ator of descendentes(Main.layoutManager.uiGroup)) {
        if (ator.name === 'ditador-aviso')
            avisos++;
        // O indicador da barra é um ícone; o controle das Configurações rápidas
        // é um botão com menu. Os dois carregam "Ditador —" no nome acessível, e
        // sem separá-los pelo tipo este teste contava um pelo outro — dois
        // indicadores e nenhum controle, que foi o que ele disse da primeira vez.
        else if (ator instanceof St.Icon && ator.accessible_name?.startsWith('Ditador —'))
            indicadores++;
        // `menu` só existe no `QuickMenuToggle` de fora: o `QuickToggle` que ele
        // cria por dentro para desenhar o conteúdo herda o mesmo `title`.
        else if (ator.title === 'Ditador' && ator.menu != null)
            controles++;
    }

    return {indicadores, controles, avisos};
}

function nomeNoBarramento() {
    // Síncrono de propósito: é um teste, e o que se quer é a resposta agora.
    const resposta = Gio.DBus.session.call_sync(
        'org.freedesktop.DBus', '/org/freedesktop/DBus',
        'org.freedesktop.DBus', 'NameHasOwner',
        new GLib.Variant('(s)', [NOME_DA_EXTENSAO]),
        null, Gio.DBusCallFlags.NONE, -1, null);
    return resposta.deepUnpack()[0];
}

function esperar(ms) {
    return new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
}

async function executar() {
    // Deixa o Shell terminar de subir e de carregar a extensão.
    await esperar(5000);

    const gerente = Main.extensionManager;
    printerr('============================================================');
    printerr(`Ciclo de vida — ${UUID}`);
    printerr('============================================================');

    confere(gerente.lookup(UUID) != null, 'a extensão foi encontrada');

    for (let volta = 1; volta <= 3; volta++) {
        printerr(`\n--- volta ${volta} ---`);

        gerente.enableExtension(UUID);
        await esperar(1200);

        const ligada = censo();
        confere(ligada.indicadores === 1,
            `habilitada: um indicador na barra (achei ${ligada.indicadores})`);
        confere(ligada.controles === 1,
            `habilitada: um controle nas Configurações rápidas (achei ${ligada.controles})`);
        confere(ligada.avisos === 1,
            `habilitada: um aviso de tela (achei ${ligada.avisos})`);
        confere(nomeNoBarramento(),
            'habilitada: o nome está no barramento, então o aplicativo recolhe o ícone dele');

        gerente.disableExtension(UUID);
        await esperar(1200);

        const desligada = censo();
        confere(desligada.indicadores === 0,
            `desabilitada: nenhum indicador sobrou (achei ${desligada.indicadores})`);
        confere(desligada.controles === 0,
            `desabilitada: nenhum controle sobrou (achei ${desligada.controles})`);
        confere(desligada.avisos === 0,
            `desabilitada: nenhum aviso sobrou (achei ${desligada.avisos})`);
        confere(!nomeNoBarramento(),
            'desabilitada: o nome saiu do barramento, então o ícone do aplicativo volta');

        const erro = gerente.lookup(UUID)?.error;
        confere(!erro, `sem erro no gerenciador de extensões${erro ? `: ${erro}` : ''}`);
    }

    printerr('\n============================================================');
    printerr(falhas === 0
        ? 'Tudo certo: nada duplicou e nada sobrou.'
        : `${falhas} verificação(ões) falharam.`);
    printerr('============================================================');
}
