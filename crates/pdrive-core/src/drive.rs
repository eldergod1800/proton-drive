use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use proton_drive_sdk::{
    client::ProtonDriveClient,
    node::{Node, NodeUid},
    utils::PotentialObject,
};
use proton_sdk_rs2::{
    PasswordMode, SessionId, UserId,
    cache::InMemoryCacheRepository,
    client::ProtonClientOptions,
    session::{ProtonAPISession, ProtonSessionOptions},
};
use semver::Version;

use crate::auth::StoredSession;

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
}

fn app_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version::new(0, 1, 0))
}

pub struct DriveClient {
    session: ProtonAPISession,
    drive: ProtonDriveClient,
}

impl DriveClient {
    pub async fn login(username: &str, password: &str) -> anyhow::Result<Self> {
        if username.is_empty() {
            anyhow::bail!("username is empty — please enter your email or username");
        }
        tracing::info!("attempting login for username: {}", username);
        let client_options = ProtonClientOptions {
            // Use the official Linux Drive client app version to avoid human verification
            app_version_override: Some("web-drive@5.0.16".to_string()),
            ..Default::default()
        };
        let options = ProtonSessionOptions::new(client_options);
        let mut session =
            ProtonAPISession::begin(username, password, app_version(), options).await
            .map_err(|e| anyhow::anyhow!("SRP auth failed: {:#}", e))?;
        if session.is_waiting_for_second_factor_code {
            anyhow::bail!("2FA is required for this account but is not yet supported");
        }
        session.apply_data_password(password).await?;
        let drive = ProtonDriveClient::new(&session, None)?;
        Ok(Self { session, drive })
    }

    /// Login using an email verification code obtained after a 9001 (human verification) response.
    /// The caller should first call `login()`, extract the HV token via `extract_hv_token()`,
    /// request a code via `send_email_verification()`, then call this with the code the user enters.
    pub async fn login_with_verification_code(
        username: &str,
        password: &str,
        hv_code: &str,
    ) -> anyhow::Result<Self> {
        if username.is_empty() {
            anyhow::bail!("username is empty — please enter your email or username");
        }
        if hv_code.is_empty() {
            anyhow::bail!("verification code is empty");
        }
        tracing::info!(
            "attempting login with email verification code for username: {}",
            username
        );
        let client_options = ProtonClientOptions {
            app_version_override: Some("web-drive@5.0.16".to_string()),
            ..Default::default()
        };
        let options = ProtonSessionOptions::new(client_options);
        let mut session = ProtonAPISession::begin_with_email_verification(
            username,
            password,
            hv_code,
            app_version(),
            options,
        )
        .await
        .map_err(|e| anyhow::anyhow!("SRP auth (with HV) failed: {:#}", e))?;
        if session.is_waiting_for_second_factor_code {
            anyhow::bail!("2FA is required for this account but is not yet supported");
        }
        session.apply_data_password(password).await?;
        let drive = ProtonDriveClient::new(&session, None)?;
        Ok(Self { session, drive })
    }

    /// Extract the HumanVerificationToken from an error returned by `login()`.
    /// Returns `Some(token)` when the error is a 9001 human-verification error.
    pub fn extract_hv_token(error: &anyhow::Error) -> Option<String> {
        let msg = error.to_string();
        if let Some(json_start) = msg.find('{') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg[json_start..]) {
                if v["Code"] == 9001 {
                    return v["Details"]["HumanVerificationToken"]
                        .as_str()
                        .map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// Request Proton to email a verification code to the user's account address.
    /// `hv_token` is the token from `extract_hv_token()`.
    pub async fn send_email_verification(username: &str, hv_token: &str) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let body = serde_json::json!({"Username": username, "Type": "login"});
        let resp = client
            .post("https://drive-api.proton.me/core/v4/users/code")
            .header("content-type", "application/json")
            .header("x-pm-appversion", "web-drive@5.0.16")
            .header("x-pm-human-verification-token", hv_token)
            .header("x-pm-human-verification-token-type", "email")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("send_email_verification {}: {}", status, body);
        }
        Ok(())
    }

    pub async fn from_stored(stored: &StoredSession, password: &str) -> anyhow::Result<Self> {
        let cache: Arc<dyn proton_sdk_rs2::cache::CacheRepository> =
            Arc::new(InMemoryCacheRepository::new());
        let password_mode = match stored.password_mode {
            2 => PasswordMode::Dual,
            _ => PasswordMode::Single,
        };
        let mut session = ProtonAPISession::resume(
            SessionId::new(stored.session_id.clone()),
            stored.username.clone(),
            UserId::new(stored.user_id.clone()),
            stored.access_token.clone(),
            stored.refresh_token.clone(),
            stored.scopes.clone(),
            stored.is_2fa,
            password_mode,
            app_version(),
            cache,
        );
        if let Err(e) = session.apply_data_password(password).await {
            tracing::warn!("apply_data_password failed on resume: {}", e);
        }
        let drive = ProtonDriveClient::new(&session, None)?;
        Ok(Self { session, drive })
    }

    pub async fn session_data(&self) -> anyhow::Result<StoredSession> {
        let (access_token, refresh_token) = self.session.token_credential.get_tokens().await?;
        Ok(StoredSession {
            session_id: self.session.session_id.raw().to_string(),
            username: self.session.username.clone(),
            user_id: self.session.user_id.raw().to_string(),
            access_token,
            refresh_token,
            scopes: self.session.scopes.clone(),
            is_2fa: self.session.is_waiting_for_second_factor_code,
            password_mode: match self.session.password_mode {
                PasswordMode::Dual => 2,
                _ => 1,
            },
        })
    }

    pub async fn list_folder(
        &self,
        folder_uid: NodeUid,
    ) -> anyhow::Result<Vec<(DriveEntry, NodeUid)>> {
        let mut stream = self.drive.enumerate_folder_children(folder_uid).await?;
        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let potential = item?;
            match potential {
                PotentialObject::Node(node) => {
                    if let Some((entry, uid)) = node_to_entry_and_uid(node) {
                        results.push((entry, uid));
                    }
                }
                PotentialObject::Degraded(_) => {
                    tracing::warn!("skipping degraded node");
                }
            }
        }
        Ok(results)
    }

    pub async fn list_root(&self) -> anyhow::Result<(Vec<(DriveEntry, NodeUid)>, NodeUid)> {
        let root = self.drive.get_my_files_folder().await?;
        let root_uid = root.base.uid.clone();
        let entries = self.list_folder(root_uid.clone()).await?;
        Ok((entries, root_uid))
    }

    pub async fn download(&self, node_uid: NodeUid, dest: &Path) -> anyhow::Result<()> {
        self.drive
            .download_to_file(node_uid, dest, Box::new(|_, _| {}))
            .await
    }

    pub fn session_token(&self) -> Option<String> {
        Some(self.session.session_id.raw().to_string())
    }
}

fn node_to_entry_and_uid(node: Node) -> Option<(DriveEntry, NodeUid)> {
    match node {
        Node::Folder(f) | Node::Album(f) => {
            let uid = f.base.uid.clone();
            Some((
                DriveEntry {
                    id: f.base.uid.to_string(),
                    name: f.base.name.clone(),
                    is_dir: true,
                    size: None,
                    modified: None,
                },
                uid,
            ))
        }
        Node::Photo(_) => None,
        Node::File(f) => {
            let uid = f.base.base.uid.clone();
            let size = u64::try_from(f.total_size_on_cloud_storage).ok();
            Some((
                DriveEntry {
                    id: f.base.base.uid.to_string(),
                    name: f.base.base.name.clone(),
                    is_dir: false,
                    size,
                    modified: None,
                },
                uid,
            ))
        }
    }
}
