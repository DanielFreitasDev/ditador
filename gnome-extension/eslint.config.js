/* ESLint para o código GJS da extensão.
 *
 * As regras seguem o estilo do próprio gnome-shell (indentação de 4, aspas
 * simples, ponto e vírgula, chaves só quando o bloco tem mais de uma linha), que
 * é também o que a revisão do extensions.gnome.org espera ler.
 *
 * O Node existe aqui só para rodar isto. A extensão em si é GJS puro: nada de
 * `node_modules` entra no pacote, e nada em tempo de execução depende dele.
 */

import js from '@eslint/js';
import globals from 'globals';

export default [
    {
        ignores: ['node_modules/**', '*.shell-extension.zip'],
    },
    js.configs.recommended,
    {
        languageOptions: {
            ecmaVersion: 2024,
            sourceType: 'module',
            globals: {
                ...globals.es2021,
                // O ambiente do GNOME Shell (`js/ui/environment.js`): não são
                // variáveis nossas, e não são do navegador nem do Node.
                global: 'readonly',
                globalThis: 'readonly',
                console: 'readonly',
                log: 'readonly',
                logError: 'readonly',
                _: 'readonly',
                C_: 'readonly',
                N_: 'readonly',
                ngettext: 'readonly',
                pkg: 'readonly',
                // Do GJS puro, usados só pelos scripts de `scripts/`.
                print: 'readonly',
                printerr: 'readonly',
                // Temporizadores do GJS, não os do navegador.
                setTimeout: 'readonly',
                clearTimeout: 'readonly',
                setInterval: 'readonly',
                clearInterval: 'readonly',
            },
        },
        rules: {
            'array-bracket-spacing': ['error', 'never'],
            'arrow-parens': ['error', 'as-needed'],
            'brace-style': ['error', '1tbs', {allowSingleLine: true}],
            'comma-dangle': ['error', 'always-multiline'],
            'curly': ['error', 'multi-or-nest', 'consistent'],
            'eqeqeq': ['error', 'smart'],
            'indent': ['error', 4, {
                SwitchCase: 0,
                // `GObject.registerClass(class X extends Y { … })` é *a* forma
                // de declarar uma classe no GNOME Shell, e o corpo dela fica na
                // coluna zero — é assim em todo o js/ui do Shell. A regra de
                // indentação do ESLint não sabe expressar isso, então o nó fica
                // de fora; o que está dentro da classe continua sendo conferido.
                ignoredNodes: ['CallExpression > ClassExpression.arguments'],
            }],
            'no-unused-vars': ['error', {argsIgnorePattern: '^_'}],
            'no-var': 'error',
            'object-curly-spacing': ['error', 'never'],
            'prefer-const': 'error',
            'quotes': ['error', 'single', {avoidEscape: true}],
            'semi': ['error', 'always'],
            // O Shell é um processo só: uma exceção não tratada num callback
            // assíncrono derruba a área de trabalho de quem está usando.
            'no-async-promise-executor': 'error',
            'no-promise-executor-return': 'error',
        },
    },
];
