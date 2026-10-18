//! The measure against reference frames (`common::reference`): its definition on drawn frames, and, opt-in, the
//! real frames of the game (`tests/inplace/prepare-references.sh` converts the JPEG references first).
//!
//! LIPAX_REFERENCES=tmp/work/refs cargo test --release -p lipa-core --test reference_metric -- --ignored --nocapture

mod common;

use common::reference::{Report, compare};
use image::{Rgb, RgbImage};

fn frame() -> RgbImage {
    RgbImage::from_pixel(500, 260, Rgb([60, 60, 70]))
}

/// A line of "text": a bright bar (the measure sees changed pixels, not letters).
fn bar(img: &mut RgbImage, x: u32, y: u32, w: u32, colour: [u8; 3]) {
    for yy in y..y + 14 { for xx in x..x + w { img.put_pixel(xx, yy, Rgb(colour)); } }
}

fn reference() -> RgbImage {
    let mut img = frame();
    bar(&mut img, 40, 40, 160, [240, 240, 240]);
    bar(&mut img, 40, 160, 220, [240, 240, 240]);
    img
}

#[test]
fn the_same_replacement_is_a_perfect_match() {
    let r = compare(&frame(), &reference(), &reference());
    assert_eq!((r.reference_blocks, r.our_blocks, r.extra_blocks), (2, 2, 0));
    assert!((r.recall - 1.0).abs() < 1e-6 && r.excess == 0.0, "{r:?}");
    assert_eq!((r.offset, r.height_error, r.colour), (Some(0.0), Some(0.0), Some(0.0)));
}

#[test]
fn a_plate_beyond_the_text_is_excess_and_a_missing_line_is_lost_recall() {
    let mut plated = reference();
    // A dark plate 24 px larger than the first line on every side: changed, but not where the reference changed.
    for y in 16..78 { for x in 16..224 { if plated.get_pixel(x, y).0 == [60, 60, 70] { plated.put_pixel(x, y, Rgb([20, 20, 24])); } } }
    let r = compare(&frame(), &plated, &reference());
    // The plate reaches 24 px beyond the text and only what lies more than `NEAR` (14 px) away counts: a 10 px frame.
    assert!(r.excess > 0.5, "the plate is a good part of the text it carries again: {r:?}");
    assert!((r.recall - 1.0).abs() < 1e-6, "the text itself is still replaced: {r:?}");
    assert!(r.height_error.unwrap() > 0.5, "the block is taller than the reference's: {r:?}");

    let mut missing = frame();
    bar(&mut missing, 40, 40, 160, [240, 240, 240]);
    let r = compare(&frame(), &missing, &reference());
    assert!(r.recall > 0.3 && r.recall < 0.6, "one of two lines is left alone: {r:?}");
    assert_eq!(r.extra_blocks, 0);
}

#[test]
fn lettering_the_reference_leaves_alone_is_an_extra_block() {
    let mut ours = reference();
    bar(&mut ours, 300, 100, 120, [200, 180, 120]);
    let r = compare(&frame(), &ours, &reference());
    assert_eq!(r.extra_blocks, 1, "{r:?}");
    assert!(r.extra_pixels > 1000 && (r.recall - 1.0).abs() < 1e-6);
}

#[test]
fn position_and_colour_of_the_text_are_measured() {
    let mut shifted = frame();
    bar(&mut shifted, 48, 44, 160, [200, 200, 120]);
    bar(&mut shifted, 48, 164, 220, [200, 200, 120]);
    let r = compare(&frame(), &shifted, &reference());
    // Moved by (8, 4): the centres are sqrt(8² + 4²) = 8.9 px apart.
    assert!((r.offset.unwrap() - 8.94).abs() < 0.5, "{r:?}");
    assert!(r.colour.unwrap() > 50.0, "yellowish text against white: {r:?}");
    assert!(r.height_error.unwrap() < 0.05);
}

#[test]
fn compression_noise_is_not_a_change() {
    let mut noisy = reference();
    for (i, p) in noisy.pixels_mut().enumerate() { let n = (i * 7 % 13) as u8; for c in 0..3 { p.0[c] = p.0[c].saturating_add(n / 2); } }
    let r = compare(&reference(), &noisy, &reference());
    assert_eq!((r.our_blocks, r.extra_blocks), (0, 0), "{r:?}");
}

fn print(name: &str, r: &Report) {
    let opt = |v: Option<f32>, unit: &str| v.map_or("—".to_owned(), |v| format!("{v:.1}{unit}"));
    println!("{name:16} ref {:>2} ours {:>2}  recall {:>5.1}%  extra {} ({} px)  excess {:>5.2}  offset {:>7}  height {:>6}  colour {:>6}", r.reference_blocks, r.our_blocks, 100.0 * r.recall, r.extra_blocks, r.extra_pixels,
        r.excess, opt(r.offset, " px"), opt(r.height_error.map(|v| v * 100.0), " %"), opt(r.colour, ""));
}

/// `$LIPAX_REFERENCES/{orig,ours,ref}/dialogue-*-N.png`: the original frame, the frame with our translation and the reference.
#[test]
#[ignore = "needs the real frames: tests/inplace/prepare-references.sh, then LIPAX_REFERENCES=tmp/work/refs"]
fn the_real_frames_against_their_references() {
    // A relative path is the path from the root of the repository (the test itself runs in the crate's directory).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(std::env::var_os("LIPAX_REFERENCES").expect("LIPAX_REFERENCES: the directory prepare-references.sh filled"));
    let mut reports = Vec::new();
    let mut numbers: Vec<u32> = std::fs::read_dir(root.join("ref")).unwrap().filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("dialogue-ru-")?.strip_suffix(".png")?.parse().ok()).collect();
    numbers.sort_unstable();
    for n in numbers {
        let paths = [root.join(format!("orig/dialogue-en-{n}.png")), root.join(format!("ours/dialogue-en-{n}.png")), root.join(format!("ref/dialogue-ru-{n}.png"))];
        if paths.iter().any(|p| !p.is_file()) { println!("dialogue {n:>2}: no frame of ours or no original, skipped"); continue; }
        let [original, ours, reference] = paths.map(|p| image::open(p).unwrap().to_rgb8());
        if original.dimensions() != ours.dimensions() || original.dimensions() != reference.dimensions() { println!("dialogue {n:>2}: the sizes differ, skipped"); continue; }
        let r = compare(&original, &ours, &reference);
        print(&format!("dialogue {n}"), &r);
        reports.push(r);
    }
    assert!(!reports.is_empty(), "no frame was compared");
    let n = reports.len() as f32;
    println!("\nmean over {} frames: recall {:.1}%  extra blocks {:.1}  excess {:.2}", reports.len(), 100.0 * reports.iter().map(|r| r.recall).sum::<f32>() / n,
        reports.iter().map(|r| r.extra_blocks as f32).sum::<f32>() / n, reports.iter().map(|r| r.excess).sum::<f32>() / n);
}
