#pragma once

#include <QObject>
#include <QVariantList>

class ChatController : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString activeProvider READ activeProvider NOTIFY activeProviderChanged)
    Q_PROPERTY(QString activeModel READ activeModel NOTIFY activeModelChanged)
    Q_PROPERTY(QString providerScope READ providerScope NOTIFY providerScopeChanged)
    Q_PROPERTY(QString providerHealth READ providerHealth NOTIFY providerHealthChanged)
    Q_PROPERTY(bool isLoading READ isLoading NOTIFY isLoadingChanged)
    Q_PROPERTY(QString selectedSessionId READ selectedSessionId NOTIFY selectedSessionIdChanged)
    Q_PROPERTY(QString streamingSessionId READ streamingSessionId NOTIFY streamingSessionIdChanged)
    Q_PROPERTY(QString selectedProjectRoot READ selectedProjectRoot NOTIFY selectedProjectRootChanged)
    Q_PROPERTY(QString startupNotice READ startupNotice NOTIFY startupNoticeChanged)
    Q_PROPERTY(QVariantList sessions READ sessions NOTIFY sessionsChanged)
    Q_PROPERTY(QVariantList availableModelsDetailed READ availableModelsDetailed NOTIFY modelsChanged)

public:
    explicit ChatController(QObject *parent = nullptr);
    ~ChatController() override;

    QString activeProvider() const;
    QString activeModel() const;
    QString providerScope() const;
    QString providerHealth() const;
    bool isLoading() const;
    QString selectedSessionId() const;
    QString streamingSessionId() const;
    QString selectedProjectRoot() const;
    QString startupNotice() const;
    QVariantList sessions() const;
    QVariantList availableModelsDetailed() const;

    Q_INVOKABLE void sendPrompt(const QString &text);
    Q_INVOKABLE void sendPromptForSession(const QString &sessionId, const QString &text);
    Q_INVOKABLE void stopGeneration();
    Q_INVOKABLE void newSession();
    Q_INVOKABLE void selectSession(const QString &id);
    Q_INVOKABLE void selectSessionAtIndex(int index);
    Q_INVOKABLE void selectModel(const QString &name, const QString &serverUrl = QString());
    Q_INVOKABLE void requestSessions();
    Q_INVOKABLE void requestModels();
    Q_INVOKABLE void requestMessages(const QString &sessionId);
    Q_INVOKABLE void deleteEmptySessions();
    Q_INVOKABLE void deleteSession(const QString &sessionId);
    Q_INVOKABLE void setSessionProjectRoot(const QString &sessionId, const QString &projectRoot);
    Q_INVOKABLE void setSelectedSessionProjectRoot(const QString &projectRoot);
    Q_INVOKABLE void copyText(const QString &text);
    Q_INVOKABLE void clearStartupNotice();
    Q_INVOKABLE bool setMissingOllamaNoticeSuppressed(bool suppressed);
    Q_INVOKABLE QString serversConfigJson();
    Q_INVOKABLE bool saveServersConfigJson(const QString &json);
    Q_INVOKABLE QString testServerConnection(const QString &baseUrl);

    void refreshSnapshot();
    void refreshCollections();
    void handleStreamFinishedState();
    void handleStreamErrorState();
    void applySessionsPayload(const QString &json);
    void applyModelsPayload(const QString &json);
    void applyMessagesPayload(const QString &json);

signals:
    void streamStarted(const QString &sessionId);
    void streamChunk(const QString &sessionId, const QString &text);
    void streamFinished(const QString &sessionId);
    void streamError(const QString &sessionId, const QString &message);
    void tokenUsageUpdated(const QString &sessionId, int promptTokens, int completionTokens, int totalTokens, bool isEstimated);

    void activeProviderChanged();
    void activeModelChanged();
    void providerScopeChanged();
    void providerHealthChanged();
    void isLoadingChanged();
    void selectedSessionIdChanged();
    void streamingSessionIdChanged();
    void selectedProjectRootChanged();
    void startupNoticeChanged();
    void sessionsChanged();
    void modelsChanged();
    void messagesHydrated(const QVariantList &messages);

private:
    QString projectRootForSession(const QString &sessionId) const;

    void *m_backend = nullptr;
    QString m_activeProvider;
    QString m_activeModel;
    QString m_providerScope;
    QString m_providerHealth;
    bool m_isLoading = false;
    QString m_selectedSessionId;
    QString m_streamingSessionId;
    QString m_selectedProjectRoot;
    QString m_startupNotice;
    QVariantList m_sessions;
    QVariantList m_models;
};
