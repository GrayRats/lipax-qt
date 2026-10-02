//! Read-only dependency checks: no package installation or model downloads.
use crate::settings::Settings;
use serde::Serialize;
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

#[derive(Serialize)]
pub struct Check { pub name: String, pub state: String, pub detail: String, pub instruction: String }
fn row(name: &str, state: &str, detail: impl Into<String>, instruction: &str) -> Check {
    Check { name: name.into(), state: state.into(), detail: detail.into(), instruction: instruction.into() }
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
    rows.push(row("Desktop Portal", if portal { "ready" } else { "warning" }, if portal { "Сервис запущен" } else { "Сервис не запущен (может активироваться при захвате)" }, "KDE/Arch: sudo pacman -S xdg-desktop-portal xdg-desktop-portal-kde"));
    rows
}
