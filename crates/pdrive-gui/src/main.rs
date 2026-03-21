slint::include_modules!();

mod dbus_client;

fn main() {
    tracing_subscriber::fmt::init();

    let window = MainWindow::new().expect("failed to create main window");

    // Channel: GUI thread → tokio thread for browse requests
    let (browse_tx, mut browse_rx) = tokio::sync::mpsc::channel::<String>(16);

    // Spawn tokio runtime in background thread
    let window_weak_bg = window.as_weak();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            // Try to connect to daemon
            match dbus_client::connect().await {
                Ok(proxy) => {
                    tracing::info!("connected to pdrive-daemon D-Bus");

                    // Update daemon status in UI
                    let ww = window_weak_bg.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_daemon_status("running".into());
                        }
                    });

                    // Handle browse requests
                    while let Some(path) = browse_rx.recv().await {
                        match proxy.browse_directory(&path).await {
                            Ok(json) => {
                                tracing::info!("browse result: {}", json);
                                // TODO Task 11: parse JSON and populate file-entries
                            }
                            Err(e) => tracing::warn!("browse failed: {}", e),
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("daemon not available: {}", e);
                    // Drop browse_rx so try_send on the GUI side returns Err
                    // and browse requests aren't silently queued while offline
                    drop(browse_rx);
                    let ww = window_weak_bg.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_daemon_status("unavailable".into());
                        }
                    });
                }
            }
        });
    });

    // Wire up browse callback: send path to tokio thread
    let window_weak = window.as_weak();
    window.on_browse_requested(move |path| {
        tracing::info!("browse requested: {}", path);
        let _ = browse_tx.try_send(path.to_string());
        if let Some(w) = window_weak.upgrade() {
            w.set_status_text(format!("Browsing: {}", path).into());
        }
    });

    window.on_upload_clicked(|| {
        tracing::info!("upload clicked");
        // TODO: file dialog in Task 11
    });

    window.run().expect("failed to run main window");
}
