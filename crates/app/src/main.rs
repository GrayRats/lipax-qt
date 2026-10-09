mod bridge;
mod model_worker;
mod icon;
mod instance;
mod logging;
mod notify;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};

/// Стиль Qt Quick по теме из настроек (`general.theme`). Стиль выбирается до создания приложения и потом не меняется;
/// тёмная и светлая темы — это стиль Universal, они переключаются на лету через `Universal.theme` в QML.
/// Переменные окружения, выставленные пользователем (например, `QT_QUICK_CONTROLS_STYLE`), не перезаписываются.
fn apply_style(theme: &str) {
    let vars: &[(&str, &str)] = match theme {
        "light" => &[("QT_QUICK_CONTROLS_STYLE", "Universal"), ("QT_QUICK_CONTROLS_UNIVERSAL_THEME", "Light"), ("QT_QUICK_CONTROLS_UNIVERSAL_ACCENT", "Teal")],
        "fusion" => &[("QT_QUICK_CONTROLS_STYLE", "Fusion")],
        "breeze" => &[("QT_QUICK_CONTROLS_STYLE", "org.kde.breeze")],
        // The desktop's own choice: Qt picks the style from the platform theme.
        "system" => &[],
        _ => &[("QT_QUICK_CONTROLS_STYLE", "Universal"), ("QT_QUICK_CONTROLS_UNIVERSAL_THEME", "Dark"), ("QT_QUICK_CONTROLS_UNIVERSAL_ACCENT", "Teal")],
    };
    for (key, value) in vars {
        if std::env::var_os(key).is_none() {
            // SAFETY: вызывается в начале main, пока потоков, читающих окружение, ещё нет.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

/// What the «General» tab asks for at start: the log level (unless `RUST_LOG` says otherwise), the autostart entry.
fn apply_general(general: &lipa_core::settings::GeneralSettings) {
    if !logging::level_from_environment() && general.log_level != "info" {
        let _ = logging::set_level(&general.log_level);
    }
    if general.autostart
        && let Some(dir) = lipa_core::autostart::default_dir().filter(|d| !lipa_core::autostart::is_enabled(d))
        && let Ok(exe) = std::env::current_exe()
        && let Err(e) = lipa_core::autostart::set(&dir, true, &exe.to_string_lossy()) {
        tracing::warn!(component = "autostart", error = %e, "запись автозапуска не восстановлена");
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
    let general = lipa_core::settings::Settings::load().general;
    apply_style(&general.theme);
    apply_general(&general);
    icon::create_application();
    icon::configure();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/io/lipa/qml/main.qml"));
    }
    #[cfg(feature = "lifecycle-test")]
    if let (Some(delay), Ok(patch)) = (std::env::var("LIPAX_TEST_SWITCH_AFTER_MS").ok().and_then(|v| v.parse::<i32>().ok()), std::env::var("LIPAX_TEST_SWITCH_PATCH")) {
        icon::test_switch_after(delay, &patch);
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
