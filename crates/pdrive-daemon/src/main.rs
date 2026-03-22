mod dbus;

use pdrive_core::{auth::TokenStore, config::Config, drive::DriveClient};
use zbus::connection;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("pdrive-daemon starting");

    let config = Config::load()?;
    let store = TokenStore::new(TokenStore::default_path());

    let drive = match (store.load_session().await?, store.load_password().await?) {
        (Some(session), Some(password)) => {
            tracing::info!("restoring session for {}", session.username);
            match DriveClient::from_stored(&session, &password).await {
                Ok(client) => {
                    tracing::info!("session restored successfully");
                    Some(client)
                }
                Err(e) => {
                    tracing::warn!("failed to restore session: {} — browsing disabled", e);
                    None
                }
            }
        }
        _ => {
            tracing::warn!("no stored session — browsing disabled until user logs in");
            None
        }
    };

    let interface = dbus::PDriveInterface::new(config, drive, TokenStore::default_path());

    let _conn = connection::Builder::session()?
        .name("org.protonmail.PDrive")?
        .serve_at(dbus::OBJECT_PATH, interface)?
        .build()
        .await?;

    tracing::info!("D-Bus interface registered, daemon ready");
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_interface_name_constant() {
        assert_eq!(crate::dbus::INTERFACE_NAME, "org.protonmail.PDrive");
    }

    #[test]
    fn test_object_path_constant() {
        assert_eq!(crate::dbus::OBJECT_PATH, "/org/protonmail/PDrive");
    }
}
