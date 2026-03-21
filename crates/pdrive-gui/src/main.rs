// cxxqt_object module defines the AppController QObject bridge.
// NOTE: cxx-qt 0.8.1 is incompatible with rustc >=1.87 due to a proc-macro
// issue with `include!` in foreign-item position in `#[cxx_qt::bridge]`.
// The bridge definition is kept in cxxqt_object.rs for when the cxx-qt
// upstream fixes this (tracked at https://github.com/KDAB/cxx-qt/issues/).
// mod cxxqt_object;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    tracing_subscriber::fmt::init();

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/ProtonDrive/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
