use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::token_store::StoredToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthProvider {
    Gemini,
    Codex,
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub authorize_url: String,
    pub token_url: String,
    pub revoke_url: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_port: u16,
}

#[derive(Debug, Clone)]
pub struct CallbackPayload {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
struct OAuthTokenPayload {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    token_type: Option<String>,
}

pub struct OAuthFlow {
    client: Client,
    configs: HashMap<AuthProvider, OAuthConfig>,
}

impl Default for OAuthFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthFlow {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            configs: HashMap::new(),
        }
    }

    pub fn register_provider(&mut self, provider: AuthProvider, config: OAuthConfig) {
        self.configs.insert(provider, config);
    }

    pub fn redirect_uri_for(&self, provider: AuthProvider) -> Result<String, OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;
        Ok(format!("http://127.0.0.1:{}/callback", cfg.redirect_port))
    }

    pub fn authorization_url(
        &self,
        provider: AuthProvider,
        state: &str,
    ) -> Result<String, OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;
        let redirect = self.redirect_uri_for(provider)?;
        let scopes = cfg.scopes.join(" ");

        Ok(format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&access_type=offline&prompt=consent",
            cfg.authorize_url,
            url_encode(&cfg.client_id),
            url_encode(&redirect),
            url_encode(&scopes),
            url_encode(state)
        ))
    }

    pub async fn listen_callback(
        &self,
        provider: AuthProvider,
        timeout: Duration,
    ) -> Result<CallbackPayload, OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;
        let bind_addr = format!("127.0.0.1:{}", cfg.redirect_port);
        let listener = TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| OAuthError::Io(e.to_string()))?;

        let accept_future = async {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|e| OAuthError::Io(e.to_string()))?;

            let mut buffer = [0u8; 4096];
            let read_len = socket
                .read(&mut buffer)
                .await
                .map_err(|e| OAuthError::Io(e.to_string()))?;
            let request = String::from_utf8_lossy(&buffer[..read_len]).to_string();
            let first_line = request
                .lines()
                .next()
                .ok_or_else(|| OAuthError::InvalidCallback("missing request line".to_string()))?;

            let path = first_line
                .split_whitespace()
                .nth(1)
                .ok_or_else(|| OAuthError::InvalidCallback("missing request path".to_string()))?;

            let payload = parse_callback_path(path)?;
            let response = "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\n\r\nLogin complete. You can close this tab.";
            socket
                .write_all(response.as_bytes())
                .await
                .map_err(|e| OAuthError::Io(e.to_string()))?;
            Ok::<CallbackPayload, OAuthError>(payload)
        };

        tokio::time::timeout(timeout, accept_future)
            .await
            .map_err(|_| OAuthError::Timeout)?
    }

    pub async fn exchange_code(
        &self,
        provider: AuthProvider,
        code: &str,
        redirect_uri: &str,
    ) -> Result<StoredToken, OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;

        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", cfg.client_id.clone()),
            ("redirect_uri", redirect_uri.to_string()),
        ];
        if let Some(secret) = &cfg.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let response = self
            .client
            .post(&cfg.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OAuthError::TokenExchangeFailed(
                response.status().to_string(),
            ));
        }

        let token_payload = response
            .json::<OAuthTokenPayload>()
            .await
            .map_err(|e| OAuthError::Parse(e.to_string()))?;

        Ok(StoredToken {
            access_token: token_payload.access_token,
            refresh_token: token_payload.refresh_token,
            expires_at: token_payload
                .expires_in
                .map(|seconds| unix_now().saturating_add(seconds)),
            token_type: token_payload.token_type,
        })
    }

    pub async fn refresh_token(
        &self,
        provider: AuthProvider,
        refresh_token: &str,
    ) -> Result<StoredToken, OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;

        let mut params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
            ("client_id", cfg.client_id.clone()),
        ];
        if let Some(secret) = &cfg.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let response = self
            .client
            .post(&cfg.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OAuthError::RefreshFailed(response.status().to_string()));
        }

        let token_payload = response
            .json::<OAuthTokenPayload>()
            .await
            .map_err(|e| OAuthError::Parse(e.to_string()))?;

        Ok(StoredToken {
            access_token: token_payload.access_token,
            refresh_token: token_payload
                .refresh_token
                .or(Some(refresh_token.to_string())),
            expires_at: token_payload
                .expires_in
                .map(|seconds| unix_now().saturating_add(seconds)),
            token_type: token_payload.token_type,
        })
    }

    pub async fn revoke_token(
        &self,
        provider: AuthProvider,
        token: &str,
    ) -> Result<(), OAuthError> {
        let cfg = self
            .configs
            .get(&provider)
            .ok_or(OAuthError::ProviderNotRegistered)?;
        let revoke_url = cfg
            .revoke_url
            .as_ref()
            .ok_or(OAuthError::RevokeUnsupported)?;

        let response = self
            .client
            .post(revoke_url)
            .form(&[("token", token.to_string())])
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OAuthError::RevokeFailed(response.status().to_string()));
        }

        Ok(())
    }
}

fn parse_callback_path(path: &str) -> Result<CallbackPayload, OAuthError> {
    let mut parts = path.splitn(2, '?');
    let route = parts.next().unwrap_or_default();
    let query = parts
        .next()
        .ok_or_else(|| OAuthError::InvalidCallback("missing query string".to_string()))?;
    if route != "/callback" {
        return Err(OAuthError::InvalidCallback(
            "unexpected callback route".to_string(),
        ));
    }

    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let mut tokens = pair.splitn(2, '=');
        let key = tokens.next().unwrap_or_default();
        let value = tokens.next().unwrap_or_default();
        match key {
            "code" => code = Some(url_decode(value)?),
            "state" => state = Some(url_decode(value)?),
            "error" => return Err(OAuthError::InvalidCallback(url_decode(value)?)),
            _ => {}
        }
    }

    let code = code.ok_or_else(|| OAuthError::InvalidCallback("code not found".to_string()))?;
    let state = state.ok_or_else(|| OAuthError::InvalidCallback("state not found".to_string()))?;
    Ok(CallbackPayload { code, state })
}

fn url_encode(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{:02X}", byte).chars().collect::<Vec<char>>(),
        })
        .collect()
}

fn url_decode(input: &str) -> Result<String, OAuthError> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(OAuthError::InvalidCallback(
                        "invalid percent encoding".to_string(),
                    ));
                }
                let hex = &input[index + 1..index + 3];
                let value = u8::from_str_radix(hex, 16)
                    .map_err(|_| OAuthError::InvalidCallback("invalid hex value".to_string()))?;
                output.push(value);
                index += 3;
            }
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            value => {
                output.push(value);
                index += 1;
            }
        }
    }

    String::from_utf8(output).map_err(|e| OAuthError::InvalidCallback(e.to_string()))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("oauth provider not registered")]
    ProviderNotRegistered,
    #[error("oauth callback timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("refresh failed: {0}")]
    RefreshFailed(String),
    #[error("token revoke not supported by provider")]
    RevokeUnsupported,
    #[error("token revoke failed: {0}")]
    RevokeFailed(String),
    #[error("invalid callback: {0}")]
    InvalidCallback(String),
}
