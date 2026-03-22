use pdrive_core::sync::{SyncEngine, SyncEvent};
use pdrive_core::config::{SyncPair, SyncDirection};
use tempfile::tempdir;
use std::fs;

#[tokio::test]
async fn test_new_local_file_emits_upload_event() {
    let dir = tempdir().unwrap();
    let local = dir.path().join("watched");
    fs::create_dir_all(&local).unwrap();

    let pair = SyncPair {
        local: local.to_str().unwrap().to_string(),
        remote: "/My Files/Test".to_string(),
        direction: SyncDirection::Bidirectional,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let mut engine = SyncEngine::new(vec![pair], tx);
    engine.start().await.unwrap();

    // Give the watcher time to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create a file in the watched dir
    fs::write(local.join("new_file.txt"), b"hello").unwrap();

    // Wait for the event
    let event = tokio::time::timeout(
        tokio::time::Duration::from_secs(2),
        rx.recv()
    ).await;

    assert!(event.is_ok(), "timed out waiting for sync event");
    let event = event.unwrap();
    assert!(event.is_some());
    match event.unwrap() {
        SyncEvent::LocalChanged { local_path, .. } => {
            assert!(local_path.ends_with("new_file.txt"));
        }
        other => panic!("expected LocalChanged, got {:?}", other),
    }
}

#[tokio::test]
async fn test_engine_with_empty_pairs_starts_cleanly() {
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let mut engine = SyncEngine::new(vec![], tx);
    let result = engine.start().await;
    assert!(result.is_ok());
}
