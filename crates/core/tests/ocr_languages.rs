//! Real Tesseract on rendered game-like text in English, Russian and Japanese (a CJK script), with and without the
//! image filters. Skipped per language where the model is not installed, and everywhere without Tesseract, Python/PIL
//! or the bundled fonts. Run with `--nocapture` to see the report. By default a short set runs (about ten seconds);
//! `LIPAX_OCR_FULL=1` runs every language on every background with every filter preset (about a minute).

use image::DynamicImage;
use lipa_core::ocr::filter::{FrameStats, PRESETS, Preprocess, pick_best, score};
use lipa_core::ocr::{AnyOcr, Ocr};
use lipa_core::settings::Settings;
use lipa_core::tesseract::TesseractManager;
use lipa_core::text::{normalize, similarity};
use std::path::Path;

struct Case {
    name: &'static str,
    language: &'static str,
    font: &'static str,
    text: &'static str,
    size: u32,
}

const CASES: [Case; 5] = [
    Case { name: "English", language: "eng", font: "Inter.ttf", text: "Where are you going, stranger? Follow me.", size: 34 },
    Case { name: "English small", language: "eng", font: "Inter.ttf", text: "Press any key to continue the game", size: 20 },
    Case { name: "Russian", language: "rus", font: "Inter.ttf", text: "Куда ты идёшь, незнакомец? Следуй за мной.", size: 34 },
    Case { name: "Russian small", language: "rus", font: "Inter.ttf", text: "Нажмите любую клавишу для продолжения", size: 20 },
    Case { name: "Japanese", language: "jpn", font: "NotoSansCJK-VF.otf", text: "こんにちは、旅人さん。どこへ行くのですか？", size: 36 },
];

/// `flat-dark`, `flat-light` or `busy` (a gradient with a texture, as behind game subtitles).
fn render(case: &Case, background: &str, png: &Path) -> bool {
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts");
    let script = r#"
import sys, random, math
from PIL import Image, ImageDraw, ImageFont
font, text, size, background, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
f = ImageFont.truetype(font, size)
w = int(f.getlength(text)) + 80
h = size * 2 + 40
random.seed(7)
if background == 'flat-light':
    im = Image.new('RGB', (w, h), (236, 232, 222)); ink = (30, 30, 36)
elif background == 'busy':
    im = Image.new('RGB', (w, h)); px = im.load()
    for y in range(h):
        for x in range(w):
            v = 70 + int(22 * math.sin(x / 23.0 + y / 11.0)) + random.randint(-9, 9)
            px[x, y] = (max(0, v - 10), max(0, v), min(255, v + 25))
    ink = (250, 250, 245)
else:
    im = Image.new('RGB', (w, h), (22, 26, 36)); ink = (240, 240, 240)
d = ImageDraw.Draw(im)
if background == 'busy':
    d.text((41, size // 2 + 21), text, font=f, fill=(0, 0, 0))
d.text((40, size // 2 + 20), text, font=f, fill=ink)
im.save(out)
"#;
    let font = fonts.join(case.font);
    std::process::Command::new("python3").args(["-c", script]).arg(&font).arg(case.text).arg(case.size.to_string()).arg(background).arg(png)
        .status().is_ok_and(|s| s.success())
}

fn installed(language: &str) -> bool {
    let status = TesseractManager::system().detect();
    status.installed && status.languages.iter().any(|l| l.code == language)
}

/// Spaces do not count: Japanese has none and the engines disagree about them elsewhere.
fn squash(text: &str) -> String {
    normalize(text).chars().filter(|c| !c.is_whitespace()).collect()
}

/// A reading with the settings a new user has: no filter by hand, automatic choice on, the default threshold.
async fn read_default(img: &DynamicImage, language: &str) -> (String, Option<f32>) {
    let mut settings = Settings::default();
    settings.recognition.language = language.into();
    let result = AnyOcr::default().recognize_detailed(img, &settings).await.expect("tesseract runs");
    (squash(&result.text), result.confidence)
}

/// Text, confidence and the auto-tune score of one reading with the given filters.
async fn read(img: &DynamicImage, language: &str, filters: Preprocess) -> (String, Option<f32>, f32) {
    let mut settings = Settings::default();
    settings.recognition.language = language.into();
    settings.recognition.minimum_confidence = 0;
    // Each preset is exactly what it says; the automatic choice is tested on its own below.
    settings.recognition.auto_filters = false;
    (settings.recognition.binarize, settings.recognition.auto_invert, settings.recognition.contrast, settings.recognition.sharpen) =
        (filters.binarize, filters.auto_invert, filters.contrast, filters.sharpen);
    let result = AnyOcr::default().recognize_detailed(img, &settings).await.expect("tesseract runs");
    (squash(&result.text), result.confidence, score(&result))
}

#[tokio::test]
async fn english_russian_and_japanese_are_read_and_auto_tune_finds_the_best_filters() {
    if !TesseractManager::system().detect().installed {
        eprintln!("skipped: no Tesseract");
        return;
    }
    let full = std::env::var_os("LIPAX_OCR_FULL").is_some();
    let mut report = String::new();
    let mut checked = 0;
    for case in &CASES {
        if !installed(case.language) {
            eprintln!("skipped {}: no `{}` model", case.name, case.language);
            continue;
        }
        let expected = squash(case.text);
        for background in ["flat-dark", "flat-light", "busy"] {
            // The short set: clean text read as shipped, and the hard backgrounds with every preset where it matters most.
            let hard = background == "busy" && matches!(case.name, "Russian small" | "Japanese");
            if !full && background != "flat-dark" && !hard { continue; }
            let presets: &[(&str, Preprocess)] = if full || hard { &PRESETS } else { &PRESETS[..1] };
            let png = std::env::temp_dir().join(format!("lipax-lang-{}-{}-{background}.png", std::process::id(), case.language));
            if !render(case, background, &png) { eprintln!("skipped: no Python/PIL or font"); return; }
            let img = image::open(&png).unwrap();
            let _ = std::fs::remove_file(&png);
            // Every preset, as auto-tune does; the first one is «no filters».
            let mut readings = Vec::new();
            for &(label, filters) in presets {
                let (text, confidence, points) = read(&img, case.language, filters).await;
                let similarity = similarity(&expected, &text);
                report.push_str(&format!("{:14} {:10} {:24} similarity {:.2} confidence {:>3} score {:>5.0}  {}\n", case.name, background, label, similarity, confidence.map_or("-".into(), |c| format!("{c:.0}")), points, text.chars().take(48).collect::<String>()));
                readings.push((label, similarity, confidence, points));
            }
            let stats = FrameStats::of(&img);
            // A tight crop of the text, as the in-place engine hands it to OCR: antialiased glyphs fill most of it.
            let tight = img.crop_imm(30, 8, img.width().saturating_sub(60), img.height().saturating_sub(16));
            let tight_stats = FrameStats::of(&tight);
            report.push_str(&format!("{:14} {:10} frame: texture {:.2} entropy {:.2} bits{}; tight crop: texture {:.2}{}\n", case.name, background, stats.texture, stats.entropy, if stats.is_noisy() { " (noisy)" } else { "" }, tight_stats.texture, if tight_stats.is_noisy() { " (noisy)" } else { "" }));
            assert_eq!(tight_stats.is_noisy(), background == "busy", "{} on {background}: a tight crop is classified wrongly (texture {:.2})\n{report}", case.name, tight_stats.texture);
            // The default settings: nothing switched on by hand, the program looks at the frame.
            let (auto_text, auto_confidence) = read_default(&img, case.language).await;
            let auto_similarity = similarity(&expected, &auto_text);
            report.push_str(&format!("{:14} {:10} {:24} similarity {:.2} confidence {:>3}  {}\n", case.name, background, "AUTOMATIC (defaults)", auto_similarity, auto_confidence.map_or("-".into(), |c| format!("{c:.0}")), auto_text.chars().take(48).collect::<String>()));
            assert!(auto_similarity >= 0.85, "{} on {background}: the default settings read it at {auto_similarity:.2}\n{report}", case.name);
            assert_eq!(stats.is_noisy(), background == "busy", "{} on {background}: the frame is classified wrongly (texture {:.2})", case.name, stats.texture);
            let best = pick_best(&readings.iter().map(|r| r.3).collect::<Vec<_>>());
            report.push_str(&format!("{:14} {:10} => auto-tune picks «{}»\n", case.name, background, readings[best].0));
            // The shipped default (no filters) reads clean text.
            if background != "busy" {
                assert!(readings[0].1 >= 0.9, "{} on {background}: raw similarity {:.2}\n{report}", case.name, readings[0].1);
            }
            // Whatever auto-tune picks reads the text, or at least is not trusted (below the default threshold of 30).
            let picked = readings[best];
            assert!(picked.1 >= 0.8 || picked.2.unwrap_or(0.0) < 35.0, "{} on {background}: picked «{}» reads it at {:.2} with confidence {:?}\n{report}", case.name, picked.0, picked.1, picked.2);
            checked += 1;
        }
    }
    eprintln!("{report}");
    assert!(checked > 0 || !installed("eng"), "nothing was checked");
}
