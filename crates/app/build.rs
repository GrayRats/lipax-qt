use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::{fs, path::Path, process::Command};

/// The repository stores the bundled fonts compressed (`assets/fonts/*.xz`); the application loads
/// the plain files. Unpacking happens here, so a clean checkout builds without any download.
/// A missing `xz` or a damaged archive is reported as a build warning; at run time the missing
/// families are logged as errors and the in-place mode skips fields it cannot render.
fn unpack_fonts() {
    let dir = Path::new("assets/fonts");
    println!("cargo:rerun-if-changed=assets/fonts");
    let Ok(entries) = fs::read_dir(dir) else { return };
    for archive in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "xz")) {
        let target = archive.with_extension("");
        let current = fs::metadata(&target).and_then(|t| t.modified()).ok()
            .zip(fs::metadata(&archive).and_then(|a| a.modified()).ok())
            .is_some_and(|(plain, packed)| plain >= packed);
        if current { continue; }
        let partial = target.with_extension("partial");
        let status = fs::File::create(&partial).and_then(|out| Command::new("xz").arg("-dc").arg(&archive).stdout(out).status());
        match status {
            Ok(s) if s.success() => { let _ = fs::rename(&partial, &target); }
            other => {
                let _ = fs::remove_file(&partial);
                println!("cargo:warning=cannot unpack {}: {other:?} (install `xz`)", archive.display());
            }
        }
    }
}

fn main() {
    // Use exactly the discovery implementation used by CXX-Qt (including QMAKE).
    let qt = qt_build_utils::QtBuild::new(vec!["Core".into()]).expect("System Qt 6.12+ is required");
    let version = qt.version();
    assert!(version.major == 6 && version.minor >= 12,
        "Unsupported Qt {version}: LipaX requires system Qt >= 6.12 and < 7");
    println!("cargo:rustc-env=LIPAX_BUILD_QT_VERSION={version}");
    println!("cargo:warning=Building LipaX with system Qt {version}");
    println!("cargo:rerun-if-env-changed=QMAKE");
    unpack_fonts();
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("io.lipa").qml_files([
        "qml/main.qml",
        "qml/SettingsWindow.qml",
        "qml/UnavailableHint.qml",
        "qml/HoverHint.qml",
        "qml/UiTheme.qml",
        "qml/ActionButton.qml",
        "qml/AppearancePreview.qml",
        "qml/InplaceTranslationContent.qml",
        "qml/RegionSelector.qml",
        "qml/TranslationWindow.qml",
        "qml/TranslationWindowContent.qml",
        "qml/PinnedTranslationSurface.qml",
        "qml/FloatingTranslationSurface.qml",
        "qml/HotkeyButton.qml",
        "qml/FrameOverlay.qml",
        "qml/ErrorPattern.qml",
        "qml/RegionFrame.qml",
        "qml/InplaceTranslation.qml",
        "qml/HistoryWindow.qml",
        "qml/OcrPreviewWindow.qml",
    ]))
    // Widgets: the KDE platform theme builds the tray icon from QWidgets, so the app is a QApplication.
    .qt_module("Widgets")
    .qt_module("QuickControls2")
    .qt_module("Quick")
    .files(["src/bridge.rs", "src/icon.rs", "src/logging.rs", "src/model_worker.rs"])
    .include_dir("src")
    .qrc("assets.qrc");
    // KWindowEffects: compositor blur behind the translation overlay.
    // SAFETY: only adds an include path; does not change flags cxx-qt relies on.
    let builder = unsafe { builder.cc_builder(|cc| {
        cc.include("/usr/include/KF6/KWindowSystem");
        cc.file("src/logging.cpp");
        cc.file("src/model_worker.cpp");
    }) };
    println!("cargo:rerun-if-changed=src/logging.cpp");
    println!("cargo:rerun-if-changed=src/model_worker.cpp");
    builder.build();
    println!("cargo:rustc-link-lib=KF6WindowSystem");
}
