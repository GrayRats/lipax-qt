//! Measures the "text replacement" background against a ground truth. Text is drawn over clean pieces of real game
//! frames, the patch is built as the engine builds it and laid over the frame as QML does, and the result is compared
//! with the same piece without text: the error and the lightness bias around the text, the visible seam at the edge of
//! the repainted pixels, and the time. Rust only (the text is drawn with the bundled font through `ab_glyph`).
//!
//!   cargo run --release -p lipa-core --example background_quality -- <out dir> [name ...]
//!
//! With names only those scenes run. Writes `<scene>.png` (original with text | result | clean) per scene.
use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};
use lipa_core::layout::{background::*, block_detector::BlockDetector, TextBlockType};
use lipa_core::settings::{InplaceBackgroundMode, InplaceSettings};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Scene {
    name: &'static str,
    source: &'static str,
    /// A piece of the frame without any text: x, y, width, height.
    crop: (u32, u32, u32, u32),
    text: &'static str,
    size: f32,
    ink: [u8; 3],
    outline: Option<[u8; 3]>,
}

const SCENES: &[Scene] = &[
    Scene { name: "sky", source: "docs/screenshots/test-game/dialogue-en-13.png", crop: (60, 120, 760, 200), text: "But my sisters were always with me", size: 34.0, ink: [20, 20, 30], outline: Some([255, 255, 255]) },
    Scene { name: "cloud-edge", source: "docs/screenshots/test-game/dialogue-en-13.png", crop: (880, 250, 760, 240), text: "The road north is closed, friend", size: 34.0, ink: [20, 20, 30], outline: Some([255, 255, 255]) },
    Scene { name: "checks", source: "docs/screenshots/test-game/dialogue-en-13.png", crop: (370, 972, 960, 56), text: "I was really happy that I wasn't left alone.", size: 30.0, ink: [255, 255, 255], outline: Some([10, 40, 90]) },
    Scene { name: "dark-photo", source: "docs/screenshots/test-game/dialogue-en-1.png", crop: (60, 560, 760, 300), text: "You will always be important to me", size: 34.0, ink: [235, 235, 235], outline: None },
    Scene { name: "space", source: "tests/inplace/fixtures/CrossxMars.exe-129055.png", crop: (40, 60, 760, 240), text: "The signal comes from the station", size: 34.0, ink: [240, 240, 240], outline: Some([0, 0, 0]) },
];

fn luma(p: &[u8]) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// `style` is the ink and the outline colour of the text, if it has one.
fn draw_text(img: &mut RgbaImage, font: &FontVec, size: f32, x: f32, baseline: f32, text: &str, style: ([u8; 3], Option<[u8; 3]>)) {
    let (ink, outline) = style;
    let scale = PxScale::from(size * font.height_unscaled() / font.units_per_em().unwrap());
    let scaled = font.as_scaled(scale);
    let mut blend = |px: i64, py: i64, c: f32, color: [u8; 3]| {
        if px < 0 || py < 0 || px as u32 >= img.width() || py as u32 >= img.height() { return; }
        let p = img.get_pixel_mut(px as u32, py as u32);
        for (channel, paint) in p.0.iter_mut().zip(color) { *channel = (*channel as f32 * (1.0 - c) + paint as f32 * c).round() as u8; }
    };
    let offsets: Vec<(f32, f32)> = match outline {
        Some(_) => vec![(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5), (-1.1, -1.1), (1.1, 1.1), (-1.1, 1.1), (1.1, -1.1), (0.0, 0.0)],
        None => vec![(0.0, 0.0)],
    };
    for (dx, dy) in offsets {
        let (color, is_ink) = if dx == 0.0 && dy == 0.0 { (ink, true) } else { (outline.unwrap(), false) };
        let mut caret = x;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(outline_glyph) = font.outline_glyph(id.with_scale_and_position(scale, point(caret + dx, baseline + dy))) {
                let b = outline_glyph.px_bounds();
                outline_glyph.draw(|gx, gy, cov| blend(b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64, if is_ink { cov } else { cov.min(1.0) }, color));
            }
            caret += scaled.h_advance(id);
        }
    }
}

fn lay_over(composite: &mut RgbaImage, patch: &RgbaImage, x0: u32, y0: u32) {
    for (x, y, p) in patch.enumerate_pixels() {
        let (dx, dy) = (x0 + x, y0 + y);
        if dx >= composite.width() || dy >= composite.height() { continue; }
        let a = p.0[3] as f32 / 255.0;
        let d = composite.get_pixel_mut(dx, dy);
        for c in 0..3 { d.0[c] = (p.0[c] as f32 * a + d.0[c] as f32 * (1.0 - a)).round() as u8; }
    }
}

struct Measure {
    /// Mean absolute luminance error over the patch area, levels (0–255).
    error: f32,
    /// Mean signed luminance error: positive is lighter than the clean frame.
    bias: f32,
    /// The largest luminance step between neighbouring pixels at the border of the repainted pixels, over the same
    /// step in the clean frame: 1 is invisible, more is a visible seam.
    seam: f32,
    /// Share of the pixels of the patch area that are visibly changed (more than 6 levels from the clean frame).
    changed: f32,
    millis: f32,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output directory"));
    let wanted: Vec<String> = args.collect();
    std::fs::create_dir_all(&out).unwrap();
    let font_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts/NotoSans.ttf");
    let font = FontVec::try_from_vec(std::fs::read(&font_path).expect("bundled NotoSans.ttf (build the app once to unpack the fonts)")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let settings = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
    println!("{:12} {:>7} {:>7} {:>6} {:>8} {:>8}  mode", "scene", "error", "bias", "seam", "changed", "ms");
    let mut totals = (0.0f32, 0.0f32, 0.0f32, 0u32);
    for scene in SCENES.iter().filter(|s| wanted.is_empty() || wanted.iter().any(|w| w == s.name)) {
        let frame = image::open(root.join(scene.source)).expect("source frame").to_rgba8();
        let (cx, cy, cw, ch) = scene.crop;
        let clean = image::imageops::crop_imm(&frame, cx, cy, cw, ch).to_image();
        let mut with_text = clean.clone();
        let baseline = ch as f32 * 0.55;
        draw_text(&mut with_text, &font, scene.size, 18.0, baseline, scene.text, (scene.ink, scene.outline));
        let mask = BlockDetector::ink_mask(&with_text);
        // The field as OCR gives it: the bounds of the ink inside the rectangle the text was drawn in.
        let scale = PxScale::from(scene.size * font.height_unscaled() / font.units_per_em().unwrap());
        let width: f32 = scene.text.chars().map(|c| font.as_scaled(scale).h_advance(font.as_scaled(scale).glyph_id(c))).sum();
        let (dx0, dy0, dx1, dy1) = (10u32, (baseline - 1.0 * scene.size).max(0.0) as u32, ((18.0 + width) as u32 + 10).min(cw), ((baseline + 0.4 * scene.size) as u32).min(ch));
        let (mut bx0, mut by0, mut bx1, mut by1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in dy0..dy1 { for x in dx0..dx1 {
            if mask.at(x as usize, y as usize) { bx0 = bx0.min(x); by0 = by0.min(y); bx1 = bx1.max(x + 1); by1 = by1.max(y + 1); }
        } }
        if bx1 == 0 { println!("{:12} the drawn text left no ink", scene.name); continue; }
        let field = lipa_core::layout::Rect::new(bx0 as f32, by0 as f32, (bx1 - bx0) as f32, (by1 - by0) as f32);
        let on_text = vec![field];
        let mut composite = with_text.clone();
        let mut patches = Vec::new();
        let started = Instant::now();
        let mut mode = None;
        for block in &on_text {
            let lh = 1.2 * scene.size;
            let analysis = BackgroundAnalyzer.analyze(&with_text, &mask, block, BackgroundAnalyzer::margin(lh));
            println!("             texture {:5.1} edges {:.3} detail {:5.2}", analysis.texture, analysis.edge_density, analysis.detail);
            let result = BackgroundInpainter.render(&with_text, &mask, block, lh, analysis, &settings, TextBlockType::Unknown);
            let patch = result.image.clone().expect("a patch");
            let (x0, y0, w, h) = result.rect.pixels(cw, ch);
            lay_over(&mut composite, &patch, x0, y0);
            mode = Some(result.mode);
            patches.push((patch, x0, y0, w, h));
        }
        let millis = started.elapsed().as_secs_f32() * 1000.0;
        let result = (mode.unwrap(), ());
        let (mut abs, mut signed, mut n, mut changed) = (0.0f32, 0.0f32, 0u32, 0u32);
        let (mut seam_after, mut seam_clean) = (0.0f32, 0.0f32);
        for (patch, x0, y0, w, h) in &patches {
            let (x0, y0, w, h) = (*x0, *y0, *w, *h);
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let d = luma(&composite.get_pixel(x, y).0) - luma(&clean.get_pixel(x, y).0);
                    abs += d.abs();
                    signed += d;
                    n += 1;
                    if d.abs() > 6.0 { changed += 1; }
                    // The border of the repainted pixels: an opaque pixel next to a pixel the patch did not touch.
                    let here = patch.get_pixel(x - x0, y - y0).0[3] >= 128;
                    if x + 1 < x0 + w && here != (patch.get_pixel(x + 1 - x0, y - y0).0[3] >= 128) {
                        seam_after = seam_after.max((luma(&composite.get_pixel(x, y).0) - luma(&composite.get_pixel(x + 1, y).0)).abs());
                        seam_clean = seam_clean.max((luma(&clean.get_pixel(x, y).0) - luma(&clean.get_pixel(x + 1, y).0)).abs());
                    }
                }
            }
        }
        let m = Measure { error: abs / n.max(1) as f32, bias: signed / n.max(1) as f32, seam: seam_after / seam_clean.max(4.0), changed: changed as f32 / n.max(1) as f32, millis };
        println!("{:12} {:7.2} {:+7.2} {:6.2} {:7.1}% {:8.1}  {:?} x{}", scene.name, m.error, m.bias, m.seam, 100.0 * m.changed, m.millis, result.0, patches.len());
        totals = (totals.0 + m.error, totals.1 + m.bias.abs(), totals.2 + m.millis, totals.3 + 1);
        // A closer look at the first 330 px of the field: original, result, clean, 3 times enlarged.
        {
            let (zx, zy) = ((field.x as u32).saturating_sub(10), (field.y as u32).saturating_sub(10));
            let (zw, zh) = (330u32.min(cw - zx), (field.h as u32 + 20).min(ch - zy));
            let mut zoom = RgbaImage::new(zw * 3, zh * 3 * 3 + 8);
            for (row, img) in [&with_text, &composite, &clean].into_iter().enumerate() {
                let crop = image::imageops::crop_imm(img, zx, zy, zw, zh).to_image();
                let big = image::imageops::resize(&crop, zw * 3, zh * 3, image::imageops::FilterType::Nearest);
                image::imageops::replace(&mut zoom, &big, 0, (row as u32 * (zh * 3 + 4)) as i64);
            }
            zoom.save(out.join(format!("{}-zoom.png", scene.name))).unwrap();
        }
        let mut sheet = RgbaImage::from_pixel(cw * 3 + 8, ch, Rgba([255, 0, 255, 255]));
        image::imageops::replace(&mut sheet, &with_text, 0, 0);
        image::imageops::replace(&mut sheet, &composite, cw as i64 + 4, 0);
        image::imageops::replace(&mut sheet, &clean, 2 * cw as i64 + 8, 0);
        sheet.save(out.join(format!("{}.png", scene.name))).unwrap();
    }
    if totals.3 > 0 {
        let n = totals.3 as f32;
        println!("{:12} {:7.2} {:7.2} {:>6} {:>8} {:8.1}  (mean error, mean |bias|, mean ms)", "mean", totals.0 / n, totals.1 / n, "", "", totals.2 / n);
    }
}
