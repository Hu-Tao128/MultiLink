use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub token_type: Option<String>,
}

impl StoredToken {
    pub fn is_expired(&self, skew_secs: u64) -> bool {
        match self.expires_at {
            Some(exp) => {
                let now = unix_now();
                now.saturating_add(skew_secs) >= exp
            }
            None => false,
        }
    }
}

pub struct TokenStore {
    root: PathBuf,
}

impl TokenStore {
    pub fn new(app_name: &str) -> Self {
        let root = platform_config_dir(app_name);
        Self { root }
    }

    pub fn root_path(&self) -> &Path {
        &self.root
    }

    pub fn for_path(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn save(&self, provider: &str, token: &StoredToken) -> Result<(), TokenStoreError> {
        fs::create_dir_all(&self.root).await?;
        restrict_permissions(&self.root)?;

        let key_path = self.root.join("master.key");
        let key = self.load_or_create_key(&key_path).await?;

        let raw = serde_json::to_vec(token)?;
        let encrypted = encrypt(&key, &raw)?;

        let token_path = self.root.join(format!("{}.token", provider));
        fs::write(token_path, encrypted).await?;
        restrict_permissions(&self.root.join(format!("{}.token", provider)))?;
        Ok(())
    }

    pub async fn load(&self, provider: &str) -> Result<Option<StoredToken>, TokenStoreError> {
        let token_path = self.root.join(format!("{}.token", provider));
        if !token_path.exists() {
            return Ok(None);
        }

        let key_path = self.root.join("master.key");
        if !key_path.exists() {
            return Ok(None);
        }

        let key = fs::read(key_path).await?;
        let encrypted = fs::read(token_path).await?;
        let raw = decrypt(&key, &encrypted)?;
        let token = serde_json::from_slice::<StoredToken>(&raw)?;
        Ok(Some(token))
    }

    pub async fn clear(&self, provider: &str) -> Result<(), TokenStoreError> {
        let token_path = self.root.join(format!("{}.token", provider));
        if token_path.exists() {
            fs::remove_file(token_path).await?;
        }
        Ok(())
    }

    async fn load_or_create_key(&self, key_path: &Path) -> Result<Vec<u8>, TokenStoreError> {
        if key_path.exists() {
            return Ok(fs::read(key_path).await?);
        }

        let mut key = vec![0u8; 32];
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| TokenStoreError::Crypto("failed to generate key".to_string()))?;

        fs::write(key_path, &key).await?;
        restrict_permissions(key_path)?;
        Ok(key)
    }
}

fn platform_config_dir(app_name: &str) -> PathBuf {
    if let Some(path) = dirs::config_dir() {
        return path.join(app_name);
    }
    PathBuf::from(format!("./{}", app_name))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
    } else {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn encrypt(key_bytes: &[u8], raw: &[u8]) -> Result<Vec<u8>, TokenStoreError> {
    let unbound = UnboundKey::new(&AES_256_GCM, key_bytes)
        .map_err(|_| TokenStoreError::Crypto("invalid key".to_string()))?;
    let key = LessSafeKey::new(unbound);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| TokenStoreError::Crypto("failed to generate nonce".to_string()))?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = raw.to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| TokenStoreError::Crypto("encryption failed".to_string()))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    Ok(BASE64.encode(result).into_bytes())
}

fn decrypt(key_bytes: &[u8], encoded: &[u8]) -> Result<Vec<u8>, TokenStoreError> {
    let encrypted = BASE64.decode(encoded)?;
    if encrypted.len() <= NONCE_LEN {
        return Err(TokenStoreError::Crypto("invalid payload".to_string()));
    }

    let (nonce_bytes, cipher_text) = encrypted.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| TokenStoreError::Crypto("invalid nonce".to_string()))?;

    let unbound = UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .map_err(|_| TokenStoreError::Crypto("invalid key".to_string()))?;
    let key = LessSafeKey::new(unbound);

    let mut in_out = cipher_text.to_vec();
    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| TokenStoreError::Crypto("decryption failed".to_string()))?;

    Ok(plain.to_vec())
}

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("crypto error: {0}")]
    Crypto(String),
}
