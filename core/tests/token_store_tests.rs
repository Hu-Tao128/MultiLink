use std::path::PathBuf;

use multilink_core::{StoredToken, TokenStore};

#[tokio::test]
async fn token_store_roundtrip() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = PathBuf::from(temp.path()).join("multilink");
    let store = TokenStore::for_path(root);

    let token = StoredToken {
        access_token: "access-123".to_string(),
        refresh_token: Some("refresh-456".to_string()),
        expires_at: Some(2_000_000_000),
        token_type: Some("Bearer".to_string()),
    };

    store.save("gemini", &token).await.expect("save token");
    let loaded = store
        .load("gemini")
        .await
        .expect("load token")
        .expect("token exists");

    assert_eq!(loaded.access_token, "access-123");
    assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-456"));
}
