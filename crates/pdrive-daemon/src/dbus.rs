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
        // Stub data — replace with real DriveClient::list() call
        let entries = match remote_path.as_str() {
            "/computers" => r#"[
                {"name":"This PC","is_dir":true,"size":"--"},
                {"name":"Work Laptop","is_dir":true,"size":"--"}
            ]"#,
            "/sync" => r#"[
                {"name":"Documents","is_dir":true,"size":"1.2 GB"},
                {"name":"Photos","is_dir":true,"size":"4.8 GB"}
            ]"#,
            _ => r#"[
                {"name":"Documents","is_dir":true,"size":"1.2 GB"},
                {"name":"Photos","is_dir":true,"size":"4.8 GB"},
                {"name":"Music","is_dir":true,"size":"820 MB"},
                {"name":"notes.txt","is_dir":false,"size":"4.2 KB"},
                {"name":"report.pdf","is_dir":false,"size":"1.1 MB"}
            ]"#,
        };
        entries.to_string()
    }
}
