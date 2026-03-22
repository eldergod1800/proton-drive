use age::secrecy::SecretString;
use std::io::{Read, Write};
use std::path::PathBuf;

fn machine_passphrase() -> SecretString {
    let machine_id = std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "unknown-machine".to_string());
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let input = format!("pdrive-{}-{}", machine_id.trim(), username.trim());
    // Derive a non-reversible passphrase via BLAKE3
    let hash = blake3::hash(input.as_bytes());
    SecretString::from(hash.to_hex().to_string())
}

pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("pdrive")
            .join("session.age")
    }

    pub async fn save(&self, token: &str) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let passphrase = machine_passphrase();
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);
        let mut encrypted = vec![];
        let mut writer = encryptor.wrap_output(&mut encrypted)?;
        writer.write_all(token.as_bytes())?;
        writer.finish()?;
        std::fs::write(&self.path, &encrypted)?;
        Ok(())
    }

    pub async fn load(&self) -> anyhow::Result<Option<String>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let encrypted = std::fs::read(&self.path)?;
        let passphrase = machine_passphrase();
        let decryptor = age::Decryptor::new(&encrypted[..])?;
        let identity = age::scrypt::Identity::new(passphrase);
        let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
        let mut plaintext = String::new();
        reader.read_to_string(&mut plaintext)?;
        Ok(Some(plaintext))
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
