//! One synchronous console backend for Rust and Qt, installed before Qt/Tokio start.
use std::{backtrace::Backtrace, io::IsTerminal};
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt::writer::MakeWriterExt};

#[cxx::bridge(namespace = "lipax")]
mod ffi {
    unsafe extern "C++" {
        include!("logging.h");
        fn installQtLogHandler();
        fn emitQtLogProbe(fatal: bool);
        fn emitQmlLogProbe();
    }
    extern "Rust" {
        #[cxx_name = "logQtMessage"]
        fn log_qt_message(level: i32, category: &str, message: &str, file: &str, line: i32);
    }
}

pub fn init() {
    let requested = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let (filter, invalid) = match EnvFilter::try_new(&requested) {
        Ok(filter) => (filter, false),
        Err(_) => (EnvFilter::new("info"), true),
    };
    // Fatal diagnostics must survive even RUST_LOG=off.
    let filter = filter
        .add_directive("lipa::panic=error".parse().unwrap())
        .add_directive("qt::fatal=error".parse().unwrap());
    let writer = std::io::stderr
        .with_max_level(Level::ERROR)
        .or_else(std::io::stdout);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(std::io::stdout().is_terminal() && std::io::stderr().is_terminal())
        .with_target(true)
        .try_init()
        .expect("initialize console logging");
    std::panic::set_hook(Box::new(|panic| {
        let reason = panic
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic с неизвестным типом сообщения");
        let location = panic
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        tracing::error!(target: "lipa::panic", %location, thread = ?std::thread::current().name(),
            "Rust panic: {reason}\nBacktrace:\n{}", Backtrace::force_capture());
    }));
    ffi::installQtLogHandler();
    if invalid {
        tracing::warn!("Некорректный RUST_LOG; используется уровень INFO");
    }
    tracing::debug!("Журнал Rust и Qt подключён");
}

fn log_qt_message(level: i32, category: &str, message: &str, file: &str, line: i32) {
    // QtMsgType numeric values are stable: Debug=0, Warning=1, Critical=2, Fatal=3, Info=4.
    match level {
        0 => tracing::debug!(target: "qt", component = category, %file, line, "{message}"),
        1 => tracing::warn!(target: "qt", component = category, %file, line, "{message}"),
        2 => tracing::error!(target: "qt", component = category, %file, line, "{message}"),
        3 => tracing::error!(target: "qt::fatal", component = category, %file, line, "{message}"),
        _ => tracing::info!(target: "qt", component = category, %file, line, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Output};

    // Run global logger, panic hooks and Qt handlers in a fresh process for each scenario.
    fn probe(mode: &str, filter: &str) -> Output {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "logging::tests::subprocess_probe", "--nocapture"])
            .env("LIPAX_LOG_TEST_MODE", mode)
            .env("RUST_LOG", filter)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("QT_LOGGING_RULES", "*.debug=true")
            .env_remove("RUST_BACKTRACE")
            .env_remove("QT_FATAL_WARNINGS")
            .env_remove("QT_FATAL_CRITICALS");
        // SAFETY: only an async-signal-safe resource-limit call in the child.
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.output().unwrap()
    }

    #[test]
    fn subprocess_probe() {
        let Ok(mode) = std::env::var("LIPAX_LOG_TEST_MODE") else {
            return;
        };
        init();
        if mode == "panic" {
            panic!("probe-rust-panic");
        }
        if mode == "core-error" {
            use lipa_core::{
                capture::{Capture, CaptureError},
                ocr::AnyOcr,
                pipeline::Pipeline,
                settings::{NormRect, Settings, WindowKey},
                translate::HttpTranslate,
            };
            struct BrokenCapture;
            impl Capture for BrokenCapture {
                fn capabilities(&self, _: &WindowKey, _: &Settings) -> lipa_core::capture::CaptureCapabilities {
                    lipa_core::capture::CaptureCapabilities::KWIN
                }
                async fn grab(
                    &self,
                    _: &WindowKey,
                    _: NormRect,
                ) -> Result<image::DynamicImage, CaptureError> {
                    Err(CaptureError::Unavailable("probe-core-capture-error".into()))
                }
            }
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut pipeline =
                        Pipeline::new(BrokenCapture, AnyOcr::default(), HttpTranslate::new());
                    let settings = Settings {
                        window: Some(WindowKey {
                            uuid: "test".into(),
                            caption: "Test".into(),
                            resource_class: "test".into(),
                        }),
                        region: Some(NormRect {
                            x: 0.0,
                            y: 0.0,
                            w: 1.0,
                            h: 1.0,
                        }),
                        ..Settings::default()
                    };
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    drop(rx); // No GUI can receive the error.
                    pipeline
                        .tick(&settings, true, std::time::Instant::now(), &tx)
                        .await;
                });
            return;
        }
        if mode == "qml" {
            let _app = cxx_qt_lib::QGuiApplication::new();
            ffi::emitQmlLogProbe();
            return;
        }
        tracing::error!("probe-rust-error");
        tracing::warn!("probe-rust-warning");
        tracing::info!("probe-rust-info");
        tracing::debug!("probe-rust-debug");
        tracing::trace!("probe-rust-trace");
        ffi::emitQtLogProbe(mode == "fatal");
    }

    #[test]
    fn core_error_survives_closed_gui_channel() {
        let result = probe("core-error", "info");
        assert!(result.status.success());
        let err = String::from_utf8_lossy(&result.stderr);
        assert!(
            err.contains("ERROR")
                && err.contains("lipa_core::pipeline")
                && err.contains("probe-core-capture-error"),
            "{err}"
        );
        assert!(!String::from_utf8_lossy(&result.stdout).contains("probe-core-capture-error"));
    }

    #[test]
    fn levels_streams_and_qt_context() {
        let result = probe("messages", "trace");
        assert!(result.status.success());
        let (out, err) = (
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr),
        );
        for message in [
            "probe-rust-warning",
            "probe-rust-info",
            "probe-rust-debug",
            "probe-rust-trace",
            "probe-qt-debug",
            "probe-qt-warning",
            "probe-qt-info",
        ] {
            assert!(out.contains(message), "missing {message}: {out}");
            assert!(!err.contains(message));
        }
        for message in ["probe-rust-error", "probe-qt-critical"] {
            assert!(err.contains(message), "missing {message}: {err}");
            assert!(!out.contains(message));
        }
        assert!(err.contains("lipax.test") && err.contains("logging.cpp"));
        assert!(
            !out.contains('\u{1b}') && !err.contains('\u{1b}'),
            "redirected logs have no ANSI escapes"
        );
    }

    #[test]
    fn env_filter_and_invalid_filter_fallback() {
        let result = probe("messages", "error");
        let out = String::from_utf8_lossy(&result.stdout);
        assert!(!out.contains("probe-rust-info") && !out.contains("probe-qt-debug"));
        assert!(String::from_utf8_lossy(&result.stderr).contains("probe-rust-error"));
        let invalid = probe("messages", "lipa=not-a-level");
        assert!(invalid.status.success());
        assert!(String::from_utf8_lossy(&invalid.stdout).contains("Некорректный RUST_LOG"));
    }

    #[test]
    fn fatal_and_panic_survive_filter_off() {
        let panic = probe("panic", "off");
        assert!(!panic.status.success());
        let err = String::from_utf8_lossy(&panic.stderr);
        assert!(
            err.contains("probe-rust-panic")
                && err.contains("Backtrace:")
                && err.contains("logging.rs"),
            "{err}"
        );
        assert!(!err.contains("disabled backtrace"));
        let fatal = probe("fatal", "off");
        assert!(!fatal.status.success());
        assert!(String::from_utf8_lossy(&fatal.stderr).contains("probe-qt-fatal"));
    }

    #[test]
    fn qml_console_and_binding_errors_reach_backend() {
        let result = probe("qml", "debug");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let (out, err) = (
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr),
        );
        assert!(
            out.contains("probe-qml-debug") && out.contains("probe-qml-warning"),
            "{out}"
        );
        assert!(
            out.contains("missingProbeValue") && out.contains("lipax-logging-probe.qml"),
            "{out}"
        );
        assert!(err.contains("probe-qml-error"), "{err}");
    }
}
