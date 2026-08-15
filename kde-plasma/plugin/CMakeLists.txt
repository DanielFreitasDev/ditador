# O proxy do serviço é **gerado** do contrato canônico, e não escrito à mão.
#
# É a regra do projeto inteiro num arquivo de build: `dbus/contrato.xml` é a
# cópia canônica, e cada cliente sai dela. Escrever `QStringLiteral("Alternar")`
# aqui daria um erro silencioso em tempo de execução no dia em que o método
# mudasse de nome; gerado, o compilador acha o problema.
#
# O CLASSNAME é uma propriedade do build, e não uma anotação no XML: o contrato
# não muda de forma para acomodar o Qt. Sem ele o qdbusxml2cpp chamaria a classe
# de `IoGithubDanielfreitasdevDitadorInterface`.
set(CONTRATO ${CMAKE_CURRENT_SOURCE_DIR}/../../dbus/contrato.xml)
set_source_files_properties(${CONTRATO} PROPERTIES
    CLASSNAME ServicoDoDitador
    NO_NAMESPACE ON)

qt_add_dbus_interface(PROXY_GERADO ${CONTRATO} servicododitador)

qt_add_qml_module(ditadorplasma
    # O namespace é nosso, derivado do mesmo domínio invertido que dá o nome no
    # barramento. Nada de `org.kde.plasma.private.*`: aquilo é do Plasma, e um
    # módulo nosso morando lá seria uma promessa que não temos como cumprir.
    URI io.github.danielfreitasdev.ditador
    VERSION 1.0
    # Uma biblioteca só, e não a dupla padrão (o `.so` dos tipos mais o `.so` do
    # plugin que o carrega). Instalados fora dos diretórios de biblioteca do
    # sistema — e um módulo QML mora no diretório de QML —, os dois só se acham
    # por RPATH, que é uma coisa a mais para dar errado na máquina de outra
    # pessoa. Sendo o mesmo alvo, não há segundo arquivo para achar.
    PLUGIN_TARGET ditadorplasma
    # O módulo é construído já no formato de uma árvore de import — o caminho
    # abaixo termina no mesmo `io/github/…` que o URI pede. Não é capricho: é o
    # que faz `qmllint -I build/qml` conseguir resolver o módulo antes de ele
    # estar instalado no sistema, e sem isso não há como conferir o QML do
    # widget senão depois de um `sudo make install`.
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/io/github/danielfreitasdev/ditador
    SOURCES
        ditadorbackend.cpp
        ditadorbackend.h
        presenca.cpp
        presenca.h
        ${PROXY_GERADO}
)

target_link_libraries(ditadorplasma PRIVATE
    Qt6::Core
    Qt6::Qml
    Qt6::DBus)

# O arquivo gerado sai no diretório de build; o `#include` dele precisa achá-lo.
target_include_directories(ditadorplasma PRIVATE ${CMAKE_CURRENT_BINARY_DIR})

# O código gerado é do qdbusxml2cpp e não nosso: os avisos dele não são
# acionáveis aqui e afogariam os nossos, que são.
set_source_files_properties(${PROXY_GERADO} PROPERTIES
    COMPILE_OPTIONS "-Wno-pedantic"
    SKIP_AUTOUIC ON)

# `qt_add_qml_module` já sabe pôr a biblioteca, o `qmldir` e os `.qmltypes` no
# lugar certo a partir do URI — daí não haver caminho nenhum escrito aqui.
qt_query_qml_module(ditadorplasma
    URI modulo_uri
    TARGET_PATH modulo_caminho
    QMLDIR modulo_qmldir
    TYPEINFO modulo_typeinfo
    PLUGIN_TARGET modulo_plugin)

set(DESTINO ${KDE_INSTALL_QMLDIR}/${modulo_caminho})
install(TARGETS ditadorplasma ${modulo_plugin} DESTINATION ${DESTINO})
install(FILES ${modulo_qmldir} ${modulo_typeinfo} DESTINATION ${DESTINO})
