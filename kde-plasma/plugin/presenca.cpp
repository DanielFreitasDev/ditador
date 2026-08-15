/* SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 */

#include "presenca.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusServiceWatcher>
#include <QLoggingCategory>
#include <QPointer>

namespace
{
/* O mesmo texto que o Rust espera, em `dbus.rs`
 * (`NOME_DA_INTEGRACAO_PLASMA`). Errar uma letra aqui não daria erro nenhum:
 * daria dois ícones do Ditador na barra, para sempre. Há um teste do lado de lá
 * guardando a constante. */
const QString NOME_DA_INTEGRACAO = QStringLiteral("io.github.danielfreitasdev.Ditador.PlasmaIntegration");

Q_LOGGING_CATEGORY(LOG, "ditador.plasma")

/* `QPointer` e não um ponteiro cru: quem destrói esta instância é o `qApp`, no
 * encerramento, e daí em diante o ponteiro precisa se anular sozinho. Um
 * ponteiro cru continuaria apontando para memória liberada, e um widget que
 * morresse depois disso a usaria. */
QPointer<Presenca> unica;
}

Presenca::Presenca(QObject *parent)
    : QObject(parent)
{
    /* Vigiar o próprio nome parece estranho até se lembrar de que ele pode ser
     * de outro processo: dois `plasmashell` numa sessão aninhada, ou o
     * `plasmoidviewer` aberto durante o desenvolvimento. Quem não conseguiu o
     * nome fica esperando ele vagar, em vez de tentar de novo de tempos em
     * tempos — que é a mesma diferença entre escutar e perguntar toda hora. */
    m_vigia = new QDBusServiceWatcher(NOME_DA_INTEGRACAO,
                                      QDBusConnection::sessionBus(),
                                      QDBusServiceWatcher::WatchForUnregistration,
                                      this);
    connect(m_vigia, &QDBusServiceWatcher::serviceUnregistered, this, [this] {
        /* Se o nome era nosso e acabou de sair, foi porque nós o largamos —
         * não há nada a reconquistar. */
        if (!m_nosso && m_quantos > 0) {
            tentarAdquirir();
        }
    });
}

void Presenca::entrou()
{
    if (!unica) {
        unica = new Presenca(qApp);
    }
    if (++unica->m_quantos == 1) {
        unica->tentarAdquirir();
    }
}

void Presenca::saiu()
{
    if (!unica) {
        return;
    }
    if (unica->m_quantos > 0 && --unica->m_quantos == 0 && unica->m_nosso) {
        QDBusConnection::sessionBus().unregisterService(NOME_DA_INTEGRACAO);
        unica->m_nosso = false;
        qCDebug(LOG) << "último widget saiu; o Ditador volta para a bandeja";
    }
}

void Presenca::tentarAdquirir()
{
    /* `registerService` é síncrono, mas não é uma chamada ao Ditador: é uma
     * troca com o `dbus-daemon`, que responde na hora e é a mesma que o Qt faz
     * para qualquer serviço registrar-se. Acontece uma vez por processo, no
     * nascimento do primeiro widget. */
    m_nosso = QDBusConnection::sessionBus().registerService(NOME_DA_INTEGRACAO);
    if (m_nosso) {
        qCDebug(LOG) << "presença anunciada; o Ditador recolhe o ícone da bandeja";
    } else {
        /* Outro processo chegou antes. Não é erro: este widget funciona
         * inteiro, ele só não é o que representa a presença. */
        qCDebug(LOG) << "outra instância já representa a integração; seguindo sem o nome";
    }
}

#include "moc_presenca.cpp"
