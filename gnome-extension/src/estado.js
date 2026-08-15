/* O vocabulário dos estados: o texto de cada um e o símbolo que o representa.
 *
 * Fica num lugar só porque três superfícies dizem a mesma coisa — o indicador
 * da barra, as Configurações rápidas e o aviso na tela. Três cópias desta
 * tabela seriam o começo de três respostas diferentes para a mesma pergunta.
 *
 * Os nomes dos estados (`pronto`, `gravando`, …) são os que vêm pelo D-Bus, em
 * `EstadoPublico::nome` do lado Rust. Mudar um deles é mudar o protocolo.
 */

import Gio from 'gi://Gio';

/* Cada símbolo do Ditador com uma reserva do tema padrão atrás dele.
 *
 * Os quatro ícones são instalados pelo aplicativo, em `symbolic/apps` do tema
 * do usuário — e não pela extensão, que por regra do extensions.gnome.org não
 * pode levar nada do aplicativo dentro dela. Se o tema não os tiver (alguém que
 * compilou o Ditador sem passar pelo `instalar.sh`), o `Gio.ThemedIcon` com
 * mais de um nome cai sozinho no segundo: é justamente para isso que ele aceita
 * uma lista, e sai mais barato que conferir o tema a cada mudança de estado. */
const RESERVAS = {
    'ditador-symbolic': 'audio-input-microphone-symbolic',
    'ditador-gravando-symbolic': 'media-record-symbolic',
    'ditador-carregando-symbolic': 'content-loading-symbolic',
    'ditador-falhou-symbolic': 'dialog-warning-symbolic',
};

/* Estado → como ele se chama e com que símbolo aparece.
 *
 * `carregando` e `transcrevendo` compartilham o símbolo de trabalho porque para
 * quem olha a barra de relance os dois querem dizer "espere" — é a mesma
 * escolha que o ícone da bandeja do aplicativo faz. */
export const VOCABULARIO = {
    indisponivel: {rotulo: 'Indisponível', icone: 'ditador-symbolic'},
    carregando: {rotulo: 'Carregando o modelo…', icone: 'ditador-carregando-symbolic'},
    pronto: {rotulo: 'Pronto', icone: 'ditador-symbolic'},
    gravando: {rotulo: 'Gravando', icone: 'ditador-gravando-symbolic'},
    transcrevendo: {rotulo: 'Transcrevendo…', icone: 'ditador-carregando-symbolic'},
    erro: {rotulo: 'O modelo não carregou', icone: 'ditador-falhou-symbolic'},
};

/**
 * O verbete de um estado, com um porto seguro para o dia em que o aplicativo
 * publicar um estado que esta versão da extensão ainda não conhece.
 *
 * @param {string} estado - o nome que veio pelo D-Bus
 * @returns {{rotulo: string, icone: string}} o verbete
 */
export function verbete(estado) {
    return VOCABULARIO[estado] ?? VOCABULARIO.indisponivel;
}

/**
 * O símbolo de um estado, pronto para um `St.Icon`.
 *
 * @param {string} estado - o nome que veio pelo D-Bus
 * @returns {Gio.ThemedIcon} o ícone, com a reserva do tema padrão atrás
 */
export function icone(estado) {
    const nome = verbete(estado).icone;
    return Gio.ThemedIcon.new_from_names([nome, RESERVAS[nome]]);
}

/**
 * Um tempo decorrido no formato do cronômetro: `00:05`, e `1:02:03` para quem
 * ditar por mais de uma hora.
 *
 * @param {number} segundos - o tempo decorrido
 * @returns {string} o texto do cronômetro
 */
export function duracao(segundos) {
    const total = Math.max(0, Math.floor(segundos));
    const horas = Math.floor(total / 3600);
    const minutos = Math.floor(total / 60) % 60;
    const resto = total % 60;
    const doisDigitos = n => `${n}`.padStart(2, '0');

    return horas > 0
        ? `${horas}:${doisDigitos(minutos)}:${doisDigitos(resto)}`
        : `${doisDigitos(minutos)}:${doisDigitos(resto)}`;
}
