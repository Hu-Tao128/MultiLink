#include "chatcontroller.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>

#include <cstdlib>
#include <utility>

extern "C" {
struct BackendCallbacks {
    void (*on_stream_started)(void *ctx, const char *session_id);
    void (*on_stream_chunk)(void *ctx, const char *session_id, const char *text);
    void (*on_stream_finished)(void *ctx, const char *session_id);
    void (*on_stream_error)(void *ctx, const char *session_id, const char *message);
    void (*on_sessions_updated)(void *ctx, const char *json);
    void (*on_models_updated)(void *ctx, const char *json);
    void (*on_messages_updated)(void *ctx, const char *json);
};

void *chat_backend_create(BackendCallbacks callbacks, void *ctx);
void chat_backend_destroy(void *backend);
void chat_backend_send_prompt(void *backend, const char *text);
void chat_backend_send_prompt_for_session(void *backend, const char *session_id, const char *text);
void chat_backend_stop_generation(void *backend);
void chat_backend_new_session(void *backend);
void chat_backend_select_session(void *backend, const char *session_id);
void chat_backend_select_model(void *backend, const char *model);
void chat_backend_set_session_project_root(void *backend, const char *session_id, const char *project_root);
void chat_backend_request_sessions(void *backend);
void chat_backend_request_models(void *backend);
void chat_backend_request_messages(void *backend, const char *session_id);
void chat_backend_delete_empty_sessions(void *backend);

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

static void onStreamStartedThunk(void *ctx, const char *session_id) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString sessionId = QString::fromUtf8(session_id ? session_id : "");
    QMetaObject::invokeMethod(self, [self]() {
        self->refreshSnapshot();
    }, Qt::QueuedConnection);
    QMetaObject::invokeMethod(self, [self, sessionId]() {
        emit self->streamStarted(sessionId);
    }, Qt::QueuedConnection);
}

static void onStreamChunkThunk(void *ctx, const char *session_id, const char *text) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString sessionId = QString::fromUtf8(session_id ? session_id : "");
    const QString chunk = QString::fromUtf8(text ? text : "");
    QMetaObject::invokeMethod(self, [self, sessionId, chunk]() {
        emit self->streamChunk(sessionId, chunk);
    }, Qt::QueuedConnection);
}

static void onStreamFinishedThunk(void *ctx, const char *session_id) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString sessionId = QString::fromUtf8(session_id ? session_id : "");
    QMetaObject::invokeMethod(self, [self, sessionId]() {
        self->handleStreamFinishedState();
        emit self->streamFinished(sessionId);
    }, Qt::QueuedConnection);
}

static void onStreamErrorThunk(void *ctx, const char *session_id, const char *message) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString sessionId = QString::fromUtf8(session_id ? session_id : "");
    const QString error = QString::fromUtf8(message ? message : "Unknown error");
    QMetaObject::invokeMethod(self, [self, sessionId, error]() {
        self->handleStreamErrorState();
        emit self->streamError(sessionId, error);
    }, Qt::QueuedConnection);
}

static QVariantList parseJsonListFromUtf8(const QString &jsonText) {
    const QJsonDocument doc = QJsonDocument::fromJson(jsonText.toUtf8());
    QVariantList output;
    if (!doc.isArray()) {
        return output;
    }
    const QJsonArray array = doc.array();
    for (const QJsonValue &value : array) {
        output.append(value.toObject().toVariantMap());
    }
    return output;
}

static void onSessionsUpdatedThunk(void *ctx, const char *json) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString payload = QString::fromUtf8(json ? json : "[]");
    QMetaObject::invokeMethod(self, [self, payload]() {
        self->applySessionsPayload(payload);
    }, Qt::QueuedConnection);
}

static void onModelsUpdatedThunk(void *ctx, const char *json) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString payload = QString::fromUtf8(json ? json : "[]");
    QMetaObject::invokeMethod(self, [self, payload]() {
        self->applyModelsPayload(payload);
    }, Qt::QueuedConnection);
}

static void onMessagesUpdatedThunk(void *ctx, const char *json) {
    auto *self = static_cast<ChatController *>(ctx);
    const QString payload = QString::fromUtf8(json ? json : "[]");
    QMetaObject::invokeMethod(self, [self, payload]() {
        self->applyMessagesPayload(payload);
    }, Qt::QueuedConnection);
}

ChatController::ChatController(QObject *parent)
    : QObject(parent) {
    BackendCallbacks callbacks{};
    callbacks.on_stream_started = &onStreamStartedThunk;
    callbacks.on_stream_chunk = &onStreamChunkThunk;
    callbacks.on_stream_finished = &onStreamFinishedThunk;
    callbacks.on_stream_error = &onStreamErrorThunk;
    callbacks.on_sessions_updated = &onSessionsUpdatedThunk;
    callbacks.on_models_updated = &onModelsUpdatedThunk;
    callbacks.on_messages_updated = &onMessagesUpdatedThunk;

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
QString ChatController::selectedSessionId() const { return m_selectedSessionId; }
QString ChatController::streamingSessionId() const { return m_streamingSessionId; }
QString ChatController::selectedProjectRoot() const { return m_selectedProjectRoot; }
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
    requestSessions();
    requestModels();
}

void ChatController::handleStreamFinishedState() {
    refreshSnapshot();
    requestSessions();
    if (!m_streamingSessionId.isEmpty()) {
        m_streamingSessionId.clear();
        emit streamingSessionIdChanged();
    }
}

void ChatController::handleStreamErrorState() {
    refreshSnapshot();
    if (!m_streamingSessionId.isEmpty()) {
        m_streamingSessionId.clear();
        emit streamingSessionIdChanged();
    }
}

void ChatController::sendPrompt(const QString &text) {
    sendPromptForSession(m_selectedSessionId, text);
}

void ChatController::sendPromptForSession(const QString &sessionId, const QString &text) {
    if (!m_backend) {
        return;
    }
    if (sessionId.isEmpty()) {
        return;
    }
    if (m_selectedSessionId != sessionId) {
        m_selectedSessionId = sessionId;
        emit selectedSessionIdChanged();
    }
    if (m_streamingSessionId != sessionId) {
        m_streamingSessionId = sessionId;
        emit streamingSessionIdChanged();
    }

    const QByteArray sessionEncoded = sessionId.toUtf8();
    const QByteArray encoded = text.toUtf8();
    chat_backend_send_prompt_for_session(m_backend, sessionEncoded.constData(), encoded.constData());
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
    requestSessions();
}

void ChatController::selectSession(const QString &id) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = id.toUtf8();
    chat_backend_select_session(m_backend, encoded.constData());
    chat_backend_request_messages(m_backend, encoded.constData());
    if (m_selectedSessionId != id) {
        m_selectedSessionId = id;
        emit selectedSessionIdChanged();
    }
    QString projectRoot;
    for (const QVariant &rowValue : std::as_const(m_sessions)) {
        const QVariantMap row = rowValue.toMap();
        const QString rowId = row.value("sessionId").toString().isEmpty()
            ? row.value("id").toString()
            : row.value("sessionId").toString();
        if (rowId == id) {
            projectRoot = row.value("projectRoot").toString();
            break;
        }
    }
    if (m_selectedProjectRoot != projectRoot) {
        m_selectedProjectRoot = projectRoot;
        emit selectedProjectRootChanged();
    }
    refreshSnapshot();
}

void ChatController::selectSessionAtIndex(int index) {
    if (index < 0 || index >= m_sessions.size()) {
        return;
    }

    const QVariantMap row = m_sessions.at(index).toMap();
    const QString sessionId = row.value("sessionId").toString().isEmpty()
        ? row.value("id").toString()
        : row.value("sessionId").toString();
    if (sessionId.isEmpty()) {
        return;
    }

    selectSession(sessionId);
}

void ChatController::selectModel(const QString &name) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = name.toUtf8();
    chat_backend_select_model(m_backend, encoded.constData());
    refreshSnapshot();
}

void ChatController::deleteEmptySessions() {
    if (!m_backend) return;
    chat_backend_delete_empty_sessions(m_backend);
    requestSessions();
    refreshSnapshot();
}

void ChatController::setSessionProjectRoot(const QString &sessionId, const QString &projectRoot) {
    if (!m_backend || sessionId.isEmpty()) {
        return;
    }

    const QByteArray sessionEncoded = sessionId.toUtf8();
    const QByteArray rootEncoded = projectRoot.toUtf8();
    chat_backend_set_session_project_root(
        m_backend,
        sessionEncoded.constData(),
        rootEncoded.constData()
    );

    if (m_selectedSessionId == sessionId && m_selectedProjectRoot != projectRoot) {
        m_selectedProjectRoot = projectRoot;
        emit selectedProjectRootChanged();
    }
}

void ChatController::setSelectedSessionProjectRoot(const QString &projectRoot) {
    if (m_selectedSessionId.isEmpty()) {
        return;
    }
    setSessionProjectRoot(m_selectedSessionId, projectRoot);
}


void ChatController::requestSessions() {
    if (!m_backend) {
        return;
    }
    chat_backend_request_sessions(m_backend);
}

void ChatController::requestModels() {
    if (!m_backend) {
        return;
    }
    chat_backend_request_models(m_backend);
}

void ChatController::requestMessages(const QString &sessionId) {
    if (!m_backend) {
        return;
    }
    const QByteArray encoded = sessionId.toUtf8();
    chat_backend_request_messages(m_backend, encoded.constData());
}

void ChatController::applySessionsPayload(const QString &json) {
    m_sessions = parseJsonListFromUtf8(json);
    QString selectedRoot;
    if (!m_selectedSessionId.isEmpty()) {
        for (const QVariant &rowValue : std::as_const(m_sessions)) {
            const QVariantMap row = rowValue.toMap();
            const QString rowId = row.value("sessionId").toString().isEmpty()
                ? row.value("id").toString()
                : row.value("sessionId").toString();
            if (rowId == m_selectedSessionId) {
                selectedRoot = row.value("projectRoot").toString();
                break;
            }
        }
    }
    if (m_selectedProjectRoot != selectedRoot) {
        m_selectedProjectRoot = selectedRoot;
        emit selectedProjectRootChanged();
    }
    emit sessionsChanged();
}

void ChatController::applyModelsPayload(const QString &json) {
    m_models = parseJsonListFromUtf8(json);
    emit modelsChanged();
}

void ChatController::applyMessagesPayload(const QString &json) {
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8());
    if (doc.isObject()) {
        const QJsonObject root = doc.object();
        const QString sessionId = root.value("sessionId").toString();
        if (!sessionId.isEmpty() && sessionId != m_selectedSessionId) {
            return;
        }

        QVariantList messages;
        const QJsonValue rawMessages = root.value("messages");
        if (rawMessages.isArray()) {
            const QJsonArray array = rawMessages.toArray();
            for (const QJsonValue &value : array) {
                messages.append(value.toObject().toVariantMap());
            }
        }
        emit messagesHydrated(messages);
        return;
    }

    const QVariantList messages = parseJsonListFromUtf8(json);
    emit messagesHydrated(messages);
}
