//! Read-only dependency checks: no package installation or model downloads.
use crate::settings::Settings;
use serde::Serialize;
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

#[derive(Serialize)]
pub struct Check { pub name: String, pub state: String, pub detail: String, pub instruction: String }
fn row(name: &str, state: &str, detail: impl Into<String>, instruction: &str) -> Check {
    let detail = detail.into();
    match state {
        "error" => tracing::error!(component = name, "{detail}"),
        "warning" => tracing::warn!(component = name, "{detail}"),
        _ => tracing::debug!(component = name, "{detail}"),
    }
    Check { name: name.into(), state: state.into(), detail, instruction: instruction.into() }
}
async fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(15), cmd.output()).await
        .map_err(|_| "Проверка не завершилась за 15 с".to_string())?.map_err(|e| e.to_string())?;
    if result.status.success() { Ok(String::from_utf8_lossy(&result.stdout).trim().to_string()) }
    else { Err(String::from_utf8_lossy(&result.stderr).chars().take(1000).collect()) }
}
async fn command_check(name: &str, program: &str, args: &[&str], instruction: &str) -> Check {
    match output(program, args).await {
        Ok(text) => row(name, "ready", text.lines().take(3).collect::<Vec<_>>().join("\n"), instruction),
        Err(error) => row(name, "error", error, instruction),
    }
}
/// Does a desktop file allow this executable to use `org.kde.KWin.ScreenShot2`? KWin decides by exactly
/// this: some `.desktop` whose `Exec` is the path of the running executable and whose
/// `X-KDE-DBUS-Restricted-Interfaces` lists the interface. Returns that file.
pub fn screenshot_permission(exe: &std::path::Path, dirs: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let exe = real(exe);
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "desktop")) {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let value = |key: &str| text.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')));
            let allows = value("X-KDE-DBUS-Restricted-Interfaces").is_some_and(|v| v.split(';').any(|i| i.trim() == "org.kde.KWin.ScreenShot2"));
            if allows && value("Exec").and_then(exec_program).is_some_and(|p| real(std::path::Path::new(&p)) == exe) { return Some(path); }
        }
    }
    None
}

/// The program of an `Exec` line: its first word, with the quoting of the Desktop Entry spec undone.
/// A quoted word is escaped on two levels (a backslash in the string, then in the argument), and `%%` is a percent sign.
fn exec_program(exec: &str) -> Option<String> {
    let exec = exec.trim();
    let word = match exec.strip_prefix('"') {
        Some(rest) => {
            let (mut word, mut chars) = (String::new(), rest.chars());
            // String level: `\\` is a backslash (a quote closes the word).
            let mut raw = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => match chars.next() { Some('\\') => raw.push('\\'), Some(other) => { raw.push('\\'); raw.push(other) } None => raw.push('\\') },
                    other => raw.push(other),
                }
            }
            // Argument level: a backslash escapes `"`, backtick, `$` and itself.
            let mut chars = raw.chars();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => match chars.next() { Some(e @ ('"' | '`' | '$' | '\\')) => word.push(e), Some(other) => { word.push('\\'); word.push(other) } None => word.push('\\') },
                    other => word.push(other),
                }
            }
            word
        }
        None => exec.split_whitespace().next()?.to_owned(),
    };
    Some(word.replace("%%", "%"))
}

/// `KWin version: 6.8.3` from the text of `org.kde.KWin.supportInformation`.
fn kwin_version(support_information: &str) -> Option<String> {
    support_information.lines().find_map(|line| line.strip_prefix("KWin version:")).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The running KWin and the version of its ScreenShot2 interface, for the Status tab and for bug reports: a
/// Plasma update that changes the scripting or capture interface is then visible next to the symptom.
/// Only reported, not enforced: no minimum interface version has been established for the options LipaX passes.
async fn kwin_row(conn: &zbus::Connection) -> Check {
    let version = async {
        let reply = conn.call_method(Some("org.kde.KWin"), "/KWin", Some("org.kde.KWin"), "supportInformation", &()).await.ok()?;
        kwin_version(&reply.body().deserialize::<String>().ok()?)
    };
    let interface = async {
        let reply = conn.call_method(Some("org.kde.KWin"), "/org/kde/KWin/ScreenShot2", Some("org.freedesktop.DBus.Properties"), "Get", &("org.kde.KWin.ScreenShot2", "Version")).await.ok()?;
        reply.body().deserialize::<zbus::zvariant::OwnedValue>().ok().and_then(|v| u32::try_from(v).ok())
    };
    let (version, interface) = tokio::join!(
        async { tokio::time::timeout(std::time::Duration::from_secs(3), version).await.ok().flatten() },
        async { tokio::time::timeout(std::time::Duration::from_secs(3), interface).await.ok().flatten() },
    );
    let version = version.unwrap_or_else(|| "версия не определена".into());
    match interface {
        Some(v) => row("KWin", "ready", format!("KWin {version}, интерфейс ScreenShot2 версии {v}"), ""),
        None => row("KWin", "warning", format!("KWin {version}, но интерфейс ScreenShot2 не отвечает"),
            "Захват окна через KWin работать не будет: «Авто» выберет portal. Проверьте, что запущен kwin_wayland (Plasma 6) и обновлён пакет kwin."),
    }
}

/// The directories KDE reads desktop files from.
fn application_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter().chain(system.split(':').filter(|d| !d.is_empty()).map(std::path::PathBuf::from)).map(|d| d.join("applications")).collect()
}

pub async fn inspect(s: &Settings) -> Vec<Check> {
    let (tess, gst, pipewire, plugins, python) = tokio::join!(
        command_check("Tesseract", "tesseract", &["--version"], "Arch: sudo pacman -S tesseract tesseract-data-eng. Языки — во вкладке «Распознавание»."),
        command_check("GStreamer", "gst-launch-1.0", &["--version"], "Arch: sudo pacman -S gstreamer gst-plugins-base gst-plugins-good gst-plugin-pipewire"),
        command_check("PipeWire (соединение)", "pw-cli", &["info", "0"], "Arch: sudo pacman -S pipewire wireplumber; systemctl --user restart pipewire wireplumber"),
        async {
            let mut rows = Vec::new();
            for plugin in ["pipewiresrc", "videorate", "videoconvert", "pngenc", "fdsink"] {
                rows.push(command_check(&format!("GStreamer: {plugin}"), "gst-inspect-1.0", &[plugin], "Arch: sudo pacman -S gst-plugins-base gst-plugins-good gst-plugin-pipewire").await);
            }
            rows
        },
        output(&s.recognition.paddle_python, &["-c", include_str!("ocr/paddle_check.py")]),
    );
    let mut rows = vec![tess, gst, pipewire];
    if s.translation.service == crate::settings::TranslationService::Bergamot {
        let binary = if s.translation.bergamot_binary.is_empty() { std::env::var("BERGAMOT_BINARY").unwrap_or_default() } else { s.translation.bergamot_binary.clone() };
        match crate::translate::bergamot::executable(&binary) {
            Ok(path) => rows.push(command_check("Bergamot", &path.to_string_lossy(), &["--help"], "Движок входит в пакет LipaX; для другого исполняемого файла укажите путь к CLI во вкладке «Перевод»." ).await),
            Err(error) => rows.push(row("Bergamot", "error", error.to_string(), "Обновите пакет LipaX или укажите исполняемый файл Bergamot.")),
        }
    }
    rows.extend(plugins);
    match python {
        Ok(json) => {
            let data: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
            let ready = data["ready"].as_bool().unwrap_or(false);
            rows.push(row("PaddleOCR / PaddlePaddle", if ready { "ready" } else { "error" }, data["detail"].as_str().unwrap_or(&json),
                "Создайте venv: python3 -m venv ~/.local/share/lipa/paddle-venv; затем ~/.local/share/lipa/paddle-venv/bin/python -m pip install 'paddlepaddle>=3,<4' 'paddleocr>=3,<4'. Укажите абсолютный путь Python во вкладке «Распознавание»."));
            let models = data["models"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            rows.push(row("Модели PaddleOCR", "warning", if models.is_empty() { "Локальные модели не найдены; первый OCR потребует загрузки из сети.".into() } else { format!("В кэше: {models}. Совместимость выбранного языка проверяется при распознавании.") },
                "Первый перевод с PaddleOCR загружает модели. Если загрузка не укладывается в 60 с, подготовьте модели отдельно по docs/PaddleOCR.md и нажмите «Повторить»."));
        },
        Err(e) => rows.push(row("Python / PaddleOCR", "error", e, "Укажите путь к Python из venv; инструкция: docs/PaddleOCR.md.")),
    }
    if s.general.keys_in_wallet {
        let help = "Если кошелёк недоступен, API-ключи остаются в config.toml. Разблокируйте кошелёк (KWallet, GNOME Keyring) и сохраните настройки ещё раз.";
        // The check is a D-Bus round trip with a time limit: not on the async worker thread.
        match tokio::task::spawn_blocking(crate::secrets::Wallet::check).await.unwrap_or_else(|e| Err(e.to_string())) {
            Ok(()) => rows.push(row("Кошелёк для API-ключей", "ready", "Secret Service доступна, ключи хранятся в ней", "")),
            Err(reason) => rows.push(row("Кошелёк для API-ключей", "warning", reason, help)),
        }
    }
    let engine = s.recognition.engine.as_str();
    let japanese = crate::tesseract::primary_lang(&s.recognition.language) == "jpn";
    if matches!(engine, "rapidocr" | "meikiocr" | "auto" | "consensus") {
        use crate::ocr::rapid;
        match rapid::library_path(None) {
            Ok(path) => {
                rows.push(row("ONNX Runtime", "ready", path.display().to_string(), ""));
                let (state, detail) = rapid::gpu_report(s.recognition.rapid_use_gpu);
                rows.push(row("GPU для RapidOCR / MeikiOCR", state, detail,
                    "Для видеокарты нужна сборка ONNX Runtime с CUDA, MIGraphX или ROCm (docs/RapidOCR.md). Пакет onnxruntime-cpu работает только на процессоре."));
            }
            Err(e) => rows.push(row("ONNX Runtime", "error", e, &format!("Arch: {}. Инструкция: docs/RapidOCR.md.", rapid::INSTALL_COMMAND))),
        }
    }
    if matches!(engine, "rapidocr" | "auto" | "consensus") {
        use crate::ocr::rapid_models;
        match rapid_models::select(&s.recognition.language, &s.recognition.rapid_variant) {
            Ok(model) if rapid_models::present(&rapid_models::cache_root(), model) => rows.push(row("Модель RapidOCR", "ready", format!("{} ({})", model.label, model.id), "")),
            Ok(model) => rows.push(row("Модель RapidOCR", "error", format!("Не скачана: {} ({}, {:.0} МБ)", model.label, model.id, model.size() as f64 / 1e6),
                "Скачайте модель: «Настройки → Распознавание → RapidOCR → Скачать».")),
            Err(e) => rows.push(row("Модель RapidOCR", "error", e, "Выберите поддерживаемый основной язык (docs/RapidOCR.md).")),
        }
    }
    // In `auto` MeikiOCR is an optional second opinion for Japanese: without its model RapidOCR reads the language.
    if engine == "meikiocr" || (engine == "auto" && japanese) {
        use crate::ocr::rapid_models;
        let missing = if engine == "auto" { "warning" } else { "error" };
        match rapid_models::select_meiki(&s.recognition.language) {
            Ok(model) if rapid_models::present(&rapid_models::cache_root(), model) => rows.push(row("Модель MeikiOCR", "ready", format!("{} ({})", model.label, model.id), "")),
            Ok(model) => rows.push(row("Модель MeikiOCR", missing, format!("Не скачана: {} ({}, {:.0} МБ)", model.label, model.id, model.size() as f64 / 1e6),
                "Скачайте модель: «Настройки → Распознавание → MeikiOCR → Скачать».")),
            Err(e) => rows.push(row("Модель MeikiOCR", "error", e, "Выберите японский основным языком или другой движок (docs/MeikiOCR.md).")),
        }
    }
    let langs = output("tesseract", &["--list-langs"]).await;
    let missing: Vec<_> = s.recognition.language.split('+').filter(|l| !langs.as_ref().is_ok_and(|text| text.lines().any(|line| line.trim() == *l))).collect();
    rows.push(row("Языки Tesseract", if missing.is_empty() { "ready" } else { "error" }, if missing.is_empty() { format!("Готовы: {}", s.recognition.language) } else { format!("Нет языков: {}", missing.join(", ")) }, "Установите языковые пакеты во вкладке «Распознавание». Arch: tesseract-data-<код языка>."));
    let portal = match zbus::Connection::session().await {
        Ok(conn) => conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &("org.freedesktop.portal.Desktop",)).await.ok().and_then(|r| r.body().deserialize::<bool>().ok()).unwrap_or(false),
        Err(_) => false,
    };
    // KWin's permission to capture belongs to the path of this very executable.
    let kwin_running = match zbus::Connection::session().await {
        Ok(conn) => conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &("org.kde.KWin",)).await.ok().and_then(|r| r.body().deserialize::<bool>().ok()).unwrap_or(false),
        Err(_) => false,
    };
    // What this desktop is promised (R-14): nothing but KWin has been run, and the row says so.
    let support = crate::desktop::support_for(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default());
    rows.push(row("Рабочий стол", if support.verified { "ready" } else { "warning" },
        format!("{}: {}", if support.desktop.is_empty() { "не определён" } else { &support.desktop }, support.note), ""));
    if kwin_running {
        if let Ok(conn) = zbus::Connection::session().await { rows.push(kwin_row(&conn).await); }
        let exe = std::env::current_exe().unwrap_or_default();
        match screenshot_permission(&exe, &application_dirs()) {
            Some(file) => rows.push(row("KWin: разрешение на захват", "ready", format!("{} разрешает {}", file.display(), exe.display()), "")),
            None => rows.push(row("KWin: разрешение на захват", "warning",
                format!("Ни один .desktop-файл не разрешает ScreenShot2 для {}: KWin откажет в захвате (NoAuthorized).", exe.display()),
                "Установленный пакет разрешает /usr/bin/lipax. Локальную сборку запускайте через packaging/run-local.sh; после установки пакета выполните kbuildsycoca6 и перезапустите LipaX. Без разрешения «Авто» выберет окно через portal.")),
        }
    }
    rows.push(row("Desktop Portal", if portal { "ready" } else { "warning" }, if portal { "Сервис запущен" } else { "Сервис не запущен (может активироваться при захвате)" }, "KDE/Arch: sudo pacman -S xdg-desktop-portal xdg-desktop-portal-kde"));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kwin_version_is_read_from_the_support_information() {
        let text = "KWin Support Information:\n\nVersion\n=======\nKWin version: 6.8.3\nQt Version: 6.12.0\n";
        assert_eq!(kwin_version(text).as_deref(), Some("6.8.3"));
        assert_eq!(kwin_version("KWin version:   \n"), None);
        assert_eq!(kwin_version("nothing here"), None);
    }

    /// `cargo test -p lipa-core -- --ignored live_kwin_row --nocapture` in a KWin session.
    #[tokio::test]
    #[ignore = "needs a running KWin on the session bus"]
    async fn live_kwin_row() {
        let conn = zbus::Connection::session().await.unwrap();
        let check = kwin_row(&conn).await;
        println!("{} | {} | {}", check.name, check.state, check.detail);
        assert_eq!(check.state, "ready", "{}", check.detail);
        assert!(check.detail.starts_with("KWin 6."), "{}", check.detail);
    }

    fn entry(dir: &std::path::Path, name: &str, exec: &str, interfaces: Option<&str>) {
        let restricted = interfaces.map(|i| format!("X-KDE-DBUS-Restricted-Interfaces={i}\n")).unwrap_or_default();
        std::fs::write(dir.join(name), format!("[Desktop Entry]\nType=Application\nName=T\nExec={exec}\n{restricted}")).unwrap();
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lipax-diag-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn kwin_permission_is_found_by_the_path_of_the_executable() {
        let dir = scratch("perm");
        let exe = dir.join("lipax");
        std::fs::write(&exe, "").unwrap();
        let exe_text = exe.display().to_string();
        entry(&dir, "other.desktop", "/usr/bin/other", Some("org.kde.KWin.ScreenShot2"));
        entry(&dir, "no-interface.desktop", &exe_text, None);
        entry(&dir, "wrong-interface.desktop", &exe_text, Some("org.kde.kwin.Effects"));
        assert_eq!(screenshot_permission(&exe, std::slice::from_ref(&dir)), None, "a file for another program or another interface does not count");
        entry(&dir, "ok.desktop", &format!("{exe_text} --flag %u"), Some("org.kde.kwin.Effects;org.kde.KWin.ScreenShot2;"));
        assert_eq!(screenshot_permission(&exe, std::slice::from_ref(&dir)), Some(dir.join("ok.desktop")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_quoted_path_with_spaces_and_a_symlink_are_understood() {
        let dir = scratch("quoted");
        let real = dir.join("my build");
        std::fs::create_dir_all(&real).unwrap();
        let exe = real.join("lipax");
        std::fs::write(&exe, "").unwrap();
        let link = dir.join("lipa");
        std::os::unix::fs::symlink(&exe, &link).unwrap();
        entry(&dir, "dev.desktop", &format!("\"{}\"", exe.display()), Some("org.kde.KWin.ScreenShot2"));
        // KWin compares the real path: the symlink is the same program.
        assert!(screenshot_permission(&link, std::slice::from_ref(&dir)).is_some());
        assert!(screenshot_permission(&exe, std::slice::from_ref(&dir)).is_some());
        assert!(screenshot_permission(&dir.join("elsewhere"), std::slice::from_ref(&dir)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_quoting_is_undone_on_both_levels() {
        assert_eq!(exec_program("/usr/bin/lipax --flag %u").as_deref(), Some("/usr/bin/lipax"));
        assert_eq!(exec_program("\"/my build/lipax\" %u").as_deref(), Some("/my build/lipax"));
        // What run-local.sh writes for /x/we$ird%dir/lipax: `$` escaped, backslash doubled, percent doubled.
        assert_eq!(exec_program("\"/x/we\\\\$ird%%dir/lipax\"").as_deref(), Some("/x/we$ird%dir/lipax"));
        assert_eq!(exec_program("/usr/bin/100%%").as_deref(), Some("/usr/bin/100%"));
        assert_eq!(exec_program("   "), None);
    }

    #[test]
    fn missing_directories_are_not_an_error() {
        assert_eq!(screenshot_permission(std::path::Path::new("/usr/bin/lipax"), &[std::path::PathBuf::from("/nonexistent/applications")]), None);
    }
}
