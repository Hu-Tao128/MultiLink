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

#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    engine.loadFromModule("MultiLink", "Main");
#else
    QUrl base(QStringLiteral("qrc:/qt/qml/MultiLink/qml/Main.qml"));
    engine.load(base);
#endif

    if (engine.rootObjects().isEmpty()) {
        return -1;
    }

    return app.exec();
}
