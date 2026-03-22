use pdrive_core::db::{SyncDb, SyncEntry, SyncStatus};
use tempfile::tempdir;

#[test]
fn test_insert_and_query_entry() {
    let dir = tempdir().unwrap();
    let db = SyncDb::open(dir.path().join("sync.db")).unwrap();

    db.upsert(&SyncEntry {
        local_path: "/home/user/doc.txt".into(),
        remote_id: "abc123".into(),
        status: SyncStatus::Synced,
        modified_at: 1700000000,
    }).unwrap();

    let entry = db.get("/home/user/doc.txt").unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.remote_id, "abc123");
    assert_eq!(entry.status, SyncStatus::Synced);
    assert_eq!(entry.modified_at, 1700000000);
}

#[test]
fn test_pending_uploads() {
    let dir = tempdir().unwrap();
    let db = SyncDb::open(dir.path().join("sync.db")).unwrap();

    db.upsert(&SyncEntry {
        local_path: "/home/user/pending.txt".into(),
        remote_id: "".into(),
        status: SyncStatus::PendingUpload,
        modified_at: 1700000001,
    }).unwrap();
    db.upsert(&SyncEntry {
        local_path: "/home/user/synced.txt".into(),
        remote_id: "xyz".into(),
        status: SyncStatus::Synced,
        modified_at: 1700000002,
    }).unwrap();

    let pending = db.pending_uploads().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].local_path, "/home/user/pending.txt");
}

#[test]
fn test_upsert_updates_existing() {
    let dir = tempdir().unwrap();
    let db = SyncDb::open(dir.path().join("sync.db")).unwrap();

    db.upsert(&SyncEntry {
        local_path: "/home/user/file.txt".into(),
        remote_id: "old-id".into(),
        status: SyncStatus::PendingUpload,
        modified_at: 100,
    }).unwrap();

    db.upsert(&SyncEntry {
        local_path: "/home/user/file.txt".into(),
        remote_id: "new-id".into(),
        status: SyncStatus::Synced,
        modified_at: 200,
    }).unwrap();

    let entry = db.get("/home/user/file.txt").unwrap().unwrap();
    assert_eq!(entry.remote_id, "new-id");
    assert_eq!(entry.status, SyncStatus::Synced);
    assert_eq!(entry.modified_at, 200);
}

#[test]
fn test_missing_entry_returns_none() {
    let dir = tempdir().unwrap();
    let db = SyncDb::open(dir.path().join("sync.db")).unwrap();
    assert!(db.get("/nonexistent").unwrap().is_none());
}
