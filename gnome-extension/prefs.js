/* As preferências da integração — e só dela.
 *
 * O que configura o *ditado* (modelo, microfone, idioma, GPU, área de
 * transferência) continua na tela de configurações do próprio Ditador, que já
 * existe e é onde a pessoa espera encontrá-la. Repetir aquilo aqui daria dois
 * lugares para mudar a mesma coisa, e um deles ficaria para trás.
 *
 * Sobram duas chaves, que são de fato desta camada: se o ícone aparece na barra
 * e se o aviso aparece na tela. Nenhum componente do Shell é importado aqui —
 * esta janela roda em outro processo, onde St, Clutter e Main não existem.
 */

import Adw from 'gi://Adw';
import Gio from 'gi://Gio';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class DitadorPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const ajustes = this.getSettings();

        const pagina = new Adw.PreferencesPage({
            title: 'Integração',
            icon_name: 'preferences-system-symbolic',
        });

        const grupo = new Adw.PreferencesGroup({
            title: 'Onde o Ditador aparece',
            description: 'O que ditar, em que idioma e com qual microfone ' +
                'continua nas configurações do próprio Ditador.',
        });
        pagina.add(grupo);

        const indicador = new Adw.SwitchRow({
            title: 'Ícone na barra superior',
            subtitle: 'Mostra o estado do Ditador ao lado do relógio. ' +
                'O controle nas Configurações rápidas não depende disto.',
        });
        grupo.add(indicador);

        const aviso = new Adw.SwitchRow({
            title: 'Aviso na tela ao ditar',
            subtitle: 'A caixa com "Gravando" e o cronômetro, no rodapé da tela.',
        });
        grupo.add(aviso);

        ajustes.bind('mostrar-indicador', indicador, 'active',
            Gio.SettingsBindFlags.DEFAULT);
        ajustes.bind('mostrar-osd', aviso, 'active',
            Gio.SettingsBindFlags.DEFAULT);

        window.add(pagina);
    }
}
