use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("io.lipa").qml_files([
        "qml/main.qml",
        "qml/SettingsWindow.qml",
        "qml/RegionSelector.qml",
        "qml/TranslationOverlay.qml",
        "qml/HotkeyButton.qml",
        "qml/FrameOverlay.qml",
    ]))
    .files(["src/bridge.rs"])
    .build();
}
