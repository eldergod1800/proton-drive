use pdrive_core::config::Config;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::interface;

pub const INTERFACE_NAME: &str = "org.protonmail.PDrive";
pub const OBJECT_PATH: &str = "/org/protonmail/PDrive";

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonStatus {
    Idle,
    Syncing,
    Paused,
    Error(String),
}

impl DaemonStatus {
    fn as_str(&self) -> String {
        match self {
            Self::Idle => "idle".to_string(),
            Self::Syncing => "syncing".to_string(),
            Self::Paused => "paused".to_string(),
            Self::Error(e) => format!("error:{}", e),
        }
    }
}

pub struct PDriveInterface {
    status: Arc<Mutex<DaemonStatus>>,
    #[allow(dead_code)]
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
        self.status.lock().await.as_str()
    }

    async fn pause_sync(&self) {
        *self.status.lock().await = DaemonStatus::Paused;
        tracing::info!("sync paused");
    }

    async fn resume_sync(&self) {
        *self.status.lock().await = DaemonStatus::Idle;
        tracing::info!("sync resumed");
    }

    async fn upload_file(&self, local_path: String, remote_path: String) -> String {
        tracing::info!("upload requested: {} -> {}", local_path, remote_path);
        // TODO: enqueue upload task
        "ok".to_string()
    }

    async fn browse_directory(&self, remote_path: String) -> String {
        tracing::info!("browse requested: {}", remote_path);
        // TODO: call drive client and return JSON array of entries
        "[]".to_string()
    }
}
