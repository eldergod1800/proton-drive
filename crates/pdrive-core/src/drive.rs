use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use proton_drive_sdk::{
    client::ProtonDriveClient,
    node::{Node, NodeUid},
    utils::PotentialObject,
};
use serde::Deserialize;
use proton_sdk_rs2::{
    PasswordMode, SessionId, UserId,
    cache::InMemoryCacheRepository,
    client::ProtonClientOptions,
    session::{BeginResult, PendingAuth, ProtonAPISession, ProtonSessionOptions},
};
use semver::Version;

use crate::auth::StoredSession;

/// Error returned by `DriveClient::login()` (or `login_complete_with_captcha()`) when the
/// account has TOTP 2FA enabled.  Pass the session + password to `login_complete_with_2fa()`.
pub struct TwoFactorRequired {
    pub session: ProtonAPISession,
    pub password: String,
}

impl std::fmt::Debug for TwoFactorRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TwoFactorRequired")
    }
}
impl std::fmt::Display for TwoFactorRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "two-factor authentication required")
    }
}
impl std::error::Error for TwoFactorRequired {}

/// Error returned by `DriveClient::login()` when Proton requires human verification.
/// Contains the preserved SRP context — pass it to `login_complete_with_captcha()`.
pub struct HumanVerificationRequired(pub PendingAuth);

impl std::fmt::Debug for HumanVerificationRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HumanVerificationRequired(web_url={})", self.0.web_url)
    }
}
impl std::fmt::Display for HumanVerificationRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "human verification required (captcha): {}", self.0.web_url)
    }
}
impl std::error::Error for HumanVerificationRequired {}

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
    /// Begin login. On 9001 captcha challenge returns `Err` with a `PendingAuth` payload.
    /// Call `login_complete_with_captcha` with the `PendingAuth` and captcha token to finish.
    pub async fn login(username: &str, password: &str) -> anyhow::Result<Self> {
        if username.is_empty() {
            anyhow::bail!("username is empty — please enter your email or username");
        }
        tracing::info!("attempting login for username: {}", username);
        let options = ProtonSessionOptions::new(ProtonClientOptions {
            app_version_override: Some("web-drive@5.0.16".to_string()),
            ..Default::default()
        });
        match ProtonAPISession::try_begin(username, password, app_version(), options).await
            .map_err(|e| anyhow::anyhow!("SRP auth failed: {:#}", e))?
        {
            BeginResult::Session(session) => {
                if session.is_waiting_for_second_factor_code {
                    return Err(TwoFactorRequired {
                        session,
                        password: password.to_string(),
                    }.into());
                }
                let mut session = session;
                session.apply_data_password(password).await?;
                let drive = ProtonDriveClient::new(&session, None)?;
                Ok(Self { session, drive })
            }
            BeginResult::AwaitingHumanVerification(pending) => {
                Err(HumanVerificationRequired(pending).into())
            }
        }
    }

    /// Complete login using the cached SRP context from a 9001 challenge and the captcha token.
    pub async fn login_complete_with_captcha(
        pending: PendingAuth,
        password: &str,
        captcha_token: &str,
    ) -> anyhow::Result<Self> {
        tracing::info!("retrying auth with captcha token (same SRP session)");
        let session = pending
            .retry_with_hv(password, captcha_token, "captcha")
            .await
            .map_err(|e| anyhow::anyhow!("SRP auth (captcha) failed: {:#}", e))?;
        if session.is_waiting_for_second_factor_code {
            return Err(TwoFactorRequired {
                session,
                password: password.to_string(),
            }.into());
        }
        let mut session = session;
        session.apply_data_password(password).await?;
        let drive = ProtonDriveClient::new(&session, None)?;
        Ok(Self { session, drive })
    }

    /// Complete login for an account requiring TOTP 2FA.
    /// `session` and `password` come from the `TwoFactorRequired` error; `totp_code` is the
    /// 6-digit authenticator code entered by the user.
    pub async fn login_complete_with_2fa(
        mut session: ProtonAPISession,
        password: &str,
        totp_code: &str,
    ) -> anyhow::Result<Self> {
        tracing::info!("completing 2FA login");
        session
            .apply_second_factor_code(totp_code.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("2FA verification failed: {:#}", e))?;
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

    /// Extract the WebUrl from a 9001 error — open this in a browser for the user to solve captcha.
    pub fn extract_hv_web_url(error: &anyhow::Error) -> Option<String> {
        let msg = error.to_string();
        if let Some(json_start) = msg.find('{') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg[json_start..]) {
                if v["Code"] == 9001 {
                    return v["Details"]["WebUrl"].as_str().map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// Request Proton to email a verification code to the user's account address.
    /// `hv_token` is the token from `extract_hv_token()`.
    pub async fn send_email_verification(username: &str, hv_token: &str) -> anyhow::Result<()> {
        // Proton expects the bare username, not the full email address
        let bare_username = username.split('@').next().unwrap_or(username);
        let client = reqwest::Client::new();
        let body = serde_json::json!({"Username": bare_username, "Type": "login"});
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
        let resume_options = proton_sdk_rs2::client::ProtonClientOptions {
            app_version_override: Some("web-drive@5.0.16".to_string()),
            ..Default::default()
        };
        let mut session = ProtonAPISession::resume_with_options(
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
            resume_options,
        );
        session.apply_data_password(password).await?;
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

    /// List backup devices (shown under "Computers" in the sidebar).
    pub async fn list_devices(&self) -> anyhow::Result<Vec<(DriveEntry, NodeUid)>> {
        let devices = self.drive.list_devices().await?;
        let entries = devices
            .into_iter()
            .map(|d| {
                let uid = d.root_uid.clone();
                let entry = DriveEntry {
                    id: d.root_uid.to_string(),
                    name: d.name,
                    is_dir: true,
                    size: None,
                    modified: None,
                };
                (entry, uid)
            })
            .collect();
        Ok(entries)
    }

    /// Fetch the user's storage quota (used bytes, total bytes).
    pub async fn get_user_quota(&self) -> anyhow::Result<(u64, u64)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct UserInfo {
            used_space: u64,
            max_space: u64,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct UserResp {
            user: UserInfo,
        }

        let (access_token, _) = self.session.token_credential.get_tokens().await?;
        let resp = reqwest::Client::new()
            .get("https://drive-api.proton.me/core/v4/users")
            .bearer_auth(&access_token)
            .header("x-pm-appversion", "web-drive@5.0.16")
            .send()
            .await?;
        let body: UserResp = resp.json().await?;
        Ok((body.user.used_space, body.user.max_space))
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
