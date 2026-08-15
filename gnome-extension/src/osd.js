/* O aviso na tela enquanto se dita.
 *
 * É o mesmo desenho do OSD do próprio Shell — a caixa escura arredondada que
 * aparece ao mudar o volume —, porque é literalmente a classe de estilo dele
 * (`osd-window`, em `js/ui/osdWindow.js` e no tema). Não copiamos o visual do
 * aplicativo: com a extensão ligada, quem avisa é o GNOME, e um aviso do GNOME
 * se parece com os outros avisos do GNOME.
 *
 * O OSD embutido não serviria: ele se esconde sozinho depois de um segundo e
 * meio, e este precisa ficar enquanto o microfone estiver aberto. Usá-lo
 * significaria remostrá-lo de segundo em segundo para reiniciar o cronômetro
 * de sumiço, e disputar a mesma caixa com o controle de volume. Um actor nosso,
 * com o estilo deles, resolve os dois problemas e some quando mandarmos.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Layout from 'resource:///org/gnome/shell/ui/layout.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {duracao, icone, verbete} from './estado.js';

/* O mesmo esmaecimento do OSD do Shell, para os dois entrarem igual. */
const ESMAECIMENTO = 100;

/* Quanto tempo um erro fica na tela. Mais que o segundo e meio do OSD de
 * volume porque aqui há uma frase para ler, e não um número. */
const ESPERA_DO_ERRO = 4000;

export const Aviso = GObject.registerClass(
class Aviso extends Clutter.Actor {
    constructor() {
        super({
            // Aparece no Looking Glass e nos scripts de teste, que é onde se
            // quer poder apontar para este ator e dizer de quem ele é.
            name: 'ditador-aviso',
            x_expand: true,
            y_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.END,
            visible: false,
            opacity: 0,
        });

        this.add_constraint(new Layout.MonitorConstraint({primary: true}));

        this._caixa = new St.BoxLayout({style_class: 'osd-window'});
        this.add_child(this._caixa);

        this._simbolo = new St.Icon({y_expand: true});
        this._caixa.add_child(this._simbolo);

        this._rotulo = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        this._caixa.add_child(this._rotulo);

        this._cronometro = new St.Label({
            style_class: 'ditador-cronometro',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._caixa.add_child(this._cronometro);

        this._tique = 0;
        this._sumico = 0;
        this._gravandoDesde = 0;
        this._semRedirecionamento = false;

        Main.uiGroup.add_child(this);
    }

    /**
     * Põe na tela o que o estado atual do Ditador pede — ou tira tudo dela.
     *
     * @param {object} backend - de onde o estado vem
     */
    sincronizar(backend) {
        switch (backend.estado) {
        case 'gravando':
            this._gravandoDesde = backend.gravandoDesde;
            this._mostrar('gravando', verbete('gravando').rotulo);
            this._contarOTempo();
            break;

        case 'transcrevendo':
            this._mostrar('transcrevendo', verbete('transcrevendo').rotulo);
            break;

        case 'erro':
            this._mostrar('erro', backend.mensagem || verbete('erro').rotulo);
            this._sumirDepois(ESPERA_DO_ERRO);
            break;

        // `pronto`, `carregando` e `indisponivel` não aparecem, e é uma escolha.
        //
        // O fim da transcrição já se anuncia sozinho: o aviso some, e é isso
        // que quem estava esperando queria saber. Um "Pronto" a cada frase
        // ditada seria uma caixa piscando no meio da tela o dia inteiro.
        //
        // A carga do modelo também não: ela acontece uma vez, no login, e um
        // aviso ali seria a primeira coisa que o GNOME mostraria a cada entrada
        // na sessão. Quem apertar o atalho durante a carga continua sendo
        // atendido — o próprio aplicativo abre a janela dizendo que está
        // esperando, e essa janela não é substituída por este aviso.
        default:
            this.esconder();
        }
    }

    _mostrar(estado, texto) {
        this._cancelarSumico();
        if (estado !== 'gravando')
            this._pararDeContar();

        this._simbolo.gicon = icone(estado);
        this._rotulo.text = texto;
        // Sem gravação não há o que cronometrar, e um `00:00` parado ao lado de
        // "Transcrevendo…" só faria pensar que alguma coisa travou.
        this._cronometro.visible = estado === 'gravando';

        if (this.visible)
            return;

        // Uma janela em tela cheia pode estar sendo desenhada direto no monitor,
        // por fora do compositor — e nesse caminho o que está por cima dela não
        // aparece. É o caso de quem dita dentro de um jogo ou de um vídeo.
        this._pegarORedirecionamento();
        this.show();
        this.get_parent().set_child_above_sibling(this, null);
        this.ease({
            opacity: 255,
            duration: ESMAECIMENTO,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
        });
    }

    async esconder() {
        this._cancelarSumico();
        this._pararDeContar();
        if (!this.visible)
            return;

        try {
            await this.easeAsync({
                opacity: 0,
                duration: ESMAECIMENTO,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
        } catch {
            // O esmaecimento foi interrompido porque alguém pediu para mostrar
            // outra coisa no meio dele. Quem manda agora é a transição nova, e
            // esconder aqui apagaria o aviso que acabou de nascer.
            return;
        }

        this.hide();
        this._devolverORedirecionamento();
    }

    // ------------------------------------------------------------ cronômetro

    /* Um tique por segundo, e só enquanto o microfone estiver aberto.
     *
     * A hora de começar vem do aplicativo (`GravandoDesde`) e nunca daqui: se
     * este lado contasse sozinho, o número mudaria conforme o Shell estivesse
     * ocupado, e discordaria do tempo de áudio que o Ditador registra. */
    _contarOTempo() {
        this._escreverOTempo();
        if (this._tique)
            return;
        this._tique = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 1, () => {
            this._escreverOTempo();
            return GLib.SOURCE_CONTINUE;
        });
        GLib.Source.set_name_by_id(this._tique, '[ditador] cronômetro do aviso');
    }

    _pararDeContar() {
        if (!this._tique)
            return;
        GLib.source_remove(this._tique);
        this._tique = 0;
    }

    _escreverOTempo() {
        if (!this._gravandoDesde) {
            this._cronometro.text = duracao(0);
            return;
        }
        const agora = GLib.get_real_time() / 1000;
        this._cronometro.text = duracao((agora - this._gravandoDesde) / 1000);
    }

    // ---------------------------------------------------------------- apoio

    _sumirDepois(ms) {
        this._cancelarSumico();
        this._sumico = GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            this._sumico = 0;
            this.esconder();
            return GLib.SOURCE_REMOVE;
        });
        GLib.Source.set_name_by_id(this._sumico, '[ditador] sumiço do aviso');
    }

    _cancelarSumico() {
        if (!this._sumico)
            return;
        GLib.source_remove(this._sumico);
        this._sumico = 0;
    }

    /* Os dois lados do redirecionamento andam sempre em par, e a bandeira
     * existe para que andem: pedir duas vezes seguidas e devolver uma deixaria
     * o compositor sem otimização de tela cheia até o fim da sessão. */
    _pegarORedirecionamento() {
        if (this._semRedirecionamento)
            return;
        global.compositor.disable_unredirect();
        this._semRedirecionamento = true;
    }

    _devolverORedirecionamento() {
        if (!this._semRedirecionamento)
            return;
        global.compositor.enable_unredirect();
        this._semRedirecionamento = false;
    }

    destroy() {
        this._pararDeContar();
        this._cancelarSumico();
        this._devolverORedirecionamento();
        super.destroy();
    }
});
