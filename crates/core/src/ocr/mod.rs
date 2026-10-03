//! OCR. Tesseract вызывается как процесс: PNG на stdin, текст из stdout —
//! без временных файлов и гонок между запусками.

pub mod paddle;

use crate::settings::Settings;
use crate::tesseract::TesseractManager;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use std::future::Future;
use std::io::Cursor;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("не удалось запустить tesseract: {0}")]
    Spawn(std::io::Error),
    #[error("tesseract завершился с ошибкой: {0}")]
    Failed(String),
    #[error("кодирование изображения: {0}")]
    Encode(#[from] image::ImageError),
    #[error("{0}")]
    Setup(String),
    #[error("неизвестный OCR-движок: {0}")]
    UnknownEngine(String),
}

pub trait Ocr: Send + Sync {
    fn recognize(
        &self,
        img: &DynamicImage,
        settings: &Settings,
    ) -> impl Future<Output = Result<String, OcrError>> + Send;
}

#[derive(Default)]
pub struct AnyOcr {
    paddle: paddle::PaddleOcr,
}

impl Ocr for AnyOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        match settings.ocr_engine.as_str() {
            "tesseract" => Tesseract.recognize(img, settings).await,
            "paddleocr" => self.paddle.recognize(img, settings).await,
            _ => Err(OcrError::UnknownEngine(settings.ocr_engine.clone())),
        }
    }
}

pub struct Tesseract;

/// Сбой запуска или нехватка языковых данных превращаются в понятное сообщение с именем пакета
/// для текущего дистрибутива. Проверка запускается только после ошибки, поэтому в обычном цикле не тормозит.
fn explain(raw: String, lang: &str, spawn_failed: bool) -> Option<OcrError> {
    let lang_problem = raw.contains("Failed loading language") || raw.contains("Error opening data file") || raw.contains("Could not initialize tesseract");
    if !spawn_failed && !lang_problem {
        return None;
    }
    TesseractManager::system().explain_ocr_failure(lang).map(OcrError::Setup)
}

/// Увеличение 2x и градации серого повышают точность на мелком игровом тексте.
pub fn preprocess(img: &DynamicImage) -> DynamicImage {
    img.resize_exact(img.width() * 2, img.height() * 2, FilterType::Lanczos3).grayscale()
}

impl Ocr for Tesseract {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        if settings.ocr_engine != "tesseract" {
            return Err(OcrError::UnknownEngine(settings.ocr_engine.clone()));
        }
        tracing::debug!(engine = "tesseract", width = img.width(), height = img.height(), language = %settings.source_lang, "Начало OCR");
        let mut png = Vec::new();
        preprocess(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;

        // Языки выбираются в настройках: основной первым, дополнительные через «+» (например, `jpn+eng`).
        let lang = settings.source_lang.clone();
        let mut child = Command::new("tesseract")
            .args(["stdin", "stdout", "-l", &lang, "--psm", "6"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| explain(e.to_string(), &lang, true).unwrap_or(OcrError::Spawn(e)))?;

        let mut stdin = child.stdin.take().expect("piped");
        let mut stdout = child.stdout.take().expect("piped");
        let mut stderr = child.stderr.take().expect("piped");
        // Запись и чтение параллельно, иначе возможен deadlock на больших кадрах.
        let write = async move {
            if let Err(e) = stdin.write_all(&png).await {
                tracing::error!(component = "tesseract", error = %e, "Не удалось передать кадр OCR");
            }
            drop(stdin);
        };
        let read = async {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let (output, diagnostic) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
            if let Err(e) = output { tracing::error!(component = "tesseract", error = %e, "Ошибка чтения stdout OCR"); }
            if let Err(e) = diagnostic { tracing::error!(component = "tesseract", error = %e, "Ошибка чтения stderr OCR"); }
            (out, err)
        };
        let ((out, err), ()) = tokio::join!(read, write);
        let status = child.wait().await.map_err(OcrError::Spawn)?;
        if !err.is_empty() {
            tracing::debug!(component = "tesseract", stderr = %String::from_utf8_lossy(&err), "Диагностика процесса OCR");
        }
        if !status.success() {
            let msg = String::from_utf8_lossy(&err).trim().to_string();
            return Err(explain(msg.clone(), &lang, false).unwrap_or(OcrError::Failed(msg)));
        }
        tracing::debug!(engine = "tesseract", bytes = out.len(), "OCR завершён");
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn preprocess_doubles_and_grays() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(10, 5, Rgba([200, 10, 10, 255])));
        let p = preprocess(&img);
        assert_eq!((p.width(), p.height()), (20, 10));
    }

    #[tokio::test]
    async fn rejects_unknown_engine() {
        let s = Settings { ocr_engine: "nope".into(), ..Settings::default() };
        let img = DynamicImage::new_rgba8(4, 4);
        assert!(matches!(Tesseract.recognize(&img, &s).await, Err(OcrError::UnknownEngine(_))));
    }
}
