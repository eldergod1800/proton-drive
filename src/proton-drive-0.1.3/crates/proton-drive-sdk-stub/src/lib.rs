//! STUB — placeholder for the real proton-drive-sdk
//! Replace this crate with the upstream git dependency once
//! the private registry issue is resolved.
//!
//! This stub defines the interface contract that drive.rs depends on.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
#[error("Proton Drive SDK error: {0}")]
pub struct SdkError(pub String);

/// A file or folder entry returned by ls/iteration
#[derive(Debug, Clone)]
pub struct DriveNode {
    pub id: String,
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
}

/// The main SDK client — stub implementation
pub struct ProtonDriveClient {
    _username: String,
}

impl ProtonDriveClient {
    /// Authenticate with Proton and return a client.
    /// Stub: always succeeds with any credentials.
    pub async fn login(username: &str, _password: &str) -> Result<Self, SdkError> {
        Ok(Self {
            _username: username.to_string(),
        })
    }

    /// List entries at a remote path.
    pub async fn ls(&self, _path: &str) -> Result<Vec<DriveNode>, SdkError> {
        Ok(vec![DriveNode {
            id: "stub-id-1".into(),
            name: "Example Folder".into(),
            is_directory: true,
            size: None,
            modified_at: Some(1700000000),
        }])
    }

    /// Upload a local file to a remote path.
    pub async fn put(&self, _local: &Path, _remote_path: &str) -> Result<(), SdkError> {
        Ok(())
    }

    /// Download a remote file by ID to a local path.
    pub async fn get(&self, _remote_id: &str, _local: &Path) -> Result<(), SdkError> {
        Ok(())
    }

    /// Return the current session token (for persistence).
    pub fn session_token(&self) -> Option<String> {
        Some("stub-session-token".to_string())
    }
}
