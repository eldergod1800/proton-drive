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
    fn as_str(&self) -> &'static str {
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
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sync_entries (
                local_path  TEXT PRIMARY KEY,
                remote_id   TEXT NOT NULL,
                status      TEXT NOT NULL,
                modified_at INTEGER NOT NULL
            );"
        )?;
        Ok(Self { conn })
    }

    pub fn upsert(&self, entry: &SyncEntry) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_entries
             (local_path, remote_id, status, modified_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                entry.local_path,
                entry.remote_id,
                entry.status.as_str(),
                entry.modified_at
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, local_path: &str) -> anyhow::Result<Option<SyncEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT local_path, remote_id, status, modified_at
             FROM sync_entries WHERE local_path = ?1"
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
            "SELECT local_path, remote_id, status, modified_at
             FROM sync_entries WHERE status = 'pending_upload'"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SyncEntry {
                local_path: row.get(0)?,
                remote_id: row.get(1)?,
                status: SyncStatus::PendingUpload,
                modified_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
