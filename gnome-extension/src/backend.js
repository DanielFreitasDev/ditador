/* Conversa com o aplicativo Ditador pelo D-Bus.
 *
 * É a única parte da extensão que sabe que existe um barramento. O resto —
 * indicador, Configurações rápidas, aviso na tela — pergunta a este objeto e
 * escuta o sinal `mudou`; nenhum deles guarda estado próprio, porque a fonte da
 * verdade é o processo Rust do outro lado e uma segunda máquina de estados aqui
 * só teria como discordar dela.
 *
 * Nada aqui pergunta de tempos em tempos. O Ditador publica cada mudança como
 * `PropertiesChanged`, e o `Gio.DBusProxy` já mantém as propriedades em dia
 * sozinho: quando ele emite `g-properties-changed`, a resposta é reler o cache,
 * que não custa uma viagem ao barramento.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';

const NOME = 'io.github.danielfreitasdev.Ditador';
const CAMINHO = '/io/github/danielfreitasdev/Ditador';

/* O nome que esta extensão segura enquanto está habilitada.
 *
 * É todo o protocolo de "estou aqui" que existe entre os dois lados. O
 * aplicativo o observa e, enquanto ele estiver no ar, recolhe o próprio ícone
 * da bandeja e a própria sobreposição de gravação — senão o mesmo recado
 * apareceria duas vezes na tela.
 *
 * Segurar um nome, e não mandar um aviso, é o que faz isso sobreviver ao que
 * pode dar errado: quem detém um nome é a conexão, e o barramento a solta
 * sozinho quando ela cai. Se o Shell reiniciar, se o GJS morrer no meio de um
 * `disable()`, se a extensão for desabilitada abruptamente — nos três casos o
 * ícone do aplicativo volta sem que ninguém precise ter dito nada. */
const NOME_DA_EXTENSAO = `${NOME}.GnomeExtension`;

/* O Ditador não está no ar. Não é um estado que venha pelo barramento: é a
 * ausência do nome nele, e por isso quem o percebe é este lado. */
export const INDISPONIVEL = 'indisponivel';

const INTERFACE = `
<node>
  <interface name="io.github.danielfreitasdev.Ditador">
    <method name="Alternar"/>
    <method name="IniciarGravacao"/>
    <method name="PararGravacao"/>
    <method name="AbrirConfiguracoes"/>
    <method name="Encerrar"/>
    <property name="Estado" type="s" access="read"/>
    <property name="Mensagem" type="s" access="read"/>
    <property name="GravandoDesde" type="t" access="read"/>
    <property name="Modelo" type="s" access="read"/>
    <property name="Idioma" type="s" access="read"/>
    <property name="Atalho" type="s" access="read"/>
    <signal name="Nivel">
      <arg name="valor" type="d"/>
    </signal>
  </interface>
</node>`;

export const Backend = GObject.registerClass({
    Signals: {
        'mudou': {},
        /* O pico do microfone agora, de 0 a 1. Chega umas quinze vezes por
         * segundo, e só enquanto se grava. É sinal e não propriedade porque não
         * é estado: nada disso precisa ser lembrado depois que passou. */
        'nivel': {param_types: [GObject.TYPE_DOUBLE]},
    },
}, class Backend extends GObject.Object {
    constructor() {
        super();

        this._cancelavel = new Gio.Cancellable();
        this._nomeProprio = 0;

        const info = Gio.DBusNodeInfo.new_for_xml(INTERFACE).lookup_interface(NOME);
        this._proxy = new Gio.DBusProxy({
            g_connection: Gio.DBus.session,
            g_name: NOME,
            g_object_path: CAMINHO,
            g_interface_name: info.name,
            g_interface_info: info,
            // Perguntar pelo estado do Ditador não é motivo para iniciá-lo. Quem
            // decide se ele sobe é o usuário, pelo serviço do systemd ou pelo
            // lançador — não uma extensão que só queria desenhar um ícone.
            g_flags: Gio.DBusProxyFlags.DO_NOT_AUTO_START,
        });

        this._proxy.connectObject(
            // O Ditador mudou de estado.
            'g-properties-changed', () => this.emit('mudou'),
            // O Ditador subiu, caiu ou foi reiniciado. O próprio proxy recarrega
            // as propriedades quando o dono do nome volta, então não há nada a
            // refazer aqui além de avisar quem desenha.
            'notify::g-name-owner', () => this.emit('mudou'),
            // Os sinais da interface chegam todos por aqui, com o nome dentro.
            'g-signal', (proxy_, remetente_, nome, parametros) => {
                if (nome === 'Nivel')
                    this.emit('nivel', parametros.deepUnpack()[0]);
            },
            this);

        this._proxy.init_async(GLib.PRIORITY_DEFAULT, this._cancelavel).catch(e => {
            if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                console.error(`Ditador: não consegui abrir o proxy D-Bus — ${e.message}`);
        });

        this._nomeProprio = Gio.bus_own_name(
            Gio.BusType.SESSION,
            NOME_DA_EXTENSAO,
            Gio.BusNameOwnerFlags.NONE,
            null, null, null);
    }

    /* O Ditador está em execução? A pergunta é literalmente "alguém detém o
     * nome dele no barramento agora". */
    get disponivel() {
        return this._proxy.g_name_owner !== null;
    }

    get estado() {
        // Enquanto o proxy ainda não terminou de carregar as propriedades a
        // resposta honesta é "não sei", e "não sei" e "não está no ar" levam à
        // mesma tela — a única em que não se promete nada ao usuário.
        return this.disponivel ? this._ler('Estado', INDISPONIVEL) : INDISPONIVEL;
    }

    get mensagem() {
        return this._ler('Mensagem', '');
    }

    /* Quando a gravação em curso começou, em milissegundos desde a época; 0
     * quando não há gravação. É a fonte da verdade do cronômetro: o lado de cá
     * nunca conta o tempo por conta própria. */
    get gravandoDesde() {
        return this._ler('GravandoDesde', 0);
    }

    get modelo() {
        return this._ler('Modelo', '');
    }

    get idioma() {
        return this._ler('Idioma', '');
    }

    get atalho() {
        return this._ler('Atalho', '');
    }

    /* Começar e parar são pedidos separados, e não um `Alternar`, de propósito.
     *
     * O rótulo do controle diz uma das duas coisas, e entre o instante em que
     * ele foi desenhado e o clique cabe um ditado inteiro pelo atalho global —
     * segurar a tecla é o uso normal deste programa. Com `Alternar`, um botão
     * escrito "Ditar agora" pararia a gravação que começou nesse meio-tempo:
     * faria exatamente o contrário do que promete. Pedindo o resultado desejado
     * em vez da troca, o botão sempre faz o que está escrito nele. */
    iniciarGravacao() {
        this._chamar('IniciarGravacao');
    }

    pararGravacao() {
        this._chamar('PararGravacao');
    }

    abrirConfiguracoes() {
        this._chamar('AbrirConfiguracoes');
    }

    encerrar() {
        this._chamar('Encerrar');
    }

    _ler(propriedade, padrao) {
        return this._proxy.get_cached_property(propriedade)?.deepUnpack() ?? padrao;
    }

    /* Os métodos `…Async()` são gerados pelo GJS a partir do
     * `g_interface_info` que o proxy recebeu, e devolvem promessas — é assim
     * que o próprio Shell chama os proxies dele (veja `ui/status/thunderbolt.js`
     * e `ui/status/location.js`). O `Gio.DBusProxy.call` cru não é
     * promissificado e exige o callback, o que daria o mesmo com mais linhas.
     *
     * Eles só existem depois que o `init_async` termina, e é o que a guarda de
     * disponibilidade garante: sem dono do nome, nem proxy pronto há. */
    async _chamar(metodo) {
        if (!this.disponivel)
            return;
        try {
            await this._proxy[`${metodo}Async`]();
        } catch (e) {
            console.warn(`Ditador: ${metodo}() falhou — ${e.message}`);
        }
    }

    destroy() {
        // Largar o nome é a primeira coisa, e antes de qualquer outra que possa
        // falhar: é ele que mantém o ícone do aplicativo recolhido, e se uma
        // exceção mais abaixo interrompesse este método o usuário ficaria sem
        // ícone nenhum até sair da sessão.
        if (this._nomeProprio) {
            Gio.bus_unown_name(this._nomeProprio);
            this._nomeProprio = 0;
        }

        this._cancelavel.cancel();
        this._proxy.disconnectObject(this);
    }
});
