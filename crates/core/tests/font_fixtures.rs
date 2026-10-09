//! Font detection on real rendered text (not on drawn rectangles).
//!
//! The fixtures are PNGs of real fonts with known labels (`cargo run -p lipa-core --example render_fixtures -- fonts`).
//! `font_report` prints the confusion table for whatever directory `LIPA_FONT_FIXTURES` points at;
//! the other tests check the committed set.

use image::DynamicImage;
use lipa_core::layout::block_detector::BlockDetector;
use lipa_core::layout::font_classifier::{FontAnalysis, FontClassifier};
use lipa_core::layout::font_database::InstalledFontDatabase;
use lipa_core::layout::font_matcher::FontMatcher;
use lipa_core::layout::{FontCategory, FontWeight, Script, TextBlockType};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
struct Fixture {
    file: String,
    font: String,
    group: String,
    weight: i32,
    italic: bool,
    script: String,
    light_on_dark: bool,
}

fn dir() -> PathBuf {
    std::env::var_os("LIPA_FONT_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fonts"))
}

fn manifest(dir: &Path) -> Vec<Fixture> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).unwrap_or_else(|e| panic!("manifest in {}: {e}", dir.display()));
    serde_json::from_str(&text).expect("manifest.json")
}

/// The detector and classifier exactly as the engine runs them: the largest block of the frame.
fn analyse(img: &DynamicImage) -> Option<FontAnalysis> {
    let rgba = img.to_rgba8();
    let mask = BlockDetector::ink_mask(&rgba);
    let blocks = BlockDetector::default().detect_text_blocks(&rgba, &mask);
    let block = blocks.iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))?;
    Some(FontClassifier.classify(&rgba, &mask, block))
}

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

fn run(dir: &Path) -> Vec<(Fixture, Option<FontAnalysis>)> {
    manifest(dir).into_iter().map(|f| {
        let img = image::open(dir.join(&f.file)).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let a = analyse(&img);
        (f, a)
    }).collect()
}

/// Whether the bundled font the matcher picks for a fixture is of the same kind. Slab and serif are
/// interchangeable (a replacement with serifs is a fair answer for either), and so are a narrow
/// font and a plain sans.
fn compatible(want: &str, got: &str) -> bool {
    fn kind(g: &str) -> &str {
        match g { "slab" | "serif" => "serif", "condensed" | "sans" | "display" => "sans", other => other }
    }
    kind(want) == kind(got)
}

fn replacement_group(a: &FontAnalysis, script: Script) -> (String, &'static str) {
    let db = InstalledFontDatabase::bundled();
    let selected = FontMatcher.select_font(a, script, TextBlockType::Dialogue, &db, &Default::default());
    let group = db.find(&selected.family).map(|f| group_of(f.category)).unwrap_or("none");
    (selected.family, group)
}

/// `cargo test -p lipa-core --test font_fixtures font_report -- --ignored --nocapture`
#[test]
#[ignore]
fn font_report() {
    let results = run(&dir());
    let (mut group_ok, mut weight_ok, mut weight_near, mut italic_ok, mut replaced_ok) = (0, 0, 0, 0, 0);
    let mut by_group: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for (f, a) in &results {
        let Some(a) = a else { println!("NO BLOCK  {}", f.file); continue };
        let got = group_of(a.category);
        let ok_group = got == f.group;
        let diff = (a.weight.value() - f.weight).abs();
        group_ok += ok_group as u32;
        weight_ok += (diff == 0) as u32;
        weight_near += (diff <= 100) as u32;
        italic_ok += (a.italic == f.italic) as u32;
        let (family, replacement) = replacement_group(a, if f.script == "cyr" { Script::Cyrillic } else { Script::Latin });
        let replaced = compatible(&f.group, replacement);
        replaced_ok += replaced as u32;
        if !replaced { println!("REPLACEMENT {:<36} want {:<9} picked {family} ({replacement})", f.file, f.group); }
        let e = by_group.entry(f.group.clone()).or_default();
        e.0 += ok_group as u32;
        e.1 += 1;
        if !ok_group || diff > 100 || a.italic != f.italic {
            println!("{:<44} want {:<9} w{} i={:<5}  got {:<9?} w{} i={:<5} conf={:.2} slant={:.2} stroke={:.1}/{:.0} ratio={:.2}",
                f.file, f.group, f.weight, f.italic, a.category, a.weight.value(), a.italic, a.confidence, a.slant, a.stroke_px, a.cap_height_px, a.width_ratio);
        }
    }
    let n = results.len() as f32;
    println!("\ngroup {:.0}%  weight exact {:.0}%  within one step {:.0}%  italic {:.0}%  suitable replacement {:.0}%  ({} fixtures)",
        100.0 * group_ok as f32 / n, 100.0 * weight_ok as f32 / n, 100.0 * weight_near as f32 / n, 100.0 * italic_ok as f32 / n,
        100.0 * replaced_ok as f32 / n, results.len());
    for (g, (ok, total)) in by_group { println!("  {g:<10} {ok}/{total}"); }
    let _ = FontWeight::Normal;
}

/// Dump features and labels for offline analysis (threshold fitting):
/// `LIPA_FONT_FIXTURES=dir LIPA_FONT_CSV=out.csv cargo test -p lipa-core --test font_fixtures font_csv -- --ignored`
#[test]
#[ignore]
fn font_csv() {
    let Some(out) = std::env::var_os("LIPA_FONT_CSV") else { return };
    let mut csv = String::from("file,font,group,weight,italic,script,dark,glyphs,cap_h,x_ratio,width_ratio,stem_w,bar_h,contrast,weight_ratio,foot,foot_glyphs,pitch_error,slant,slant_gain\n");
    for (f, a) in run(&dir()) {
        let Some(a) = a else { continue };
        let x = &a.features;
        csv += &format!("{},{},{},{},{},{},{},{},{:.2},{:.3},{:.3},{:.2},{:.2},{:.3},{:.4},{:.3},{},{:.3},{:.3},{:.3}\n",
            f.file, f.font, f.group, f.weight, f.italic as u8, f.script, f.light_on_dark as u8, x.glyphs, x.cap_h, x.x_ratio,
            x.width_ratio, x.stem_w, x.bar_h, x.contrast, x.weight_ratio, x.foot, x.foot_glyphs, x.pitch_error, x.slant, x.slant_gain);
    }
    std::fs::write(out, csv).unwrap();
}

// ── Regression checks on the committed fixtures ──
// Measured when the thresholds were chosen: group 88 %, weight within one step 99 %, italic 100 %,
// suitable replacement 94 %. The limits sit a little below, so noise does not trip them but a real
// loss of quality does. Raise them when the classifier improves.

struct Tally { n: u32, group: u32, near: u32, italic: u32, replaced: u32, by_group: std::collections::BTreeMap<String, (u32, u32)> }

fn tally() -> Tally {
    let mut t = Tally { n: 0, group: 0, near: 0, italic: 0, replaced: 0, by_group: Default::default() };
    for (f, a) in run(&dir()) {
        let a = a.unwrap_or_else(|| panic!("no text block found in {}", f.file));
        t.n += 1;
        let ok = group_of(a.category) == f.group;
        t.group += ok as u32;
        t.near += ((a.weight.value() - f.weight).abs() <= 100) as u32;
        t.italic += (a.italic == f.italic) as u32;
        let script = if f.script == "cyr" { Script::Cyrillic } else { Script::Latin };
        // A CJK fixture is read as ideographs by the OCR, which the engine passes on as the script.
        let analysis = if f.group == "cjk" { FontAnalysis { category: FontCategory::CjkSans, ..a.clone() } } else { a.clone() };
        let target = if f.group == "cjk" { Script::ChineseSimplified } else { script };
        let (_, replacement) = replacement_group(&analysis, target);
        t.replaced += compatible(&f.group, replacement) as u32;
        let e = t.by_group.entry(f.group).or_default();
        e.0 += ok as u32;
        e.1 += 1;
    }
    t
}

fn rate(ok: u32, n: u32) -> f32 { ok as f32 / n.max(1) as f32 }

#[test]
fn real_fonts_are_told_apart() {
    let t = tally();
    assert!(t.n >= 150, "the fixture set is present ({} files)", t.n);
    assert!(rate(t.group, t.n) >= 0.85, "style group {:.0} %", 100.0 * rate(t.group, t.n));
    let group = |g: &str| t.by_group.get(g).map(|(ok, n)| rate(*ok, *n)).unwrap_or(0.0);
    assert!(group("mono") >= 0.97, "monospaced {:.0} %", 100.0 * group("mono"));
    assert!(group("sans") >= 0.92, "sans {:.0} %", 100.0 * group("sans"));
    assert!(group("serif") >= 0.75, "serif {:.0} %", 100.0 * group("serif"));
}

#[test]
fn weight_and_italic_are_read_from_real_glyphs() {
    let t = tally();
    assert!(rate(t.italic, t.n) >= 0.98, "italic {:.0} %", 100.0 * rate(t.italic, t.n));
    assert!(rate(t.near, t.n) >= 0.95, "weight within one step {:.0} %", 100.0 * rate(t.near, t.n));
}

#[test]
fn the_picked_bundled_font_is_of_a_suitable_kind() {
    let t = tally();
    assert!(rate(t.replaced, t.n) >= 0.90, "suitable replacement {:.0} %", 100.0 * rate(t.replaced, t.n));
}

#[test]
fn one_block_gets_one_answer_regardless_of_the_text() {
    // The same font rendered with different words and colours must not flip categories: the
    // engine locks the first answer, so the first answer has to be a stable one.
    let results = run(&dir());
    let mut by_font: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for (f, a) in &results {
        if f.script != "latin" || f.file.contains("oblique") { continue; }
        if let Some(a) = a { by_font.entry(f.font.as_str()).or_default().push(group_of(a.category)); }
    }
    let unstable = by_font.iter().filter(|(_, g)| g.windows(2).any(|w| w[0] != w[1])).count();
    assert!(unstable * 10 <= by_font.len(), "{unstable} of {} fonts change category between light-on-dark and dark-on-light", by_font.len());
}
