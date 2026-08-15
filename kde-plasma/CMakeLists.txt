# O CMake daqui constrói **só** o plugin QML em C++ que dá D-Bus ao widget.
#
# O widget em si (`plasmoid/package/`) é QML e JSON: não se compila, e quem o
# instala é o `kpackagetool6`, que é o caminho oficial e não pede senha nenhuma.
# São duas metades com ciclos de vida diferentes — mexer no QML e reinstalar o
# pacote leva um segundo; mexer no C++ obriga a recompilar — e juntá-las num
# alvo só faria a metade barata pagar o preço da cara.
#
# O Ditador continua sendo Cargo. Isto aqui não o toca.

cmake_minimum_required(VERSION 3.20)

# A versão sai do Cargo.toml, que é a única fonte da verdade dela no projeto.
# Escrevê-la aqui também seria um segundo lugar para esquecer de atualizar — e o
# CLAUDE.md registra que isso já aconteceu com o número da versão antes.
file(READ ${CMAKE_CURRENT_SOURCE_DIR}/../Cargo.toml CARGO_TOML)
string(REGEX MATCH "\nversion = \"([0-9]+\\.[0-9]+\\.[0-9]+)\"" _ ${CARGO_TOML})
if(NOT CMAKE_MATCH_1)
    message(FATAL_ERROR "não achei a versão em ../Cargo.toml")
endif()

project(ditador-plasma
    VERSION ${CMAKE_MATCH_1}
    DESCRIPTION "Integração do Ditador com o KDE Plasma 6"
    LANGUAGES CXX)

# Os mínimos são os do alvo declarado (Kubuntu 26.04: Qt 6.10.2, KF6 6.24,
# Plasma 6.6), arredondados para baixo até onde as APIs usadas aqui existem.
# Nada de Qt 5, KF5 nem Plasma 5 — veja o README.
set(QT_MINIMO 6.6)
set(ECM_MINIMO 6.0)

find_package(ECM ${ECM_MINIMO} REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH ${ECM_MODULE_PATH})

# Só o `KDEInstallDirs`, e de propósito.
#
# O `KDECMakeSettings` que costuma vir junto zera o prefixo das bibliotecas do
# tipo MODULE — a convenção dos plugins do KDE, que se chamam `nome.so` e não
# `libnome.so`. Só que isto aqui não é um plugin do KDE: é um módulo QML, e o
# motor do Qt procura `libnome.so` a partir do que o `qmldir` declara. Com o
# prefixo zerado o módulo era encontrado e o plugin dele não, com a mensagem
# `module "…" plugin "ditadorplasma" not found`.
#
# O `KDECompilerSettings` também fica de fora: as opções de compilação que
# interessam aqui estão escritas logo abaixo, e uma linha que se lê vale mais do
# que um pacote de padrões herdados.
include(KDEInstallDirs)

# Core pelo QObject, Qml pelo módulo, DBus pelo proxy gerado. Mais nada: a
# camada é um tradutor, e um tradutor não precisa de Widgets, Gui nem de
# framework nenhum do KDE. O que ela não puxa é o que não pesa no plasmashell.
find_package(Qt6 ${QT_MINIMO} REQUIRED COMPONENTS Core Qml DBus)

# O prefixo de recursos `:/qt/qml/` do Qt 6.5 em diante. Dizê-lo explicitamente
# tira um aviso do configure e prende o comportamento: sem isto, o dia em que o
# padrão mudar o módulo passa a procurar os próprios recursos noutro lugar.
qt_policy(SET QTP0001 NEW)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Compilação limpa é requisito, não meta: este código roda dentro do processo
# que desenha a área de trabalho inteira.
add_compile_options(-Wall -Wextra -Wpedantic)

# Sem conversões implícitas de e para QString: o proxy do D-Bus é gerado, e um
# `const char *` virando QString sem que ninguém peça é justamente o tipo de
# descuido que passaria batido numa camada de tradução.
add_compile_definitions(
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    QT_NO_URL_CAST_FROM_STRING
    QT_USE_QSTRINGBUILDER)

add_subdirectory(plugin)

message(STATUS "")
message(STATUS "Ditador · integração Plasma ${PROJECT_VERSION}")
message(STATUS "  Qt              ${Qt6_VERSION}")
message(STATUS "  plugin QML em   ${KDE_INSTALL_FULL_QMLDIR}/io/github/danielfreitasdev/ditador")
message(STATUS "")
message(STATUS "  O widget não passa por aqui: ./instalar.sh o entrega ao kpackagetool6.")
message(STATUS "")
