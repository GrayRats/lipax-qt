//! The in-place engine on game-like scenes drawn with real fonts
//! (packaging/render-scene-fixtures.py): detection of independent fields, font choice, fitting
//! and overlap protection, end to end.

use lipa_core::capture::kwin::WindowGeometry;
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

fn expectations() -> HashMap<String, Expect> {
    let file = std::fs::File::open(dir().join("manifest.json")).expect("scene manifest");
    serde_json::from_reader(file).expect("valid scene manifest")
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

#[test]
fn scene_quality_thresholds() {
    let mut correct_style = 0;
    let mut total_style = 0;
    for (scene, expect) in expectations() {
        let frame = run(&scene);
        assert_eq!(frame.blocks.len(), expect.fields, "{scene}: detected fields");
        assert_eq!(frame.blocks.iter().filter(|b| b.style.source_lines > 1).count(), expect.multiline,
            "{scene}: multiline fields must remain grouped");
        let mut blocks: Vec<_> = frame.blocks.iter().collect();
        blocks.sort_by(|a, b| a.text_rect.y.total_cmp(&b.text_rect.y).then(a.text_rect.x.total_cmp(&b.text_rect.x)));
        assert_eq!(expect.kinds.len(), blocks.len(), "{scene}: expected style labels");
        for (index, (block, wanted)) in blocks.iter().zip(&expect.kinds).enumerate() {
            let actual = kind(&block.font.family);
            if scene.starts_with("menu") {
                assert_eq!(actual, wanted, "{scene}: menu row {index} must keep its monospaced family even when highlighted");
            }
            correct_style += usize::from(actual == wanted);
            total_style += 1;
        }
        let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
        let placed = place_regions(&[region], &WindowGeometry::from([0.0, 0.0, frame.frame.0 as f64, frame.frame.1 as f64]),
            &settings().inplace, &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default());
        assert_eq!(placed.placed.len(), expect.fields, "{scene}: translated fields fit without collisions");
        assert!(placed.fallback.is_empty(), "{scene}: nothing is sent to the translation window");
        assert!(placed.placed.iter().all(|p| p.degraded.is_none()), "{scene}: a scene that fits is not degraded: {:?}", placed.placed.iter().map(|p| p.degraded).collect::<Vec<_>>());
    }
    assert!(correct_style * 10 >= total_style * 9,
        "font style accuracy {correct_style}/{total_style} is below 90%");
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
    for scene in ["dialogue", "subtitles", "menu", "menu_selected_quit"] {
        let frame = run(scene);
        println!("== {scene}: {} fields, frame {:?}", frame.blocks.len(), frame.frame);
        for b in &frame.blocks {
            println!("  #{:<2} {:?} lines={} rect=({:.0},{:.0},{:.0},{:.0}) font={} ({}) weight={} italic={} bg={:?} text={}",
                b.id, b.block_type, b.style.source_lines, b.text_rect.x, b.text_rect.y, b.text_rect.w, b.text_rect.h,
                b.font.family, kind(&b.font.family), b.style.font_weight.value(), b.style.italic, b.background.mode, b.style.alignment as u8);
        }
        let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
        let placed = place_regions(&[region], &WindowGeometry::from([0.0, 0.0, frame.frame.0 as f64, frame.frame.1 as f64]), &settings().inplace, &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default()).placed;
        println!("  placed {} of {}", placed.len(), frame.blocks.len());
    }
}
