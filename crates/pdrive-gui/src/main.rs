slint::include_modules!();

mod dbus_client;
mod tray;

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tray::PdriveTray;
use pdrive_core::{auth::TokenStore, drive::{DriveClient, HumanVerificationRequired, TwoFactorRequired}};
use pdrive_core::NodeUid;
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

/// Build breadcrumb segments for a given path.
fn breadcrumb_for_path(path: &str) -> Vec<BreadcrumbSegment> {
    let (root_label, root_path) = if path == "/computers" || path.starts_with("/computers/") {
        ("Computers", "/computers")
    } else {
        ("My Files", "/")
    };

    let mut segments = vec![BreadcrumbSegment {
        label: root_label.into(),
        path: root_path.into(),
    }];

    // Strip the root prefix to get the relative portion
    let rel = if root_path == "/computers" {
        path.strip_prefix("/computers").unwrap_or("").trim_start_matches('/')
    } else {
        path.trim_start_matches('/')
    };

    if rel.is_empty() {
        return segments;
    }

    let mut current = if root_path == "/computers" {
        "/computers".to_string()
    } else {
        String::new()
    };
    for part in rel.split('/') {
        if !part.is_empty() {
            current.push('/');
            current.push_str(part);
            segments.push(BreadcrumbSegment {
                label: part.into(),
                path: current.clone().into(),
            });
        }
    }
    segments
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,pgp=error"))
        )
        .init();

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
        // Load drive client once from keyring — tokens are fresh on startup
        let drive = rt.block_on(async {
            let store = TokenStore::new(TokenStore::default_path());
            match (store.load_session().await, store.load_password().await) {
                (Ok(Some(session)), Ok(Some(password))) => {
                    match DriveClient::from_stored(&session, &password).await {
                        Ok(client) => {
                            tracing::info!("startup: session loaded for {}", session.username);
                            Some(Arc::new(client))
                        }
                        Err(e) => {
                            tracing::warn!("startup: failed to restore session: {}", e);
                            None
                        }
                    }
                }
                _ => None,
            }
        });
        // If the session could not be restored (expired tokens), fall through to login
        if let Some(d) = drive {
            let logged_out = run_main_window(rt.clone(), Some(d));
            if logged_out {
                show_login_or_main(rt);
            }
            return;
        }
        tracing::info!("session restore failed — showing login");
        // fall through to login dialog below
    }

    let dialog = LoginDialog::new().expect("login dialog");
    dialog.set_dark_mode(load_dark_mode());
    let login_done = Arc::new(AtomicBool::new(false));
    // Holds the DriveClient from a successful login so run_main_window can use it directly
    let fresh_client: Arc<std::sync::Mutex<Option<Arc<DriveClient>>>> =
        Arc::new(std::sync::Mutex::new(None));

    let dialog_weak = dialog.as_weak();
    let login_done_clone = login_done.clone();
    let fresh_client_login = fresh_client.clone();
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
        let fresh_client_store = fresh_client_login.clone();

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
                                // Find a python3 that has the WebKit2/gi bindings.
                                // Probe each candidate; PATH-based "python3" may be a
                                // venv/conda install without system GTK bindings.
                                let probe = "import gi; gi.require_version('WebKit2','4.1'); \
                                             from gi.repository import WebKit2, Gtk";
                                let python_bin = [
                                    "/usr/bin/python3",
                                    "/usr/local/bin/python3",
                                    "python3",
                                    "python",
                                ]
                                .iter()
                                .find(|&&p| {
                                    std::process::Command::new(p)
                                        .args(["-c", probe])
                                        .output()
                                        .map(|o| o.status.success())
                                        .unwrap_or(false)
                                })
                                .copied()
                                .unwrap_or("python3");
                                tracing::info!("captcha_webview: using python={}", python_bin);
                                // Write a Rust-side diagnostic log so failures are visible
                                // even when pdrive is not started from a terminal.
                                let rust_log_path = std::env::var("HOME")
                                    .map(|h| format!("{}/pdrive-captcha-rust.log", h))
                                    .unwrap_or_else(|_| "/tmp/pdrive-captcha-rust.log".into());
                                let _ = std::fs::write(&rust_log_path, format!(
                                    "python_bin={python_bin}\nurl={url}\nhv_token_len={}\n",
                                    hv_token.len()
                                ));
                                match tokio::process::Command::new(python_bin)
                                    .arg("-c").arg(script)
                                    .arg(&url)
                                    .arg(&hv_token)
                                    .output().await
                                {
                                    Ok(output) => {
                                        let stderr_str = String::from_utf8_lossy(&output.stderr);
                                        for line in stderr_str.lines() {
                                            tracing::info!("captcha_webview: {}", line);
                                        }
                                        let _ = std::fs::write(&rust_log_path, format!(
                                            "python_bin={python_bin}\nexit={:?}\nstdout_bytes={}\nstderr=\n{}",
                                            output.status.code(),
                                            output.stdout.len(),
                                            stderr_str,
                                        ));
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
                                        let _ = std::fs::write(&rust_log_path, format!(
                                            "python_bin={python_bin}\nspawn_error={e}\n"
                                        ));
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
                    // Share the live client so run_main_window can use it directly
                    *fresh_client_store.lock().unwrap() = Some(Arc::new(client));
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
    let fresh_client_totp = fresh_client.clone();
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
        let fresh_client_store = fresh_client_totp.clone();

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
                    *fresh_client_store.lock().unwrap() = Some(Arc::new(client));
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
        let drive = fresh_client.lock().unwrap().take();
        let logged_out = run_main_window(rt.clone(), drive);
        if logged_out {
            show_login_or_main(rt);
        }
    }
}

/// Returns true if the user signed out (so caller should show login again).
fn run_main_window(rt: Arc<tokio::runtime::Runtime>, drive: Option<Arc<DriveClient>>) -> bool {
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

    // Clone for background task to trigger initial auto-browse
    let browse_tx_bg = browse_tx.clone();

    let window_weak_bg = window.as_weak();
    let shutdown_bg = shutdown.clone();

    // In-process path cache shared between browse and download tasks
    let path_cache: Arc<tokio::sync::Mutex<HashMap<String, NodeUid>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::<String, NodeUid>::new()));

    rt.spawn({
        let drive = drive.clone();
        let path_cache = path_cache.clone();
        async move {
            // Check daemon status for display only
            match dbus_client::connect().await {
                Ok(_) => {
                    let ww = window_weak_bg.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_daemon_status("running".into());
                        }
                    });
                }
                Err(_) => {
                    let ww = window_weak_bg.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_daemon_status("unavailable".into());
                        }
                    });
                }
            }

            if drive.is_none() {
                let ww = window_weak_bg.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww.upgrade() {
                        w.set_status_text("Not signed in — please restart and log in".into());
                    }
                });
                drop(browse_rx);
                drop(open_rx);
                return;
            }

            // Fetch storage quota directly from live DriveClient
            if let Some(ref d) = drive {
                match d.get_user_quota().await {
                    Ok((used, total)) => {
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
                    Err(e) => tracing::warn!("get_user_quota failed: {}", e),
                }
            }

            // Auto-browse root on startup
            let root_segs = breadcrumb_for_path("/");
            let ww_init = window_weak_bg.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = ww_init.upgrade() {
                    w.set_current_path("/".into());
                    w.set_status_text("Loading...".into());
                    w.set_breadcrumb_segments(
                        slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(root_segs)))
                    );
                }
            });
            let _ = browse_tx_bg.send("/".to_string()).await;

            // File-open task
            let drive_open = drive.clone();
            let path_cache_open = path_cache.clone();
            let ww_open = window_weak_bg.clone();
            let shutdown_open = shutdown_bg.clone();
            tokio::spawn(async move {
                while let Some(remote_path) = open_rx.recv().await {
                    if shutdown_open.load(Ordering::Relaxed) { break; }

                    let name = remote_path.rsplit('/').next().unwrap_or(&remote_path).to_string();

                    let node_uid = path_cache_open.lock().await.get(&remote_path).cloned();
                    let Some(node_uid) = node_uid else {
                        tracing::warn!("download: path not in cache: {}", remote_path);
                        let ww = ww_open.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_status_text("File not in cache — browse folder first".into());
                            }
                        });
                        continue;
                    };

                    let filename = remote_path.rsplit('/').next().unwrap_or("file").to_string();
                    // Validate filename is a plain name with no path components
                    let safe_name = match std::path::Path::new(&filename).components().collect::<Vec<_>>().as_slice() {
                        [std::path::Component::Normal(_)] => filename.clone(),
                        _ => {
                            tracing::warn!("download: invalid filename '{}'", filename);
                            continue;
                        }
                    };

                    let cache_dir = expected_cache_dir();
                    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                        tracing::warn!("download: could not create cache dir: {}", e);
                        continue;
                    }
                    let dest = cache_dir.join(&safe_name);
                    if !dest.is_absolute() || !dest.starts_with(&cache_dir) {
                        tracing::warn!("download: blocked unsafe path");
                        continue;
                    }

                    if let Some(ref d) = drive_open {
                        let name_dl = name.clone();
                        let ww = ww_open.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_status_text(format!("Downloading {}...", name_dl).into());
                            }
                        });

                        match d.download(node_uid, &dest).await {
                            Ok(()) => {
                                let local_path = dest.to_string_lossy().to_string();
                                tracing::info!("opening {} with xdg-open", local_path);
                                let _ = std::process::Command::new("xdg-open").arg(&local_path).spawn();
                                let name2 = name.clone();
                                let ww2 = ww_open.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = ww2.upgrade() {
                                        w.set_status_text(format!("Opened {}", name2).into());
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!("download failed: {}", e);
                                let ww4 = ww_open.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = ww4.upgrade() {
                                        w.set_status_text("Failed to open file".into());
                                    }
                                });
                            }
                        }
                    }
                }
            });

            // Browse loop — uses DriveClient directly, no daemon required
            while let Some(path) = browse_rx.recv().await {
                if shutdown_bg.load(Ordering::Relaxed) { break; }

                let Some(ref d) = drive else { break; };

                let entries_and_uids: Vec<(pdrive_core::drive::DriveEntry, NodeUid)> =
                    if path == "/" || path.is_empty() {
                        match d.list_root().await {
                            Ok((entries, root_uid)) => {
                                path_cache.lock().await.insert("/".to_string(), root_uid);
                                entries
                            }
                            Err(e) => {
                                tracing::warn!("list_root failed: {}", e);
                                vec![]
                            }
                        }
                    } else if path == "/computers" || path == "/sync" {
                        match d.list_devices().await {
                            Ok(entries) => entries,
                            Err(e) => {
                                tracing::warn!("list_devices failed: {}", e);
                                vec![]
                            }
                        }
                    } else {
                        let uid = path_cache.lock().await.get(&path).cloned();
                        match uid {
                            Some(uid) => match d.list_folder(uid).await {
                                Ok(entries) => entries,
                                Err(e) => {
                                    tracing::warn!("list_folder failed for {}: {}", path, e);
                                    vec![]
                                }
                            },
                            None => {
                                tracing::warn!("browse: path not in cache: {}", path);
                                vec![]
                            }
                        }
                    };

                // Cache child paths for subsequent navigation and downloads
                {
                    let mut cache = path_cache.lock().await;
                    for (entry, uid) in &entries_and_uids {
                        let child_path = if path == "/" || path.is_empty() {
                            format!("/{}", entry.name)
                        } else {
                            format!("{}/{}", path.trim_end_matches('/'), entry.name)
                        };
                        cache.insert(child_path, uid.clone());
                    }
                }

                let entries: Vec<BrowseEntry> = entries_and_uids.iter().map(|(e, _)| BrowseEntry {
                    name: e.name.clone(),
                    is_dir: e.is_dir,
                    size: e.size.map(human_size).unwrap_or_else(|| "--".to_string()),
                }).collect();

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
        }
    });

    // Sidebar / folder browse
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
            let segs = breadcrumb_for_path(&path);
            w.set_breadcrumb_segments(
                slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(segs)))
            );
        }
    });

    // Back navigation
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
            w.set_current_path(parent.clone().into());
            w.set_status_text("Loading...".into());
            let segs = breadcrumb_for_path(&parent);
            w.set_breadcrumb_segments(
                slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(segs)))
            );
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
