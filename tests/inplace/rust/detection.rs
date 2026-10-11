//! Detection regressions: block limit and its report, oversized type, small labels beside a title,
//! patterns of solid discs, low-contrast text for the local-contrast pass, and the two real frames.
//! Synthetic frames use H-shaped glyphs (one connected component per letter); the real frames are
//! recorded game captures. None of this is an OCR accuracy measurement.
use image::{Rgba, RgbaImage};
use lipa_core::layout::{Rect, block_detector::{BlockDetector, DetectionReport}};

fn canvas(w: u32, h: u32, bg: u8) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([bg, bg, bg, 255]))
}

/// A row of `n` H letters: height `h`, width `w`, stroke `s`, gap `gap`. Returns the right edge.
fn row(img: &mut RgbaImage, x: u32, y: u32, n: u32, w: u32, h: u32, s: u32, gap: u32, ink: u8) -> u32 {
    let mut gx = x;
    for _ in 0..n {
        for yy in y..y + h {
            for xx in gx..gx + w {
                if xx < gx + s || xx >= gx + w - s || (yy >= y + h / 2 && yy < y + h / 2 + s) {
                    img.put_pixel(xx, yy, Rgba([ink, ink, ink, 255]));
                }
            }
        }
        gx += w + gap;
    }
    gx
}

/// Letters of uneven width and height on one baseline, like ordinary lower-case text.
fn prose(img: &mut RgbaImage, x: u32, base: u32, n: usize, s: u32, gap: u32, ink: u8) -> u32 {
    let (widths, heights) = ([12u32, 16, 10, 14, 18, 11, 15, 13], [24u32, 17, 17, 24, 18, 17, 24, 17]);
    let mut gx = x;
    for i in 0..n {
        let (w, h) = (widths[i % 8], heights[i % 8]);
        for yy in base - h..base {
            for xx in gx..gx + w {
                if xx < gx + s || xx >= gx + w - s || (yy >= base - h / 2 && yy < base - h / 2 + s) {
                    img.put_pixel(xx, yy, Rgba([ink, ink, ink, 255]));
                }
            }
        }
        gx += w + gap;
    }
    gx
}

fn disc(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, ink: u8) {
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            if (x - cx).pow(2) + (y - cy).pow(2) <= r * r { img.put_pixel(x as u32, y as u32, Rgba([ink, ink, ink, 255])); }
        }
    }
}

fn detect(img: &RgbaImage, max_blocks: usize) -> (Vec<Rect>, DetectionReport) {
    let mask = BlockDetector::ink_mask(img);
    let d = BlockDetector { max_blocks, ..BlockDetector::default() };
    let (blocks, report) = d.detect(img, &mask);
    (blocks.into_iter().map(|b| b.rect).collect(), report)
}

fn detect_local(img: &RgbaImage) -> (Vec<Rect>, DetectionReport) {
    let mut mask = BlockDetector::ink_mask(img);
    let (blocks, report) = BlockDetector::default().detect_with_local(img, &mut mask);
    (blocks.into_iter().map(|b| b.rect).collect(), report)
}

#[test]
fn more_than_sixteen_independent_labels_are_all_detected_in_a_stable_order() {
    // 6 rows x 7 columns of separate labels, far apart horizontally and vertically.
    let mut img = canvas(1400, 480, 20);
    for r in 0..6 { for c in 0..7 { row(&mut img, 20 + c * 195, 20 + r * 75, 5, 14, 22, 3, 6, 235); } }
    let (rects, report) = detect(&img, 64);
    assert_eq!(rects.len(), 42, "{report:?}");
    assert_eq!((report.candidates, report.dropped.len()), (42, 0));
    assert!(rects.windows(2).all(|p| (p[0].y, p[0].x) <= (p[1].y, p[1].x)), "reading order");
    // Unrelated neighbours are never merged: every block holds exactly one label.
    assert!(rects.iter().all(|r| r.w < 120.0 && r.h < 30.0));
    assert_eq!(detect(&img, 64).0, rects, "same frame, same blocks");
}

#[test]
fn the_block_limit_is_explicit_deterministic_and_reported() {
    let mut img = canvas(1400, 480, 20);
    for r in 0..6 { for c in 0..7 { row(&mut img, 20 + c * 195, 20 + r * 75, 5, 14, 22, 3, 6, 235); } }
    let (kept, report) = detect(&img, 16);
    assert_eq!((kept.len(), report.candidates, report.dropped.len()), (16, 42, 26));
    assert!(report.dropped.iter().all(|d| !kept.contains(d)), "a dropped block is not also kept");
    assert_eq!(detect(&img, 16).0, kept, "which blocks survive does not depend on hash order");
    // A larger limit restores every block.
    assert_eq!(detect(&img, 42).0.len(), 42);
}

#[test]
fn oversized_type_is_text_but_a_single_huge_shape_is_scenery() {
    // 600 px tall frame: ordinary limit is max(0.15 * 600, 64) = 90 px; this heading's letters are 140 px.
    let mut img = canvas(1000, 600, 20);
    row(&mut img, 60, 40, 5, 90, 140, 22, 30, 240);
    let (rects, report) = detect(&img, 64);
    assert_eq!(rects.len(), 1, "{report:?}");
    assert!(rects[0].h >= 138.0 && rects[0].w >= 450.0);

    // The same height as one filled blob, no neighbours: scenery.
    let mut scene = canvas(1000, 600, 20);
    for y in 200..340 { for x in 300..440 { scene.put_pixel(x, y, Rgba([240, 240, 240, 255])); } }
    let (rects, report) = detect(&scene, 64);
    assert!(rects.is_empty() && report.rejected_scenery >= 1, "{rects:?} {report:?}");
}

#[test]
fn a_small_label_below_a_large_title_is_a_block_of_its_own() {
    let mut img = canvas(1000, 600, 20);
    row(&mut img, 60, 40, 5, 90, 140, 22, 30, 240); // title
    row(&mut img, 80, 260, 7, 9, 14, 2, 4, 200); // label under it
    let (rects, report) = detect(&img, 64);
    assert_eq!(rects.len(), 2, "{rects:?} {report:?}");
    let label = rects.iter().find(|r| r.h < 30.0).expect("small label kept");
    assert!(label.y > 250.0 && label.w > 80.0);
}

#[test]
fn a_row_of_solid_discs_is_not_text_but_ring_and_heavy_letters_are() {
    let mut dots = canvas(800, 300, 20);
    for (i, r) in [18, 16, 14, 12, 10, 8].into_iter().enumerate() { disc(&mut dots, 60 + i as i32 * 45, 120, r, 235); }
    let (rects, report) = detect(&dots, 64);
    assert!(rects.is_empty(), "{rects:?}");
    assert!(report.rejected_pattern >= 1, "{report:?}");

    // Ring digits "000" (counters), heavy: not a pattern of solid discs.
    let mut zeros = canvas(800, 300, 20);
    for i in 0..3 { disc(&mut zeros, 100 + i * 60, 120, 22, 235); disc(&mut zeros, 100 + i * 60, 120, 9, 20); }
    let (rects, report) = detect(&zeros, 64);
    assert_eq!(rects.len(), 1, "{rects:?} {report:?}");
    assert_eq!(report.rejected_pattern, 0);
}

#[test]
fn low_contrast_text_missed_by_the_global_threshold_is_found_by_the_local_pass() {
    // Left third black, right third white: Otsu splits those; text between them (luma 70 on 135) never
    // reaches the ink class.
    let mut img = canvas(900, 300, 120);
    for y in 0..300 { for x in 0..300 { img.put_pixel(x, y, Rgba([0, 0, 0, 255])); } for x in 600..900 { img.put_pixel(x, y, Rgba([255, 255, 255, 255])); } }
    for y in 100..140 { for x in 320..580 { img.put_pixel(x, y, Rgba([135, 135, 135, 255])); } }
    prose(&mut img, 330, 132, 8, 3, 8, 70);
    assert!(detect(&img, 64).0.iter().all(|r| r.intersection(&Rect::new(320.0, 100.0, 260.0, 40.0)) == 0.0),
        "the global pass must not see it (otherwise this test proves nothing)");
    let mut mask = BlockDetector::ink_mask(&img);
    let (blocks, report) = BlockDetector::default().detect_with_local(&img, &mut mask);
    let found = blocks.iter().find(|b| b.rect.intersection(&Rect::new(330.0, 108.0, 200.0, 24.0)) > 0.0)
        .unwrap_or_else(|| panic!("{blocks:?} {report:?}"));
    assert_eq!(report.local_blocks, 1);
    assert!(found.rect.w > 100.0 && found.rect.h >= 20.0);
    // The mask now carries the glyph pixels for the font and background stages.
    assert!(mask.at(332, 120), "left stroke of the first H");
}

#[test]
fn a_mesh_of_identical_cells_is_not_text() {
    let mut img = canvas(600, 300, 90);
    for gy in 0..4 { for gx in 0..9 { for y in 0..14 { for x in 0..18 { img.put_pixel(60 + gx * 40 + x, 60 + gy * 40 + y, Rgba([150, 150, 150, 255])); } } } }
    let (rects, report) = detect_local(&img);
    assert!(rects.is_empty(), "{rects:?} {report:?}");
}

fn fixture(name: &str) -> RgbaImage {
    image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures").join(name)).unwrap().into_rgba8()
}

#[test]
fn crossxmars_hud_readout_is_found_only_with_the_local_pass_and_the_dialogue_is_kept() {
    let img = fixture("CrossxMars.exe-129055.png");
    let readout = Rect::new(941.0, 589.0, 161.0, 57.0); // "rSpeed / Temp" as seen in the frame
    let (global, _) = detect(&img, 64);
    assert!(global.iter().all(|r| r.intersection(&readout) == 0.0), "global pass alone cannot see the readout");
    let (all, report) = detect_local(&img);
    assert!(all.iter().any(|r| r.iou(&readout) > 0.5), "{all:?}");
    assert!(report.local_blocks >= 1);
    assert!(all.iter().any(|r| r.w > 900.0 && r.y > 760.0), "dialogue band still detected");
}

#[test]
fn engine_exe_halftone_is_not_a_text_block_and_the_dialogue_is_one_three_line_block() {
    let img = fixture("engine.exe-100656.png");
    let mut mask = BlockDetector::ink_mask(&img);
    let (blocks, report) = BlockDetector::default().detect_with_local(&img, &mut mask);
    let halftone = Rect::new(8.0, 770.0, 265.0, 71.0); // the dot field that RapidOCR once read as "0"
    assert!(blocks.iter().all(|b| b.rect.iou(&halftone) < 0.3), "{:?}", blocks.iter().map(|b| b.rect).collect::<Vec<_>>());
    assert!(report.rejected_pattern >= 1, "{report:?}");
    let dialogue = blocks.iter().find(|b| b.rect.w > 800.0).expect("dialogue");
    assert_eq!(dialogue.lines.len(), 3);
}

#[test]
fn word_boxes_and_baseline_match_the_geometry_that_was_drawn() {
    // Ground truth is the drawing itself: three words of 4, 3 and 5 letters, letter gap 3, word gap 14,
    // lower-case-like letters on baseline y = 160 (a descender letter reaches 6 px below it).
    let mut img = canvas(900, 260, 20);
    let mut truth: Vec<(u32, u32)> = Vec::new();
    let mut x = 40u32;
    for letters in [4usize, 3, 5] {
        let start = x;
        x = prose(&mut img, x, 160, letters, 3, 3, 235) - 3;
        truth.push((start, x));
        x += 14;
    }
    // A descender: extend the second letter of the first word below the baseline.
    for y in 160..166 { for xx in 52..55 { img.put_pixel(xx, y, Rgba([235, 235, 235, 255])); } }
    let mask = BlockDetector::ink_mask(&img);
    let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
    assert_eq!(blocks.len(), 1);
    let line = &blocks[0].lines[0];
    let baseline = line.baseline().unwrap();
    assert!((baseline - 160.0).abs() <= 1.0, "baseline {baseline}, a descender must not move it");
    let words = line.words();
    assert_eq!(words.len(), 3, "{words:?}");
    for (w, (x0, x1)) in words.iter().zip(truth) {
        assert!((w.x - x0 as f32).abs() <= 1.0 && (w.right() - x1 as f32).abs() <= 1.0, "{w:?} vs {x0}..{x1}");
    }
}

fn dialogue_of(img: &RgbaImage) -> Option<lipa_core::layout::block_detector::DetectedTextBlock> {
    let mut mask = BlockDetector::ink_mask(img);
    let (blocks, _) = BlockDetector::default().detect_with_local(img, &mut mask);
    blocks.into_iter().filter(|b| b.rect.w > 0.3 * img.width() as f32 && b.lines.len() >= 2).max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
}

#[test]
fn recorded_dialogue_survives_other_window_sizes_moves_and_clipping() {
    use image::imageops::{FilterType, resize};
    let base = fixture("engine.exe-100656.png");
    let reference = dialogue_of(&base).expect("dialogue in the recorded frame");
    // Different window sizes (the game window resized): the block scales with the frame.
    for scale in [0.6f32, 0.8, 1.25] {
        let (w, h) = ((base.width() as f32 * scale) as u32, (base.height() as f32 * scale) as u32);
        let scaled = resize(&base, w, h, FilterType::Lanczos3);
        let found = dialogue_of(&scaled).unwrap_or_else(|| panic!("no dialogue at scale {scale}"));
        let expected = Rect::new(reference.rect.x * scale, reference.rect.y * scale, reference.rect.w * scale, reference.rect.h * scale);
        assert!(found.rect.iou(&expected) > 0.8, "scale {scale}: {:?} vs {:?}", found.rect, expected);
        assert_eq!(found.lines.len(), reference.lines.len(), "scale {scale}");
    }
    // Moving text / changed source coordinates: the frame shifted by (dx, dy) on a larger canvas.
    let (dx, dy) = (137u32, 29u32);
    let mut moved = RgbaImage::from_pixel(base.width() + 300, base.height() + 60, *base.get_pixel(base.width() - 1, base.height() / 2));
    image::imageops::replace(&mut moved, &base, dx as i64, dy as i64);
    let found = dialogue_of(&moved).expect("dialogue after the move");
    // The padding changes the global histogram (Otsu), so the extent may differ by a few pixels at the
    // glyph edges; the block must still be the same text at the moved place.
    let expected = Rect::new(reference.rect.x + dx as f32, reference.rect.y + dy as f32, reference.rect.w, reference.rect.h);
    assert!(found.rect.iou(&expected) > 0.8 && found.lines.len() == reference.lines.len(), "{:?} vs {:?}", found.rect, expected);
    // Partially clipped: the window's bottom edge cuts through the third line. The first two lines stay one block.
    let cut = reference.lines[2].rect.y as u32 + 8;
    let clipped = image::imageops::crop_imm(&base, 0, 0, base.width(), cut).to_image();
    let found = dialogue_of(&clipped).expect("dialogue with a clipped last line");
    assert!(found.lines.len() >= 2 && found.rect.y == reference.rect.y, "{:?}", found.rect);
}

#[test]
fn changed_dialogue_text_changes_the_block_not_the_detector_state() {
    // The detector is stateless: a different sentence in the same place gives a block of the new extent.
    let mut a = canvas(1000, 300, 20);
    row(&mut a, 60, 100, 30, 9, 16, 2, 4, 235);
    row(&mut a, 60, 130, 20, 9, 16, 2, 4, 235);
    let mut b = canvas(1000, 300, 20);
    row(&mut b, 60, 100, 30, 9, 16, 2, 4, 235);
    row(&mut b, 60, 130, 40, 9, 16, 2, 4, 235);
    let (ra, rb) = (detect(&a, 64).0, detect(&b, 64).0);
    assert_eq!((ra.len(), rb.len()), (1, 1));
    assert!(rb[0].w > ra[0].w && (rb[0].x - ra[0].x).abs() < 1.0 && (rb[0].y - ra[0].y).abs() < 1.0);
}
