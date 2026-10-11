//! Typography measurement and confidence calibration against independent references.
//!
//! References are *not* produced by the code under test: text is rendered with `ab_glyph` from the bundled
//! font files, so cap height and stem width come from the font's own outlines, and the committed font
//! fixtures (`crates/core/tests/fixtures/fonts`) carry the label of the font that rendered them. What these
//! tests measure is agreement with those references on rendered text, not on arbitrary game art.
use ab_glyph::{Font, FontVec, PxScale, ScaleFont, VariableFont, point};
use image::{Rgba, RgbaImage};
use lipa_core::layout::{
    FontCategory,
    block_detector::BlockDetector,
    font_classifier::{FontAnalysis, FontClassifier},
};
use std::path::{Path, PathBuf};

fn fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts")
}

/// Renders `text` black-on-white-ish into an image: light text on a dark panel, baseline at `base`.
fn render(font: &FontVec, size: f32, text: &str) -> (RgbaImage, f32) {
    let scaled = font.as_scaled(PxScale::from(size));
    let (w, h) = ((size * text.chars().count() as f32 * 0.9) as u32 + 80, (size * 2.6) as u32);
    let mut img = RgbaImage::from_pixel(w, h, Rgba([24, 26, 30, 255]));
    let base = size * 1.6;
    let mut x = 40.0;
    for c in text.chars() {
        let id = font.glyph_id(c);
        let glyph = id.with_scale_and_position(PxScale::from(size), point(x, base));
        if let Some(o) = font.outline_glyph(glyph) {
            let b = o.px_bounds();
            o.draw(|gx, gy, cov| {
                let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    let v = (24.0 + cov * (240.0 - 24.0)) as u8;
                    let p = img.get_pixel_mut(px as u32, py as u32);
                    if v > p.0[0] { *p = Rgba([v, v, v, 255]); }
                }
            });
        }
        x += scaled.h_advance(id);
    }
    (img, base)
}

/// Ground truth from the outlines, at 16x the size so rounding to whole pixels does not decide the answer:
/// the height of the tallest of `H l d h` (what the classifier calls cap/ascender height) and the width of the
/// vertical stem of `l`, taken as the coverage across its mid-height row (serifs are not at mid-height).
fn truth(font: &FontVec, size: f32) -> (f32, f32) {
    const K: f32 = 16.0;
    let outline = |c| font.outline_glyph(font.glyph_id(c).with_scale_and_position(PxScale::from(size * K), point(0.0, 0.0)));
    let tall = ['H', 'l', 'd', 'h'].into_iter().filter_map(|c| outline(c).map(|o| o.px_bounds().height())).fold(0.0, f32::max) / K;
    let stem = outline('l').map(|o| {
        let b = o.px_bounds();
        let mid = (b.height() / 2.0) as u32;
        let mut covered = 0.0;
        o.draw(|_, y, cov| if y == mid { covered += cov });
        covered / K
    }).unwrap_or(0.0);
    (tall, stem)
}

fn analyse(img: &RgbaImage) -> Option<FontAnalysis> {
    let mask = BlockDetector::ink_mask(img);
    let blocks = BlockDetector::default().detect_text_blocks(img, &mask);
    let block = blocks.iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))?;
    Some(FontClassifier.classify(img, &mask, block))
}

fn load(file: &str, weight: Option<f32>) -> FontVec {
    let mut f = FontVec::try_from_vec(std::fs::read(fonts_dir().join(file)).unwrap()).unwrap();
    if let Some(w) = weight { f.set_variation(b"wght", w); }
    f
}

const SAMPLE: &str = "Hello Heldon Highland Idle, Illegal hold";

/// (file, weight axis) of bundled fonts that have a Latin `H` and `l`.
const FACES: &[(&str, Option<f32>)] = &[
    ("Inter.ttf", Some(400.0)), ("Inter.ttf", Some(700.0)), ("Roboto.ttf", Some(400.0)), ("NotoSerif.ttf", Some(400.0)),
    ("FiraSans-Regular.ttf", None), ("FiraSans-Bold.ttf", None), ("PTSerif-Regular.ttf", None), ("OpenSans.ttf", Some(400.0)),
];

/// Cap/ascender height and stem width agree with the outlines' own values on rendered text. Tolerance:
/// one pixel or 6 % for the height, half a pixel or 25 % for the stem (anti-aliasing decides below that).
#[test]
fn measured_height_and_stem_agree_with_font_outlines() {
    for &(file, weight) in FACES {
        for size in [20.0f32, 32.0, 48.0] {
            let font = load(file, weight);
            let (cap, stem) = truth(&font, size);
            let a = analyse(&render(&font, size, SAMPLE).0).unwrap();
            assert!((a.cap_height_px - cap).abs() <= 1.0f32.max(0.06 * cap), "{file} {size}px cap {} vs {cap}", a.cap_height_px);
            assert!((a.stroke_px - stem).abs() <= 0.5f32.max(0.25 * stem), "{file} {size}px stem {} vs {stem}", a.stroke_px);
        }
    }
}

/// `cargo test -p lipa-core --test inplace_typography -- --nocapture measurement_report`
#[test]
fn measurement_report() {
    let (mut cap_err, mut stem_err) = (Vec::new(), Vec::new());
    for &(file, weight) in FACES {
        for size in [20.0f32, 32.0, 48.0] {
            let font = load(file, weight);
            let (cap, stem) = truth(&font, size);
            let (img, _) = render(&font, size, SAMPLE);
            let a = analyse(&img).unwrap_or_else(|| panic!("no block for {file} {size}"));
            let (ce, se) = ((a.cap_height_px - cap) / cap, (a.stroke_px - stem) / stem.max(0.5));
            println!("{file:<22} w{:<5?} {size:>4}px  cap truth {cap:5.1} got {:5.1} ({:+5.1}%)  stem truth {stem:4.1} got {:4.1} ({:+5.0}%)  conf {:.2} {:?} {:?}",
                weight, a.cap_height_px, ce * 100.0, a.stroke_px, se * 100.0, a.confidence, a.category, a.weight);
            cap_err.push(ce.abs());
            stem_err.push(se.abs());
        }
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    println!("mean |cap error| {:.1}%  mean |stem error| {:.0}%", mean(&cap_err) * 100.0, mean(&stem_err) * 100.0);
    let _ = FontCategory::SansSerif;
}

#[derive(serde::Deserialize)]
struct Fixture { file: String, group: String }

fn group_of(c: FontCategory) -> &'static str {
    match c {
        c if c.is_sans() => "sans",
        c if c.is_serif() => "serif",
        FontCategory::SlabSerif => "slab",
        FontCategory::Monospace => "mono",
        FontCategory::Condensed => "condensed",
        FontCategory::Display => "display",
        c if c.is_cjk() => "cjk",
        _ => "unknown",
    }
}

fn labelled() -> Vec<(bool, FontAnalysis)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fonts");
    let manifest: Vec<Fixture> = serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    manifest.into_iter().map(|f| {
        let a = analyse(&image::open(dir.join(&f.file)).unwrap().into_rgba8()).unwrap_or_else(|| panic!("no block in {}", f.file));
        (group_of(a.category) == f.group, a)
    }).collect()
}

/// The calibration table reproduces the labelled fixtures, and the calibrated figure is a better predictor
/// of being right than the old evidence-only `confidence` (lower Brier score).
#[test]
fn category_confidence_is_calibrated_on_the_labelled_fixtures() {
    use lipa_core::layout::font_classifier::category_precision;
    let data = labelled();
    assert!(data.len() >= 190);
    for cat in [FontCategory::SansSerif, FontCategory::Serif, FontCategory::Monospace, FontCategory::SlabSerif, FontCategory::Condensed] {
        let of_cat: Vec<_> = data.iter().filter(|(_, a)| a.category == cat || (cat == FontCategory::SansSerif && a.category.is_sans() && a.category != FontCategory::Condensed)).collect();
        if of_cat.len() < 5 { continue; }
        let precision = of_cat.iter().filter(|(ok, _)| *ok).count() as f32 / of_cat.len() as f32;
        assert!((category_precision(cat) - precision).abs() <= 0.12, "{cat:?}: table {} vs observed {precision} on {} fixtures", category_precision(cat), of_cat.len());
    }
    let brier = |f: &dyn Fn(&FontAnalysis) -> f32| data.iter().map(|(ok, a)| (f(a) - *ok as u8 as f32).powi(2)).sum::<f32>() / data.len() as f32;
    let (raw, calibrated) = (brier(&|a| a.confidence), brier(&|a| a.category_confidence()));
    println!("Brier: evidence-only confidence {raw:.3}, category_confidence {calibrated:.3}");
    assert!(calibrated < raw, "calibration must not make the estimate worse: {calibrated} vs {raw}");
}

/// A prediction is never reported as an exact font: the selection says it is the closest available font.
#[test]
fn a_selection_is_the_closest_available_font_never_an_exact_identification() {
    use lipa_core::layout::{Script, TextBlockType, font_database::InstalledFontDatabase, font_matcher::{FontMatchKind, FontMatcher}};
    let db = InstalledFontDatabase::bundled();
    let font = load("Inter.ttf", Some(400.0));
    let a = analyse(&render(&font, 32.0, SAMPLE).0).unwrap();
    let selected = FontMatcher.select_font(&a, Script::Latin, TextBlockType::Dialogue, &db, &Default::default());
    assert_eq!(selected.match_kind(), FontMatchKind::ClosestAvailable);
    assert!(selected.confidence <= a.category_confidence() + 1e-6, "selection confidence {} cannot exceed the category estimate {}", selected.confidence, a.category_confidence());
    assert!(selected.confidence < 1.0, "a heuristic match is never certain");
}

// ── Per-block typography across frames ──

mod sequence {
    use super::*;
    use lipa_core::layout::{block_detector::DetectedTextBlock, font_matcher::FontSelection, tracker::TextBlockTracker};
    use std::time::{Duration, Instant};

    /// One frame of a sequence: render `text` in a face at a size (light on dark, optionally dimmed) and detect its block.
    fn frame(face: (&str, Option<f32>), size: f32, text: &str, ink: u8) -> (RgbaImage, DetectedTextBlock) {
        let font = load(face.0, face.1);
        let (mut img, _) = render(&font, size, text);
        if ink != 240 { for p in img.pixels_mut() { if p.0[0] > 24 { let v = (24.0 + (p.0[0] as f32 - 24.0) * (ink as f32 - 24.0) / 216.0) as u8; *p = Rgba([v, v, v, 255]); } } }
        let mask = BlockDetector::ink_mask(&img);
        let block = BlockDetector::default().detect_text_blocks(&img, &mask).into_iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area())).expect("block");
        (img, block)
    }

    struct Scene { tracker: TextBlockTracker, now: Instant }

    impl Scene {
        fn new() -> Self { Self { tracker: TextBlockTracker::default(), now: Instant::now() } }
        /// Feed one frame; returns the block id and applies the engine's probe (`observe_font`).
        fn show(&mut self, face: (&str, Option<f32>), size: f32, text: &str, ink: u8) -> u64 {
            self.now += Duration::from_millis(500);
            let (img, block) = frame(face, size, text, ink);
            let id = self.tracker.update(std::slice::from_ref(&block), self.now)[0].id;
            let mask = BlockDetector::ink_mask(&img);
            let analysis = FontClassifier.classify(&img, &mask, &block);
            self.tracker.get_mut(id).unwrap().observe_font(analysis, self.now);
            id
        }
        fn lock(&mut self, id: u64) {
            self.tracker.get_mut(id).unwrap().font = Some(FontSelection::generic(FontCategory::SansSerif));
        }
        fn weight(&self, id: u64) -> i32 { self.tracker.get(id).unwrap().font_analysis.as_ref().unwrap().weight.value() }
        fn locked(&self, id: u64) -> bool { self.tracker.get(id).unwrap().font_selection_locked() }
    }

    const INTER: (&str, Option<f32>) = ("Inter.ttf", Some(400.0));
    const INTER_BOLD: (&str, Option<f32>) = ("Inter.ttf", Some(700.0));
    const SERIF: (&str, Option<f32>) = ("NotoSerif.ttf", Some(400.0));

    #[test]
    fn changed_dialogue_text_keeps_the_block_and_its_font() {
        let mut s = Scene::new();
        let id = s.show(INTER, 32.0, "Hello Heldon Highland Idle", 240);
        s.lock(id);
        let revision = s.tracker.get(id).unwrap().revision;
        for text in ["Illegal hold on the line", "Another sentence Holds Here", "Hello Heldon Highland Idle"] {
            assert_eq!(s.show(INTER, 32.0, text, 240), id, "same place, same style: one block");
            assert!(s.locked(id), "different text alone does not unlock the font");
        }
        assert_eq!(s.tracker.get(id).unwrap().revision, revision);
    }

    #[test]
    fn a_weight_change_needs_two_agreeing_probes_then_invalidates_the_old_font() {
        let mut s = Scene::new();
        let id = s.show(INTER, 32.0, SAMPLE, 240);
        s.lock(id);
        let before = s.weight(id);
        assert_eq!(s.show(INTER_BOLD, 32.0, SAMPLE, 240), id);
        assert!(s.locked(id) && s.weight(id) == before, "one probe is noise, not a change");
        assert_eq!(s.show(INTER, 32.0, SAMPLE, 240), id); // back to normal: the pending change is dropped
        assert_eq!(s.show(INTER_BOLD, 32.0, SAMPLE, 240), id);
        assert!(s.locked(id), "a lone bold probe after a regular one is still unconfirmed");
        let revision = s.tracker.get(id).unwrap().revision;
        assert_eq!(s.show(INTER_BOLD, 32.0, SAMPLE, 240), id);
        assert!(!s.locked(id), "confirmed: the obsolete font selection is dropped");
        assert!(s.weight(id) > before);
        assert!(s.tracker.get(id).unwrap().typography.is_none() && s.tracker.get(id).unwrap().revision > revision);
    }

    #[test]
    fn a_character_name_that_changes_style_is_updated_and_keeps_its_identity() {
        let mut s = Scene::new();
        let id = s.show(SERIF, 32.0, SAMPLE, 240);
        s.lock(id);
        let category = s.tracker.get(id).unwrap().font_analysis.as_ref().unwrap().category;
        assert!(category.is_serif());
        s.show(INTER, 32.0, SAMPLE, 240);
        assert_eq!(s.show(INTER, 32.0, SAMPLE, 240), id, "same geometry: same block id");
        let now = s.tracker.get(id).unwrap().font_analysis.as_ref().unwrap().category;
        assert!(now.is_sans() && !s.locked(id), "{now:?}");
    }

    #[test]
    fn a_menu_item_that_only_dims_when_inactive_does_not_change_its_font() {
        let mut s = Scene::new();
        let id = s.show(INTER, 32.0, SAMPLE, 240);
        s.lock(id);
        for ink in [150, 240, 150, 150, 240] {
            assert_eq!(s.show(INTER, 32.0, SAMPLE, ink), id);
            assert!(s.locked(id), "ink {ink}: brightness is not typography");
        }
    }

    #[test]
    fn a_block_that_returns_at_the_same_place_after_a_gap_is_measured_afresh() {
        let mut s = Scene::new();
        let old = s.show(SERIF, 32.0, SAMPLE, 240);
        s.lock(old);
        // The block disappears for longer than the tracker's time to live.
        s.now += Duration::from_secs(3);
        s.tracker.update(&[], s.now);
        assert!(s.tracker.get(old).is_none());
        let new = s.show(INTER_BOLD, 32.0, SAMPLE, 240);
        assert_ne!(new, old);
        let t = s.tracker.get(new).unwrap();
        assert!(!t.font_selection_locked(), "nothing is inherited from the obsolete block");
        assert!(t.font_analysis.as_ref().unwrap().category.is_sans());
    }

    #[test]
    fn a_rescaled_block_updates_its_size_but_not_its_family() {
        let mut s = Scene::new();
        let id = s.show(INTER, 24.0, SAMPLE, 240);
        let small = s.tracker.get(id).unwrap().font_analysis.as_ref().unwrap().cap_height_px;
        // Geometry changes with the window size; keep the block position by drawing the larger text at the same origin.
        s.show(INTER, 40.0, SAMPLE, 240);
        let id2 = s.show(INTER, 40.0, SAMPLE, 240);
        let a = s.tracker.get(id2).unwrap().font_analysis.as_ref().unwrap();
        // The tracker may call the larger block a new field (line height changed by > 1.4x is a different field) or update it.
        assert!(a.cap_height_px > 1.4 * small, "{} vs {small}", a.cap_height_px);
        assert!(a.category.is_sans() && a.weight.value() <= 500);
    }
}
