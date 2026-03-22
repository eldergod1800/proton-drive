use pdrive_core::drive::{DriveClient, DriveEntry};

#[tokio::test]
async fn test_login_returns_client() {
    // Stub always succeeds
    let result = DriveClient::login("user@proton.me", "password").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_returns_entries() {
    let client = DriveClient::login("user@proton.me", "pass").await.unwrap();
    let entries = client.list("/").await.unwrap();
    assert!(!entries.is_empty());
    // First entry should be a directory (as per stub)
    assert!(entries[0].is_dir);
}

#[tokio::test]
async fn test_upload_succeeds() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, b"hello").unwrap();

    let client = DriveClient::login("user@proton.me", "pass").await.unwrap();
    let result = client.upload(&file, "/My Files/test.txt").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_session_token() {
    let client = DriveClient::login("user@proton.me", "pass").await.unwrap();
    // Stub returns Some(token)
    assert!(client.session_token().is_some());
}
