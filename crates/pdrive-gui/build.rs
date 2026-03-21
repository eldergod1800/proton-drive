use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("ProtonDrive")
            .version(1, 0)
            .qml_files(["qml/main.qml", "qml/MainWindow.qml"]),
    )
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .build();
}
