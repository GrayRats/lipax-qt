//! Opt-in end-to-end tests of RapidOCR with the real PP-OCRv5 models and ONNX Runtime.
//!
//! LIPAX_RAPID_TEST_MODELS=~/.local/share/LipaX/ocr-models \
//! cargo test --release -p lipa-core --test rapid_native -- --ignored --nocapture --test-threads=1
//!
//! `LIPAX_RAPID_TEST_MODELS` is a directory with the installed models (`en-mobile/`, `eslav-mobile/`, `ch-mobile/`;
//! downloaded in the settings or by hand from `ocr_models_catalog.json`). `LIPAX_RAPID_TEST_ORT` names the ONNX Runtime
//! library (default: the system one), `LIPAX_RAPID_TEST_THREADS` the threads. The benchmark also reads with Tesseract
//! (when it has the language) and PaddleOCR (`LIPAX_RAPID_TEST_PADDLE_PYTHON=/path/to/venv/bin/python`).
//!
//! The English and Russian lines are the committed font fixtures; the Japanese one is rendered like in
//! `ocr_languages.rs` (Python/PIL and the bundled Noto Sans CJK; skipped without them).

use image::{DynamicImage, GenericImage, Rgba, RgbaImage};
use lipa_core::ocr::paddle::PaddleOcr;
use lipa_core::ocr::rapid::RapidOcr;
use lipa_core::ocr::{Ocr, OcrResult, Tesseract};
use lipa_core::settings::{OcrEngine, Settings};
use lipa_core::tesseract::TesseractManager;
use lipa_core::text::normalize;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Case {
    name: &'static str,
    language: &'static str,
    text: &'static str,
    /// A committed fixture, or `None` to render the text with Noto Sans CJK.
    fixture: Option<&'static str>,
    /// Highest character error rate accepted on the clean line.
    max_cer: f32,
}

const CASES: [Case; 3] = [
    Case { name: "English", language: "eng", text: "Hello, how are you today? Quick brown fox", fixture: Some("fonts/Inter-latin-dark.png"), max_cer: 0.1 },
    // The fixture is cut after «ли»: a perfect reading has CER 0.054.
    Case { name: "Russian", language: "rus", text: "Привет, как твои дела сегодня? Быстрая лиса", fixture: Some("fonts/Inter-cyr-dark.png"), max_cer: 0.1 },
    Case { name: "Japanese", language: "jpn", text: "こんにちは、旅人さん。どこへ行くのですか？", fixture: None, max_cer: 0.2 },
];

fn line(case: &Case) -> Option<DynamicImage> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if let Some(file) = case.fixture {
        return Some(image::open(fixtures.join(file)).expect("committed fixture"));
    }
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts/NotoSansCJK-VF.otf");
    let png = std::env::temp_dir().join(format!("lipax-rapid-{}-{}.png", std::process::id(), case.language));
    let script = "import sys\nfrom PIL import Image, ImageDraw, ImageFont\nf = ImageFont.truetype(sys.argv[1], 36)\nw = int(f.getlength(sys.argv[2])) + 80\nim = Image.new('RGB', (w, 112), (22, 26, 36))\nImageDraw.Draw(im).text((40, 38), sys.argv[2], font=f, fill=(240, 240, 240))\nim.save(sys.argv[3])";
    let ok = std::process::Command::new("python3").args(["-c", script]).arg(&font).arg(case.text).arg(&png).status().is_ok_and(|s| s.success());
    let img = ok.then(|| image::open(&png).ok()).flatten();
    let _ = std::fs::remove_file(&png);
    img
}

fn engine() -> RapidOcr {
    let models = std::env::var_os("LIPAX_RAPID_TEST_MODELS").map(PathBuf::from).expect("LIPAX_RAPID_TEST_MODELS: directory with the installed models");
    RapidOcr::new(models, std::env::var_os("LIPAX_RAPID_TEST_ORT").map(PathBuf::from))
}

fn settings(language: &str) -> Settings {
    let mut s = Settings::default();
    s.recognition.engine = OcrEngine::RapidOcr;
    s.recognition.language = language.into();
    s.recognition.rapid_threads = std::env::var("LIPAX_RAPID_TEST_THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(0);
    s
}

/// Spaces do not count: Japanese has none and the engines disagree about them elsewhere.
fn squash(text: &str) -> String {
    normalize(text).chars().filter(|c| !c.is_whitespace()).collect()
}

fn edit_distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            current[j + 1] = (previous[j] + usize::from(x != y)).min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

/// Character error rate without spaces.
fn cer(expected: &str, got: &str) -> f32 {
    let (e, g): (Vec<char>, Vec<char>) = (squash(expected).chars().collect(), squash(got).chars().collect());
    edit_distance(&e, &g) as f32 / e.len().max(1) as f32
}

/// Word error rate; `None` for a language without spaces between words.
fn wer(expected: &str, got: &str, language: &str) -> Option<f32> {
    if matches!(language, "jpn" | "chi_sim" | "chi_tra") {
        return None;
    }
    let words = |t: &str| normalize(t).split(' ').map(str::to_owned).collect::<Vec<_>>();
    let (e, g) = (words(expected), words(got));
    Some(edit_distance(&e, &g) as f32 / e.len().max(1) as f32)
}

#[tokio::test]
#[ignore = "requires the PP-OCRv5 models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime"]
async fn reads_english_russian_and_japanese() {
    let ocr = engine();
    let mut report = String::new();
    for case in &CASES {
        let Some(img) = line(case) else {
            eprintln!("skipped {}: no Python/PIL or font", case.name);
            continue;
        };
        // The line alone, in game frames (where the orientation classifier once turned an upright Cyrillic line and
        // lost it) and upside down.
        let views = [("line", img.clone()), ("1280×200", frame(&img, 1280, 200)), ("1920×1080", frame(&img, 1920, 1080)), ("upside down", img.rotate180())];
        for (view, img) in views {
            let r = ocr.recognize_detailed(&img, &settings(case.language)).await.unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let error = cer(case.text, &r.text);
            report.push_str(&format!("{:9} {view:11} CER {error:.3} confidence {:?} lines {}  {}\n", case.name, r.confidence, r.lines.len(), r.text.replace('\n', " / ")));
            assert_eq!(r.engine, "rapidocr");
            assert!(error <= case.max_cer, "{} ({view}): CER {error:.3}\n{report}", case.name);
            assert!(r.confidence.is_some_and(|c| c >= 50.0), "{} ({view}): {:?}\n{report}", case.name, r.confidence);
            for l in &r.lines {
                assert!(l.rect.x >= 0.0 && l.rect.y >= 0.0 && l.rect.x + l.rect.w <= img.width() as f32 && l.rect.y + l.rect.h <= img.height() as f32, "{}: {:?}", case.name, l.rect);
            }
        }
    }
    eprintln!("{report}");
}

/// A game-like frame: a dark gradient with some grain, the text line where subtitles sit.
fn frame(text: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    let mut state = 7u32;
    let mut canvas = RgbaImage::from_fn(width, height, |x, y| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let v = 30.0 + 25.0 * (x as f32 / width as f32) + 15.0 * (y as f32 / height as f32) + ((state >> 24) % 8) as f32;
        Rgba([v as u8, (v * 1.05) as u8, (v * 1.25) as u8, 255])
    });
    let (w, h) = (text.width().min(width), text.height().min(height));
    let x = (width - w) / 2;
    let y = height.saturating_sub(h + height / 10);
    canvas.copy_from(&text.crop_imm(0, 0, w, h).to_rgba8(), x, y).unwrap();
    DynamicImage::ImageRgba8(canvas)
}

const RUNS: usize = 5;

/// Median latency over `RUNS` readings after one warm-up (models loaded, caches filled), and the last result as the
/// user would see it: lines below the engine's confidence threshold in LipaX and stray marks removed.
async fn measure<F, Fut>(settings: &Settings, mut read: F) -> Result<(f64, OcrResult), String>
where F: FnMut() -> Fut, Fut: std::future::Future<Output = Result<OcrResult, lipa_core::ocr::OcrError>> {
    read().await.map_err(|e| e.to_string())?;
    let mut times = Vec::with_capacity(RUNS);
    let mut last = OcrResult::default();
    for _ in 0..RUNS {
        let started = Instant::now();
        last = read().await.map_err(|e| e.to_string())?;
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok((times[RUNS / 2], lipa_core::ocr::filter::clean(last, settings.effective_minimum_confidence(), settings.recognition.filter_noise)))
}

#[tokio::test]
#[ignore = "requires the PP-OCRv5 models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime; slow"]
async fn benchmark_frames_against_tesseract_and_paddle() {
    let rapid = engine();
    let paddle_python = std::env::var("LIPAX_RAPID_TEST_PADDLE_PYTHON").ok();
    let paddle = PaddleOcr::default();
    // Tesseract dominates the run (about 48 s per 1920×1080 Japanese frame): `LIPAX_RAPID_TEST_SKIP_TESSERACT=1` leaves it out.
    let skip_tesseract = std::env::var_os("LIPAX_RAPID_TEST_SKIP_TESSERACT").is_some();
    let tesseract = TesseractManager::system().detect();
    let mut table = String::from("| Кадр | Язык | Движок | Задержка, мс (медиана) | CER | WER |\n|---|---|---|---:|---:|---:|\n");
    for case in &CASES {
        let Some(text) = line(case) else {
            eprintln!("skipped {}: no Python/PIL or font", case.name);
            continue;
        };
        for (width, height) in [(1280, 200), (1920, 1080)] {
            let img = frame(&text, width, height);
            let mut row = |engine: &str, outcome: Result<(f64, OcrResult), String>| {
                let cells = match outcome {
                    Ok((ms, r)) => format!("{ms:.0} | {:.3} | {}", cer(case.text, &r.text), wer(case.text, &r.text, case.language).map_or("—".into(), |w| format!("{w:.3}"))),
                    Err(e) => format!("ошибка: {} | — | —", e.chars().take(60).collect::<String>()),
                };
                table.push_str(&format!("| {width}×{height} | {} | {engine} | {cells} |\n", case.language));
            };
            let s = settings(case.language);
            row("RapidOCR (PP-OCRv5 mobile)", measure(&s, || rapid.recognize_detailed(&img, &s)).await);
            if !skip_tesseract && tesseract.installed && tesseract.languages.iter().any(|l| l.code == case.language) {
                let mut s = s.clone();
                s.recognition.engine = OcrEngine::Tesseract;
                row("Tesseract", measure(&s, || Tesseract.run_detailed(&img, case.language)).await);
            }
            if let Some(python) = &paddle_python {
                let mut s = s.clone();
                s.recognition.engine = OcrEngine::PaddleOcr;
                s.recognition.paddle_python = python.clone();
                row("PaddleOCR 3.x", measure(&s, || paddle.recognize_detailed(&img, &s)).await);
            }
        }
    }
    eprintln!("\nRapidOCR threads: {}, CPU: {} logical cores\n\n{table}", lipa_core::ocr::rapid::effective_threads(settings("eng").recognition.rapid_threads),
        std::thread::available_parallelism().map_or(0, |n| n.get()));
}

#[test]
fn error_rates_count_characters_and_words() {
    assert_eq!(cer("Hello world", "Hello world"), 0.0);
    assert!((cer("abcd", "abed") - 0.25).abs() < 1e-6);
    assert!((wer("one two three four", "one too three four", "eng").unwrap() - 0.25).abs() < 1e-6);
    assert_eq!(wer("こんにちは", "こんにちは", "jpn"), None);
}
