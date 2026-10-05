use super::{Ocr, OcrError, OcrLine, OcrResult};
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
        "PaddleOCR: {message}. Нужны PaddleOCR 3.x и PaddlePaddle в выбранном Python (см. docs/PaddleOCR.md)."
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

#[derive(serde::Deserialize)]
struct Reply {
    text: Option<String>,
    error: Option<String>,
    /// Optional: older workers and unusual PaddleOCR versions send only the text.
    #[serde(default)]
    lines: Vec<ReplyLine>,
}

#[derive(serde::Deserialize)]
struct ReplyLine {
    text: String,
    /// 0–1.
    score: f32,
    /// x1, y1, x2, y2 in pixels of the image.
    #[serde(rename = "box")]
    rect: [f32; 4],
}

fn reply_to_result(text: String, lines: Vec<ReplyLine>) -> OcrResult {
    let lines: Vec<OcrLine> = lines.into_iter().map(|l| OcrLine {
        rect: crate::layout::CropRect::in_space(l.rect[0], l.rect[1], (l.rect[2] - l.rect[0]).max(0.0), (l.rect[3] - l.rect[1]).max(0.0)),
        text: l.text,
        confidence: (l.score * 100.0).clamp(0.0, 100.0),
    }).collect();
    let confidence = (!lines.is_empty()).then(|| lines.iter().map(|l| l.confidence).sum::<f32>() / lines.len() as f32);
    OcrResult { text, lines, confidence, engine: "paddleocr" }
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
                tracing::debug!(component = "paddleocr", stderr = %String::from_utf8_lossy(&buffer[..n]), "Диагностика Python worker");
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

    async fn recognize(&mut self, png: &[u8]) -> Result<OcrResult, OcrError> {
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
        let reply: Reply = serde_json::from_str(&line).map_err(setup)?;
        if let Some(error) = reply.error {
            return Err(setup(error));
        }
        let text = reply.text.ok_or_else(|| setup("ответ не содержит текста"))?;
        Ok(reply_to_result(text, reply.lines))
    }
}

impl Ocr for PaddleOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        Ok(self.recognize_detailed(img, settings).await?.text)
    }

    async fn recognize_detailed(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        tracing::debug!(engine = "paddleocr", width = img.width(), height = img.height(), language = %settings.recognition.language, "Начало OCR");
        let language = language(&settings.recognition.language)?;
        let mut png = Vec::new();
        // Paddle's detector handles resizing; retain colour and original resolution.
        img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
        let mut slot = self.worker.lock().await;
        if !slot
            .as_ref()
            .is_some_and(|w| w.language == language && w.python == settings.recognition.paddle_python)
        {
            *slot = None;
            *slot = Some(Worker::spawn(
                &settings.recognition.paddle_python,
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

    #[test]
    fn lines_and_scores_become_percent_and_rectangles() {
        let reply: Reply = serde_json::from_str(r#"{"text":"A\nB","lines":[{"text":"A","score":0.9,"box":[10,20,110,40]},{"text":"B","score":0.7,"box":[10,50,60,70]}]}"#).unwrap();
        let result = reply_to_result(reply.text.unwrap(), reply.lines);
        assert_eq!(result.lines[0].rect, crate::layout::CropRect::in_space(10.0, 20.0, 100.0, 20.0));
        assert!((result.confidence.unwrap() - 80.0).abs() < 0.01);
        assert_eq!(result.engine, "paddleocr");
        // A worker that sends only text still works, just without confidence.
        let plain: Reply = serde_json::from_str(r#"{"text":"only"}"#).unwrap();
        let plain = reply_to_result(plain.text.unwrap(), plain.lines);
        assert_eq!((plain.lines.len(), plain.confidence), (0, None));
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
        assert_eq!(worker.recognize(b"image").await.unwrap().text, "Привет 0");
        assert_eq!(worker.recognize(b"image").await.unwrap().text, "Привет 1");
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
