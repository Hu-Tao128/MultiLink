#include <QCoreApplication>
#include <QGuiApplication>
#include <QObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QString>
#include <QUrl>

#include "chatcontroller.h"

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    qmlRegisterUncreatableType<ChatController>("MultiLink", 1, 0, "ChatController",
                                               "Injected by C++ context property");
    QQmlApplicationEngine engine;

    ChatController controller;
    engine.rootContext()->setContextProperty("chatController", &controller);

    const QUrl url(QStringLiteral("qrc:/MultiLink/qml/Main.qml"));
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     []() { QCoreApplication::exit(-1); }, Qt::QueuedConnection);

    engine.load(url);
    return app.exec();
}
