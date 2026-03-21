slint::include_modules!();

fn main() {
    tracing_subscriber::fmt::init();

    let window = MainWindow::new().expect("failed to create main window");

    // Wire up callbacks
    let window_weak = window.as_weak();
    window.on_browse_requested(move |path| {
        tracing::info!("browse requested: {}", path);
        // TODO: call D-Bus in Task 10
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
