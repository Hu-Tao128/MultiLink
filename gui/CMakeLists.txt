cmake_minimum_required(VERSION 3.21)
project(MultiLinkQt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Core Quick Qml Network)

qt_standard_project_setup()

if(COMMAND qt_policy)
    if(DEFINED Qt6Core_VERSION AND Qt6Core_VERSION VERSION_GREATER_EQUAL "6.5")
        qt_policy(SET QTP0001 NEW)
    endif()
    #    qt_policy(SET QTP0004 NEW)
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

qt_add_executable(multilink_gui
    src/main.cpp
    src/chatcontroller.cpp
    src/chatcontroller.h
)

if(APPLE)
    set_target_properties(multilink_gui PROPERTIES
        OUTPUT_NAME "multilink_gui"
    )
else()
    set_target_properties(multilink_gui PROPERTIES
        OUTPUT_NAME "multilink"
    )
endif()

qt_add_qml_module(multilink_gui
    URI MultiLink
    VERSION 1.0
    RESOURCE_PREFIX "/qt/qml"
    QML_FILES
        qml/Main.qml
        qml/ChatView.qml
        qml/Settings.qml
)

target_link_libraries(multilink_gui PRIVATE Qt6::Core Qt6::Quick Qt6::Qml Qt6::Network chat_controller)

if(WIN32)
    target_link_libraries(multilink_gui PRIVATE bcrypt ntdll userenv ws2_32 pdh ole32 propsys PowrProf Iphlpapi Psapi Netapi32 Secur32)
endif()
