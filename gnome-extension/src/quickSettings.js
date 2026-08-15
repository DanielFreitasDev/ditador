/* O controle do Ditador nas Configurações rápidas.
 *
 * Um `QuickMenuToggle`: o corpo alterna a gravação e a setinha abre o menu com
 * o resto. O visual é o padrão do Shell, sem nenhum CSS nosso — é assim que ele
 * se parece com os controles vizinhos em qualquer tema.
 */

import GObject from 'gi://GObject';

import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {QuickMenuToggle} from 'resource:///org/gnome/shell/ui/quickSettings.js';

import {INDISPONIVEL} from './backend.js';
import {icone, verbete} from './estado.js';

export const Controle = GObject.registerClass(
class Controle extends QuickMenuToggle {
    constructor(backend) {
        super({
            title: 'Ditador',
            // A gravação não é uma chave que fica ligada: é uma ação que
            // começa e termina. Sem `toggleMode`, quem decide o `checked` é o
            // estado que vem do aplicativo, e não o clique — que é o que se
            // quer, já que o atalho global também começa e para a gravação sem
            // passar por aqui.
            toggleMode: false,
        });

        this._backend = backend;

        this._resumo = new PopupMenu.PopupMenuItem('', {reactive: false});
        this._ditar = new PopupMenu.PopupMenuItem('Ditar agora');
        this._configuracoes = new PopupMenu.PopupMenuItem('Configurações do Ditador');
        this._encerrar = new PopupMenu.PopupMenuItem('Encerrar Ditador');

        this.menu.addMenuItem(this._ditar);
        this.menu.addMenuItem(this._configuracoes);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addMenuItem(this._resumo);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addMenuItem(this._encerrar);

        this.connect('clicked', () => this._ditarOuParar());
        this._ditar.connect('activate', () => this._ditarOuParar());
        this._configuracoes.connect('activate', () => this._backend.abrirConfiguracoes());
        this._encerrar.connect('activate', () => this._backend.encerrar());

        // `this` é um actor, então o rastreador de sinais desfaz esta conexão
        // sozinho quando ele for destruído.
        this._backend.connectObject('mudou', () => this.sincronizar(), this);
        this.sincronizar();
    }

    /* O botão faz o que o rótulo dele promete, e não o contrário disso.
     *
     * Ver o comentário de `Backend.iniciarGravacao`: entre desenhar "Ditar
     * agora" e o clique chegar, o atalho global pode ter começado uma gravação.
     * Perguntando o estado agora, em vez de mandar alternar, o pior que
     * acontece é o pedido não ter efeito — nunca o efeito oposto. */
    _ditarOuParar() {
        if (this._backend.estado === 'gravando')
            this._backend.pararGravacao();
        else
            this._backend.iniciarGravacao();
    }

    sincronizar() {
        const estado = this._backend.estado;
        const disponivel = estado !== INDISPONIVEL;
        const gravando = estado === 'gravando';
        // Sem o modelo carregado não há ditado nenhum a começar.
        const podeDitar = disponivel && estado !== 'carregando' && estado !== 'erro';

        this.gicon = icone(estado);
        this.subtitle = this._legenda(estado);
        this.checked = gravando;
        this.reactive = podeDitar;
        this.menuEnabled = disponivel;

        // O estado também por escrito, e não só pela cor do controle: quem não
        // distingue o realce de "ligado" precisa poder ler o que está havendo.
        this.accessible_name = `Ditador — ${this.subtitle}`;
        this.menuButtonAccessibleName = 'Abrir o menu do Ditador';

        this._ditar.label.text = gravando ? 'Parar e transcrever' : 'Ditar agora';
        this._ditar.sensitive = podeDitar;
        this._configuracoes.sensitive = disponivel;
        this._encerrar.sensitive = disponivel;

        this._resumo.label.text = this._detalhe(disponivel);
        this.menu.setHeader(icone(estado), 'Ditador', this._legenda(estado));
    }

    _legenda(estado) {
        // A mensagem do aplicativo é mais específica que "deu erro" — ela diz
        // qual arquivo faltou, ou o que o microfone respondeu.
        if (estado === 'erro')
            return this._backend.mensagem || verbete(estado).rotulo;
        return verbete(estado).rotulo;
    }

    /* A linha de rodapé do menu: o que ditar com esta configuração significa. */
    _detalhe(disponivel) {
        if (!disponivel)
            return 'O Ditador não está em execução';

        const partes = [];
        if (this._backend.atalho)
            partes.push(`Segure ${this._backend.atalho}`);
        if (this._backend.idioma)
            partes.push(this._backend.idioma);
        if (this._backend.modelo)
            partes.push(this._backend.modelo);
        return partes.join(' · ');
    }
});
