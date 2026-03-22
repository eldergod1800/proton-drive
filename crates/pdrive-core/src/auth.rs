// crates/pdrive-core/src/auth.rs
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "proton-drive";
const KEYRING_SESSION: &str = "session-v2";
const KEYRING_PASSWORD: &str = "session-password";

/// Everything needed to restore a session after a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub session_id: String,
    pub username: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    pub is_2fa: bool,
    /// 1 = Single, 2 = Dual
    pub password_mode: u8,
}

pub struct TokenStore;

impl TokenStore {
    pub fn new(_path: PathBuf) -> Self {
        Self
    }

    pub fn default_path() -> PathBuf {
        PathBuf::new()
    }

    /// Save the full session data. Separate from the password.
    pub async fn save_session(&self, session: &StoredSession) -> anyhow::Result<()> {
        let json = serde_json::to_string(session)?;
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_SESSION)?.set_password(&json)?;
        Ok(())
    }

    /// Save the login password so the daemon can re-derive key passphrases on restart.
    /// Stored securely in the keyring (KWallet / libsecret).
    pub fn save_password(&self, password: &str) -> anyhow::Result<()> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD)?.set_password(password)?;
        Ok(())
    }

    /// Load the stored session, if any.
    pub async fn load_session(&self) -> anyhow::Result<Option<StoredSession>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_SESSION)?;
        match entry.get_password() {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keyring read error: {}", e)),
        }
    }

    /// Load the stored password, if any.
    pub fn load_password(&self) -> anyhow::Result<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD)?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keyring read password error: {}", e)),
        }
    }

    /// Clear both the session and password from the keyring.
    pub fn clear(&self) -> anyhow::Result<()> {
        for entry_name in &[KEYRING_SESSION, KEYRING_PASSWORD] {
            let entry = keyring::Entry::new(KEYRING_SERVICE, entry_name)?;
            match entry.delete_password() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(anyhow::anyhow!("keyring delete error: {}", e)),
            }
        }
        Ok(())
    }

    // ── Backward-compat shim used by GUI startup check ────────────────────
    /// Returns the session_id string if a session is stored. Used by GUI to
    /// test whether a session exists without needing the full struct.
    pub async fn load(&self) -> anyhow::Result<Option<String>> {
        Ok(self.load_session().await?.map(|s| s.session_id))
    }
}
