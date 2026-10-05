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
            // The first word of Exec; a path with spaces is quoted.
            let program = value("Exec").map(str::trim).and_then(|e| match e.strip_prefix('"') {
                Some(rest) => rest.split('"').next(),
                None => e.split_whitespace().next(),
            });
            if allows && program.is_some_and(|p| real(std::path::Path::new(&p.replace("%%", "%"))) == exe) { return Some(path); }
        }
    }
    None
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
        output(&s.paddle_python, &["-c", include_str!("ocr/paddle_check.py")]),
    );
    let mut rows = vec![tess, gst, pipewire];
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
    let langs = output("tesseract", &["--list-langs"]).await;
    let missing: Vec<_> = s.source_lang.split('+').filter(|l| !langs.as_ref().is_ok_and(|text| text.lines().any(|line| line.trim() == *l))).collect();
    rows.push(row("Языки Tesseract", if missing.is_empty() { "ready" } else { "error" }, if missing.is_empty() { format!("Готовы: {}", s.source_lang) } else { format!("Нет языков: {}", missing.join(", ")) }, "Установите языковые пакеты во вкладке «Распознавание». Arch: tesseract-data-<код языка>."));
    let portal = match zbus::Connection::session().await {
        Ok(conn) => conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &("org.freedesktop.portal.Desktop",)).await.ok().and_then(|r| r.body().deserialize::<bool>().ok()).unwrap_or(false),
        Err(_) => false,
    };
    // KWin's permission to capture belongs to the path of this very executable.
    let kwin_running = match zbus::Connection::session().await {
        Ok(conn) => conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &("org.kde.KWin",)).await.ok().and_then(|r| r.body().deserialize::<bool>().ok()).unwrap_or(false),
        Err(_) => false,
    };
    if kwin_running {
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
    fn missing_directories_are_not_an_error() {
        assert_eq!(screenshot_permission(std::path::Path::new("/usr/bin/lipax"), &[std::path::PathBuf::from("/nonexistent/applications")]), None);
    }
}
