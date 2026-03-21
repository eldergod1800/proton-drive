mod dbus;

use pdrive_core::config::Config;
use zbus::connection;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("pdrive-daemon starting");

    let config = Config::load()?;
    let interface = dbus::PDriveInterface::new(config);

    let _conn = connection::Builder::session()?
        .name("org.protonmail.PDrive")?
        .serve_at(dbus::OBJECT_PATH, interface)?
        .build()
        .await?;

    tracing::info!("D-Bus interface registered at {} on {}", dbus::OBJECT_PATH, dbus::INTERFACE_NAME);
    tracing::info!("pdrive-daemon ready");

    // Keep running
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
