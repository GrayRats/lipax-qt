//! OCR. Tesseract вызывается как процесс: PNG на stdin, текст из stdout —
//! без временных файлов и гонок между запусками.

pub mod paddle;
pub mod paddle_env;

use crate::layout::CropRect;
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

/// One line of text found by the engine, in pixels of the image that was given to it: a crop of the
/// frame, hence `CropRect` (`in_frame` moves it into the frame).
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub rect: CropRect,
    pub text: String,
    /// 0–100.
    pub confidence: f32,
}

/// Text plus what the engine knows about it: where the lines are and how sure it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcrResult {
    pub text: String,
    /// Empty if the engine reports no geometry.
    pub lines: Vec<OcrLine>,
    /// 0–100, mean over words; `None` if the engine does not report it.
    pub confidence: Option<f32>,
    /// Which engine produced the text (the choice of `auto` is visible here).
    pub engine: &'static str,
}

impl OcrResult {
    pub fn text_only(text: String, engine: &'static str) -> Self {
        Self { text, lines: Vec::new(), confidence: None, engine }
    }
}

pub trait Ocr: Send + Sync {
    fn recognize(
        &self,
        img: &DynamicImage,
        settings: &Settings,
    ) -> impl Future<Output = Result<String, OcrError>> + Send;

    /// Like `recognize`, with geometry and confidence where the engine has them. The default
    /// is the plain text: a new engine works without it and just has no confidence gate.
    fn recognize_detailed<'a>(
        &'a self,
        img: &'a DynamicImage,
        settings: &'a Settings,
    ) -> impl Future<Output = Result<OcrResult, OcrError>> + Send + 'a {
        async move { Ok(OcrResult::text_only(self.recognize(img, settings).await?, "")) }
    }
}

/// `auto`: Tesseract is fast; below this confidence PaddleOCR gets a chance.
pub const AUTO_ACCEPT: f32 = 60.0;
/// After PaddleOCR failed in `auto` (not installed, no model), it is not tried again for this long:
/// every attempt costs a Python start.
const PADDLE_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Default)]
pub struct AnyOcr {
    paddle: paddle::PaddleOcr,
    paddle_down_until: std::sync::Mutex<Option<std::time::Instant>>,
}

impl AnyOcr {
    async fn auto(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        let first = Tesseract.run_detailed(img, &settings.recognition.language).await?;
        // A result is good enough unless the engine itself is unsure.
        if first.confidence.is_none_or(|c| c >= AUTO_ACCEPT) { return Ok(first); }
        let down = self.paddle_down_until.lock().unwrap().is_some_and(|t| std::time::Instant::now() < t);
        if down { return Ok(first); }
        match self.paddle.recognize_detailed(img, settings).await {
            Ok(second) if !second.text.trim().is_empty() && second.confidence.is_none_or(|c| c > first.confidence.unwrap_or(0.0)) => {
                tracing::debug!(engine = "auto", tesseract = ?first.confidence, paddleocr = ?second.confidence, "OCR: PaddleOCR is more sure");
                Ok(second)
            }
            Ok(_) => Ok(first),
            Err(e) => {
                tracing::debug!(engine = "auto", error = %e, "OCR: PaddleOCR unavailable, Tesseract result kept");
                *self.paddle_down_until.lock().unwrap() = Some(std::time::Instant::now() + PADDLE_RETRY_AFTER);
                Ok(first)
            }
        }
    }
}

impl Ocr for AnyOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        match settings.recognition.engine.as_str() {
            "tesseract" => Tesseract.recognize(img, settings).await,
            "paddleocr" => self.paddle.recognize(img, settings).await,
            "auto" => Ok(self.auto(img, settings).await?.text),
            _ => Err(OcrError::UnknownEngine(settings.recognition.engine.as_str().to_owned())),
        }
    }

    async fn recognize_detailed(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        match settings.recognition.engine.as_str() {
            "tesseract" => Tesseract.run_detailed(img, &settings.recognition.language).await,
            "paddleocr" => self.paddle.recognize_detailed(img, settings).await,
            "auto" => self.auto(img, settings).await,
            _ => Err(OcrError::UnknownEngine(settings.recognition.engine.as_str().to_owned())),
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

/// Tesseract processes the image enlarged twice; geometry from it is scaled back.
const SCALE: f32 = 2.0;

impl Tesseract {
    /// Runs the `tesseract` process on the preprocessed image and returns what it printed.
    async fn run(img: &DynamicImage, lang: &str, extra: &[&str]) -> Result<Vec<u8>, OcrError> {
        tracing::debug!(engine = "tesseract", width = img.width(), height = img.height(), language = %lang, "Начало OCR");
        let mut png = Vec::new();
        preprocess(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;

        // Языки выбираются в настройках: основной первым, дополнительные через «+» (например, `jpn+eng`).
        let lang = lang.to_owned();
        let mut child = Command::new("tesseract")
            .args(["stdin", "stdout", "-l", &lang, "--psm", "6"])
            // Скачанные модели лежат в каталоге пользователя (рядом со ссылками на системные).
            .args(crate::tesseract::tessdata_arg().iter().flat_map(|dir| ["--tessdata-dir".to_string(), dir.display().to_string()]))
            .args(extra)
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
        Ok(out)
    }

    /// Text, lines and confidence from the TSV output.
    pub async fn run_detailed(&self, img: &DynamicImage, lang: &str) -> Result<OcrResult, OcrError> {
        let out = Self::run(img, lang, &["tsv"]).await?;
        Ok(parse_tsv(&String::from_utf8_lossy(&out)))
    }
}

/// CJK text has no spaces between words; Tesseract's TSV still splits it into tokens.
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xFF00..=0xFFEF)
}

fn join_words(line: &mut String, word: &str) {
    let tight = line.chars().next_back().zip(word.chars().next()).is_some_and(|(a, b)| is_cjk(a) || is_cjk(b));
    if !line.is_empty() && !tight { line.push(' '); }
    line.push_str(word);
}

/// Parse `tesseract … tsv`: one row per word with block/paragraph/line numbers, box and confidence.
/// Words are grouped into lines; the line box is in pixels of the image given to `recognize`
/// (the 2x enlargement is undone), the line confidence is the mean over its words.
pub fn parse_tsv(tsv: &str) -> OcrResult {
    struct Line { rect: (f32, f32, f32, f32), text: String, confidence: Vec<f32> }
    let mut lines: Vec<((u32, u32, u32), Line)> = Vec::new();
    for row in tsv.lines().skip(1) {
        let cols: Vec<&str> = row.splitn(12, '\t').collect();
        if cols.len() < 12 || cols[0] != "5" { continue; }
        let text = cols[11].trim();
        let conf: f32 = cols[10].parse().unwrap_or(-1.0);
        if text.is_empty() || conf < 0.0 { continue; }
        let n = |i: usize| cols[i].parse::<f32>().unwrap_or(0.0);
        let key = (cols[2].parse().unwrap_or(0), cols[3].parse().unwrap_or(0), cols[4].parse().unwrap_or(0));
        let (x, y, w, h) = (n(6), n(7), n(8), n(9));
        match lines.iter_mut().find(|(k, _)| *k == key) {
            Some((_, line)) => {
                let (lx, ly, lw, lh) = line.rect;
                let (x2, y2) = ((lx + lw).max(x + w), (ly + lh).max(y + h));
                let (x1, y1) = (lx.min(x), ly.min(y));
                line.rect = (x1, y1, x2 - x1, y2 - y1);
                join_words(&mut line.text, text);
                line.confidence.push(conf);
            }
            None => lines.push((key, Line { rect: (x, y, w, h), text: text.to_owned(), confidence: vec![conf] })),
        }
    }
    let all: Vec<f32> = lines.iter().flat_map(|(_, l)| l.confidence.iter().copied()).collect();
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let lines: Vec<OcrLine> = lines.into_iter().map(|(_, l)| OcrLine {
        rect: CropRect::in_space(l.rect.0 / SCALE, l.rect.1 / SCALE, l.rect.2 / SCALE, l.rect.3 / SCALE),
        confidence: mean(&l.confidence),
        text: l.text,
    }).collect();
    OcrResult {
        text: lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"),
        confidence: (!all.is_empty()).then(|| mean(&all)),
        lines,
        engine: "tesseract",
    }
}

impl Ocr for Tesseract {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        if settings.recognition.engine != "tesseract" {
            return Err(OcrError::UnknownEngine(settings.recognition.engine.as_str().to_owned()));
        }
        let out = Self::run(img, &settings.recognition.language, &[]).await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    async fn recognize_detailed(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        if settings.recognition.engine != "tesseract" {
            return Err(OcrError::UnknownEngine(settings.recognition.engine.as_str().to_owned()));
        }
        self.run_detailed(img, &settings.recognition.language).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::ops::Not;

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
1\t1\t0\t0\t0\t0\t0\t0\t400\t200\t-1\t\n\
4\t1\t1\t1\t1\t0\t20\t10\t300\t40\t-1\t\n\
5\t1\t1\t1\t1\t1\t20\t10\t100\t40\t96.5\tWhere\n\
5\t1\t1\t1\t1\t2\t140\t14\t80\t36\t90.0\tare\n\
5\t1\t1\t1\t1\t3\t240\t10\t80\t40\t30.0\tyou?\n\
5\t1\t1\t1\t2\t1\t20\t80\t60\t30\t80.0\tGoing\n\
5\t1\t1\t1\t2\t2\t90\t80\t10\t30\t-1\t \n";

    #[test]
    fn tsv_words_become_lines_with_geometry_and_confidence() {
        let r = parse_tsv(TSV);
        assert_eq!(r.text, "Where are you?\nGoing");
        assert_eq!(r.lines.len(), 2);
        // The 2x enlargement is undone: the first line spans x 20..320, y 10..50 in the enlarged image.
        assert_eq!(r.lines[0].rect, CropRect::in_space(10.0, 5.0, 150.0, 20.0));
        assert!((r.lines[0].confidence - 72.1667).abs() < 0.01, "{}", r.lines[0].confidence);
        // Blank words and rows that are not words do not count.
        assert_eq!(r.lines[1].text, "Going");
        assert!((r.confidence.unwrap() - 74.125).abs() < 0.01, "{:?}", r.confidence);
        assert_eq!(r.engine, "tesseract");
    }

    #[test]
    fn empty_or_garbage_tsv_means_no_text_and_no_confidence() {
        for tsv in ["", "level\tpage_num\n", "garbage without tabs"] {
            let r = parse_tsv(tsv);
            assert!(r.text.is_empty() && r.lines.is_empty() && r.confidence.is_none(), "{tsv:?}");
        }
    }

    #[test]
    fn cjk_words_are_not_separated_by_spaces() {
        let mut line = String::new();
        for word in ["こんにちは", "世界", "Hello", "world"] { join_words(&mut line, word); }
        assert_eq!(line, "こんにちは世界Hello world");
    }

    /// Real Tesseract on a rendered line of text; skipped where Tesseract, Python/PIL or the font are missing.
    #[tokio::test]
    async fn real_tesseract_reports_lines_and_confidence() {
        let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts/Inter.ttf");
        let png = std::env::temp_dir().join(format!("lipax-ocr-{}.png", std::process::id()));
        let script = "import sys\nfrom PIL import Image, ImageDraw, ImageFont\nim = Image.new('RGB', (420, 120), (20, 20, 30))\nd = ImageDraw.Draw(im)\nf = ImageFont.truetype(sys.argv[1], 36)\nd.text((20, 10), 'Where are you going?', font=f, fill=(240, 240, 240))\nd.text((20, 64), 'Follow me', font=f, fill=(240, 240, 240))\nim.save(sys.argv[2])";
        let rendered = std::process::Command::new("python3").args(["-c", script]).arg(&font).arg(&png).status().is_ok_and(|s| s.success());
        if !rendered || TesseractManager::system().detect().installed.not() {
            eprintln!("skipped: no Tesseract, PIL or font");
            return;
        }
        let img = image::open(&png).unwrap();
        let _ = std::fs::remove_file(&png);
        let r = Tesseract.run_detailed(&img, "eng").await.unwrap();
        assert!(r.text.contains("Where are you going") && r.text.contains("Follow me"), "{:?}", r.text);
        assert_eq!(r.lines.len(), 2, "{:?}", r.lines);
        assert!(r.confidence.unwrap() > 70.0, "{:?}", r.confidence);
        // Lines are in pixels of the input image, top to bottom, inside it.
        assert!(r.lines[0].rect.y < r.lines[1].rect.y);
        for line in &r.lines {
            assert!(line.rect.x >= 0.0 && line.rect.y >= 0.0 && line.rect.x + line.rect.w <= 420.0 && line.rect.y + line.rect.h <= 120.0, "{:?}", line.rect);
        }
        assert!(r.lines[0].rect.w > 200.0, "the first line is wide: {:?}", r.lines[0].rect);
    }

    #[test]
    fn preprocess_doubles_and_grays() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(10, 5, Rgba([200, 10, 10, 255])));
        let p = preprocess(&img);
        assert_eq!((p.width(), p.height()), (20, 10));
    }

    #[tokio::test]
    async fn rejects_unknown_engine() {
        let s = { let mut value = Settings::default(); value.recognition.engine = "nope".into(); value };
        let img = DynamicImage::new_rgba8(4, 4);
        assert!(matches!(Tesseract.recognize(&img, &s).await, Err(OcrError::UnknownEngine(_))));
    }
}
