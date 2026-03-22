use pdrive_core::{config::Config, drive::{DriveClient, DriveEntry}};
use proton_drive_sdk::node::NodeUid;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use zbus::interface;

pub const INTERFACE_NAME: &str = "org.protonmail.PDrive";
pub const OBJECT_PATH: &str = "/org/protonmail/PDrive";

pub struct PDriveInterface {
    #[allow(dead_code)]
    config: Arc<Mutex<Config>>,
    drive: Arc<Mutex<Option<DriveClient>>>,
    path_cache: Arc<Mutex<HashMap<String, NodeUid>>>,
}

impl PDriveInterface {
    pub fn new(config: Config, drive: Option<DriveClient>) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            drive: Arc::new(Mutex::new(drive)),
            path_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[interface(name = "org.protonmail.PDrive")]
impl PDriveInterface {
    async fn get_status(&self) -> String {
        let guard = self.drive.lock().await;
        if guard.is_some() {
            "running".to_string()
        } else {
            "no-session".to_string()
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
        let drive_guard = self.drive.lock().await;
        let drive = match drive_guard.as_ref() {
            Some(d) => d,
            None => {
                tracing::warn!("download_file: no session");
                return String::new();
            }
        };

        let cache = self.path_cache.lock().await;
        let node_uid = match cache.get(&remote_path).cloned() {
            Some(uid) => uid,
            None => {
                tracing::warn!(
                    "download_file: path not in cache, browse parent first: {}",
                    remote_path
                );
                return String::new();
            }
        };
        drop(cache);

        let filename = remote_path.rsplit('/').next().unwrap_or("file").to_string();
        let raw = match std::path::Path::new(&filename)
            .components()
            .collect::<Vec<_>>()
            .as_slice()
        {
            [std::path::Component::Normal(_)] => filename.clone(),
            _ => {
                tracing::warn!("download_file: invalid filename '{}'", filename);
                return String::new();
            }
        };

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("pdrive");
        if std::fs::create_dir_all(&cache_dir).is_err() {
            return String::new();
        }
        let dest = cache_dir.join(&raw);

        // SEC-4: validate path stays inside cache dir
        let safe = dest.is_absolute() && dest.starts_with(&cache_dir);
        if !safe {
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
        let drive_guard = self.drive.lock().await;
        let drive = match drive_guard.as_ref() {
            Some(d) => d,
            None => {
                tracing::warn!("browse_directory: no session loaded");
                return "[]".to_string();
            }
        };

        let entries_and_uids: Vec<(DriveEntry, NodeUid)> =
            if remote_path == "/" || remote_path.is_empty() {
                match drive.list_root().await {
                    Ok((entries, _root_uid)) => entries,
                    Err(e) => {
                        tracing::warn!("browse_directory root failed: {}", e);
                        return "[]".to_string();
                    }
                }
            } else {
                let uid = {
                    let cache = self.path_cache.lock().await;
                    cache.get(&remote_path).cloned()
                };
                match uid {
                    Some(uid) => match drive.list_folder(uid).await {
                        Ok(entries) => entries,
                        Err(e) => {
                            tracing::warn!(
                                "browse_directory failed for {}: {}",
                                remote_path,
                                e
                            );
                            return "[]".to_string();
                        }
                    },
                    None => {
                        tracing::warn!(
                            "browse_directory: path not in cache: {}",
                            remote_path
                        );
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
        let json_entries: Vec<serde_json::Value> = entries_and_uids
            .iter()
            .map(|(e, _)| {
                serde_json::json!({
                    "name": e.name,
                    "is_dir": e.is_dir,
                    "size": e.size.map(human_size).unwrap_or_else(|| "--".to_string()),
                })
            })
            .collect();

        serde_json::to_string(&json_entries).unwrap_or_else(|_| "[]".to_string())
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
