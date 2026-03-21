use proton_drive_sdk::{DriveNode, ProtonDriveClient};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
}

impl From<DriveNode> for DriveEntry {
    fn from(node: DriveNode) -> Self {
        Self {
            id: node.id,
            name: node.name,
            is_dir: node.is_directory,
            size: node.size,
            modified: node.modified_at,
        }
    }
}

pub struct DriveClient {
    inner: ProtonDriveClient,
}

impl DriveClient {
    pub async fn login(username: &str, password: &str) -> anyhow::Result<Self> {
        let inner = ProtonDriveClient::login(username, password)
            .await
            .map_err(|e| anyhow::anyhow!("login failed: {}", e))?;
        Ok(Self { inner })
    }

    pub async fn list(&self, remote_path: &str) -> anyhow::Result<Vec<DriveEntry>> {
        let nodes = self
            .inner
            .ls(remote_path)
            .await
            .map_err(|e| anyhow::anyhow!("list failed: {}", e))?;
        Ok(nodes.into_iter().map(DriveEntry::from).collect())
    }

    pub async fn upload(&self, local: &Path, remote_path: &str) -> anyhow::Result<()> {
        self.inner
            .put(local, remote_path)
            .await
            .map_err(|e| anyhow::anyhow!("upload failed: {}", e))?;
        Ok(())
    }

    pub async fn download(&self, remote_id: &str, local: &Path) -> anyhow::Result<()> {
        self.inner
            .get(remote_id, local)
            .await
            .map_err(|e| anyhow::anyhow!("download failed: {}", e))?;
        Ok(())
    }

    pub fn session_token(&self) -> Option<String> {
        self.inner.session_token()
    }
}
