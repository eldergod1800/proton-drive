slint::include_modules!();

mod dbus_client;
mod tray;

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tray::PdriveTray;
use pdrive_core::{auth::TokenStore, drive::{DriveClient, HumanVerificationRequired, TwoFactorRequired}};
use zeroize::Zeroizing;

#[derive(serde::Deserialize)]
struct BrowseEntry {
    name: String,
    is_dir: bool,
    size: String,
}

fn human_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    }
}

// ── Dark mode persistence ─────────────────────────────────────────────────────

fn dark_mode_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("pdrive")
        .join("dark_mode")
}

fn load_dark_mode() -> bool {
    std::fs::read_to_string(dark_mode_path())
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

fn save_dark_mode(dark: bool) {
    let path = dark_mode_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, if dark { "true" } else { "false" });
}

// ── Expected cache directory for downloaded files ─────────────────────────────

fn expected_cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("pdrive")
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt::init();

    // Single shared runtime for all async work — never created on the event loop thread
    let rt = Arc::new(
        tokio::runtime::Runtime::new().expect("tokio runtime"),
    );

    show_login_or_main(rt);
}

fn show_login_or_main(rt: Arc<tokio::runtime::Runtime>) {
    let token_store = TokenStore::new(TokenStore::default_path());
    let has_session = rt.block_on(async {
        token_store.load().await.unwrap_or(None).is_some()
    });

    if has_session {
        let logged_out = run_main_window(rt.clone());
        if logged_out {
            show_login_or_main(rt);
        }
        return;
    }

    let dialog = LoginDialog::new().expect("login dialog");
    let login_done = Arc::new(AtomicBool::new(false));

    let dialog_weak = dialog.as_weak();
    let login_done_clone = login_done.clone();
    let rt_login = rt.clone();

    // "Open Verification Page" fallback — only used if python3 WebView launch fails
    let hv_url_fallback: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Pending 2FA state — set when SRP + captcha succeeded but TOTP is required
    let pending_2fa: Arc<std::sync::Mutex<Option<TwoFactorRequired>>> =
        Arc::new(std::sync::Mutex::new(None));

    let hv_url_open = hv_url_fallback.clone();
    dialog.on_open_captcha_page_requested(move || {
        if let Some(ref url) = *hv_url_open.lock().unwrap() {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
    });

    let pending_2fa_login = pending_2fa.clone();

    dialog.on_login_requested(move |username, password| {
        let username = username.to_string();
        let password = Zeroizing::new(password.to_string());

        if let Some(d) = dialog_weak.upgrade() {
            d.set_busy(true);
            d.set_error_text("".into());
        }

        let dw = dialog_weak.clone();
        let done = login_done_clone.clone();
        let hv_url_store = hv_url_fallback.clone();
        let pending_2fa_store = pending_2fa_login.clone();

        rt_login.spawn(async move {
            let first_result = DriveClient::login(&username, &password).await;

            let final_result = match first_result {
                Ok(client) => Ok(client),
                Err(e) => {
                    // Check if this is a captcha challenge (HumanVerificationRequired error)
                    if let Some(hv) = e.downcast_ref::<HumanVerificationRequired>() {
                        let url = hv.0.web_url.clone();
                        *hv_url_store.lock().unwrap() = Some(url.clone());

                        let dw2 = dw.clone();
                        let url_for_ui = url.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(d) = dw2.upgrade() {
                                d.set_error_text(
                                    "Verification required — complete the captcha in the window that opened.".into()
                                );
                                d.set_show_verification(true);
                                d.set_captcha_url(url_for_ui.into());
                            }
                        });

                        // Downcast to take ownership of PendingAuth
                        match e.downcast::<HumanVerificationRequired>() {
                            Ok(hv_owned) => {
                                let pending = hv_owned.0;
                                let hv_token = pending.hv_token.clone();
                                let script = include_str!("captcha_webview.py");
                                match tokio::process::Command::new("python3")
                                    .arg("-c").arg(script)
                                    .arg(&url)
                                    .arg(&hv_token)
                                    .output().await
                                {
                                    Ok(output) => {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        for line in stderr.lines() {
                                            tracing::info!("captcha_webview: {}", line);
                                        }
                                        // Python emits one line: the combined token from pm_captcha/HUMAN_VERIFICATION_SUCCESS
                                        // Format: <HV_TOKEN>:<signature><captcha_hex>
                                        let token = String::from_utf8_lossy(&output.stdout)
                                            .lines()
                                            .find(|l| !l.trim().is_empty())
                                            .map(|l| l.trim().to_string());
                                        tracing::info!("captcha combined token: {:?}", token);
                                        if let Some(token) = token {
                                            DriveClient::login_complete_with_captcha(
                                                pending, &password, &token,
                                            ).await
                                        } else {
                                            Err(anyhow::anyhow!("Captcha window closed — please try again."))
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("python3 captcha webview failed: {}", e);
                                        Err(anyhow::anyhow!(
                                            "Could not open verification window. Use \"Open Verification Page\" below."
                                        ))
                                    }
                                }
                            }
                            Err(e) => Err(anyhow::anyhow!("{:#}", e)),
                        }
                    } else {
                        Err(e)
                    }
                }
            };

            match final_result {
                Ok(client) => {
                    let store = TokenStore::new(TokenStore::default_path());
                    match client.session_data().await {
                        Ok(session_data) => {
                            if let Err(e) = store.save_session(&session_data).await {
                                tracing::error!("failed to save session: {}", e);
                            }
                        }
                        Err(e) => tracing::error!("failed to get session data: {}", e),
                    }
                    if let Err(e) = store.save_password(password.as_str()).await {
                        tracing::error!("failed to save password: {}", e);
                    }
                    done.store(true, Ordering::Relaxed);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(d) = dw.upgrade() {
                            let _ = d.hide();
                        }
                    });
                }
                Err(e) if e.downcast_ref::<TwoFactorRequired>().is_some() => {
                    match e.downcast::<TwoFactorRequired>() {
                        Ok(tfa) => {
                            *pending_2fa_store.lock().unwrap() = Some(tfa);
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(d) = dw.upgrade() {
                                    d.set_busy(false);
                                    d.set_show_2fa(true);
                                    d.set_error_text(
                                        "Enter your two-factor authentication code.".into()
                                    );
                                }
                            });
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(d) = dw.upgrade() {
                                    d.set_busy(false);
                                    d.set_error_text(msg.into());
                                }
                            });
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(d) = dw.upgrade() {
                            d.set_busy(false);
                            d.set_error_text(msg.into());
                        }
                    });
                }
            }
        });
    });

    let dialog_weak_totp = dialog.as_weak();
    let login_done_totp = login_done.clone();
    let rt_totp = rt.clone();
    dialog.on_totp_requested(move |totp_code| {
        let totp_code = totp_code.to_string();
        let pending = pending_2fa.lock().unwrap().take();
        let Some(tfa) = pending else {
            tracing::warn!("on_totp_requested: no pending 2FA session");
            return;
        };
        let password = tfa.password.clone();

        let dw = dialog_weak_totp.clone();
        let done = login_done_totp.clone();

        let _ = slint::invoke_from_event_loop({
            let dw = dw.clone();
            move || {
                if let Some(d) = dw.upgrade() {
                    d.set_busy(true);
                    d.set_error_text("".into());
                }
            }
        });

        rt_totp.spawn(async move {
            match DriveClient::login_complete_with_2fa(tfa.session, &password, &totp_code).await {
                Ok(client) => {
                    let store = TokenStore::new(TokenStore::default_path());
                    match client.session_data().await {
                        Ok(session_data) => {
                            if let Err(e) = store.save_session(&session_data).await {
                                tracing::error!("failed to save session: {}", e);
                            }
                        }
                        Err(e) => tracing::error!("failed to get session data: {}", e),
                    }
                    if let Err(e) = store.save_password(&password).await {
                        tracing::error!("failed to save password: {}", e);
                    }
                    done.store(true, Ordering::Relaxed);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(d) = dw.upgrade() {
                            let _ = d.hide();
                        }
                    });
                }
                Err(e) => {
                    let msg = format!("{} — please try signing in again", e);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(d) = dw.upgrade() {
                            d.set_busy(false);
                            d.set_show_2fa(false);
                            d.set_error_text(msg.into());
                        }
                    });
                }
            }
        });
    });

    dialog.run().expect("dialog run");

    if login_done.load(Ordering::Relaxed) {
        let logged_out = run_main_window(rt.clone());
        if logged_out {
            show_login_or_main(rt);
        }
    }
}

/// Returns true if the user signed out (so caller should show login again).
fn run_main_window(rt: Arc<tokio::runtime::Runtime>) -> bool {
    let window = MainWindow::new().expect("main window");
    let logged_out = std::rc::Rc::new(std::cell::Cell::new(false));

    // Shutdown signal shared with all background tasks (BUG-4, BUG-9)
    let shutdown = Arc::new(AtomicBool::new(false));

    // Restore dark mode
    window.set_dark_mode(load_dark_mode());

    // Dark mode toggle
    let window_weak_dm = window.as_weak();
    window.on_toggle_dark_mode(move || {
        if let Some(w) = window_weak_dm.upgrade() {
            let new_dark = !w.get_dark_mode();
            w.set_dark_mode(new_dark);
            save_dark_mode(new_dark);
        }
    });

    // Tray icon (BUG-9: shutdown signal lets thread exit cleanly)
    let window_weak_tray = window.as_weak();
    let shutdown_tray = shutdown.clone();
    std::thread::spawn(move || {
        let service = ksni::TrayService::new(PdriveTray {
            on_open: Box::new(move || {
                let ww = window_weak_tray.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww.upgrade() {
                        w.show().unwrap();
                    }
                });
            }),
            on_pause: Box::new(|| tracing::info!("pause sync")),
            on_resume: Box::new(|| tracing::info!("resume sync")),
            on_quit: Box::new(|| {
                let _ = slint::invoke_from_event_loop(|| slint::quit_event_loop().unwrap());
            }),
        });
        // ksni blocks until the StatusNotifierItem is unregistered; the shutdown
        // flag is checked below but ksni has no external stop handle, so the thread
        // will linger until the tray icon is removed by the compositor on process exit.
        let _ = service.run();
        shutdown_tray.store(true, Ordering::Relaxed);
    });

    // Minimize to tray on close
    let window_weak_close = window.as_weak();
    window.window().on_close_requested(move || {
        if let Some(w) = window_weak_close.upgrade() {
            w.hide().unwrap();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });

    // Channels: GUI → background thread
    let (browse_tx, mut browse_rx) = tokio::sync::mpsc::channel::<String>(16);
    let (open_tx, mut open_rx) = tokio::sync::mpsc::channel::<String>(8);

    let window_weak_bg = window.as_weak();
    let shutdown_bg = shutdown.clone();

    rt.spawn(async move {
        match dbus_client::connect().await {
            Ok(proxy) => {
                tracing::info!("connected to pdrive-daemon D-Bus");
                let ww = window_weak_bg.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww.upgrade() {
                        w.set_daemon_status("running".into());
                    }
                });

                // Fetch storage quota once on connect
                if let Ok(json) = proxy.get_storage().await {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                        if let (Some(used), Some(total)) = (
                            v["used"].as_u64(),
                            v["total"].as_u64(),
                        ) {
                            let ratio = if total > 0 { used as f32 / total as f32 } else { 0.0 };
                            let ww = window_weak_bg.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = ww.upgrade() {
                                    w.set_storage_used(human_size(used).into());
                                    w.set_storage_total(human_size(total).into());
                                    w.set_storage_ratio(ratio);
                                }
                            });
                        }
                    }
                }

                // File-open task (BUG-6: Rc created inside closure, on event loop thread)
                let proxy2 = proxy.clone();
                let ww_open = window_weak_bg.clone();
                let shutdown_open = shutdown_bg.clone();
                tokio::spawn(async move {
                    while let Some(remote_path) = open_rx.recv().await {
                        if shutdown_open.load(Ordering::Relaxed) { break; }

                        let name = remote_path.rsplit('/').next().unwrap_or(&remote_path).to_string();
                        let name_dl = name.clone();
                        let ww = ww_open.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_status_text(format!("Downloading {}...", name_dl).into());
                            }
                        });

                        match proxy2.download_file(&remote_path).await {
                            Ok(local_path) if !local_path.is_empty() => {
                                // SEC-4: validate path is inside expected cache dir
                                let path = std::path::Path::new(&local_path);
                                let cache_dir = expected_cache_dir();
                                let safe = path.is_absolute()
                                    && !local_path.contains("://")
                                    && path.starts_with(&cache_dir);

                                if safe {
                                    tracing::info!("opening {} with xdg-open", local_path);
                                    let _ = std::process::Command::new("xdg-open")
                                        .arg(&local_path)
                                        .spawn();
                                    let name2 = name.clone();
                                    let ww2 = ww_open.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(w) = ww2.upgrade() {
                                            w.set_status_text(format!("Opened {}", name2).into());
                                        }
                                    });
                                } else {
                                    tracing::warn!("blocked unsafe path from daemon: {}", local_path);
                                    let ww3 = ww_open.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(w) = ww3.upgrade() {
                                            w.set_status_text("Blocked unsafe file path".into());
                                        }
                                    });
                                }
                            }
                            _ => {
                                let ww4 = ww_open.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = ww4.upgrade() {
                                        w.set_status_text("Failed to open file".into());
                                    }
                                });
                            }
                        }
                    }
                });

                // Browse loop
                while let Some(path) = browse_rx.recv().await {
                    if shutdown_bg.load(Ordering::Relaxed) { break; }

                    match proxy.browse_directory(&path).await {
                        Ok(json) => {
                            let entries: Vec<BrowseEntry> =
                                serde_json::from_str(&json).unwrap_or_default();
                            let count = entries.len();
                            let ww = window_weak_bg.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = ww.upgrade() {
                                    let items: Vec<FileEntry> = entries
                                        .iter()
                                        .map(|e| FileEntry {
                                            name: e.name.clone().into(),
                                            is_dir: e.is_dir,
                                            size: e.size.clone().into(),
                                        })
                                        .collect();
                                    // Rc created here, on the event loop thread (BUG-6 safe)
                                    let model = std::rc::Rc::new(slint::VecModel::from(items));
                                    w.set_file_entries(model.into());
                                    w.set_status_text(
                                        format!("{} item{}", count, if count == 1 { "" } else { "s" }).into()
                                    );
                                }
                            });
                        }
                        Err(e) => tracing::warn!("browse failed: {}", e),
                    }
                }
            }
            Err(e) => {
                tracing::warn!("daemon not available: {}", e);
                drop(browse_rx);
                drop(open_rx);
                let ww = window_weak_bg.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww.upgrade() {
                        w.set_daemon_status("unavailable".into());
                        w.set_status_text("Daemon not running — start pdrive-daemon".into());
                    }
                });
            }
        }
    });

    // Sidebar / folder browse (BUG-2: show "unavailable" if daemon not connected)
    let window_weak = window.as_weak();
    let browse_tx2 = browse_tx.clone();
    window.on_browse_requested(move |path| {
        tracing::info!("browse: {}", path);
        if let Err(e) = browse_tx2.try_send(path.to_string()) {
            tracing::warn!("browse_tx send failed: {}", e);
        }
        if let Some(w) = window_weak.upgrade() {
            w.set_file_entries(Default::default());
            w.set_current_path(path.clone());
            w.set_status_text("Loading...".into());
        }
    });

    // Back navigation (BUG-5: trim trailing slash before rfind)
    let window_weak2 = window.as_weak();
    window.on_navigate_up(move || {
        if let Some(w) = window_weak2.upgrade() {
            let path = w.get_current_path().to_string();
            let path = path.trim_end_matches('/');
            let parent = if let Some(pos) = path.rfind('/') {
                if pos == 0 { "/".to_string() } else { path[..pos].to_string() }
            } else {
                "/".to_string()
            };
            let _ = browse_tx.try_send(parent.clone());
            w.set_file_entries(Default::default());
            w.set_current_path(parent.into());
            w.set_status_text("Loading...".into());
        }
    });

    // File open
    let window_weak_open = window.as_weak();
    window.on_open_file_requested(move |path| {
        let path_str = path.to_string();
        let short = path_str.rsplit('/').next().unwrap_or(&path_str).to_string();
        if let Err(e) = open_tx.try_send(path_str) {
            tracing::warn!("open_tx send failed: {}", e);
        }
        if let Some(w) = window_weak_open.upgrade() {
            w.set_status_text(format!("Opening {}...", short).into());
        }
    });

    // New / upload stubs
    window.on_new_folder_requested(|| tracing::info!("new folder requested"));
    window.on_new_file_requested(|| tracing::info!("new file requested"));
    window.on_upload_files_requested(|| tracing::info!("upload files requested"));
    window.on_upload_folder_requested(|| tracing::info!("upload folder requested"));

    // Logout
    let logged_out_clone = logged_out.clone();
    let shutdown_logout = shutdown.clone();
    window.on_logout_requested(move || {
        let store = TokenStore::new(TokenStore::default_path());
        let _ = store.clear();
        logged_out_clone.set(true);
        shutdown_logout.store(true, Ordering::Relaxed);
        let _ = slint::invoke_from_event_loop(|| slint::quit_event_loop().unwrap());
    });

    window.run().expect("main window run");
    logged_out.get()
}
