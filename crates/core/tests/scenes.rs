//! The in-place engine on game-like scenes drawn with real fonts
//! (packaging/render-scene-fixtures.py): detection of independent fields, font choice, fitting
//! and overlap protection, end to end.

use image::DynamicImage;
use lipa_core::layout::engine::{InplaceEngine, InplaceFrame};
use lipa_core::layout::fit::ApproxMeasure;
use lipa_core::layout::font_database::InstalledFontDatabase;
use lipa_core::layout::place::{PlacementCache, RegionInput, place_regions};
use lipa_core::settings::{NormRect, Settings, TranslationDisplay};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Deserialize)]
struct Expect {
    fields: usize,
    multiline: usize,
    kinds: Vec<String>,
}

fn dir() -> PathBuf {
    std::env::var_os("LIPA_SCENES").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenes"))
}

fn settings() -> Settings {
    Settings { target_lang: "ru".into(), translation_display: TranslationDisplay::Inplace, ..Settings::default() }
}

/// What the OCR + translator would return: the same text in Russian, a bit longer than English.
fn translate(original_index: usize) -> (String, String) {
    (format!("original {original_index}"), format!("Это переведённый текст поля номер {original_index}, он заметно длиннее оригинала"))
}

fn run(scene: &str) -> InplaceFrame {
    let img: DynamicImage = image::open(dir().join(format!("{scene}.png"))).unwrap_or_else(|e| panic!("{scene}: {e}"));
    let s = settings();
    let mut engine = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = engine.begin(img, &s, Instant::now(), false);
    for (i, job) in jobs.into_iter().enumerate() { engine.complete(job.id, Some(translate(i)), &s); }
    engine.finish(&s).unwrap_or_else(|| panic!("{scene}: nothing to show"))
}

fn kind(family: &str) -> &'static str {
    let db = InstalledFontDatabase::bundled();
    match db.find(family).map(|f| f.category) {
        Some(c) if c.is_serif() || c == lipa_core::layout::FontCategory::SlabSerif => "serif",
        Some(lipa_core::layout::FontCategory::Monospace) => "mono",
        Some(_) => "sans",
        None => "none",
    }
}

/// `LIPA_SCENES=dir cargo test -p lipa-core --test scenes scene_report -- --ignored --nocapture`
#[test]
#[ignore]
fn scene_report() {
    for scene in ["dialogue", "subtitles", "menu"] {
        let frame = run(scene);
        println!("== {scene}: {} fields, frame {:?}", frame.blocks.len(), frame.frame);
        for b in &frame.blocks {
            println!("  #{:<2} {:?} lines={} rect=({:.0},{:.0},{:.0},{:.0}) font={} ({}) weight={} italic={} bg={:?} text={}",
                b.id, b.block_type, b.style.source_lines, b.text_rect.x, b.text_rect.y, b.text_rect.w, b.text_rect.h,
                b.font.family, kind(&b.font.family), b.style.font_weight.value(), b.style.italic, b.background.mode, b.style.alignment as u8);
        }
        let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
        let placed = place_regions(&[region], [0.0, 0.0, frame.frame.0 as f64, frame.frame.1 as f64], &settings().inplace, &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default());
        println!("  placed {} of {}", placed.len(), frame.blocks.len());
    }
    let _ = (Expect { fields: 0, multiline: 0, kinds: vec![] }.fields,);
}
