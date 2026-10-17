//! A/B check of the automatic background choice on real screenshots: for every detected text block it renders
//! the background as the engine would (`Auto`), prints the chosen mode and the time, and writes a composite
//! (the frame with every patch laid over it as QML does) next to the original.
//!
//! cargo run --release -p lipa-core --example background_compare -- <out dir> <image>...
//!
//! The program prints one line per image and a summary; build it before and after a change and compare.
use lipa_core::layout::{background::*, block_detector::BlockDetector, TextBlockType};
use lipa_core::settings::InplaceSettings;
use image::{imageops::FilterType, RgbaImage};
use std::collections::BTreeMap;

fn lay_over(composite: &mut RgbaImage, patch: &RgbaImage, x0: u32, y0: u32) {
    for (x, y, p) in patch.enumerate_pixels() {
        let (dx, dy) = (x0 + x, y0 + y);
        if dx >= composite.width() || dy >= composite.height() { continue; }
        let a = p.0[3] as f32 / 255.0;
        let d = composite.get_pixel_mut(dx, dy);
        for c in 0..3 { d.0[c] = (p.0[c] as f32 * a + d.0[c] as f32 * (1.0 - a)) as u8; }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().expect("output directory"));
    std::fs::create_dir_all(&out).unwrap();
    // FORCE_REPLACE=1: the explicit "text replacement" mode instead of Auto.
    let mut settings = InplaceSettings::default();
    if std::env::var_os("FORCE_REPLACE").is_some() { settings.background_mode = lipa_core::settings::InplaceBackgroundMode::TextReplacement; }
    let mut modes: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = std::time::Duration::ZERO;
    let mut slowest = (std::time::Duration::ZERO, String::new());
    for path in args {
        let img = image::open(&path).expect("image").to_rgba8();
        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let mut composite = img.clone();
        let (mut spent, mut count) = (std::time::Duration::ZERO, 0);
        for b in blocks.iter().filter(|b| b.rect.w >= 24.0 && b.rect.h >= 8.0) {
            let lh = b.line_height();
            let started = std::time::Instant::now();
            let a = BackgroundAnalyzer.analyze(&img, &mask, &b.rect, BackgroundAnalyzer::margin(lh));
            let r = BackgroundInpainter.render(&img, &mask, &b.rect, lh, a, &settings, TextBlockType::Unknown);
            let took = started.elapsed();
            if std::env::var_os("VERBOSE").is_some() {
                println!("  {:?} at ({:.0},{:.0} {:.0}x{:.0}) texture {:.1} edges {:.2} gradient {:+.1} reliable {} {:?}", r.mode, b.rect.x, b.rect.y, b.rect.w, b.rect.h,
                    r.analysis.texture, r.analysis.edge_density, r.analysis.gradient, r.analysis.reliable, took);
            }
            spent += took;
            count += 1;
            if took > slowest.0 { slowest = (took, format!("{path} {:?}", b.rect)); }
            *modes.entry(format!("{:?}", r.mode)).or_default() += 1;
            if let Some(patch) = &r.image {
                let (x0, y0, w, h) = r.rect.pixels(img.width(), img.height());
                if patch.dimensions() == (w, h) { lay_over(&mut composite, patch, x0, y0); }
                else { lay_over(&mut composite, &image::imageops::resize(patch.as_ref(), w, h, FilterType::Triangle), x0, y0); }
            } else if matches!(r.mode, BackgroundRenderMode::SolidFill | BackgroundRenderMode::AdaptivePaddingFill) {
                let (x0, y0, w, h) = r.rect.pixels(img.width(), img.height());
                let plate = RgbaImage::from_pixel(w, h, image::Rgba([r.color[0], r.color[1], r.color[2], 255]));
                lay_over(&mut composite, &plate, x0, y0);
            }
        }
        total += spent;
        println!("{path}: {count} blocks, {spent:?}");
        let name = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().into_owned();
        composite.save(out.join(format!("{name}.png"))).unwrap();
    }
    println!("modes: {modes:?}");
    println!("total {total:?}; slowest {:?} ({})", slowest.0, slowest.1);
}
