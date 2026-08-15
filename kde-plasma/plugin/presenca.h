/* Quem diz ao Ditador que o widget do Plasma existe.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 */

#pragma once

#include <QObject>

class QDBusServiceWatcher;

/* Segura `io.github.danielfreitasdev.Ditador.PlasmaIntegration` enquanto
 * houver pelo menos um widget carregado.
 *
 * É todo o protocolo de "estou aqui" que existe entre os dois lados, e é de
 * propósito que ele não seja um aviso. Quem detém um nome no barramento é uma
 * *conexão*: quando o `plasmashell` cai, quando o plugin morre no meio de um
 * destrutor, quando a sessão encerra — em todos esses casos o barramento solta
 * o nome sozinho, e o ícone do Ditador volta à bandeja sem que ninguém tenha
 * dito nada. Um "avise quando sair" perderia exatamente os casos em que não há
 * quem avise.
 *
 * ## Por que uma contagem, e não um nome por widget
 *
 * O usuário pode pôr dois widgets no painel, sem querer ou de propósito, e
 * `plasmoidviewer` cria mais um em outro processo. O nome, porém, é um só: o
 * barramento o dá a quem chegar primeiro, e a segunda tentativa falha. Falhar
 * ali não pode ser fatal — o segundo widget continua funcionando inteiro, ele
 * só não é o que representa a presença.
 *
 * Dentro do mesmo processo a contagem resolve isso sem tentar duas vezes: o
 * nome é adquirido quando o primeiro widget nasce e largado quando o último
 * morre. Entre processos, quem não conseguiu fica vigiando o nome e assume se
 * ele vagar — é o caso do `plasmoidviewer` fechado com o painel ainda de pé.
 *
 * ## Thread
 *
 * Tudo isto roda na thread do motor QML do `plasmashell`, que é a única que
 * cria e destrói widgets. Daí não haver trava nenhuma aqui.
 *
 * ## Tempo de vida
 *
 * A instância única é filha do `qApp`, e não um `static` de função. A diferença
 * importa dentro do `plasmashell`: um `static` local só é destruído depois que
 * `main()` retorna, quando o `QCoreApplication` já se foi e com ele a conexão de
 * sessão — e o destrutor do `QDBusServiceWatcher` ainda a procuraria. Filha do
 * `qApp`, ela morre *durante* o encerramento dele, com o barramento ainda de pé.
 */
class Presenca : public QObject
{
    Q_OBJECT

public:
    /* Um widget nasceu. Adquire o nome, se ainda não for nosso. */
    static void entrou();

    /* Um widget morreu. Larga o nome quando o último sai.
     *
     * Nunca ressuscita a instância: se ela já se foi, não há nome nosso no
     * barramento para largar. */
    static void saiu();

private:
    explicit Presenca(QObject *parent);

    void tentarAdquirir();

    /* Quantos widgets estão vivos neste processo. */
    int m_quantos = 0;
    /* O nome é nosso agora. */
    bool m_nosso = false;
    QDBusServiceWatcher *m_vigia = nullptr;
};
