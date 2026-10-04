mod bridge;
mod icon;
mod logging;

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
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Запуск LipaX");
    apply_dark_style();
    let mut app = QGuiApplication::new();
    icon::configure();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/io/lipa/qml/main.qml"));
    }
    #[cfg(feature = "lifecycle-test")]
    if let Some(delay_ms) = std::env::var("LIPAX_TEST_CLOSE_AFTER_MS").ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|delay| (1..=30_000).contains(delay)) {
        icon::test_close_after(delay_ms);
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
    bridge::shutdown();
    tracing::info!("LipaX завершён");
}
