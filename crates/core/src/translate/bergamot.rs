//! Optional local Bergamot CLI adapter for Linux.
//! Each language pair uses `<models_dir>/<source>-<target>.yml`, a patched Bergamot config.

use super::TranslateError;
use std::{path::{Path, PathBuf}, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(45);
/// Added to the timeout of a batch for every started group of 8 texts.
const BATCH_EXTRA_PER_8: Duration = Duration::from_secs(2);

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

/// A resolved engine invocation: executable plus model config (its directory is the child's cwd).
struct Engine { executable: PathBuf, config: PathBuf }

async fn engine(binary: &str, models_dir: &str, src: &str, dst: &str) -> Result<Engine, TranslateError> {
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
    Ok(Engine { executable, config })
}

/// One process: `input` on stdin, UTF-8 stdout back (not trimmed).
async fn run(engine: &Engine, input: &str, timeout: Duration) -> Result<String, TranslateError> {
    let child = Command::new(&engine.executable)
        .arg("--model-config-paths").arg(&engine.config)
        .arg("--cpu-threads").arg("2")
        .current_dir(engine.config.parent().expect("model config has parent"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn()
        .map_err(|e| unavailable(&engine.executable.display().to_string(), &e.to_string()))?;
    let output = tokio::time::timeout(timeout, exchange(child, input)).await
        .map_err(|_| TranslateError::Local(format!("Bergamot не ответил за {} с", timeout.as_secs())))?
        .map_err(|e| TranslateError::Local(format!("обмен с Bergamot: {e}")))?;
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
        return Err(TranslateError::Local(format!("Bergamot завершился с {}: {detail}", output.status)));
    }
    String::from_utf8(output.stdout).map_err(|_| TranslateError::Local("Bergamot вернул текст не в UTF-8".into()))
}

pub async fn translate(binary: &str, models_dir: &str, text: &str, src: &str, dst: &str) -> Result<String, TranslateError> {
    let engine = engine(binary, models_dir, src, dst).await?;
    let translated = run(&engine, text, TRANSLATE_TIMEOUT).await?;
    let translated = translated.trim_end_matches(['\r', '\n']);
    if translated.is_empty() { return Err(TranslateError::Local("Bergamot вернул пустой перевод".into())); }
    Ok(translated.to_owned())
}

/// Timeout of a batch: `TRANSLATE_TIMEOUT` plus `BATCH_EXTRA_PER_8` for every started group of 8 texts.
/// (The model is loaded once per process, so a batch costs far less than `n` single calls; this only
/// leaves room for the sentences themselves.)
fn batch_timeout(texts: usize) -> Duration {
    TRANSLATE_TIMEOUT + BATCH_EXTRA_PER_8 * texts.div_ceil(8) as u32
}

/// Translate several texts with ONE process start. The engine is line-oriented (N input lines give
/// N output lines, an empty line stays empty), so each text is flattened to one line. The result has
/// one entry per input text, in order; texts that are empty after flattening give an empty string.
/// A different number of output lines is an error, see [`is_line_mismatch`]; the caller falls back.
pub async fn translate_many(binary: &str, models_dir: &str, texts: &[String], src: &str, dst: &str) -> Result<Vec<String>, TranslateError> {
    let lines: Vec<String> = texts.iter().map(|t| flatten(t)).collect();
    if lines.iter().all(String::is_empty) { return Ok(lines); }
    let engine = engine(binary, models_dir, src, dst).await?;
    let stdout = run(&engine, &lines.join("\n"), batch_timeout(texts.len())).await?;
    split_lines(&stdout, lines.len())
}

/// A text as a single line: line breaks become spaces, ends are trimmed.
fn flatten(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_owned()
}

const MISMATCH: &str = "число строк ответа Bergamot не совпало с числом строк запроса";

/// The error of [`split_lines`] when the engine returned another number of lines.
pub fn is_line_mismatch(error: &TranslateError) -> bool {
    matches!(error, TranslateError::Local(m) if m.starts_with(MISMATCH))
}

/// Split stdout into exactly `expected` lines (one trailing `\r` per line is dropped; a single
/// final line break is tolerated).
fn split_lines(stdout: &str, expected: usize) -> Result<Vec<String>, TranslateError> {
    let mut lines: Vec<&str> = stdout.split('\n').collect();
    if lines.len() == expected + 1 && lines.last() == Some(&"") { lines.pop(); }
    if lines.len() != expected {
        return Err(TranslateError::Local(format!("{MISMATCH}: ожидалось {expected}, получено {}", lines.len())));
    }
    Ok(lines.into_iter().map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned()).collect())
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
    fn texts_are_flattened_to_single_lines_and_joined() {
        assert_eq!(flatten("a\r\nb\nc "), "a  b c");
        assert_eq!(flatten(" \n "), "");
        let lines: Vec<String> = ["Hello\nthere", "", "Start game"].iter().map(|t| flatten(t)).collect();
        assert_eq!(lines.join("\n"), "Hello there\n\nStart game");
    }

    #[test]
    fn output_is_split_back_into_the_same_number_of_lines() {
        assert_eq!(split_lines("Привет\n\nИгра", 3).unwrap(), ["Привет", "", "Игра"]);
        assert_eq!(split_lines("а\r\nб\r\n", 2).unwrap(), ["а", "б"], "CRLF and a final break");
        assert_eq!(split_lines("", 1).unwrap(), [""]);
        let error = split_lines("а\nб", 3).unwrap_err();
        assert!(is_line_mismatch(&error), "{error}");
        assert!(is_line_mismatch(&split_lines("а\nб\nв\n\n", 3).unwrap_err()));
        assert!(!is_line_mismatch(&TranslateError::Local("другое".into())));
    }

    #[test]
    fn batch_timeout_grows_modestly() {
        assert_eq!(batch_timeout(1), Duration::from_secs(47));
        assert_eq!(batch_timeout(8), Duration::from_secs(47));
        assert_eq!(batch_timeout(9), Duration::from_secs(49));
    }

    #[tokio::test]
    async fn an_all_empty_batch_needs_no_process() {
        let out = translate_many("/nonexistent", "/nonexistent", &[" ".into(), "\n".into()], "en", "ru").await.unwrap();
        assert_eq!(out, ["", ""]);
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
