use pdrive_core::{auth::TokenStore, config::Config, drive::DriveClient};
use proton_drive_sdk::node::NodeUid;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use zbus::interface;

#[allow(dead_code)]
pub const INTERFACE_NAME: &str = "org.protonmail.PDrive";
pub const OBJECT_PATH: &str = "/org/protonmail/PDrive";

/// Cache TTL for directory listings — avoids re-fetching on every back-navigation.
const LISTING_TTL_SECS: u64 = 60;

pub struct PDriveInterface {
    #[allow(dead_code)]
    config: Arc<Mutex<Config>>,
    drive: Arc<Mutex<Option<Arc<DriveClient>>>>,
    path_cache: Arc<Mutex<HashMap<String, NodeUid>>>,
    /// listing_cache: path → (json_result, fetched_at)
    listing_cache: Arc<Mutex<HashMap<String, (String, Instant)>>>,
    token_store_path: PathBuf,
}

impl PDriveInterface {
    pub fn new(config: Config, drive: Option<DriveClient>, token_store_path: PathBuf) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            drive: Arc::new(Mutex::new(drive.map(Arc::new))),
            path_cache: Arc::new(Mutex::new(HashMap::new())),
            listing_cache: Arc::new(Mutex::new(HashMap::new())),
            token_store_path,
        }
    }

    /// Return the active DriveClient, lazy-loading from the token store if needed.
    async fn get_or_load_drive(&self) -> Option<Arc<DriveClient>> {
        {
            let guard = self.drive.lock().await;
            if guard.is_some() {
                return guard.clone();
            }
        }
        // No session in memory — try loading from the token store
        let store = TokenStore::new(self.token_store_path.clone());
        match (store.load_session().await, store.load_password().await) {
            (Ok(Some(session)), Ok(Some(password))) => {
                tracing::info!("lazy-loading session for {}", session.username);
                match DriveClient::from_stored(&session, &password).await {
                    Ok(client) => {
                        let client = Arc::new(client);
                        *self.drive.lock().await = Some(client.clone());
                        Some(client)
                    }
                    Err(e) => {
                        tracing::warn!("lazy-load session failed: {}", e);
                        None
                    }
                }
            }
            _ => {
                tracing::warn!("browse_directory: no session loaded");
                None
            }
        }
    }
}

#[interface(name = "org.protonmail.PDrive")]
impl PDriveInterface {
    async fn get_status(&self) -> String {
        let guard = self.drive.lock().await;
        if guard.is_some() { "running".to_string() } else { "no-session".to_string() }
    }

    /// Reload the drive session from the keyring.  Called by the GUI after a
    /// successful login so the daemon immediately picks up the new session
    /// instead of continuing with a stale one from startup.
    async fn reload_session(&self) -> String {
        // Clear in-memory session so get_or_load_drive re-fetches from keyring
        *self.drive.lock().await = None;
        // Clear listing cache — it belongs to the old session / account
        self.listing_cache.lock().await.clear();
        self.path_cache.lock().await.clear();

        match self.get_or_load_drive().await {
            Some(_) => {
                tracing::info!("reload_session: new session loaded successfully");
                "ok".to_string()
            }
            None => {
                tracing::warn!("reload_session: no session found in keyring");
                "no-session".to_string()
            }
        }
    }

    async fn get_storage(&self) -> String {
        let drive = match self.get_or_load_drive().await {
            Some(d) => d,
            None => return r#"{"error":"no session"}"#.to_string(),
        };
        match drive.get_user_quota().await {
            Ok((used, total)) => {
                serde_json::json!({"used": used, "total": total}).to_string()
            }
            Err(e) => {
                tracing::warn!("get_storage failed: {}", e);
                r#"{"error":"unavailable"}"#.to_string()
            }
        }
    }

    async fn pause_sync(&self) {
        tracing::info!("sync paused");
    }

    async fn resume_sync(&self) {
        tracing::info!("sync resumed");
    }

    async fn upload_file(&self, local_path: String, remote_path: String) -> String {
        tracing::info!("upload requested: {} -> {}", local_path, remote_path);
        "ok".to_string()
    }

    async fn download_file(&self, remote_path: String) -> String {
        let drive = match self.get_or_load_drive().await {
            Some(d) => d,
            None => return String::new(),
        };

        let node_uid = {
            let cache = self.path_cache.lock().await;
            match cache.get(&remote_path).cloned() {
                Some(uid) => uid,
                None => {
                    tracing::warn!("download_file: path not in cache, browse parent first: {}", remote_path);
                    return String::new();
                }
            }
        };

        let filename = remote_path.rsplit('/').next().unwrap_or("file").to_string();
        let raw = match std::path::Path::new(&filename).components().collect::<Vec<_>>().as_slice() {
            [std::path::Component::Normal(_)] => filename.clone(),
            _ => {
                tracing::warn!("download_file: invalid filename '{}'", filename);
                return String::new();
            }
        };

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("pdrive");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("download_file: could not create cache dir {:?}: {}", cache_dir, e);
            return String::new();
        }
        let dest = cache_dir.join(&raw);

        // SEC-4: validate path stays inside cache dir
        if !dest.is_absolute() || !dest.starts_with(&cache_dir) {
            tracing::warn!("download_file: blocked unsafe path");
            return String::new();
        }

        match drive.download(node_uid, &dest).await {
            Ok(()) => dest.to_string_lossy().into_owned(),
            Err(e) => {
                tracing::warn!("download failed: {}", e);
                String::new()
            }
        }
    }

    async fn browse_directory(&self, remote_path: String) -> String {
        // Return cached listing if still fresh
        {
            let cache = self.listing_cache.lock().await;
            if let Some((json, fetched_at)) = cache.get(&remote_path) {
                if fetched_at.elapsed().as_secs() < LISTING_TTL_SECS {
                    tracing::debug!("browse_directory: cache hit for {}", remote_path);
                    return json.clone();
                }
            }
        }

        let drive = match self.get_or_load_drive().await {
            Some(d) => d,
            None => return "[]".to_string(),
        };

        let entries_and_uids = if remote_path == "/" || remote_path.is_empty() {
            match drive.list_root().await {
                Ok((entries, root_uid)) => {
                    self.path_cache.lock().await.insert("/".to_string(), root_uid);
                    entries
                }
                Err(e) => {
                    tracing::warn!("browse_directory root failed: {}", e);
                    return "[]".to_string();
                }
            }
        } else if remote_path == "/computers" || remote_path == "/sync" {
            match drive.list_devices().await {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!("list_devices failed: {}", e);
                    return "[]".to_string();
                }
            }
        } else {
            let uid = self.path_cache.lock().await.get(&remote_path).cloned();
            match uid {
                Some(uid) => match drive.list_folder(uid).await {
                    Ok(entries) => entries,
                    Err(e) => {
                        tracing::warn!("browse_directory failed for {}: {}", remote_path, e);
                        return "[]".to_string();
                    }
                },
                None => {
                    tracing::warn!("browse_directory: path not in cache: {}", remote_path);
                    return "[]".to_string();
                }
            }
        };

        // Cache all child paths for subsequent navigation and downloads
        {
            let mut cache = self.path_cache.lock().await;
            for (entry, uid) in &entries_and_uids {
                let child_path = if remote_path == "/" || remote_path.is_empty() {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", remote_path.trim_end_matches('/'), entry.name)
                };
                cache.insert(child_path, uid.clone());
            }
        }

        // Serialize to JSON for D-Bus transport
        let json_entries: Vec<serde_json::Value> = entries_and_uids.iter().map(|(e, _)| {
            serde_json::json!({
                "name": e.name,
                "is_dir": e.is_dir,
                "size": e.size.map(human_size).unwrap_or_else(|| "--".to_string()),
            })
        }).collect();

        let result = serde_json::to_string(&json_entries).unwrap_or_else(|_| "[]".to_string());
        self.listing_cache.lock().await.insert(remote_path, (result.clone(), Instant::now()));
        result
    }
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
