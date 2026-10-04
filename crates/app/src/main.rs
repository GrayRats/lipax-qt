mod bridge;
mod icon;
mod instance;
mod logging;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};

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
    let action = match instance::parse_args(std::env::args().skip(1)) {
        Ok(instance::Cli::Run(action)) => action,
        Ok(instance::Cli::Help) => return print!("{}", instance::usage()),
        Ok(instance::Cli::Version) => return println!("LipaX {}", env!("CARGO_PKG_VERSION")),
        Err(error) => {
            eprintln!("lipax: {error}\n\n{}", instance::usage());
            std::process::exit(2);
        }
    };
    logging::init();
    // Второй запуск только передаёт команду уже работающему экземпляру и завершается.
    match instance::acquire(action) {
        instance::Startup::Primary => {}
        instance::Startup::Forwarded | instance::Startup::NothingToDo => return,
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Запуск LipaX");
    apply_dark_style();
    icon::create_application();
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
    icon::exec_application();
    // Выход: event loop закончился (QML уже остановил слежение и закрыл окна, значок трея скрыт).
    // Сначала останавливаются задачи backend (захват, OCR, перевод, portal, KWin-скрипт),
    // затем уничтожаются QML-движок с окнами, значком трея и Controller, и только потом — приложение.
    bridge::shutdown();
    drop(engine);
    icon::destroy_application();
    tracing::info!("LipaX завершён");
}
