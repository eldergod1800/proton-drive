use pdrive_core::auth::TokenStore;
use tempfile::tempdir;

#[tokio::test]
async fn test_token_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.age");
    let store = TokenStore::new(path.clone());
    store.save("my-secret-token").await.unwrap();
    let loaded = store.load().await.unwrap();
    assert_eq!(loaded, Some("my-secret-token".to_string()));
}

#[tokio::test]
async fn test_missing_token_returns_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.age");
    let store = TokenStore::new(path);
    let loaded = store.load().await.unwrap();
    assert_eq!(loaded, None);
}

#[tokio::test]
async fn test_clear_token() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.age");
    let store = TokenStore::new(path.clone());
    store.save("token").await.unwrap();
    store.clear().unwrap();
    assert!(!path.exists());
}
