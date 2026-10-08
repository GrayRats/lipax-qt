//! Optional local Bergamot CLI adapter for Linux.
//! Each language pair uses `<models_dir>/<source>-<target>.yml`, a patched Bergamot config.

use super::TranslateError;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(45);

fn language_code(code: &str) -> Result<String, TranslateError> {
    if code.is_empty() || code.len() > 16 || !code.bytes().all(|c| c.is_ascii_alphabetic() || c == b'-') {
        return Err(TranslateError::Local(format!("недопустимый код языка: {code}")));
    }
    Ok(code.to_ascii_lowercase())
}

pub async fn translate(binary: &str, models_dir: &str, text: &str, src: &str, dst: &str) -> Result<String, TranslateError> {
    if models_dir.is_empty() { return Err(TranslateError::NotConfigured("каталог моделей Bergamot / BERGAMOT_MODELS_DIR")); }
    if src == "auto" { return Err(TranslateError::Local("Bergamot требует явный исходный язык или язык OCR".into())); }
    let source = language_code(src)?;
    let target = language_code(dst)?;
    let config = PathBuf::from(models_dir).join(format!("{source}-{target}.yml"));
    let config = config.canonicalize().map_err(|_| TranslateError::Local(format!(
        "нет модели {source} → {target}: ожидается {}", config.display()
    )))?;
    let executable = if binary.is_empty() { "bergamot" } else { binary };
    let mut child = Command::new(executable)
        .arg("--model-config-paths").arg(&config)
        .arg("--cpu-threads").arg("2")
        .current_dir(config.parent().expect("model config has parent"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn()
        .map_err(|e| TranslateError::Local(format!("не удалось запустить Bergamot: {e}")))?;
    let mut input = child.stdin.take().expect("piped stdin");
    let exchange = async {
        let write = async {
            input.write_all(text.as_bytes()).await?;
            input.shutdown().await
        };
        let ((), output) = tokio::try_join!(write, child.wait_with_output())?;
        Ok::<_, std::io::Error>(output)
    };
    let output = tokio::time::timeout(TRANSLATE_TIMEOUT, exchange).await
        .map_err(|_| TranslateError::Local("Bergamot не ответил за 45 с".into()))?
        .map_err(|e| TranslateError::Local(format!("обмен с Bergamot: {e}")))?;
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
        return Err(TranslateError::Local(format!("Bergamot завершился с {}: {detail}", output.status)));
    }
    let translated = String::from_utf8(output.stdout)
        .map_err(|_| TranslateError::Local("Bergamot вернул текст не в UTF-8".into()))?;
    let translated = translated.trim_end_matches(|c| c == '\r' || c == '\n');
    if translated.is_empty() { return Err(TranslateError::Local("Bergamot вернул пустой перевод".into())); }
    Ok(translated.to_owned())
}
