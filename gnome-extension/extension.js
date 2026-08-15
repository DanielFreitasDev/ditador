/* Integração do Ditador com o GNOME Shell.
 *
 * Este arquivo só monta e desmonta: quem faz alguma coisa são os módulos de
 * `src/`. Nada nasce fora do `enable()` — nem no `import`, nem no construtor da
 * classe —, e tudo que nasce nele é desfeito no `disable()`, na ordem inversa.
 *
 * O aplicativo Ditador continua sendo o programa; esta extensão é uma vitrine
 * dele. Ela não grava áudio, não transcreve, não lê o teclado, não abre
 * subprocessos e não acessa a rede — só fala D-Bus com o processo Rust que faz
 * tudo isso, e desenha o que ele responde.
 */

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {Backend} from './src/backend.js';
import {Indicador} from './src/indicator.js';
import {Aviso} from './src/osd.js';

export default class DitadorExtension extends Extension {
    enable() {
        this._ajustes = this.getSettings();
        this._backend = new Backend();

        this._indicador = new Indicador(this._backend, this._ajustes);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicador);

        this._aviso = new Aviso();
        this._backend.connectObject(
            'mudou', () => this._mostrarOAviso(),
            'nivel', (backend_, valor) => this._aviso.nivel(valor),
            this._aviso);
        this._ajustes.connectObject(
            'changed::mostrar-osd', () => this._mostrarOAviso(), this._aviso);
        this._mostrarOAviso();
    }

    disable() {
        // O aviso primeiro: ele é o que está por cima de tudo na tela, e as
        // conexões dele morrem junto com o actor (o rastreador de sinais do
        // Shell desfaz sozinho o que foi conectado tendo um actor como dono).
        this._aviso?.destroy();
        this._aviso = null;

        this._indicador?.destroy();
        this._indicador = null;

        // O backend por último, porque é ele que larga o nome no barramento —
        // e é largar esse nome que faz o aplicativo trazer de volta o ícone
        // dele. Fazendo isso antes, haveria um instante com os dois ícones.
        this._backend?.destroy();
        this._backend = null;

        this._ajustes = null;
    }

    _mostrarOAviso() {
        if (this._ajustes.get_boolean('mostrar-osd'))
            this._aviso.sincronizar(this._backend);
        else
            this._aviso.esconder();
    }
}
