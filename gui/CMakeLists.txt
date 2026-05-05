cmake_minimum_required(VERSION 3.21)
project(MultiLinkQt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Core Quick QuickControls2 Qml Network)

qt_standard_project_setup()

if(COMMAND qt_policy)
    if(DEFINED Qt6Core_VERSION AND Qt6Core_VERSION VERSION_GREATER_EQUAL "6.5")
        qt_policy(SET QTP0001 NEW)
        qt_policy(SET QTP0004 NEW)
    endif()
endif()

if(APPLE)
    add_custom_target(chat_controller_rust
        COMMAND ${CMAKE_COMMAND} -E env MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET} cargo build --release
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/rust/chat_controller
        COMMENT "Building Rust chat backend"
    )
else()
    add_custom_target(chat_controller_rust
        COMMAND cargo build --release
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/rust/chat_controller
        COMMENT "Building Rust chat backend"
    )
endif()

add_library(chat_controller STATIC IMPORTED GLOBAL)
if(WIN32)
    set(RUST_LIB_NAME "chat_controller.lib")
else()
    set(RUST_LIB_NAME "libchat_controller.a")
endif()

set_target_properties(chat_controller PROPERTIES
    IMPORTED_LOCATION ${CMAKE_CURRENT_SOURCE_DIR}/rust/chat_controller/target/release/${RUST_LIB_NAME}
)
add_dependencies(chat_controller chat_controller_rust)

# Remove stale directory that may conflict with linker output (macOS CI)
if(APPLE)
    add_custom_command(TARGET multilink PRE_LINK
        COMMAND ${CMAKE_COMMAND} -E rm -f $<TARGET_FILE:multilink>
        COMMAND ${CMAKE_COMMAND} -E rm -rf "${CMAKE_CURRENT_BINARY_DIR}/multilink"
        COMMENT "Cleaning stale linker output"
    )
endif()

qt_add_executable(multilink
    src/main.cpp
    src/chatcontroller.cpp
    src/chatcontroller.h
)

set_target_properties(multilink PROPERTIES
    OUTPUT_NAME "multilink"
)

qt_add_qml_module(multilink
    URI MultiLink
    VERSION 1.0
    RESOURCE_PREFIX "/qt/qml"
    QML_FILES
        qml/Main.qml
        qml/ChatView.qml
        qml/Settings.qml
)

target_link_libraries(multilink PRIVATE Qt6::Core Qt6::Quick Qt6::Qml Qt6::Network chat_controller)

if(WIN32)
    target_link_libraries(multilink PRIVATE bcrypt ntdll userenv ws2_32 pdh ole32 propsys PowrProf Iphlpapi Psapi Netapi32 Secur32)
endif()

# ── Packaging ──────────────────────────────────────────────────────────────
set(CPACK_PACKAGE_NAME "MultiLink")
set(CPACK_PACKAGE_VERSION "0.1.0")
set(CPACK_PACKAGE_VENDOR "MultiLink")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "MultiLink - Local-first AI coding agent")
set(CPACK_PACKAGE_HOMEPAGE_URL "https://github.com/anomalyco/MultiLink")

if(UNIX AND NOT APPLE)
    # .deb packaging
    set(CPACK_GENERATOR "DEB")
    set(CPACK_DEBIAN_PACKAGE_MAINTAINER "MultiLink Team")
    set(CPACK_DEBIAN_PACKAGE_SECTION "devel")
    set(CPACK_DEBIAN_PACKAGE_DEPENDS
        "qt6-qpa-plugins, libqt6quick6, qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts"
    )
elseif(APPLE)
    # .dmg packaging
    set(CPACK_GENERATOR "DragNDrop")
    set(CPACK_DMG_VOLUME_NAME "MultiLink")
    set(CPACK_DMG_DS_STORE_SETUP_SCRIPT "")
elseif(WIN32)
    # NSIS installer
    set(CPACK_GENERATOR "NSIS")
    set(CPACK_NSIS_DISPLAY_NAME "MultiLink")
endif()

install(TARGETS multilink
    BUNDLE DESTINATION .
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)

# Install QML imports bundle (for non-bundle platforms)
if(NOT APPLE)
    install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/qml/ DESTINATION ${CMAKE_INSTALL_BINDIR}/qml)
endif()
