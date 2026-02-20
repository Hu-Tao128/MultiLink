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
    QVariantList sessions() const;
    QVariantList availableModelsDetailed() const;

    Q_INVOKABLE void sendPrompt(const QString &text);
    Q_INVOKABLE void stopGeneration();
    Q_INVOKABLE void newSession();
    Q_INVOKABLE void selectSession(const QString &id);
    Q_INVOKABLE void selectModel(const QString &name);

    void refreshSnapshot();
    void refreshCollections();

signals:
    void streamStarted();
    void streamChunk(const QString &text);
    void streamFinished();
    void streamError(const QString &message);

    void activeProviderChanged();
    void activeModelChanged();
    void providerScopeChanged();
    void providerHealthChanged();
    void isLoadingChanged();
    void sessionsChanged();
    void modelsChanged();

private:
    void *m_backend = nullptr;
    QString m_activeProvider;
    QString m_activeModel;
    QString m_providerScope;
    QString m_providerHealth;
    bool m_isLoading = false;
    QVariantList m_sessions;
    QVariantList m_models;
};
