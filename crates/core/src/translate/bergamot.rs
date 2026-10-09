//! Optional local Bergamot CLI adapter for Linux.
//! Each language pair uses `<models_dir>/<source>-<target>.yml`, a patched Bergamot config.

use super::TranslateError;
use std::{path::{Path, PathBuf}, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(45);

/// Resolve before changing the child's working directory to the model directory.
/// A package contains its own engine; an explicit setting still takes precedence.
pub fn executable(binary: &str) -> Result<PathBuf, TranslateError> {
    let bundled = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.join("../lib/lipax/bergamot")))
        .unwrap_or_else(|| PathBuf::from("/usr/lib/lipax/bergamot"));
    let paths = std::env::var_os("PATH").unwrap_or_default();
    resolve_executable(binary, &bundled, &std::env::split_paths(&paths).collect::<Vec<_>>())
}

fn resolve_executable(binary: &str, bundled: &Path, paths: &[PathBuf]) -> Result<PathBuf, TranslateError> {
    use std::os::unix::fs::PermissionsExt;
    let usable = |path: &Path| std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let name = if binary.is_empty() { "bergamot" } else { binary };
    let candidates = if binary.is_empty() {
        std::iter::once(bundled.to_path_buf()).chain(paths.iter().map(|p| p.join(name))).collect::<Vec<_>>()
    } else if name.contains('/') {
        vec![PathBuf::from(name)]
    } else {
        paths.iter().map(|p| p.join(name)).collect()
    };
    candidates.iter().filter(|p| usable(p)).find_map(|p| p.canonicalize().ok())
        .ok_or_else(|| unavailable(name, "файл не найден или не имеет права на запуск"))
}

fn unavailable(binary: &str, detail: &str) -> TranslateError {
    TranslateError::EngineUnavailable(format!("Не удалось запустить движок Bergamot ({binary}): {detail}. Обновите пакет LipaX со встроенным движком или укажите исполняемый файл в Настройках → Перевод → Bergamot: путь к CLI. После исправления нажмите «Перевести» или перезапустите слежение."))
}

fn language_code(code: &str) -> Result<String, TranslateError> {
    if code.is_empty() || code.len() > 16 || !code.bytes().all(|c| c.is_ascii_alphabetic() || c == b'-') {
        return Err(TranslateError::Local(format!("недопустимый код языка: {code}")));
    }
    Ok(code.to_ascii_lowercase())
}

pub async fn translate(binary: &str, models_dir: &str, text: &str, src: &str, dst: &str) -> Result<String, TranslateError> {
    let executable = executable(binary)?;
    let source = language_code(src)?;
    let target = language_code(dst)?;
    let pair = super::models::pair(&source, &target).map_err(TranslateError::Local)?;
    let explicit = PathBuf::from(models_dir);
    let config = tokio::task::spawn_blocking(move || {
        super::models::installed_config(&explicit, &pair)
            .or_else(|| super::models::installed_config(&super::models::cache_root().join(&pair), &pair))
            .ok_or_else(|| super::models::missing(&pair))
    }).await.map_err(|e| TranslateError::Local(e.to_string()))?
        .map_err(TranslateError::Local)?;
    let config = config.canonicalize().map_err(|e| TranslateError::Local(e.to_string()))?;
    let child = Command::new(&executable)
        .arg("--model-config-paths").arg(&config)
        .arg("--cpu-threads").arg("2")
        .current_dir(config.parent().expect("model config has parent"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn()
        .map_err(|e| unavailable(&executable.display().to_string(), &e.to_string()))?;
    let output = tokio::time::timeout(TRANSLATE_TIMEOUT, exchange(child, text)).await
        .map_err(|_| TranslateError::Local("Bergamot не ответил за 45 с".into()))?
        .map_err(|e| TranslateError::Local(format!("обмен с Bergamot: {e}")))?;
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
        return Err(TranslateError::Local(format!("Bergamot завершился с {}: {detail}", output.status)));
    }
    let translated = String::from_utf8(output.stdout)
        .map_err(|_| TranslateError::Local("Bergamot вернул текст не в UTF-8".into()))?;
    let translated = translated.trim_end_matches(['\r', '\n']);
    if translated.is_empty() { return Err(TranslateError::Local("Bergamot вернул пустой перевод".into())); }
    Ok(translated.to_owned())
}


async fn exchange(mut child: tokio::process::Child, text: &str) -> std::io::Result<std::process::Output> {
    let mut input = child.stdin.take().expect("piped stdin");
    let write = async move {
        input.write_all(text.as_bytes()).await?;
        input.shutdown().await?;
        // ChildStdin::shutdown does not close the pipe. Bergamot reads until EOF.
        // Drop our writer before waiting for its response, otherwise both sides wait forever.
        drop(input);
        Ok::<(), std::io::Error>(())
    };
    let ((), output) = tokio::try_join!(write, child.wait_with_output())?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn child_receives_eof_before_we_wait_for_its_output() {
        let child = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::piped())
            .stderr(Stdio::piped()).kill_on_drop(true).spawn().unwrap();
        let output = tokio::time::timeout(Duration::from_secs(2), exchange(child, "text without newline"))
            .await.expect("child must receive EOF").unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"text without newline");
    }

    #[test]
    fn bundled_engine_and_manual_override_are_resolved_to_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        let bundled = root.path().join("bundled");
        let manual = root.path().join("bergamot");
        for file in [&bundled, &manual] {
            std::fs::write(file, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(resolve_executable("", &bundled, &[root.path().into()]).unwrap(), bundled);
        assert_eq!(resolve_executable("bergamot", &bundled, &[root.path().into()]).unwrap(), manual);
        assert_eq!(resolve_executable(manual.to_str().unwrap(), &bundled, &[]).unwrap(), manual);
        let error = resolve_executable("/missing/bergamot", &bundled, &[]).unwrap_err();
        assert!(error.stops_automatic_retries());
        assert!(error.to_string().contains("путь к CLI"));
        std::fs::set_permissions(&manual, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(resolve_executable("bergamot", &bundled, &[root.path().into()]).is_err());
    }
}
