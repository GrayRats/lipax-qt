//! Engine stress: many blocks, rapidly changing dialogue, identical and partially changed frames, window
//! resizing. Checks boundedness (tracked fields, jobs per frame, resident memory) and that work is not
//! repeated for unchanged pixels. Timings are printed for the record, never asserted: they depend on the
//! machine, and this is a debug build.
use image::{DynamicImage, Rgba, RgbaImage};
use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
use std::time::{Duration, Instant};

fn row(img: &mut RgbaImage, x: u32, y: u32, n: u32, seed: u32) {
    // Letters of uneven width and height; `seed` changes the text (widths), not the place.
    let mut gx = x;
    for i in 0..n {
        let w = 8 + (i * 7 + seed * 3) % 9;
        let h = if (i + seed) % 3 == 0 { 22 } else { 15 };
        for yy in y + 22 - h..y + 22 {
            for xx in gx..gx + w {
                if xx < gx + 3 || xx >= gx + w - 3 || (yy >= y + 11 && yy < y + 14) { img.put_pixel(xx, yy, Rgba([235, 235, 235, 255])); }
            }
        }
        gx += w + 4;
    }
}

/// `labels` independent one-line fields on a grid in a frame of the given width.
fn scene(width: u32, labels: u32, seed: u32) -> DynamicImage {
    let mut img = RgbaImage::from_pixel(width, 60 + 70 * labels.div_ceil(6), Rgba([20, 24, 30, 255]));
    for k in 0..labels {
        row(&mut img, 20 + (k % 6) * 220, 30 + (k / 6) * 70, 9, seed + k);
    }
    DynamicImage::ImageRgba8(img)
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1).and_then(|p| p.parse::<u64>().ok())).map_or(0, |pages| pages * 4)
}

fn answer_all(e: &mut InplaceEngine, jobs: Vec<lipa_core::layout::engine::OcrJob>, s: &Settings, seed: u32) {
    for job in jobs {
        // Deterministic reading and translation, as the offline tests do: no OCR or service is involved.
        let text = format!("Label {} {seed}", job.id);
        e.complete(job.id, Some((text.clone(), format!("Метка {text}"))), s);
    }
    e.finish(s);
}

#[test]
fn many_blocks_are_all_tracked_and_each_costs_one_job_once() {
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let t = Instant::now();
    let frame = scene(1400, 40, 1);
    let jobs = e.begin(frame.clone(), &s, t, false);
    assert_eq!(jobs.len(), 40, "every independent label is read: {:?}", e.detection());
    assert!(e.detection().dropped.is_empty());
    assert_eq!(e.tracker().blocks().len(), 40);
    answer_all(&mut e, jobs, &s, 1);
    // The identical frame: nothing is read again, nothing is added.
    for i in 1..=5 {
        assert!(e.begin(frame.clone(), &s, t + Duration::from_millis(100 * i), false).is_empty());
    }
    assert_eq!(e.tracker().blocks().len(), 40);
}

#[test]
fn a_frame_beyond_the_block_limit_reports_the_blocks_it_did_not_read() {
    let mut s = Settings::default();
    s.appearance.inplace.max_blocks = 24;
    let mut e = InplaceEngine::new();
    let jobs = e.begin(scene(1400, 40, 1), &s, Instant::now(), false);
    assert_eq!(jobs.len(), 24);
    let report = e.detection();
    assert_eq!((report.candidates, report.dropped.len()), (40, 16), "{report:?}");
}

#[test]
fn partially_changed_frames_read_only_what_changed() {
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let t = Instant::now();
    let first = scene(1400, 30, 1);
    let jobs = e.begin(first, &s, t, false);
    answer_all(&mut e, jobs, &s, 1);
    // Five labels get new text (different glyph widths at the same places), 25 stay as they were.
    let mut changed = scene(1400, 30, 1).into_rgba8();
    for k in [2u32, 7, 11, 19, 28] {
        let (x, y) = (20 + (k % 6) * 220, 30 + (k / 6) * 70);
        for yy in y - 2..y + 26 { for xx in x - 2..x + 215 { changed.put_pixel(xx, yy, Rgba([20, 24, 30, 255])); } }
        row(&mut changed, x, y, 9, 500 + k);
    }
    let jobs = e.begin(DynamicImage::ImageRgba8(changed), &s, t + Duration::from_millis(300), false);
    assert!((5..=8).contains(&jobs.len()), "{} jobs for 5 changed labels", jobs.len());
}

#[test]
fn rapidly_changing_dialogue_and_resizing_keep_state_and_memory_bounded() {
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let t = Instant::now();
    let warm = scene(1400, 12, 0);
    let jobs = e.begin(warm, &s, t, false);
    answer_all(&mut e, jobs, &s, 0);
    let start = rss_kib();
    let began = Instant::now();
    let mut max_tracked = 0;
    let mut total_jobs = 0;
    for i in 1..=200u32 {
        // Every frame the text of every label changes; every 20th frame the window is resized.
        let width = if i % 20 == 0 { 1400 + (i % 7) * 40 } else { 1400 };
        let frame = scene(width, 12, i);
        let jobs = e.begin(frame, &s, t + Duration::from_millis(50 * i as u64), false);
        total_jobs += jobs.len();
        assert!(jobs.len() <= 12, "never more jobs than fields in a frame (frame {i}: {})", jobs.len());
        answer_all(&mut e, jobs, &s, i);
        max_tracked = max_tracked.max(e.tracker().blocks().len());
    }
    let elapsed = began.elapsed();
    println!("200 changing frames x 12 labels: {:.1} ms/frame (debug build), {} jobs, tracked <= {max_tracked}, RSS {} -> {} KiB",
        elapsed.as_secs_f64() * 1000.0 / 200.0, total_jobs, start, rss_kib());
    assert!(max_tracked <= 24, "stale fields are forgotten: {max_tracked} tracked for 12 labels");
    assert!(rss_kib() < start + 64 * 1024, "memory grew by more than 64 MiB over 200 frames: {start} -> {}", rss_kib());
}
