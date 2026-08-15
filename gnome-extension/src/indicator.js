/* O indicador na barra superior, e o dono do controle das Configurações
 * rápidas.
 *
 * Um `SystemIndicator` é as duas coisas ao mesmo tempo: a caixa de ícones que
 * vai para a barra e a lista `quickSettingsItems`, que o painel recolhe e
 * coloca na grade. Por isso os dois moram aqui, e não em arquivos separados —
 * separá-los daria dois arquivos que só sabem existir juntos.
 */

import GObject from 'gi://GObject';

import {SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';

import {INDISPONIVEL} from './backend.js';
import {icone, verbete} from './estado.js';
import {Controle} from './quickSettings.js';

export const Indicador = GObject.registerClass(
class Indicador extends SystemIndicator {
    constructor(backend, ajustes) {
        super();

        this._backend = backend;
        this._ajustes = ajustes;

        this._simbolo = this._addIndicator();

        this._controle = new Controle(backend);
        this.quickSettingsItems.push(this._controle);

        this._backend.connectObject('mudou', () => this._sincronizar(), this);
        this._ajustes.connectObject(
            'changed::mostrar-indicador', () => this._sincronizar(), this);

        this._sincronizar();
    }

    _sincronizar() {
        const estado = this._backend.estado;

        this._simbolo.gicon = icone(estado);
        // O ícone da barra não tem rótulo — o nome acessível é a única coisa
        // que um leitor de tela tem para dizer o que ele é e o que está
        // acontecendo.
        this._simbolo.accessible_name = `Ditador — ${verbete(estado).rotulo}`;

        // Com o Ditador fora do ar não há estado para indicar, e um ícone
        // apagado ocupando a barra não diria nada a ninguém. O controle das
        // Configurações rápidas continua lá, escrito "Indisponível": é onde a
        // pergunta "cadê o Ditador?" tem espaço para ser respondida.
        this._simbolo.visible =
            estado !== INDISPONIVEL && this._ajustes.get_boolean('mostrar-indicador');
    }

    destroy() {
        this._backend.disconnectObject(this);
        this._ajustes.disconnectObject(this);

        // Os itens das Configurações rápidas foram parar na grade do painel, e
        // não dentro desta caixa: destruir o indicador não os leva junto.
        this.quickSettingsItems.forEach(item => item.destroy());
        this.quickSettingsItems.length = 0;

        super.destroy();
    }
});
