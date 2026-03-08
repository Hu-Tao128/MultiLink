use std::time::Duration;

use super::{AuthProvider, OAuthError, OAuthFlow, StoredToken, TokenStore, TokenStoreError};

pub struct AuthService {
    pub oauth: OAuthFlow,
    pub store: TokenStore,
}

impl AuthService {
    pub fn new(oauth: OAuthFlow, store: TokenStore) -> Self {
        Self { oauth, store }
    }

    pub fn provider_key(provider: AuthProvider) -> &'static str {
        match provider {
            AuthProvider::Gemini => "gemini",
            AuthProvider::Codex => "codex",
        }
    }

    pub async fn begin_login_url(
        &self,
        provider: AuthProvider,
        state: &str,
    ) -> Result<String, AuthServiceError> {
        self.oauth
            .authorization_url(provider, state)
            .map_err(AuthServiceError::OAuth)
    }

    pub async fn complete_login(
        &self,
        provider: AuthProvider,
        expected_state: &str,
    ) -> Result<StoredToken, AuthServiceError> {
        let callback = self
            .oauth
            .listen_callback(provider, Duration::from_secs(180))
            .await
            .map_err(AuthServiceError::OAuth)?;

        if callback.state != expected_state {
            return Err(AuthServiceError::InvalidState);
        }

        let redirect_uri = self
            .oauth
            .redirect_uri_for(provider)
            .map_err(AuthServiceError::OAuth)?;
        let token = self
            .oauth
            .exchange_code(provider, &callback.code, &redirect_uri)
            .await
            .map_err(AuthServiceError::OAuth)?;

        self.store
            .save(Self::provider_key(provider), &token)
            .await
            .map_err(AuthServiceError::Store)?;
        Ok(token)
    }

    pub async fn get_valid_access_token(
        &self,
        provider: AuthProvider,
    ) -> Result<Option<String>, AuthServiceError> {
        let key = Self::provider_key(provider);
        let current = self
            .store
            .load(key)
            .await
            .map_err(AuthServiceError::Store)?;
        let Some(token) = current else {
            return Ok(None);
        };

        if !token.is_expired(30) {
            return Ok(Some(token.access_token));
        }

        let Some(refresh_token) = token.refresh_token.as_deref() else {
            return Ok(None);
        };

        let refreshed = self
            .oauth
            .refresh_token(provider, refresh_token)
            .await
            .map_err(AuthServiceError::OAuth)?;

        self.store
            .save(key, &refreshed)
            .await
            .map_err(AuthServiceError::Store)?;

        Ok(Some(refreshed.access_token))
    }

    pub async fn logout(&self, provider: AuthProvider) -> Result<(), AuthServiceError> {
        let key = Self::provider_key(provider);
        if let Some(token) = self
            .store
            .load(key)
            .await
            .map_err(AuthServiceError::Store)?
        {
            let _ = self.oauth.revoke_token(provider, &token.access_token).await;
        }
        self.store.clear(key).await.map_err(AuthServiceError::Store)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthServiceError {
    #[error("oauth flow error: {0}")]
    OAuth(#[from] OAuthError),
    #[error("token store error: {0}")]
    Store(#[from] TokenStoreError),
    #[error("oauth state does not match")]
    InvalidState,
}
