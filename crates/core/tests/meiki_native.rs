//! Opt-in end-to-end test of MeikiOCR with the real models and ONNX Runtime.
//!
//! LIPAX_RAPID_TEST_MODELS=~/.local/share/LipaX/ocr-models \
//! cargo test --release -p lipa-core --test meiki_native -- --ignored --nocapture --test-threads=1
//!
//! The directory holds `meiki-ja/` (downloaded in the settings or with `cargo run --example ocr_model -- download
//! meiki-ja`). `LIPAX_RAPID_TEST_ORT` names the ONNX Runtime library (default: the system one). The text is drawn with
//! the bundled Noto Sans CJK (see `common`).

mod common;

use image::{DynamicImage, GenericImage, Rgb, RgbImage, Rgba, RgbaImage};
use lipa_core::ocr::meiki::MeikiOcr;
use lipa_core::ocr::{Ocr, OcrResult};
use lipa_core::settings::{OcrEngine, Settings};
use lipa_core::text::normalize;
use std::path::PathBuf;
use std::time::Instant;

const LINES: [&str; 3] = ["こんにちは、旅人さん。どこへ行くのですか？", "この先は危険だ。引き返した方がいい。", "魔王を倒すには、伝説の剣が必要です。"];

fn engine() -> MeikiOcr {
    let models = std::env::var_os("LIPAX_RAPID_TEST_MODELS").map(PathBuf::from).expect("LIPAX_RAPID_TEST_MODELS: directory with the installed models");
    MeikiOcr::new(models, std::env::var_os("LIPAX_RAPID_TEST_ORT").map(PathBuf::from))
}

fn settings() -> Settings {
    let mut s = Settings::default();
    s.recognition.engine = OcrEngine::MeikiOcr;
    s.recognition.language = "jpn".into();
    s
}

fn squash(text: &str) -> String {
    normalize(text).chars().filter(|c| !c.is_whitespace()).collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
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

fn cer(expected: &str, got: &str) -> f32 {
    let (e, g): (Vec<char>, Vec<char>) = (squash(expected).chars().collect(), squash(got).chars().collect());
    edit_distance(&e, &g) as f32 / e.len().max(1) as f32
}

/// Dark strip with the lines of text one under another.
fn strip(lines: &[&str], size: u32) -> Option<DynamicImage> {
    let text = common::Text::load("NotoSansCJK-VF.otf", size)?;
    let width = lines.iter().map(|l| text.width(l) as u32).max()? + 80;
    let mut im = RgbImage::from_pixel(width, lines.len() as u32 * (size * 3 / 2) + 60, Rgb([22, 26, 36]));
    for (i, line) in lines.iter().enumerate() {
        text.draw(&mut im, 40.0, 30.0 + (i as u32 * size * 3 / 2) as f32, line, [240, 240, 240]);
    }
    Some(DynamicImage::ImageRgb8(im))
}

/// Dark strip with the text written in a column, one character under another (columns run right to left).
fn column_spaced(columns: &[&str], size: u32, num: u32, den: u32) -> Option<DynamicImage> {
    let text = common::Text::load("NotoSansCJK-VF.otf", size)?;
    let height = columns.iter().map(|c| c.chars().count() as u32).max()? * size * num / den + 80;
    let width = columns.len() as u32 * size * 2 + 80;
    let mut im = RgbImage::from_pixel(width, height, Rgb([22, 26, 36]));
    for (i, column) in columns.iter().enumerate() {
        let x = (width - 40 - size) as f32 - (i as u32 * size * 2) as f32;
        for (j, c) in column.chars().enumerate() {
            text.draw(&mut im, x, 40.0 + (j as u32 * size * num / den) as f32, &c.to_string(), [240, 240, 240]);
        }
    }
    Some(DynamicImage::ImageRgb8(im))
}

/// A game-like frame: a dark gradient with some grain, the text where subtitles sit.
fn frame(text: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    let mut noise = common::Noise::new(7);
    let mut canvas = RgbaImage::from_fn(width, height, |x, y| {
        let v = 30.0 + 25.0 * (x as f32 / width as f32) + 15.0 * (y as f32 / height as f32) + noise.next(4) as f32;
        Rgba([v as u8, (v * 1.05) as u8, (v * 1.25) as u8, 255])
    });
    let (w, h) = (text.width().min(width), text.height().min(height));
    canvas.copy_from(&text.crop_imm(0, 0, w, h).to_rgba8(), (width - w) / 2, height.saturating_sub(h + height / 10)).unwrap();
    DynamicImage::ImageRgba8(canvas)
}

async fn read(ocr: &MeikiOcr, img: &DynamicImage) -> OcrResult {
    ocr.recognize_detailed(img, &settings()).await.unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
#[ignore = "requires the MeikiOCR models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime"]
async fn reads_japanese_lines_in_strips_and_game_frames() {
    let ocr = engine();
    let mut report = String::new();
    for (i, line) in LINES.iter().enumerate() {
        let Some(strip) = strip(&[line], 36) else { eprintln!("skipped: no font"); return };
        for (view, img) in [("strip", strip.clone()), ("1280×200", frame(&strip, 1280, 200)), ("1920×1080", frame(&strip, 1920, 1080))] {
            let r = read(&ocr, &img).await;
            let error = cer(line, &r.text);
            report.push_str(&format!("line {i} {view:10} CER {error:.3} confidence {:?} lines {}  {}\n", r.confidence.map(|c| c.round()), r.lines.len(), r.text.replace('\n', " / ")));
            assert_eq!(r.engine, "meikiocr");
            assert!(error <= 0.15, "line {i} ({view}): CER {error:.3}\n{report}");
            for l in &r.lines {
                assert!(l.rect.x >= 0.0 && l.rect.y >= 0.0 && l.rect.x + l.rect.w <= img.width() as f32 && l.rect.y + l.rect.h <= img.height() as f32, "{:?}", l.rect);
            }
        }
    }
    eprintln!("{report}");
}

#[tokio::test]
#[ignore = "requires the MeikiOCR models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime"]
async fn reads_several_lines_top_down_and_other_text_sizes() {
    let ocr = engine();
    let Some(three) = strip(&LINES, 32) else { return };
    let r = read(&ocr, &three).await;
    eprintln!("three lines: {}", r.text.replace('\n', " / "));
    assert_eq!(r.lines.len(), 3, "{:?}", r.lines);
    for (line, got) in LINES.iter().zip(r.text.lines()) {
        assert!(cer(line, got) <= 0.15, "{line} ≠ {got}");
    }
    assert!(r.lines.windows(2).all(|w| w[0].rect.y < w[1].rect.y), "top to bottom");
    for size in [20, 28, 48, 64] {
        let img = strip(&[LINES[0]], size).unwrap();
        let r = read(&ocr, &img).await;
        eprintln!("size {size}: CER {:.3}  {}", cer(LINES[0], &r.text), r.text);
        assert!(cer(LINES[0], &r.text) <= 0.25, "size {size}: {}", r.text);
    }
    // A frame without text reads as nothing.
    let empty = frame(&DynamicImage::ImageRgb8(RgbImage::new(8, 8)), 1280, 720);
    assert!(read(&ocr, &empty).await.text.trim().is_empty());
}

/// Vertical text is a beta feature of the models (the authors say so): it is read well when it is set solid, as in
/// games (glyphs touching their neighbours' boxes: CER 0 to 0.08), and worse with loose gaps between the letters, so
/// the limits are looser than for a line. The long column is cut into overlapping segments for the recognizer.
#[tokio::test]
#[ignore = "requires the MeikiOCR models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime"]
async fn reads_vertical_text_short_long_and_in_several_columns() {
    let ocr = engine();
    let short = "伝説の剣を求めて旅に出る";
    let long = "伝説の剣を求めて旅人は遠い山を越え深い森を抜けて古い城へ向かった";
    for (what, columns, limit) in [("short", vec![short], 0.2), ("long, cut into segments", vec![long], 0.25), ("two columns", vec![short, "この先は危険だ"], 0.3)] {
        let r = read(&ocr, &column_spaced(&columns, 40, 1, 1).unwrap()).await;
        let got: Vec<&str> = r.text.lines().collect();
        eprintln!("{what}: {}", got.join(" / "));
        assert_eq!(got.len(), columns.len(), "{what}: {:?}", r.lines);
        for (expected, got) in columns.iter().zip(&got) {
            assert!(cer(expected, got) <= limit, "{what}: {expected} ≠ {got}");
        }
        // Columns run from the right to the left.
        assert!(r.lines.windows(2).all(|w| w[0].rect.x > w[1].rect.x), "{what}: right to left");
    }
}

#[tokio::test]
#[ignore = "requires the MeikiOCR models (LIPAX_RAPID_TEST_MODELS) and ONNX Runtime; slow"]
async fn benchmark_frames() {
    let ocr = engine();
    let strip = strip(&[LINES[0]], 36).unwrap();
    let mut table = String::from("| Кадр | Задержка, мс (медиана) | CER |\n|---|---:|---:|\n");
    for (width, height) in [(1280, 200), (1920, 1080)] {
        let img = frame(&strip, width, height);
        read(&ocr, &img).await;
        let mut times = Vec::new();
        let mut last = OcrResult::default();
        for _ in 0..5 {
            let started = Instant::now();
            last = read(&ocr, &img).await;
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        table.push_str(&format!("| {width}×{height} | {:.0} | {:.3} |\n", times[2], cer(LINES[0], &last.text)));
    }
    eprintln!("\nMeikiOCR threads: {}\n\n{table}", lipa_core::ocr::rapid::effective_threads(0));
}
