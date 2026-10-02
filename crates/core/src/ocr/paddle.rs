use super::{Ocr, OcrError};
use crate::settings::Settings;
use image::{DynamicImage, ImageFormat};
use std::{
    io::Cursor,
    process::Stdio,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

#[derive(Default)]
pub struct PaddleOcr {
    worker: Mutex<Option<Worker>>,
}

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    stderr: Arc<StdMutex<Vec<u8>>>,
    reader: tokio::task::JoinHandle<()>,
    language: String,
    python: String,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.reader.abort();
        let _ = self.child.start_kill();
    }
}

fn setup(message: impl std::fmt::Display) -> OcrError {
    OcrError::Setup(format!(
        "PaddleOCR: {message}. Нужны PaddleOCR 3.x и PaddlePaddle в выбранном Python (см. docs/PADDLEOCR.md)."
    ))
}

fn language(source: &str) -> Result<&'static str, OcrError> {
    match crate::tesseract::primary_lang(source) {
        "eng" => Ok("en"),
        "rus" => Ok("ru"),
        "jpn" => Ok("japan"),
        "kor" => Ok("korean"),
        "chi_sim" => Ok("ch"),
        "chi_tra" => Ok("chinese_cht"),
        "deu" => Ok("de"),
        "fra" => Ok("fr"),
        "spa" => Ok("es"),
        "ita" => Ok("it"),
        "por" => Ok("pt"),
        "ukr" => Ok("uk"),
        "pol" => Ok("pl"),
        other => Err(setup(format!(
            "язык «{other}» пока не поддержан интеграцией"
        ))),
    }
}

impl Worker {
    fn spawn(python: &str, language: &str, script: &str) -> Result<Self, OcrError> {
        let mut child = Command::new(python)
            .args(["-u", "-c", script, language])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(setup)?;
        let input = child.stdin.take().expect("piped");
        let output = BufReader::new(child.stdout.take().expect("piped"));
        let mut pipe = child.stderr.take().expect("piped");
        let stderr = Arc::new(StdMutex::new(Vec::new()));
        let tail = stderr.clone();
        let reader = tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            while let Ok(n) = pipe.read(&mut buffer).await {
                if n == 0 {
                    break;
                }
                let mut bytes = tail.lock().unwrap();
                bytes.extend_from_slice(&buffer[..n]);
                let excess = bytes.len().saturating_sub(8192);
                bytes.drain(..excess);
            }
        });
        Ok(Self {
            child,
            input,
            output,
            stderr,
            reader,
            language: language.into(),
            python: python.into(),
        })
    }

    async fn recognize(&mut self, png: &[u8]) -> Result<String, OcrError> {
        let size = u32::try_from(png.len()).map_err(setup)?;
        self.input
            .write_all(&size.to_be_bytes())
            .await
            .map_err(setup)?;
        self.input.write_all(png).await.map_err(setup)?;
        self.input.flush().await.map_err(setup)?;
        let mut line = String::new();
        if self.output.read_line(&mut line).await.map_err(setup)? == 0 {
            let detail = String::from_utf8_lossy(&self.stderr.lock().unwrap()).into_owned();
            return Err(setup(format!("процесс завершился: {detail}")));
        }
        #[derive(serde::Deserialize)]
        struct Reply {
            text: Option<String>,
            error: Option<String>,
        }
        let reply: Reply = serde_json::from_str(&line).map_err(setup)?;
        if let Some(error) = reply.error {
            return Err(setup(error));
        }
        reply.text.ok_or_else(|| setup("ответ не содержит текста"))
    }
}

impl Ocr for PaddleOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        let language = language(&settings.source_lang)?;
        let mut png = Vec::new();
        // Paddle's detector handles resizing; retain colour and original resolution.
        img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
        let mut slot = self.worker.lock().await;
        if !slot
            .as_ref()
            .is_some_and(|w| w.language == language && w.python == settings.paddle_python)
        {
            *slot = None;
            *slot = Some(Worker::spawn(
                &settings.paddle_python,
                language,
                include_str!("paddle_worker.py"),
            )?);
        }
        // Own the worker across await: cancellation kills it instead of leaving a stale reply.
        let mut worker = slot.take().unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(120),
            worker.recognize(&png),
        )
        .await
        .unwrap_or_else(|_| {
            Err(setup(
                "превышено время ожидания (120 с); при первом запуске загружаются модели",
            ))
        });
        if result.is_ok() { *slot = Some(worker); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_primary_language_without_requiring_tesseract_data() {
        assert_eq!(language("jpn+eng").unwrap(), "japan");
        assert_eq!(language("rus").unwrap(), "ru");
        assert!(language("unknown").is_err());
    }

    #[tokio::test]
    async fn worker_protocol_reuses_process_and_reports_failure() {
        let script = r#"
import sys, struct, json
for i in range(3):
    size, = struct.unpack('>I', sys.stdin.buffer.read(4))
    data = sys.stdin.buffer.read(size)
    print(json.dumps({'text': 'Привет ' + str(i)} if i < 2 else {'error': 'model unavailable'}), flush=True)
"#;
        let mut worker = Worker::spawn("python3", "en", script).unwrap();
        assert_eq!(worker.recognize(b"image").await.unwrap(), "Привет 0");
        assert_eq!(worker.recognize(b"image").await.unwrap(), "Привет 1");
        assert!(
            worker
                .recognize(b"image")
                .await
                .unwrap_err()
                .to_string()
                .contains("model unavailable")
        );
    }
}
