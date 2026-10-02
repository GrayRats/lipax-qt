mod bridge;
mod icon;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

/// Тёмное оформление всех окон: стиль Universal с тёмной темой. Переменные заданы по умолчанию,
/// явно выставленные пользователем (например, `QT_QUICK_CONTROLS_STYLE`) не перезаписываются.
fn apply_dark_style() {
    for (key, value) in [
        ("QT_QUICK_CONTROLS_STYLE", "Universal"),
        ("QT_QUICK_CONTROLS_UNIVERSAL_THEME", "Dark"),
        ("QT_QUICK_CONTROLS_UNIVERSAL_ACCENT", "Teal"),
    ] {
        if std::env::var_os(key).is_none() {
            // SAFETY: вызывается в начале main, пока потоков, читающих окружение, ещё нет.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

fn main() {
    apply_dark_style();
    let mut app = QGuiApplication::new();
    icon::configure();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/io/lipa/qml/main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
    bridge::shutdown();
}
