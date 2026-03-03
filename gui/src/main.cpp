#include <QCoreApplication>
#include <QGuiApplication>
#include <QObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QString>
#include <QtGlobal>
#include <QUrl>

#include "chatcontroller.h"

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    qmlRegisterUncreatableType<ChatController>("MultiLink", 1, 0, "ChatController",
                                               "Injected by C++ context property");
    QQmlApplicationEngine engine;

    ChatController controller;
    engine.rootContext()->setContextProperty("chatController", &controller);

    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     []() { QCoreApplication::exit(-1); }, Qt::QueuedConnection);

#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    engine.loadFromModule("MultiLink", "Main");
#else
    engine.load(QUrl(QStringLiteral("qrc:/qt/qml/MultiLink/qml/Main.qml")));
#endif
    return app.exec();
}
