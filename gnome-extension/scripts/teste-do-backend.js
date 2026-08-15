/* Teste do `src/backend.js` contra o Ditador que está rodando de verdade.
 *
 * Roda fora do GNOME Shell:
 *
 *     gjs -m scripts/teste-do-backend.js
 *
 * Dá para fazer isso porque o backend não toca em nada do Shell — só Gio, GLib
 * e GObject —, e é justamente por isso que ele é um arquivo separado. O que
 * este teste cobre é a metade que o `teste-de-ciclo.js` não alcança: lá o
 * barramento é privado e o Ditador não existe; aqui ele existe, e o que se
 * confere é a conversa em si.
 *
 * Precisa do Ditador em execução (`systemctl --user start ditador`).
 *
 * Enquanto isto roda, o ícone do Ditador some da barra: este teste segura o
 * mesmo nome que a extensão segura, e é isso que manda o aplicativo recolhê-lo.
 * Ele volta sozinho quando o teste termina.
 */

/* `system` é módulo do próprio GJS, e não uma introspecção de biblioteca — daí
 * não ter o prefixo `gi://`. É a forma moderna do antigo `imports.system`, que
 * estava aqui e é justamente o estilo que o resto do projeto não usa. */
import System from 'system';

import GLib from 'gi://GLib';
import GObject from 'gi://GObject';

import {Backend, INDISPONIVEL} from '../src/backend.js';

/* `connectObject` não é do GJS: quem o acrescenta ao `GObject.Object` é o
 * `js/ui/environment.js` do GNOME Shell. Fora dele — que é onde este teste
 * roda — é preciso repor o mínimo, conectar e desconectar por dono.
 *
 * O de verdade faz mais: quando o dono é um ator, ele desfaz as conexões
 * sozinho ao destruí-lo. Aqui isso não faz falta, porque o dono é o próprio
 * backend e o teste chama `destroy()` no fim. */
if (!GObject.Object.prototype.connectObject) {
    const porDono = new WeakMap();

    GObject.Object.prototype.connectObject = function (...args) {
        const dono = args.pop();
        const ids = porDono.get(dono) ?? [];
        for (let i = 0; i < args.length; i += 2)
            ids.push([this, this.connect(args[i], args[i + 1])]);
        porDono.set(dono, ids);
    };

    GObject.Object.prototype.disconnectObject = function (dono) {
        for (const [emissor, id] of porDono.get(dono) ?? [])
            emissor.disconnect(id);
        porDono.delete(dono);
    };
}

let falhas = 0;

function confere(condicao, descricao) {
    if (!condicao)
        falhas++;
    print(`${condicao ? 'ok    ' : 'FALHOU'}  ${descricao}`);
}

function esperar(ms) {
    return new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
}

/* Espera o estado virar um dos esperados, ou desiste. Esperar por evento, e
 * não por um tempo fixo, é o que torna o teste honesto: se o estado nunca
 * chegar, ele falha em vez de passar porque o `sleep` foi generoso. */
function esperarEstado(backend, esperados, limiteMs = 8000) {
    return new Promise(resolve => {
        if (esperados.includes(backend.estado)) {
            resolve(backend.estado);
            return;
        }
        let prazo = 0;
        const id = backend.connect('mudou', () => {
            if (!esperados.includes(backend.estado))
                return;
            backend.disconnect(id);
            GLib.source_remove(prazo);
            resolve(backend.estado);
        });
        prazo = GLib.timeout_add(GLib.PRIORITY_DEFAULT, limiteMs, () => {
            backend.disconnect(id);
            resolve(backend.estado);
            return GLib.SOURCE_REMOVE;
        });
    });
}

const laco = new GLib.MainLoop(null, false);

async function executar() {
    const backend = new Backend();

    print('============================================================');
    print('Conversa com o Ditador');
    print('============================================================');

    // O proxy conecta de forma assíncrona; nada existe no instante seguinte ao
    // construtor, e é assim que ele roda dentro do Shell também.
    confere(backend.estado === INDISPONIVEL,
        'antes de conectar, o estado é "indisponivel" — nunca uma promessa vazia');

    await esperar(1500);

    if (!backend.disponivel) {
        print('\n!! O Ditador não está rodando. Suba-o e rode de novo:');
        print('   systemctl --user start ditador');
        falhas++;
        return backend;
    }

    confere(backend.disponivel, 'achei o Ditador no barramento');
    confere(['carregando', 'pronto', 'gravando', 'transcrevendo', 'erro']
        .includes(backend.estado), `o estado é reconhecível: "${backend.estado}"`);
    confere(backend.atalho.length > 0, `o atalho chegou: "${backend.atalho}"`);
    confere(backend.idioma.length > 0, `o idioma chegou: "${backend.idioma}"`);
    confere(backend.modelo.length > 0, `o modelo chegou: "${backend.modelo}"`);

    if (await esperarEstado(backend, ['pronto']) !== 'pronto') {
        print(`\n!! O Ditador está em "${backend.estado}" e não em "pronto"; ` +
              'o resto do teste precisa dele pronto.');
        falhas++;
        return backend;
    }

    confere(backend.gravandoDesde === 0, 'parado, o começo da gravação é zero');

    print('\n--- ditar por dois segundos ---');
    backend.iniciarGravacao();
    confere(await esperarEstado(backend, ['gravando']) === 'gravando',
        'IniciarGravacao() abriu o microfone');

    const comecou = backend.gravandoDesde;
    confere(comecou > 0, `o começo da gravação chegou: ${comecou}`);
    const agora = GLib.get_real_time() / 1000;
    confere(Math.abs(agora - comecou) < 5000,
        'o começo da gravação é uma hora de parede plausível (o cronômetro conta a partir dela)');

    await esperar(2000);
    // O número não pode ter mudado no meio da gravação: é ele que o cronômetro
    // da tela usa, e mudá-lo faria o contador voltar para zero.
    confere(backend.gravandoDesde === comecou,
        'o começo da gravação não se mexeu enquanto ela corria');

    backend.pararGravacao();
    const depois = await esperarEstado(backend, ['transcrevendo', 'pronto']);
    confere(depois === 'transcrevendo' || depois === 'pronto',
        `PararGravacao() encerrou o microfone (estado: "${depois}")`);
    confere(backend.gravandoDesde === 0, 'parada, o começo da gravação voltou a zero');

    await esperarEstado(backend, ['pronto', 'erro'], 60000);
    print(`\n(o ditado terminou em "${backend.estado}")`);

    return backend;
}

executar()
    .catch(e => {
        printerr(`o teste explodiu: ${e}\n${e.stack}`);
        falhas++;
        return null;
    })
    .then(backend => {
        backend?.destroy();
        print('\n============================================================');
        print(falhas === 0
            ? 'Tudo certo: a extensão e o Ditador se entendem.'
            : `${falhas} verificação(ões) falharam.`);
        print('============================================================');
        laco.quit();
        if (falhas > 0)
            System.exit(1);
    });

laco.run();
