/* SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 */

#include "ditadorbackend.h"
#include "presenca.h"
#include "servicododitador.h"

#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QLoggingCategory>
#include <QMetaProperty>

namespace
{
Q_LOGGING_CATEGORY(LOG, "ditador.plasma")

/* O serviço e a interface têm o mesmo nome — é assim do lado do Rust, e vem daí
 * a única fonte deste texto: `staticInterfaceName()` é gerado do
 * `dbus/contrato.xml`. Escrevê-lo à mão aqui seria a sétima cópia. */
QString servico()
{
    return QString::fromLatin1(ServicoDoDitador::staticInterfaceName());
}

/* O caminho do objeto é a única coisa que o XML não carrega: introspecção
 * descreve o que há num caminho, não onde ele fica. É o valor de `CAMINHO`, em
 * `src/dbus.rs`. */
const QString CAMINHO = QStringLiteral("/io/github/danielfreitasdev/Ditador");

const QString PROPRIEDADES = QStringLiteral("org.freedesktop.DBus.Properties");

/* As chaves do contrato, uma vez só. */
const QString ESTADO = QStringLiteral("Estado");
const QString MENSAGEM = QStringLiteral("Mensagem");
const QString GRAVANDO_DESDE = QStringLiteral("GravandoDesde");
const QString MODELO = QStringLiteral("Modelo");
const QString IDIOMA = QStringLiteral("Idioma");
const QString ATALHO = QStringLiteral("Atalho");

/* Confere, uma vez por processo, que as chaves acima existem mesmo no contrato.
 *
 * O proxy gerado carrega as propriedades do XML no `staticMetaObject` dele, e é
 * contra isso que se compara. Sem esta conferência, uma propriedade renomeada no
 * contrato não daria erro de compilação nenhum — daria um popup com os campos
 * em branco, que é o tipo de falha que só aparece na tela de quem instalou.
 *
 * Aviso e não erro fatal: um campo a menos no popup não é motivo para derrubar
 * o painel de ninguém. */
void conferirOContrato()
{
    const QMetaObject &molde = ServicoDoDitador::staticMetaObject;
    for (const QString &chave : {ESTADO, MENSAGEM, GRAVANDO_DESDE, MODELO, IDIOMA, ATALHO}) {
        if (molde.indexOfProperty(chave.toLatin1().constData()) < 0) {
            qCWarning(LOG) << "o contrato D-Bus não tem mais a propriedade" << chave
                           << "— o widget vai mostrar esse campo vazio";
        }
    }
}
}

DitadorBackend::DitadorBackend(QObject *parent)
    : QObject(parent)
{
    [[maybe_unused]] static const bool conferido = [] {
        conferirOContrato();
        return true;
    }();

    m_servico = new ServicoDoDitador(servico(), CAMINHO, QDBusConnection::sessionBus(), this);

    /* O sinal do nível vem pelo proxy gerado: tipado, e com o nome saído do
     * XML. É o único sinal da interface. */
    connect(m_servico, &ServicoDoDitador::Nivel, this, &DitadorBackend::nivel);

    /* Cada mudança de estado do Ditador chega por aqui. A assinatura vem antes
     * da primeira leitura, e não depois: entre "como ele está?" e "me avise
     * quando mudar" cabe uma gravação inteira começar, e a mudança que caísse
     * nessa fresta não chegaria nunca. */
    QDBusConnection::sessionBus().connect(servico(),
                                          CAMINHO,
                                          PROPRIEDADES,
                                          QStringLiteral("PropertiesChanged"),
                                          this,
                                          SLOT(aoMudarPropriedades(QString, QVariantMap, QStringList)));

    /* Subir e cair do Ditador. Nada de perguntar de tempos em tempos se ele
     * ainda está lá: o barramento avisa. */
    auto *vigia = new QDBusServiceWatcher(servico(),
                                          QDBusConnection::sessionBus(),
                                          QDBusServiceWatcher::WatchForOwnerChange,
                                          this);
    connect(vigia, &QDBusServiceWatcher::serviceRegistered, this, [this] {
        qCDebug(LOG) << "o Ditador subiu";
        pedirTudo();
    });
    connect(vigia, &QDBusServiceWatcher::serviceUnregistered, this, [this] {
        qCDebug(LOG) << "o Ditador saiu";
        esquecer();
    });

    /* Ele quase sempre já está de pé quando o widget nasce: o próprio widget só
     * é carregado porque o serviço apareceu (veja `X-Plasma-DBusActivationService`
     * no `metadata.json`). Perguntar é assíncrono, e a resposta — sucesso ou
     * erro — é que decide se estamos disponíveis. */
    pedirTudo();

    Presenca::entrou();
}

DitadorBackend::~DitadorBackend()
{
    /* Largar a presença é a primeira coisa, antes de qualquer outra que possa
     * falhar: é ela que mantém o ícone do Ditador recolhido, e uma exceção mais
     * abaixo deixaria o usuário sem ícone nenhum até sair da sessão. */
    Presenca::saiu();
}

QString DitadorBackend::estado() const
{
    return m_propriedades.value(ESTADO).toString();
}

QString DitadorBackend::mensagem() const
{
    return m_propriedades.value(MENSAGEM).toString();
}

qulonglong DitadorBackend::gravandoDesde() const
{
    return m_propriedades.value(GRAVANDO_DESDE).toULongLong();
}

QString DitadorBackend::modelo() const
{
    return m_propriedades.value(MODELO).toString();
}

QString DitadorBackend::idioma() const
{
    return m_propriedades.value(IDIOMA).toString();
}

QString DitadorBackend::atalho() const
{
    return m_propriedades.value(ATALHO).toString();
}

void DitadorBackend::iniciarGravacao()
{
    if (!m_disponivel) {
        return;
    }
    chamar(m_servico->IniciarGravacao(), QStringLiteral("IniciarGravacao"));
}

void DitadorBackend::pararGravacao()
{
    if (!m_disponivel) {
        return;
    }
    chamar(m_servico->PararGravacao(), QStringLiteral("PararGravacao"));
}

void DitadorBackend::abrirConfiguracoes()
{
    if (!m_disponivel) {
        return;
    }
    chamar(m_servico->AbrirConfiguracoes(), QStringLiteral("AbrirConfiguracoes"));
}

void DitadorBackend::encerrar()
{
    if (!m_disponivel) {
        return;
    }
    chamar(m_servico->Encerrar(), QStringLiteral("Encerrar"));
}

void DitadorBackend::aoMudarPropriedades(const QString &interface,
                                         const QVariantMap &mudaram,
                                         const QStringList &invalidaram)
{
    if (interface != servico()) {
        return;
    }

    /* O Ditador manda só o que mudou de verdade, e nunca invalida nada — mas o
     * contrato do `org.freedesktop.DBus.Properties` permite invalidar, e um
     * cliente que ignore essa lista fica com valor velho na tela. Esquecer a
     * chave é o certo: o getter passa a devolver vazio, que é honesto. */
    for (auto it = mudaram.cbegin(); it != mudaram.cend(); ++it) {
        m_propriedades.insert(it.key(), it.value());
    }
    for (const QString &chave : invalidaram) {
        m_propriedades.remove(chave);
    }

    if (!mudaram.isEmpty() || !invalidaram.isEmpty()) {
        Q_EMIT mudou();
    }
}

void DitadorBackend::pedirTudo()
{
    QDBusMessage pergunta =
        QDBusMessage::createMethodCall(servico(), CAMINHO, PROPRIEDADES, QStringLiteral("GetAll"));
    pergunta << servico();

    /* Perguntar pelo estado do Ditador não é motivo para iniciá-lo. Quem decide
     * se ele sobe é o usuário, pelo serviço do systemd ou pelo lançador — não um
     * widget que só queria desenhar um ícone. É o mesmo
     * `DO_NOT_AUTO_START` da extensão do GNOME. */
    pergunta.setAutoStartService(false);

    auto *observador =
        new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(pergunta), this);
    connect(observador, &QDBusPendingCallWatcher::finished, this, [this](QDBusPendingCallWatcher *o) {
        o->deleteLater();
        const QDBusPendingReply<QVariantMap> resposta = *o;
        if (resposta.isError()) {
            /* Não está no ar, ou não respondeu. Não é erro para registrar em
             * voz alta: é o estado normal antes de o Ditador subir. */
            qCDebug(LOG) << "o Ditador não respondeu —" << resposta.error().message();
            esquecer();
            return;
        }
        m_propriedades = resposta.value();
        m_disponivel = true;
        Q_EMIT mudou();
    });
}

void DitadorBackend::esquecer()
{
    if (!m_disponivel && m_propriedades.isEmpty()) {
        return;
    }
    m_disponivel = false;
    /* O retrato inteiro vai junto. Guardá-lo seria mostrar "pronto" e o nome do
     * modelo ao lado de um Ditador que não está mais lá. */
    m_propriedades.clear();
    Q_EMIT mudou();
}

void DitadorBackend::chamar(const QDBusPendingReply<> &resposta, const QString &qual)
{
    auto *observador = new QDBusPendingCallWatcher(resposta, this);
    connect(observador, &QDBusPendingCallWatcher::finished, this, [qual](QDBusPendingCallWatcher *o) {
        o->deleteLater();
        const QDBusPendingReply<> r = *o;
        if (r.isError()) {
            qCWarning(LOG) << qual << "falhou —" << r.error().message();
        }
    });
}

#include "moc_ditadorbackend.cpp"
