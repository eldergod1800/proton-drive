slint::include_modules!();

mod dbus_client;

use pdrive_core::{auth::TokenStore, drive::DriveClient};

fn main() {
    tracing_subscriber::fmt::init();

    let token_store = TokenStore::new(TokenStore::default_path());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let has_session = rt.block_on(async {
        token_store.load().await.unwrap_or(None).is_some()
    });

    if has_session {
        run_main_window();
        return;
    }

    // Show login dialog
    let dialog = LoginDialog::new().expect("login dialog");
    let main_shown = std::rc::Rc::new(std::cell::Cell::new(false));

    let dialog_weak = dialog.as_weak();
    let main_shown_clone = main_shown.clone();
    dialog.on_login_requested(move |username, password| {
        let username = username.to_string();
        let password = password.to_string();
        let dw = dialog_weak.clone();
        let ms = main_shown_clone.clone();

        if let Some(d) = dw.upgrade() {
            d.set_busy(true);
            d.set_error_text("".into());
        }

        let rt2 = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match rt2.block_on(DriveClient::login(&username, &password)) {
            Ok(client) => {
                let token = client
                    .session_token()
                    .unwrap_or_else(|| "stub-token".to_string());
                let store = TokenStore::new(TokenStore::default_path());
                let _ = rt2.block_on(store.save(&token));
                ms.set(true);
                if let Some(d) = dw.upgrade() {
                    let _ = d.hide();
                }
            }
            Err(e) => {
                if let Some(d) = dw.upgrade() {
                    d.set_busy(false);
                    d.set_error_text(format!("Login failed: {}", e).into());
                }
            }
        }
    });

    dialog.run().expect("dialog run");

    if main_shown.get() {
        run_main_window();
    }
}

fn run_main_window() {
    let window = MainWindow::new().expect("main window");

    let (browse_tx, mut browse_rx) = tokio::sync::mpsc::channel::<String>(16);

    let window_weak_bg = window.as_weak();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            match dbus_client::connect().await {
                Ok(proxy) => {
                    tracing::info!("connected to pdrive-daemon D-Bus");
                    let ww = window_weak_bg.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_daemon_status("running".into());
                        }
                    });
                    while let Some(path) = browse_rx.recv().await {
                        match proxy.browse_directory(&path).await {
                            Ok(json) => tracing::info!("browse result: {}", json),
                            Err(e) => tracing::warn!("browse failed: {}", e),
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("daemon not available: {}", e);
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
    });

    window.run().expect("main window run");
}
