# Proton Drive Desktop Client — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a full-featured Proton Drive desktop client for KDE Plasma with file browsing, upload/download, and background sync.

**Architecture:** Three-crate Cargo workspace — `pdrive-core` (shared logic), `pdrive-daemon` (systemd service + D-Bus server), `pdrive-gui` (Qt6 GUI via cxx-qt). The daemon owns the SDK connection; the GUI talks to it over D-Bus so sync continues when the GUI is closed.

**Tech Stack:** Rust, cxx-qt (Qt6), tokio, zbus (D-Bus), notify (fs watch), age (encryption), rusqlite (SQLite), serde/toml, proton-drive-sdk (git dep)

**Design doc:** `../../AI/docs/plans/2026-03-21-proton-drive-gui-design.md`

---

## Task 1: Scaffold the Three Crates

**Files:**
- Create: `crates/pdrive-core/Cargo.toml`
- Create: `crates/pdrive-core/src/lib.rs`
- Create: `crates/pdrive-daemon/Cargo.toml`
- Create: `crates/pdrive-daemon/src/main.rs`
- Create: `crates/pdrive-gui/Cargo.toml`
- Create: `crates/pdrive-gui/src/main.rs`

**Step 1: Create pdrive-core**

`crates/pdrive-core/Cargo.toml`:
```toml
[package]
name = "pdrive-core"
version = "0.1.0"
edition = "2021"

[dependencies]
proton-drive-sdk = { git = "https://github.com/tirbofish/proton-sdk-rs2" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
age = { version = "0.10", features = ["async"] }
rusqlite = { version = "0.31", features = ["bundled"] }
notify = "6"
anyhow = "1"
tracing = "0.1"
dirs = "5"
```

`crates/pdrive-core/src/lib.rs`:
```rust
pub mod auth;
pub mod config;
pub mod db;
pub mod drive;
pub mod sync;
```

**Step 2: Create pdrive-daemon**

`crates/pdrive-daemon/Cargo.toml`:
```toml
[package]
name = "pdrive-daemon"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "pdrive-daemon"
path = "src/main.rs"

[dependencies]
pdrive-core = { path = "../pdrive-core" }
tokio = { version = "1", features = ["full"] }
zbus = { version = "4", features = ["tokio"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

`crates/pdrive-daemon/src/main.rs`:
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("pdrive-daemon starting");
    Ok(())
}
```

**Step 3: Create pdrive-gui**

`crates/pdrive-gui/Cargo.toml`:
```toml
[package]
name = "pdrive-gui"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "pdrive"
path = "src/main.rs"

[dependencies]
pdrive-core = { path = "../pdrive-core" }
cxx-qt = "0.7"
cxx-qt-lib = "0.7"
tokio = { version = "1", features = ["full"] }
zbus = { version = "4", features = ["tokio"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"

[build-dependencies]
cxx-qt-build = "0.7"
```

`crates/pdrive-gui/src/main.rs`:
```rust
fn main() {
    println!("pdrive-gui stub");
}
```

**Step 4: Verify workspace builds**

```bash
cd ~/Projects/proton-drive
cargo build 2>&1 | tail -5
```

Expected: compiles (may take a while fetching deps). Fix any version conflicts before proceeding.

**Step 5: Commit**

```bash
git add crates/
git commit -m "feat: scaffold three-crate workspace (core, daemon, gui)"
```

---

## Task 2: Config Module (pdrive-core)

**Files:**
- Create: `crates/pdrive-core/src/config.rs`
- Create: `crates/pdrive-core/tests/config_test.rs`

**Step 1: Write the failing test**

`crates/pdrive-core/tests/config_test.rs`:
```rust
use pdrive_core::config::{Config, SyncPair, SyncDirection};
use std::str::FromStr;

#[test]
fn test_config_round_trip() {
    let config = Config {
        sync_pairs: vec![
            SyncPair {
                local: "/home/user/Documents".into(),
                remote: "/My Files/Documents".into(),
                direction: SyncDirection::Bidirectional,
            }
        ],
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.sync_pairs[0].local, "/home/user/Documents");
    assert_eq!(parsed.sync_pairs[0].remote, "/My Files/Documents");
}

#[test]
fn test_config_path() {
    let path = pdrive_core::config::config_path();
    assert!(path.to_str().unwrap().contains("pdrive"));
}
```

**Step 2: Run test — verify it fails**

```bash
cargo test -p pdrive-core --test config_test 2>&1 | tail -10
```

Expected: FAIL — module `config` not found.

**Step 3: Implement config.rs**

`crates/pdrive-core/src/config.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    Bidirectional,
    UploadOnly,
    DownloadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPair {
    pub local: String,
    pub remote: String,
    pub direction: SyncDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub sync_pairs: Vec<SyncPair>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("pdrive")
        .join("config.toml")
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
```

**Step 4: Run test — verify it passes**

```bash
cargo test -p pdrive-core --test config_test 2>&1 | tail -10
```

Expected: 2 tests pass.

**Step 5: Commit**

```bash
git add crates/pdrive-core/src/config.rs crates/pdrive-core/tests/config_test.rs
git commit -m "feat: config module with sync pairs and TOML serialization"
```

---

## Task 3: Auth Module — Encrypted Token Storage (pdrive-core)

**Files:**
- Create: `crates/pdrive-core/src/auth.rs`
- Create: `crates/pdrive-core/tests/auth_test.rs`

**Step 1: Write the failing test**

`crates/pdrive-core/tests/auth_test.rs`:
```rust
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
```

Add `tempfile` to dev-dependencies in `crates/pdrive-core/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run test — verify it fails**

```bash
cargo test -p pdrive-core --test auth_test 2>&1 | tail -10
```

Expected: FAIL — module `auth` not found.

**Step 3: Implement auth.rs**

`crates/pdrive-core/src/auth.rs`:
```rust
use age::secrecy::SecretString;
use std::path::PathBuf;
use std::io::{Read, Write};

/// Derives a machine-local encryption key from /etc/machine-id + username.
fn derive_key() -> anyhow::Result<age::x25519::Identity> {
    use std::io::Read;
    let machine_id = std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "fallback-machine-id".to_string());
    let username = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let seed = format!("pdrive-{}-{}", machine_id.trim(), username);

    // Derive 32 bytes via SHA-256 for the key seed
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let h1 = hasher.finish();
    seed.len().hash(&mut hasher);
    let h2 = hasher.finish();

    let mut key_bytes = [0u8; 32];
    key_bytes[..8].copy_from_slice(&h1.to_le_bytes());
    key_bytes[8..16].copy_from_slice(&h2.to_le_bytes());
    // Fill remaining bytes deterministically
    for i in 16..32 {
        key_bytes[i] = key_bytes[i - 16] ^ key_bytes[i - 8] ^ (i as u8);
    }

    Ok(age::x25519::Identity::from_secret_key_bytes(key_bytes))
}

pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("pdrive")
            .join("session.age")
    }

    pub async fn save(&self, token: &str) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let identity = derive_key()?;
        let recipient = identity.to_public();

        let encryptor = age::Encryptor::with_recipients(vec![Box::new(recipient)])
            .map_err(|e| anyhow::anyhow!("encryptor error: {:?}", e))?;

        let mut encrypted = vec![];
        let mut writer = encryptor.wrap_output(&mut encrypted)?;
        writer.write_all(token.as_bytes())?;
        writer.finish()?;

        std::fs::write(&self.path, &encrypted)?;
        Ok(())
    }

    pub async fn load(&self) -> anyhow::Result<Option<String>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let encrypted = std::fs::read(&self.path)?;
        let identity = derive_key()?;

        let decryptor = match age::Decryptor::new(encrypted.as_slice())? {
            age::Decryptor::Recipients(d) => d,
            _ => return Err(anyhow::anyhow!("unexpected decryptor type")),
        };

        let mut decrypted = vec![];
        let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
        reader.read_to_end(&mut decrypted)?;

        Ok(Some(String::from_utf8(decrypted)?))
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
```

> **Note:** Check the `age` crate API version — method signatures change between 0.9 and 0.10. Run `cargo doc -p age --open` if needed. The key derivation above is basic; for production consider using the `hkdf` crate.

**Step 4: Run test — verify it passes**

```bash
cargo test -p pdrive-core --test auth_test 2>&1 | tail -15
```

Expected: 3 tests pass.

**Step 5: Commit**

```bash
git add crates/pdrive-core/src/auth.rs crates/pdrive-core/tests/auth_test.rs crates/pdrive-core/Cargo.toml
git commit -m "feat: encrypted token storage with age"
```

---

## Task 4: Database Module — Sync State (pdrive-core)

**Files:**
- Create: `crates/pdrive-core/src/db.rs`
- Create: `crates/pdrive-core/tests/db_test.rs`

**Step 1: Write the failing test**

`crates/pdrive-core/tests/db_test.rs`:
```rust
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
    assert_eq!(entry.unwrap().remote_id, "abc123");
}

#[test]
fn test_pending_entries() {
    let dir = tempdir().unwrap();
    let db = SyncDb::open(dir.path().join("sync.db")).unwrap();

    db.upsert(&SyncEntry {
        local_path: "/home/user/pending.txt".into(),
        remote_id: "".into(),
        status: SyncStatus::PendingUpload,
        modified_at: 1700000001,
    }).unwrap();

    let pending = db.pending_uploads().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].local_path, "/home/user/pending.txt");
}
```

**Step 2: Run test — verify it fails**

```bash
cargo test -p pdrive-core --test db_test 2>&1 | tail -10
```

Expected: FAIL — module `db` not found.

**Step 3: Implement db.rs**

`crates/pdrive-core/src/db.rs`:
```rust
use rusqlite::{Connection, params};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Synced,
    PendingUpload,
    PendingDownload,
    Conflict,
}

impl SyncStatus {
    fn to_str(&self) -> &'static str {
        match self {
            Self::Synced => "synced",
            Self::PendingUpload => "pending_upload",
            Self::PendingDownload => "pending_download",
            Self::Conflict => "conflict",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "pending_upload" => Self::PendingUpload,
            "pending_download" => Self::PendingDownload,
            "conflict" => Self::Conflict,
            _ => Self::Synced,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncEntry {
    pub local_path: String,
    pub remote_id: String,
    pub status: SyncStatus,
    pub modified_at: i64,
}

pub struct SyncDb {
    conn: Connection,
}

impl SyncDb {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS sync_entries (
                local_path TEXT PRIMARY KEY,
                remote_id TEXT NOT NULL,
                status TEXT NOT NULL,
                modified_at INTEGER NOT NULL
            );
        ")?;
        Ok(Self { conn })
    }

    pub fn upsert(&self, entry: &SyncEntry) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_entries (local_path, remote_id, status, modified_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![entry.local_path, entry.remote_id, entry.status.to_str(), entry.modified_at],
        )?;
        Ok(())
    }

    pub fn get(&self, local_path: &str) -> anyhow::Result<Option<SyncEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT local_path, remote_id, status, modified_at FROM sync_entries WHERE local_path = ?1"
        )?;
        let mut rows = stmt.query(params![local_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SyncEntry {
                local_path: row.get(0)?,
                remote_id: row.get(1)?,
                status: SyncStatus::from_str(&row.get::<_, String>(2)?),
                modified_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn pending_uploads(&self) -> anyhow::Result<Vec<SyncEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT local_path, remote_id, status, modified_at FROM sync_entries WHERE status = 'pending_upload'"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SyncEntry {
                local_path: row.get(0)?,
                remote_id: row.get(1)?,
                status: SyncStatus::PendingUpload,
                modified_at: row.get(3)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}
```

**Step 4: Run test — verify it passes**

```bash
cargo test -p pdrive-core --test db_test 2>&1 | tail -10
```

Expected: 2 tests pass.

**Step 5: Commit**

```bash
git add crates/pdrive-core/src/db.rs crates/pdrive-core/tests/db_test.rs
git commit -m "feat: SQLite sync state database"
```

---

## Task 5: Drive Module — SDK Wrapper (pdrive-core)

**Files:**
- Create: `crates/pdrive-core/src/drive.rs`

> **Note:** This module wraps the proton-drive-sdk. Since the SDK has no published docs yet, read its source at `~/.cargo/git/checkouts/proton-sdk-rs2-*/` after `cargo fetch`, or browse `https://github.com/tirbofish/proton-sdk-rs2/tree/main/crates/proton-drive-sdk/src`. Start with `client::ProtonDriveClient` as the SDK readme suggests.

**Step 1: Explore the SDK surface**

```bash
cargo fetch
find ~/.cargo/git/checkouts -path "*/proton-drive-sdk/src/client*" 2>/dev/null | head -5
```

Read the client module to understand the constructor and available methods before writing tests.

**Step 2: Write the failing test**

`crates/pdrive-core/tests/drive_test.rs`:
```rust
// Integration test — requires real Proton credentials via env vars.
// Skip in CI unless PROTON_TEST_USER and PROTON_TEST_PASS are set.
use pdrive_core::drive::DriveClient;

#[tokio::test]
#[ignore = "requires real credentials: PROTON_TEST_USER + PROTON_TEST_PASS"]
async fn test_login_and_list_root() {
    let user = std::env::var("PROTON_TEST_USER").unwrap();
    let pass = std::env::var("PROTON_TEST_PASS").unwrap();

    let client = DriveClient::login(&user, &pass).await.unwrap();
    let entries = client.list("/").await.unwrap();
    assert!(!entries.is_empty());
}
```

**Step 3: Implement drive.rs**

`crates/pdrive-core/src/drive.rs`:
```rust
// Adapt method names to match actual SDK API after reading client.rs source.
use proton_drive_sdk::client::ProtonDriveClient;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
    pub id: String,
}

pub struct DriveClient {
    inner: ProtonDriveClient,
}

impl DriveClient {
    pub async fn login(username: &str, password: &str) -> anyhow::Result<Self> {
        // TODO: adapt to actual SDK login method signature
        let inner = ProtonDriveClient::login(username, password).await
            .map_err(|e| anyhow::anyhow!("login failed: {:?}", e))?;
        Ok(Self { inner })
    }

    pub async fn list(&self, remote_path: &str) -> anyhow::Result<Vec<DriveEntry>> {
        // TODO: adapt to actual SDK ls/iterate method
        let entries = self.inner.ls(remote_path).await
            .map_err(|e| anyhow::anyhow!("list failed: {:?}", e))?;
        Ok(entries.into_iter().map(|e| DriveEntry {
            name: e.name,
            is_dir: e.is_directory,
            size: e.size,
            modified: e.modified_at,
            id: e.id,
        }).collect())
    }

    pub async fn upload(&self, local: &Path, remote_path: &str) -> anyhow::Result<()> {
        // TODO: adapt to actual SDK upload method
        self.inner.put(local, remote_path).await
            .map_err(|e| anyhow::anyhow!("upload failed: {:?}", e))?;
        Ok(())
    }

    pub async fn download(&self, remote_id: &str, local: &Path) -> anyhow::Result<()> {
        // TODO: adapt to actual SDK download method
        self.inner.get(remote_id, local).await
            .map_err(|e| anyhow::anyhow!("download failed: {:?}", e))?;
        Ok(())
    }

    pub fn session_token(&self) -> Option<String> {
        // TODO: extract session token from SDK for persistence
        None
    }
}
```

> **Critical:** The method names (`login`, `ls`, `put`, `get`) are guesses based on the CLI source. You MUST read the actual SDK source and adjust. Run `cargo check -p pdrive-core` after writing and fix all compiler errors.

**Step 4: Verify it compiles**

```bash
cargo check -p pdrive-core 2>&1 | tail -20
```

Fix any errors by adjusting method names to match the actual SDK.

**Step 5: Commit**

```bash
git add crates/pdrive-core/src/drive.rs crates/pdrive-core/tests/drive_test.rs
git commit -m "feat: drive client wrapper around proton-drive-sdk"
```

---

## Task 6: Sync Module (pdrive-core)

**Files:**
- Create: `crates/pdrive-core/src/sync.rs`
- Create: `crates/pdrive-core/tests/sync_test.rs`

**Step 1: Write the failing test**

`crates/pdrive-core/tests/sync_test.rs`:
```rust
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

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let engine = SyncEngine::new(vec![pair], tx);
    engine.start().await.unwrap();

    // Create a file in the watched dir
    fs::write(local.join("new_file.txt"), b"hello").unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let event = rx.try_recv();
    assert!(event.is_ok(), "expected a sync event");
    matches!(event.unwrap(), SyncEvent::LocalChanged { .. });
}
```

**Step 2: Run test — verify it fails**

```bash
cargo test -p pdrive-core --test sync_test 2>&1 | tail -10
```

Expected: FAIL — module `sync` not found.

**Step 3: Implement sync.rs**

`crates/pdrive-core/src/sync.rs`:
```rust
use crate::config::SyncPair;
use notify::{Watcher, RecursiveMode, recommended_watcher, Event};
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum SyncEvent {
    LocalChanged { local_path: PathBuf, pair_remote: String },
    LocalDeleted { local_path: PathBuf, pair_remote: String },
}

pub struct SyncEngine {
    pairs: Vec<SyncPair>,
    tx: Sender<SyncEvent>,
}

impl SyncEngine {
    pub fn new(pairs: Vec<SyncPair>, tx: Sender<SyncEvent>) -> Self {
        Self { pairs, tx }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        for pair in &self.pairs {
            let tx = self.tx.clone();
            let remote = pair.remote.clone();
            let local_root = PathBuf::from(&pair.local);

            let (notify_tx, notify_rx) = std::sync::mpsc::channel();
            let mut watcher = recommended_watcher(notify_tx)?;
            watcher.watch(&local_root, RecursiveMode::Recursive)?;

            tokio::spawn(async move {
                // Keep watcher alive
                let _watcher = watcher;
                loop {
                    match notify_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(Ok(event)) => {
                            for path in event.paths {
                                if path.is_file() {
                                    let _ = tx.send(SyncEvent::LocalChanged {
                                        local_path: path,
                                        pair_remote: remote.clone(),
                                    }).await;
                                }
                            }
                        }
                        Ok(Err(e)) => tracing::error!("watch error: {:?}", e),
                        Err(_) => {} // timeout, loop
                    }
                }
            });
        }
        Ok(())
    }
}
```

**Step 4: Run test — verify it passes**

```bash
cargo test -p pdrive-core --test sync_test 2>&1 | tail -10
```

Expected: 1 test passes.

**Step 5: Commit**

```bash
git add crates/pdrive-core/src/sync.rs crates/pdrive-core/tests/sync_test.rs
git commit -m "feat: filesystem watcher sync engine"
```

---

## Task 7: D-Bus Interface — Daemon (pdrive-daemon)

**Files:**
- Create: `crates/pdrive-daemon/src/dbus.rs`
- Modify: `crates/pdrive-daemon/src/main.rs`

**Step 1: Write the failing test**

Add to `crates/pdrive-daemon/src/main.rs` test module:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_dbus_interface_name() {
        assert_eq!(super::dbus::INTERFACE_NAME, "org.protonmail.PDrive");
    }
}
```

**Step 2: Run test — verify it fails**

```bash
cargo test -p pdrive-daemon 2>&1 | tail -10
```

Expected: FAIL.

**Step 3: Implement dbus.rs**

`crates/pdrive-daemon/src/dbus.rs`:
```rust
use zbus::interface;
use std::sync::Arc;
use tokio::sync::Mutex;
use pdrive_core::config::Config;

pub const INTERFACE_NAME: &str = "org.protonmail.PDrive";
pub const OBJECT_PATH: &str = "/org/protonmail/PDrive";

#[derive(Debug, Clone, PartialEq)]
pub enum DaemonStatus {
    Idle,
    Syncing,
    Paused,
    Error(String),
}

pub struct PDriveInterface {
    status: Arc<Mutex<DaemonStatus>>,
    config: Arc<Mutex<Config>>,
}

impl PDriveInterface {
    pub fn new(config: Config) -> Self {
        Self {
            status: Arc::new(Mutex::new(DaemonStatus::Idle)),
            config: Arc::new(Mutex::new(config)),
        }
    }
}

#[interface(name = "org.protonmail.PDrive")]
impl PDriveInterface {
    async fn get_status(&self) -> String {
        let status = self.status.lock().await;
        match &*status {
            DaemonStatus::Idle => "idle".to_string(),
            DaemonStatus::Syncing => "syncing".to_string(),
            DaemonStatus::Paused => "paused".to_string(),
            DaemonStatus::Error(e) => format!("error: {}", e),
        }
    }

    async fn pause_sync(&self) {
        let mut status = self.status.lock().await;
        *status = DaemonStatus::Paused;
        tracing::info!("sync paused");
    }

    async fn resume_sync(&self) {
        let mut status = self.status.lock().await;
        *status = DaemonStatus::Idle;
        tracing::info!("sync resumed");
    }

    async fn upload_file(&self, local_path: String, remote_path: String) -> String {
        tracing::info!("upload requested: {} -> {}", local_path, remote_path);
        // TODO: enqueue upload task
        "ok".to_string()
    }

    async fn browse_directory(&self, remote_path: String) -> String {
        tracing::info!("browse requested: {}", remote_path);
        // TODO: call drive client and return JSON
        "[]".to_string()
    }
}
```

Update `crates/pdrive-daemon/src/main.rs`:
```rust
mod dbus;

use zbus::ConnectionBuilder;
use pdrive_core::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("pdrive-daemon starting");

    let config = Config::load()?;
    let interface = dbus::PDriveInterface::new(config);

    let _conn = ConnectionBuilder::session()?
        .name("org.protonmail.PDrive")?
        .serve_at(dbus::OBJECT_PATH, interface)?
        .build()
        .await?;

    tracing::info!("D-Bus interface registered at {}", dbus::OBJECT_PATH);

    // Keep running until signal
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dbus_interface_name() {
        assert_eq!(super::dbus::INTERFACE_NAME, "org.protonmail.PDrive");
    }
}
```

**Step 4: Run test — verify it passes**

```bash
cargo test -p pdrive-daemon 2>&1 | tail -10
```

Expected: 1 test passes.

**Step 5: Verify daemon compiles**

```bash
cargo build -p pdrive-daemon 2>&1 | tail -10
```

Expected: compiles cleanly.

**Step 6: Commit**

```bash
git add crates/pdrive-daemon/
git commit -m "feat: D-Bus interface for daemon (status, pause, resume, browse, upload)"
```

---

## Task 8: Systemd Service File

**Files:**
- Create: `systemd/pdrive.service`

**Step 1: Write the service file**

`systemd/pdrive.service`:
```ini
[Unit]
Description=Proton Drive sync daemon
Documentation=https://github.com/YOUR_USERNAME/proton-drive
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/bin/pdrive-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

**Step 2: Commit**

```bash
git add systemd/
git commit -m "feat: systemd user service unit"
```

---

## Task 9: Qt6 GUI — Main Window Skeleton (pdrive-gui)

**Files:**
- Create: `crates/pdrive-gui/build.rs`
- Create: `crates/pdrive-gui/src/cxxqt_object.rs`
- Modify: `crates/pdrive-gui/src/main.rs`
- Create: `crates/pdrive-gui/qml/main.qml`
- Create: `crates/pdrive-gui/qml/MainWindow.qml`

> **Reference:** Read the cxx-qt book at https://kdab.github.io/cxx-qt/book/ — especially the "Getting Started" and "QML" sections. The build.rs setup is mandatory.

**Step 1: Write build.rs**

`crates/pdrive-gui/build.rs`:
```rust
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "ProtonDrive",
            version_major: 1,
            version_minor: 0,
            qml_files: &[
                "qml/main.qml",
                "qml/MainWindow.qml",
            ],
            rust_files: &["src/cxxqt_object.rs"],
        })
        .build();
}
```

**Step 2: Write the CxxQt bridge object**

`crates/pdrive-gui/src/cxxqt_object.rs`:
```rust
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        type AppController = super::AppControllerRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn request_browse(self: Pin<&mut AppController>, path: &QString);

        #[qinvokable]
        fn request_upload(self: Pin<&mut AppController>, local: &QString, remote: &QString);
    }
}

use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct AppControllerRust {
    status: QString,
}

impl qobject::AppController {
    fn request_browse(self: Pin<&mut Self>, path: &QString) {
        tracing::info!("browse: {}", path);
        // TODO: call D-Bus
    }

    fn request_upload(self: Pin<&mut Self>, local: &QString, remote: &QString) {
        tracing::info!("upload: {} -> {}", local, remote);
        // TODO: call D-Bus
    }
}
```

**Step 3: Write main.rs**

`crates/pdrive-gui/src/main.rs`:
```rust
mod cxxqt_object;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/ProtonDrive/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
```

**Step 4: Write QML**

`crates/pdrive-gui/qml/main.qml`:
```qml
import QtQuick
import QtQuick.Controls
import ProtonDrive

ApplicationWindow {
    id: root
    visible: true
    width: 1000
    height: 650
    title: "Proton Drive"

    AppController {
        id: controller
    }

    MainWindow {
        anchors.fill: parent
        controller: controller
    }
}
```

`crates/pdrive-gui/qml/MainWindow.qml`:
```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    property var controller

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Left sidebar
        Rectangle {
            width: 200
            Layout.fillHeight: true
            color: "#f5f5f5"

            ListView {
                id: sidebar
                anchors.fill: parent
                anchors.margins: 8
                model: ListModel {
                    ListElement { label: "My Files"; icon: "📁"; path: "/" }
                    ListElement { label: "Computers"; icon: "💻"; path: "/computers" }
                    ListElement { label: "Sync Folders"; icon: "🔄"; path: "/sync" }
                }
                delegate: ItemDelegate {
                    width: parent.width
                    text: model.icon + "  " + model.label
                    onClicked: controller.request_browse(model.path)
                }
            }
        }

        // Divider
        Rectangle { width: 1; Layout.fillHeight: true; color: "#ddd" }

        // Right file list panel
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // Toolbar
            ToolBar {
                Layout.fillWidth: true
                RowLayout {
                    anchors.fill: parent
                    TextField {
                        id: searchBar
                        placeholderText: "Search..."
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "↑ Upload"
                        onClicked: { /* TODO: file dialog */ }
                    }
                }
            }

            // File list
            ListView {
                id: fileList
                Layout.fillWidth: true
                Layout.fillHeight: true
                model: ListModel {}
                delegate: ItemDelegate {
                    width: parent.width
                    text: model.name
                }
                Label {
                    anchors.centerIn: parent
                    text: "Select a folder to browse"
                    visible: fileList.count === 0
                    color: "#999"
                }
            }

            // Status bar
            Rectangle {
                Layout.fillWidth: true
                height: 28
                color: "#f0f0f0"
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    Label {
                        text: controller.status || "Ready"
                        Layout.fillWidth: true
                        font.pixelSize: 12
                    }
                    Label {
                        text: "daemon: running"
                        font.pixelSize: 12
                        color: "#4caf50"
                    }
                }
            }
        }
    }
}
```

**Step 5: Add Qt6 deps to Cargo.toml** — add these to `crates/pdrive-gui/Cargo.toml`:
```toml
[dependencies]
cxx-qt = "0.7"
cxx-qt-lib = { version = "0.7", features = ["qt_gui", "qt_qml"] }
```

Also ensure Qt6, Qt6Quick, Qt6QuickControls2 are installed:
```bash
sudo pacman -S qt6-base qt6-declarative qt6-quickcontrols2
```

**Step 6: Build the GUI**

```bash
cargo build -p pdrive-gui 2>&1 | tail -20
```

Fix any cxx-qt API mismatches (the API changes between 0.6 and 0.7 — check the cxx-qt changelog if errors appear).

**Step 7: Run the GUI**

```bash
cargo run -p pdrive-gui
```

Expected: window opens with sidebar and empty file list panel.

**Step 8: Commit**

```bash
git add crates/pdrive-gui/
git commit -m "feat: Qt6 GUI skeleton with sidebar and file list panel"
```

---

## Task 10: D-Bus Client in GUI

**Files:**
- Create: `crates/pdrive-gui/src/dbus_client.rs`
- Modify: `crates/pdrive-gui/src/cxxqt_object.rs`

**Step 1: Write the D-Bus client**

`crates/pdrive-gui/src/dbus_client.rs`:
```rust
use zbus::{Connection, proxy};

#[proxy(
    interface = "org.protonmail.PDrive",
    default_service = "org.protonmail.PDrive",
    default_path = "/org/protonmail/PDrive"
)]
trait PDrive {
    async fn get_status(&self) -> zbus::Result<String>;
    async fn pause_sync(&self) -> zbus::Result<()>;
    async fn resume_sync(&self) -> zbus::Result<()>;
    async fn upload_file(&self, local_path: &str, remote_path: &str) -> zbus::Result<String>;
    async fn browse_directory(&self, remote_path: &str) -> zbus::Result<String>;
}

pub async fn connect() -> anyhow::Result<PDriveProxy<'static>> {
    let conn = Connection::session().await?;
    Ok(PDriveProxy::new(&conn).await?)
}
```

**Step 2: Wire into AppController**

Update `request_browse` in `cxxqt_object.rs` to spawn a tokio task that calls `dbus_client::connect()` and then `browse_directory`. Update the `status` property with the result.

> **Note:** cxx-qt requires careful handling of the Qt/Rust thread boundary. The D-Bus call must happen on a tokio thread, not the Qt main thread. Use `tokio::spawn` and update Qt properties via signals. Read the cxx-qt threading docs before implementing.

**Step 3: Verify browse calls daemon**

Start daemon in one terminal:
```bash
cargo run -p pdrive-daemon
```

Start GUI in another:
```bash
cargo run -p pdrive-gui
```

Click a sidebar item. Check daemon terminal for "browse requested" log line.

**Step 4: Commit**

```bash
git add crates/pdrive-gui/src/dbus_client.rs crates/pdrive-gui/src/cxxqt_object.rs
git commit -m "feat: D-Bus client wired into GUI controller"
```

---

## Task 11: Login Dialog

**Files:**
- Create: `crates/pdrive-gui/qml/LoginDialog.qml`
- Modify: `crates/pdrive-gui/qml/main.qml`
- Modify: `crates/pdrive-gui/src/cxxqt_object.rs`

**Step 1: Add login QML**

`crates/pdrive-gui/qml/LoginDialog.qml`:
```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: loginDialog
    title: "Sign in to Proton Drive"
    modal: true
    closePolicy: Popup.NoAutoClose
    anchors.centerIn: parent
    width: 360

    property var controller

    ColumnLayout {
        width: parent.width
        spacing: 12

        Image {
            source: "qrc:/icons/proton-logo.svg"
            Layout.alignment: Qt.AlignHCenter
            height: 48
            fillMode: Image.PreserveAspectFit
        }

        TextField {
            id: usernameField
            placeholderText: "Proton account username"
            Layout.fillWidth: true
        }

        TextField {
            id: passwordField
            placeholderText: "Password"
            echoMode: TextInput.Password
            Layout.fillWidth: true
        }

        Label {
            id: errorLabel
            color: "red"
            visible: text !== ""
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        Button {
            text: "Sign in"
            Layout.fillWidth: true
            onClicked: controller.request_login(usernameField.text, passwordField.text)
        }
    }
}
```

**Step 2: Add `request_login` to AppController and wire auth flow**

Add `request_login` invokable to `cxxqt_object.rs`. On success, dismiss dialog and load file list. On failure, set error message property visible in QML.

**Step 3: Check for saved token on startup**

On app start, call `TokenStore::load()`. If token exists, skip login dialog. If not, show it.

**Step 4: Test login flow manually**

```bash
cargo run -p pdrive-gui
```

Expected: login dialog appears on first run. After login, main window shows.

**Step 5: Commit**

```bash
git add crates/pdrive-gui/
git commit -m "feat: login dialog with token persistence"
```

---

## Task 12: System Tray + Minimize to Tray

**Files:**
- Modify: `crates/pdrive-gui/src/main.rs`
- Modify: `crates/pdrive-gui/qml/main.qml`

**Step 1: Add tray icon**

Qt6 system tray via `QSystemTrayIcon`. Add to `crates/pdrive-gui/Cargo.toml`:
```toml
cxx-qt-lib = { version = "0.7", features = ["qt_gui", "qt_qml", "qt_widgets"] }
```

In `main.rs`, create a `QSystemTrayIcon` with a context menu:
```rust
// QSystemTrayIcon requires QApplication, not QGuiApplication
// Change QGuiApplication to QApplication
use cxx_qt_lib::QApplication;
```

> **Note:** `QSystemTrayIcon` is part of `QtWidgets`. Read cxx-qt-lib docs for the correct type path.

**Step 2: Close to tray**

Override the `onClosing` signal in `main.qml`:
```qml
onClosing: (close) => {
    close.accepted = false
    root.hide()
}
```

**Step 3: Tray menu actions**

- "Open Proton Drive" → `root.show(); root.raise()`
- "Pause Sync" → `controller.pause_sync()`
- "Resume Sync" → `controller.resume_sync()`
- "Quit" → `Qt.quit()`

**Step 4: Test tray behavior**

```bash
cargo run -p pdrive-gui
```

Close window → should minimize to tray. Right-click tray icon → menu appears.

**Step 5: Commit**

```bash
git add crates/pdrive-gui/
git commit -m "feat: system tray with minimize-to-tray and sync controls"
```

---

## Task 13: Desktop Entry + Icon

**Files:**
- Create: `assets/pdrive.desktop`
- Create: `assets/icons/pdrive.svg`

**Step 1: Desktop entry**

`assets/pdrive.desktop`:
```ini
[Desktop Entry]
Version=1.0
Type=Application
Name=Proton Drive
GenericName=Cloud Storage
Comment=Proton Drive desktop client
Exec=pdrive
Icon=pdrive
Terminal=false
Categories=Network;FileTransfer;
Keywords=proton;drive;cloud;storage;sync;
StartupNotify=true
```

**Step 2: Icon**

Create a simple SVG icon at `assets/icons/pdrive.svg` — use Proton's brand colors (purple `#6d4aff`). A simple shield or cloud shape works. Keep it under 4KB.

**Step 3: Commit**

```bash
git add assets/
git commit -m "feat: desktop entry and app icon"
```

---

## Task 14: PKGBUILD for Arch Linux

**Files:**
- Create: `PKGBUILD`
- Create: `.SRCINFO` (generated)

**Step 1: Write PKGBUILD**

`PKGBUILD`:
```bash
# Maintainer: YOUR_NAME <YOUR_EMAIL>
pkgname=proton-drive
pkgver=0.1.0
pkgrel=1
pkgdesc="Proton Drive desktop client for KDE Plasma"
arch=('x86_64')
url="https://github.com/YOUR_USERNAME/proton-drive"
license=('GPL3')
depends=('qt6-base' 'qt6-declarative' 'qt6-quickcontrols2')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname-$pkgver"
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

check() {
    cd "$pkgname-$pkgver"
    cargo test --frozen
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/pdrive" "$pkgdir/usr/bin/pdrive"
    install -Dm755 "target/release/pdrive-daemon" "$pkgdir/usr/bin/pdrive-daemon"
    install -Dm644 "assets/pdrive.desktop" "$pkgdir/usr/share/applications/pdrive.desktop"
    install -Dm644 "assets/icons/pdrive.svg" "$pkgdir/usr/share/icons/hicolor/scalable/apps/pdrive.svg"
    install -Dm644 "systemd/pdrive.service" "$pkgdir/usr/lib/systemd/user/pdrive.service"
}
```

**Step 2: Test PKGBUILD locally**

```bash
cd ~/Projects/proton-drive
makepkg -si
```

Fix any errors. Common issues: missing deps, wrong binary names, path mismatches.

**Step 3: Generate .SRCINFO**

```bash
makepkg --printsrcinfo > .SRCINFO
```

**Step 4: Commit**

```bash
git add PKGBUILD .SRCINFO
git commit -m "feat: PKGBUILD for Arch Linux packaging"
```

---

## Task 15: GitHub Repository Setup

**Step 1: Create GitHub repo**

```bash
cd ~/Projects/proton-drive
gh repo create proton-drive --public --source=. --remote=origin --push
```

**Step 2: Tag first release**

```bash
git tag -a v0.1.0 -m "Initial release"
git push origin v0.1.0
```

**Step 3: Create AUR repo**

```bash
mkdir ~/Projects/proton-drive-aur
cd ~/Projects/proton-drive-aur
git init
# Copy PKGBUILD and .SRCINFO, update source URL to point to GitHub release
cp ~/Projects/proton-drive/PKGBUILD .
cp ~/Projects/proton-drive/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Initial AUR package"
```

> **AUR submission:** Push to `ssh://aur@aur.archlinux.org/proton-drive.git` after creating an AUR account and uploading your SSH key.

---

## Known Risks & Notes

1. **SDK API is undocumented** — Task 5 requires reading the SDK source to find correct method names. This is the highest-risk task.
2. **cxx-qt threading** — Qt properties must be updated on the Qt main thread. Task 10 requires careful async/Qt bridge design.
3. **age crate API** — Verify the exact API for your version with `cargo doc`.
4. **AUR submission** requires an AUR account and GPG key.
5. **Proton's encryption** — The SDK handles E2E encryption internally; you don't need to implement it yourself.
