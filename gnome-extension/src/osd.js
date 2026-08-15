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

/* Quantas barras o medidor de voz tem, e o vão entre elas em pixels. */
const BARRAS = 22;
const VAO = 2;

/* A altura mínima de uma barra, para o medidor continuar sendo uma linha de
 * pontinhos no silêncio em vez de sumir da tela. */
const REPOUSO = 2;

/**
 * O medidor de voz: as barras que sobem e descem com o que o microfone ouve.
 *
 * Desenhado com o Cairo, num actor só, e não com uma fileira de widgets — vinte
 * e duas alturas mudando quinze vezes por segundo seriam quinze recálculos de
 * layout por segundo dentro do processo que desenha a área de trabalho. Uma
 * repintura de um retângulo de cem pixels não é nada perto disso.
 *
 * A cor vem do tema (`get_foreground_color`), como a do `BarLevel` do Shell:
 * assim o medidor acompanha claro, escuro e alto contraste sem uma linha a
 * respeito de nenhum deles.
 */
const Medidor = GObject.registerClass(
class Medidor extends St.DrawingArea {
    constructor(params) {
        super(params);
        this._historico = new Array(BARRAS).fill(0);
    }

    /** Empurra uma leitura nova na ponta direita e joga a mais velha fora.
     *
     * @param {number} valor - o pico do microfone, de 0 a 1
     */
    empurrar(valor) {
        this._historico.shift();
        this._historico.push(valor);
        this.queue_repaint();
    }

    limpar() {
        this._historico.fill(0);
        this.queue_repaint();
    }

    vfunc_repaint() {
        const cr = this.get_context();
        const [largura, altura] = this.get_surface_size();
        const meio = altura / 2;
        const larguraBarra = Math.max(1, (largura - VAO * (BARRAS - 1)) / BARRAS);

        cr.setSourceColor(this.get_theme_node().get_foreground_color());

        for (let i = 0; i < BARRAS; i++) {
            // Raiz quadrada: dá presença aos sons baixos, que é o que faz o
            // medidor parecer acompanhar a fala em vez de só os picos. É a
            // mesma correção que a janela do próprio Ditador aplica.
            const valor = Math.sqrt(Math.min(1, Math.max(0, this._historico[i])));
            const h = Math.max(REPOUSO, valor * altura);
            cr.rectangle(i * (larguraBarra + VAO), meio - h / 2, larguraBarra, h);
        }
        cr.fill();

        cr.$dispose();
    }
});

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

        // Onde o OSD do Shell põe a barra de nível do volume, este põe a voz.
        this._medidor = new Medidor({
            style_class: 'ditador-medidor',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._caixa.add_child(this._medidor);

        this._cronometro = new St.Label({
            style_class: 'ditador-cronometro',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._caixa.add_child(this._cronometro);

        this._tique = 0;
        this._sumico = 0;
        this._gravandoDesde = 0;
        this._semRedirecionamento = false;
        /* Verdadeiro entre o começo do esmaecimento de saída e o `hide()`. */
        this._saindo = false;

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
            // Gravação nova é medidor limpo: as barras da frase anterior não
            // têm nada a dizer sobre esta.
            if (this._gravandoDesde !== backend.gravandoDesde)
                this._medidor.limpar();
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
        // Sem gravação não há o que cronometrar nem o que medir, e um `00:00`
        // parado ao lado de "Transcrevendo…", ou barras congeladas, só fariam
        // pensar que alguma coisa travou.
        this._cronometro.visible = estado === 'gravando';
        this._medidor.visible = estado === 'gravando';

        // Já na tela e sem estar saindo: não há nada a animar, só o texto que
        // acabou de ser trocado acima.
        if (this.visible && !this._saindo)
            return;

        // Aqui é o caso que faltava. Se um esmaecimento de saída estiver em
        // curso, `this.visible` ainda é `true` — quem chama `hide()` é o
        // `esconder()`, e só depois do `await`. Voltando cedo, a transição de
        // saída seguia até o fim e escondia o aviso que acabou de nascer: um
        // `transcrevendo → pronto` seguido de um Pause dentro dos 100 ms deixava
        // o ditado inteiro sem aviso, sem cronômetro e sem medidor, com o
        // temporizador do relógio girando num ator invisível.
        //
        // Reanimar até 255 resolve os dois lados: o Clutter descarta a transição
        // anterior sobre a mesma propriedade — é o que o `easeAsync` do Shell
        // trata como interrupção —, e a promessa que o `esconder()` está
        // esperando rejeita, caindo no `catch` que existe exatamente para isto.
        this._saindo = false;

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

        // Enquanto isto for verdade, o aviso ainda está visível mas já é passado:
        // é o que o `_mostrar` consulta para saber que precisa reanimar em vez
        // de voltar cedo achando que já está tudo na tela.
        this._saindo = true;

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

        // Uma segunda saída pode ter começado enquanto esta esperava — e uma
        // entrada também pode ter passado por aqui e zerado a marca. Só esconde
        // quem ainda é a saída em vigor.
        if (!this._saindo)
            return;

        this._saindo = false;
        this.hide();
        this._devolverORedirecionamento();
    }

    /**
     * Uma leitura nova do microfone. Ignorada quando o aviso não está na tela —
     * o Ditador só emite durante a gravação, mas quem desenha não precisa
     * confiar nisso para estar certo.
     *
     * @param {number} valor - o pico do microfone, de 0 a 1
     */
    nivel(valor) {
        if (this._medidor.visible)
            this._medidor.empurrar(valor);
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
        // Uma saída em curso não deve sobreviver ao `destroy()`: o `await` dela
        // volta depois que o ator já não existe, e um `hide()` ali seria uma
        // chamada num objeto destruído.
        this._saindo = false;
        this._devolverORedirecionamento();
        super.destroy();
    }
});
