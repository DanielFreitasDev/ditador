/* Conversa com o aplicativo Ditador pelo D-Bus, e entrega o resultado ao QML.
 *
 * SPDX-FileCopyrightText: 2026 Daniel Freitas
 * SPDX-License-Identifier: MIT
 */

#pragma once

#include <QDBusPendingReply>
#include <QObject>
#include <QQmlEngine>
#include <QVariantMap>

class QDBusPendingCallWatcher;
class ServicoDoDitador;

/* A única parte da integração que sabe que existe um barramento.
 *
 * É um tradutor, e nada além disso: recebe estado, entrega estado, encaminha
 * comando. Não há regra de negócio aqui, nem máquina de estados própria — a
 * fonte da verdade é o processo Rust do outro lado, e uma segunda cópia deste
 * lado só teria como discordar dela. É o mesmo desenho de
 * `gnome-extension/src/backend.js`, pelo mesmo motivo.
 *
 * ## Nada bloqueia
 *
 * Este objeto vive dentro do `plasmashell` — o processo que desenha a área de
 * trabalho inteira. Uma chamada síncrona ao Ditador congelaria o painel pelo
 * tempo que o outro lado levasse para responder, e o outro lado às vezes está
 * carregando 574 MB de modelo na GPU.
 *
 * Por isso:
 *
 *   - o proxy é **gerado** do XML (`qt_add_dbus_interface`), e não um
 *     `QDBusInterface`, que introspecta o serviço dentro do construtor e
 *     bloqueia enquanto o faz;
 *   - as propriedades **não** são lidas pelo proxy gerado. Os getters que o
 *     `qdbusxml2cpp` produz fazem um `Get` síncrono a cada leitura, e o QML
 *     lê propriedade a cada repintura. O que se usa aqui é um `GetAll`
 *     assíncrono quando o serviço aparece, mais o `PropertiesChanged` que o
 *     Ditador emite a cada mudança — o mesmo caminho que o `Gio.DBusProxy` da
 *     extensão do GNOME percorre sozinho;
 *   - os métodos vão por `asyncCall`, e o retorno é só conferido para registrar
 *     a falha.
 *
 * ## Um sinal só para todas as propriedades
 *
 * `mudou()` avisa que qualquer coisa mudou, e não há um sinal por propriedade.
 * Não é economia de digitação: é a granularidade real do outro lado. O Ditador
 * manda **um** `PropertiesChanged` com tudo o que mudou junto, de propósito —
 * começar a gravar muda `Estado` e `GravandoDesde` no mesmo instante, e quem
 * desenha o cronômetro lê os dois para desenhar uma coisa só. Sinais separados
 * daqui inventariam uma ordem entre eles que a mensagem não tem.
 */
class DitadorBackend : public QObject
{
    Q_OBJECT
    QML_ELEMENT

    /* O Ditador está em execução? A pergunta é literalmente "alguém detém o
     * nome dele no barramento agora" — não existe um estado `indisponivel`
     * vindo de lá, porque quem não está no ar não responde nada. */
    Q_PROPERTY(bool disponivel READ disponivel NOTIFY mudou)

    /* `carregando`, `pronto`, `gravando`, `transcrevendo` ou `erro`, exatamente
     * como o contrato os escreve. Vazio enquanto não se sabe. */
    Q_PROPERTY(QString estado READ estado NOTIFY mudou)

    /* A última mensagem de erro ou aviso, vazia quando não há nenhuma. */
    Q_PROPERTY(QString mensagem READ mensagem NOTIFY mudou)

    /* Quando a gravação em curso começou, em milissegundos desde a época; 0
     * quando não há gravação. É a fonte da verdade do cronômetro: este lado
     * nunca conta o tempo por conta própria. */
    Q_PROPERTY(qulonglong gravandoDesde READ gravandoDesde NOTIFY mudou)

    Q_PROPERTY(QString modelo READ modelo NOTIFY mudou)
    Q_PROPERTY(QString idioma READ idioma NOTIFY mudou)

    /* O atalho global, como se escreve numa frase (`Pause`). */
    Q_PROPERTY(QString atalho READ atalho NOTIFY mudou)

public:
    explicit DitadorBackend(QObject *parent = nullptr);
    ~DitadorBackend() override;

    bool disponivel() const
    {
        return m_disponivel;
    }
    QString estado() const;
    QString mensagem() const;
    qulonglong gravandoDesde() const;
    QString modelo() const;
    QString idioma() const;
    QString atalho() const;

    /* Começar e parar são pedidos separados, e não um `Alternar`, de propósito.
     *
     * O rótulo do botão diz uma das duas coisas, e entre o instante em que ele
     * foi desenhado e o clique cabe um ditado inteiro pelo atalho global —
     * segurar a tecla é o uso normal deste programa. Com `Alternar`, um botão
     * escrito "Ditar agora" pararia a gravação que começou nesse meio-tempo, e
     * faria exatamente o contrário do que promete. Pedindo o resultado desejado
     * em vez da troca, o botão sempre faz o que está escrito nele.
     *
     * O contrato tem `Alternar` — é o que a tecla e o `ditador --alternar`
     * usam —, e ele fica de fora daqui por escolha, não por esquecimento. */
    Q_INVOKABLE void iniciarGravacao();
    Q_INVOKABLE void pararGravacao();

    /* Abre a janela de configurações do próprio Ditador. Quem desenha aquilo é
     * o egui, e não este widget: são dezenas de controles (modelo, microfone,
     * idioma, colagem, download) que já existem, funcionam e não cabem num
     * popup de painel. */
    Q_INVOKABLE void abrirConfiguracoes();

    Q_INVOKABLE void encerrar();

Q_SIGNALS:
    /* Alguma propriedade mudou — veja o cabeçalho da classe. */
    void mudou();

    /* O pico do microfone agora, de 0 a 1. Chega umas quinze vezes por segundo,
     * e só enquanto se grava. É sinal e não propriedade porque não é estado:
     * nada disso precisa ser lembrado depois que passou. */
    void nivel(double valor);

private Q_SLOTS:
    /* Assinado com os tipos crus do `org.freedesktop.DBus.Properties` porque a
     * conexão é feita pela macro `SLOT()`, que casa por assinatura. */
    void aoMudarPropriedades(const QString &interface,
                             const QVariantMap &mudaram,
                             const QStringList &invalidaram);

private:
    void pedirTudo();
    void esquecer();
    void chamar(const QDBusPendingReply<> &resposta, const QString &qual);

    ServicoDoDitador *m_servico = nullptr;
    /* O estado como o Ditador o publicou, pelas chaves do contrato. Guardar o
     * mapa cru, em vez de campos, é o que faz `GetAll` e `PropertiesChanged`
     * caírem no mesmo lugar sem tradução no meio. */
    QVariantMap m_propriedades;
    bool m_disponivel = false;
};
