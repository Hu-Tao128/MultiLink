#include "chatcontroller.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>

#include <cstdlib>

extern "C" {
struct BackendCallbacks {
    void (*on_stream_started)(void *ctx);
    void (*on_stream_chunk)(void *ctx, const char *text);
    void (*on_stream_finished)(void *ctx);
    void (*on_stream_error)(void *ctx, const char *message);
};

void *chat_backend_create(BackendCallbacks callbacks, void *ctx);
void chat_backend_destroy(void *backend);
void chat_backend_send_prompt(void *backend, const char *text);
void chat_backend_stop_generation(void *backend);
void chat_backend_new_session(void *backend);
void chat_backend_select_session(void *backend, const char *session_id);
void chat_backend_select_model(void *backend, const char *model);

char *chat_backend_sessions_json(void *backend);
char *chat_backend_models_json(void *backend);
char *chat_backend_get_active_provider(void *backend);
char *chat_backend_get_active_model(void *backend);
char *chat_backend_get_provider_scope(void *backend);
char *chat_backend_get_provider_health(void *backend);
bool chat_backend_get_is_loading(void *backend);
void chat_backend_string_free(char *ptr);
}

static QString takeRustString(char *raw) {
    if (!raw) {
        return QString();
    }
    QString value = QString::fromUtf8(raw);
    chat_backend_string_free(raw);
    return value;
}

static QVariantList parseJsonList(char *raw) {
    QVariantList output;
    if (!raw) {
        return output;
    }

    const QByteArray bytes(raw);
    chat_backend_string_free(raw);
    const QJsonDocument doc = QJsonDocument::fromJson(bytes);
    if (!doc.isArray()) {
        return output;
    }

    const QJsonArray array = doc.array();
    for (const QJsonValue &value : array) {
        output.append(value.toObject().toVariantMap());
    }
    return output;
}

static void onStreamStartedThunk(void *ctx) {
    auto *self = static_cast<ChatController *>(ctx);
    QMetaObject::invokeMethod(self, [self]() {
        self->refreshSnapshot();
        emit self->streamStarted();
    }, Qt::QueuedConnection);
}

static void onStreamChunkThunk(void *ctx, const char *text) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString chunk = QString::fromUtf8(text ? text : "");
    QMetaObject::invokeMethod(self, [self, chunk]() {
        self->refreshSnapshot();
        emit self->streamChunk(chunk);
    }, Qt::QueuedConnection);
}

static void onStreamFinishedThunk(void *ctx) {
    auto *self = static_cast<ChatController *>(ctx);
    QMetaObject::invokeMethod(self, [self]() {
        self->refreshSnapshot();
        self->refreshCollections();
        emit self->streamFinished();
    }, Qt::QueuedConnection);
}

static void onStreamErrorThunk(void *ctx, const char *message) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString error = QString::fromUtf8(message ? message : "Unknown error");
    QMetaObject::invokeMethod(self, [self, error]() {
        self->refreshSnapshot();
        emit self->streamError(error);
    }, Qt::QueuedConnection);
}

ChatController::ChatController(QObject *parent)
    : QObject(parent) {
    BackendCallbacks callbacks{};
    callbacks.on_stream_started = &onStreamStartedThunk;
    callbacks.on_stream_chunk = &onStreamChunkThunk;
    callbacks.on_stream_finished = &onStreamFinishedThunk;
    callbacks.on_stream_error = &onStreamErrorThunk;

    m_backend = chat_backend_create(callbacks, this);
    refreshSnapshot();
    refreshCollections();
}

ChatController::~ChatController() {
    if (m_backend) {
        chat_backend_destroy(m_backend);
        m_backend = nullptr;
    }
}

QString ChatController::activeProvider() const { return m_activeProvider; }
QString ChatController::activeModel() const { return m_activeModel; }
QString ChatController::providerScope() const { return m_providerScope; }
QString ChatController::providerHealth() const { return m_providerHealth; }
bool ChatController::isLoading() const { return m_isLoading; }
QVariantList ChatController::sessions() const { return m_sessions; }
QVariantList ChatController::availableModelsDetailed() const { return m_models; }

void ChatController::refreshSnapshot() {
    if (!m_backend) {
        return;
    }

    const QString newProvider = takeRustString(chat_backend_get_active_provider(m_backend));
    const QString newModel = takeRustString(chat_backend_get_active_model(m_backend));
    const QString newScope = takeRustString(chat_backend_get_provider_scope(m_backend));
    const QString newHealth = takeRustString(chat_backend_get_provider_health(m_backend));
    const bool newLoading = chat_backend_get_is_loading(m_backend);

    if (m_activeProvider != newProvider) {
        m_activeProvider = newProvider;
        emit activeProviderChanged();
    }
    if (m_activeModel != newModel) {
        m_activeModel = newModel;
        emit activeModelChanged();
    }
    if (m_providerScope != newScope) {
        m_providerScope = newScope;
        emit providerScopeChanged();
    }
    if (m_providerHealth != newHealth) {
        m_providerHealth = newHealth;
        emit providerHealthChanged();
    }
    if (m_isLoading != newLoading) {
        m_isLoading = newLoading;
        emit isLoadingChanged();
    }
}

void ChatController::refreshCollections() {
    if (!m_backend) {
        return;
    }

    m_sessions = parseJsonList(chat_backend_sessions_json(m_backend));
    m_models = parseJsonList(chat_backend_models_json(m_backend));
    emit sessionsChanged();
    emit modelsChanged();
}

void ChatController::sendPrompt(const QString &text) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = text.toUtf8();
    chat_backend_send_prompt(m_backend, encoded.constData());
    refreshSnapshot();
}

void ChatController::stopGeneration() {
    if (!m_backend) {
        return;
    }
    chat_backend_stop_generation(m_backend);
    refreshSnapshot();
}

void ChatController::newSession() {
    if (!m_backend) {
        return;
    }
    chat_backend_new_session(m_backend);
    refreshSnapshot();
    refreshCollections();
}

void ChatController::selectSession(const QString &id) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = id.toUtf8();
    chat_backend_select_session(m_backend, encoded.constData());
    refreshSnapshot();
}

void ChatController::selectModel(const QString &name) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = name.toUtf8();
    chat_backend_select_model(m_backend, encoded.constData());
    refreshSnapshot();
}
