use zbus::{proxy, Connection};

#[proxy(
    interface = "org.protonmail.PDrive",
    default_service = "org.protonmail.PDrive",
    default_path = "/org/protonmail/PDrive"
)]
trait PDrive {
    async fn get_status(&self) -> zbus::Result<String>;
    async fn pause_sync(&self) -> zbus::Result<()>;
    async fn resume_sync(&self) -> zbus::Result<()>;
    async fn upload_file(&self, local_path: &str, remote_path: &str) -> zbus::Result<String>;
    async fn browse_directory(&self, remote_path: &str) -> zbus::Result<String>;
    async fn download_file(&self, remote_path: &str) -> zbus::Result<String>;
    async fn get_storage(&self) -> zbus::Result<String>;
}

pub async fn connect() -> anyhow::Result<PDriveProxy<'static>> {
    let conn = Connection::session().await?;
    let proxy = PDriveProxy::new(&conn).await?;
    Ok(proxy)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_proxy_interface_name() {
        // Compile-time check that the proxy macro expanded correctly
        assert_eq!(
            <super::PDriveProxy as zbus::proxy::ProxyDefault>::INTERFACE,
            Some("org.protonmail.PDrive")
        );
    }
}
