use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("io.lipa").qml_files([
        "qml/main.qml",
        "qml/SettingsWindow.qml",
        "qml/RegionSelector.qml",
        "qml/TranslationOverlay.qml",
        "qml/HotkeyButton.qml",
        "qml/FrameOverlay.qml",
        "qml/ErrorPattern.qml",
        "qml/RegionFrame.qml",
        "qml/InplaceText.qml",
        "qml/HistoryWindow.qml",
    ]))
    .files(["src/bridge.rs", "src/icon.rs", "src/logging.rs"])
    .include_dir("src")
    .qrc("assets.qrc");
    // KWindowEffects: compositor blur behind the translation overlay.
    // SAFETY: only adds an include path; does not change flags cxx-qt relies on.
    let builder = unsafe { builder.cc_builder(|cc| {
        cc.include("/usr/include/KF6/KWindowSystem");
        cc.file("src/logging.cpp");
    }) };
    println!("cargo:rerun-if-changed=src/logging.cpp");
    builder.build();
    println!("cargo:rustc-link-lib=KF6WindowSystem");
}
