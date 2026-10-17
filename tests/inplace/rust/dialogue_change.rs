//! Live-recorded CrossxMars.exe dialogue frames (KWin client area cut to the "subtitles" region of the profile)
//! replayed through the engine and the desktop placement, with the RapidOCR readings recorded from the same pixels
//! by `cargo run -p lipa-core --example record_replay <fixture>`.
use lipa_core::{
    capture::kwin::WindowGeometry,
    layout::{
        Rect,
        engine::{InplaceEngine, InplaceFrame, LinesVerdict},
        fit::ApproxMeasure,
        place::{PlacementCache, PlacementOutcome, RegionInput, Viewport, place_regions},
    },
    settings::{InplaceBackgroundMode, NormRect, Settings},
};
use std::time::{Duration, Instant};

fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures")
}

fn settings() -> Settings {
    let mut s = Settings::default();
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
    s
}

/// The region of the CrossxMars profile (bottom quarter of the client area) and the client area KWin reported.
const REGION: NormRect = NormRect { x: 0.007415254237288136, y: 0.7448680288378219, w: 0.9904661016949152, h: 0.254286029946969 };
fn window() -> WindowGeometry { WindowGeometry { x: 794.0, y: 63.0, w: 1739.0, h: 978.0 } }

/// One engine fed with the recorded readings of each fixture in turn, scanned until it settles.
struct Replay { engine: InplaceEngine, now: Instant, scan: u64, last: Option<InplaceFrame> }

impl Replay {
    fn new() -> Self { Self { engine: InplaceEngine::new(), now: Instant::now(), scan: 0, last: None } }

    fn show(&mut self, fixture: &str, s: &Settings) -> InplaceFrame {
        let image = image::open(fixtures().join(format!("{fixture}.png"))).unwrap();
        let history: serde_json::Value = serde_json::from_slice(&std::fs::read(fixtures().join(format!("{fixture}-local-rapidocr.json"))).unwrap()).unwrap();
        let records = history["ocr_history"].as_array().unwrap();
        for _ in 0..4 {
            let at = self.now + Duration::from_millis(700 * self.scan);
            let jobs = self.engine.begin(image.clone(), s, at, false);
            let jobs_len = jobs.len();
            for job in jobs {
                // The latest reading recorded for these detector pixels (the recorder scans the same frame again).
                let record = records.iter().rfind(|r| {
                    let rect: Rect = serde_json::from_value(r["detector_rect"].clone()).unwrap();
                    rect.iou(&job.rect) >= 0.99
                }).unwrap_or_else(|| panic!("{fixture}: no recorded reading for {:?}; record it with record_replay", job.rect));
                let result = lipa_core::ocr::OcrResult {
                    text: record["raw_original"].as_str().unwrap().into(),
                    lines: record["spans"].as_array().unwrap().iter().map(|span| {
                        let rect: Rect = serde_json::from_value(span["rect"].clone()).unwrap();
                        lipa_core::ocr::OcrLine {
                            rect: lipa_core::layout::CropRect::in_space(rect.x - job.origin.0 as f32, rect.y - job.origin.1 as f32, rect.w, rect.h),
                            text: span["text"].as_str().unwrap().into(),
                            confidence: span["confidence"].as_f64().unwrap() as f32,
                        }
                    }).collect(),
                    confidence: record["confidence"].as_f64().map(|c| c as f32),
                    engine: "rapidocr-replay",
                    ..Default::default()
                };
                let accepted = result.confidence.is_none_or(|c| c >= s.effective_minimum_confidence() as f32)
                    && lipa_core::text::is_meaningful(&result.text);
                if accepted {
                    let o = self.engine.observe_ocr_result(job.id, &result, job.origin, s);
                    let settled = o.verdict == LinesVerdict::Normal && lipa_core::text::is_meaningful(&o.text);
                    if std::env::var_os("LIPAX_TEST_TRACE").is_some() {
                        eprintln!("    job {} {:?} read «{}» -> «{}» verdict {:?} settled {settled}", job.id, job.rect, result.text, o.text, o.verdict);
                    }
                    let translation = format!("RU {}", o.text);
                    self.engine.complete(job.id, settled.then_some((o.text, translation)), s);
                } else {
                    if std::env::var_os("LIPAX_TEST_TRACE").is_some() { eprintln!("    job {} {:?} rejected «{}»", job.id, job.rect, result.text); }
                    self.engine.reject_ocr(job.id, s);
                }
            }
            if let Some(f) = self.engine.finish(s) { self.last = Some(f); }
            if std::env::var_os("LIPAX_TEST_TRACE").is_some() {
                eprintln!("[{fixture}] scan {} jobs={} needs_rescan={} blocks={:?} undrawable={:?}", self.scan, jobs_len, self.engine.needs_rescan(),
                    self.last.as_ref().map(|f| f.blocks.iter().map(|b| (b.id, b.original.chars().take(24).collect::<String>())).collect::<Vec<_>>()),
                    self.last.as_ref().map(|f| f.undrawable.iter().map(|u| (u.id, u.reason)).collect::<Vec<_>>()));
            }
            self.scan += 1;
            if !self.engine.needs_rescan() { break; }
        }
        self.last.clone().expect("a published frame")
    }
}

fn place(frame: &InplaceFrame, s: &Settings) -> PlacementOutcome {
    place_regions(&[RegionInput { id: "subtitles", rect: REGION, frame }], &Viewport::window(window()),
        &s.appearance.inplace, &std::collections::HashMap::new(), &ApproxMeasure, &mut PlacementCache::default())
}

fn describe(frame: &InplaceFrame, placed: &PlacementOutcome) -> String {
    let mut out = String::new();
    for b in &frame.blocks { out += &format!("block {} {:?} «{}»\n", b.id, b.text_rect, b.original); }
    for u in &frame.undrawable { out += &format!("undrawable {} «{}» {}\n", u.id, u.original, u.reason); }
    for p in &placed.placed { out += &format!("placed {} «{}»\n", p.block_id, p.original); }
    for d in &placed.dropped { out += &format!("dropped {} «{}» {}\n", d.block_id, d.original, d.reason); }
    out
}

#[test]
fn the_speaker_name_and_every_dialogue_line_are_replaced() {
    let s = settings();
    let mut replay = Replay::new();
    let frame = replay.show("CrossxMars.exe-live-mika", &s);
    let placed = place(&frame, &s);
    let report = describe(&frame, &placed);
    eprintln!("{report}");
    assert!(placed.placed.iter().any(|p| p.original.starts_with("It's a shame you can't figure it out")), "the first dialogue line must be translated:\n{report}");
    assert!(placed.placed.iter().any(|p| p.original.contains("I sometimes wonder how I ended up")), "the second line:\n{report}");
    // Only repeated readings of pixels another field already owns may stay out; nothing of the dialogue is lost.
    assert!(placed.dropped.iter().all(|d| d.reason == "повторный OCR того же текста"), "a dialogue field was dropped:\n{report}");
    // No two drawn fields overlap (one translation over another).
    for (i, a) in placed.placed.iter().enumerate() {
        for b in &placed.placed[i + 1..] {
            let (ax, ay, aw, ah) = (a.box_rect[0], a.box_rect[1], a.box_rect[2], a.box_rect[3]);
            let (bx, by, bw, bh) = (b.box_rect[0], b.box_rect[1], b.box_rect[2], b.box_rect[3]);
            let overlap = (ax + aw).min(bx + bw) - ax.max(bx) > 1e-6 && (ay + ah).min(by + bh) - ay.max(by) > 1e-6;
            assert!(!overlap, "«{}» and «{}» overlap:\n{report}", a.original, b.original);
        }
    }
    // Every drawn field lies inside the selected region of the game window.
    for p in &placed.placed {
        let (x, y, w, h) = (p.box_rect[0], p.box_rect[1], p.box_rect[2], p.box_rect[3]);
        assert!(x >= -1e-6 && y >= -1e-6 && x + w <= 1.0 + 1e-6 && y + h <= 1.0 + 1e-6, "«{}» leaves the region: {:?}", p.original, p.box_rect);
    }
}

/// Distinctive words of each recorded dialogue; a frame must show its own text and none of the earlier ones.
const DIALOGUES: [(&str, &str); 3] = [
    ("CrossxMars.exe-live-d1", "creative way to tell me"),
    ("CrossxMars.exe-live-d2", "confession of a lie"),
    ("CrossxMars.exe-live-d3", "fifty complains each year"),
];

#[test]
fn a_new_dialogue_replaces_the_old_translation_in_the_same_engine() {
    let s = settings();
    let mut replay = Replay::new();
    for (step, (fixture, marker)) in DIALOGUES.iter().enumerate() {
        let frame = replay.show(fixture, &s);
        let placed = place(&frame, &s);
        let report = describe(&frame, &placed);
        let drawn = |word: &str| placed.placed.iter().filter(|p| p.original.contains(word) || p.text.contains(word)).count();
        assert!(drawn(marker) >= 1, "step {step} ({fixture}): the new dialogue is not drawn:\n{report}");
        for (_, old) in &DIALOGUES[..step] {
            assert_eq!(drawn(old), 0, "step {step} ({fixture}): the translation of the previous dialogue «{old}» is still drawn:\n{report}");
        }
        // Several fields of one dialogue never stack on each other.
        for (i, a) in placed.placed.iter().enumerate() {
            for b in &placed.placed[i + 1..] {
                let overlap = (a.box_rect[0] + a.box_rect[2]).min(b.box_rect[0] + b.box_rect[2]) - a.box_rect[0].max(b.box_rect[0]) > 1e-6
                    && (a.box_rect[1] + a.box_rect[3]).min(b.box_rect[1] + b.box_rect[3]) - a.box_rect[1].max(b.box_rect[1]) > 1e-6;
                assert!(!overlap, "step {step}: «{}» and «{}» overlap:\n{report}", a.original, b.original);
            }
        }
        for p in &placed.placed {
            let (x, y, w, h) = (p.box_rect[0], p.box_rect[1], p.box_rect[2], p.box_rect[3]);
            assert!(x >= -1e-6 && y >= -1e-6 && x + w <= 1.0 + 1e-6 && y + h <= 1.0 + 1e-6, "step {step}: «{}» leaves the region: {:?}", p.original, p.box_rect);
        }
    }
}

#[test]
fn the_old_translation_goes_when_the_new_text_appears_not_when_it_has_been_read() {
    let s = settings();
    let mut replay = Replay::new();
    let before = replay.show("CrossxMars.exe-live-d1", &s);
    assert!(before.blocks.iter().any(|b| b.translation.contains("creative way to tell me")), "the first dialogue is drawn");
    // The next frame arrives. Neither its OCR nor its translation has run: only the detector has looked at it.
    let next = image::open(fixtures().join("CrossxMars.exe-live-d2.png")).unwrap();
    let at = replay.now + Duration::from_millis(700 * (replay.scan + 1));
    let jobs = replay.engine.begin(next, &s, at, false);
    assert!(!jobs.is_empty(), "the new text has to be read");
    let frame = replay.engine.finish(&s).expect("withdrawing the old translation is published at once");
    let report = frame.blocks.iter().map(|b| format!("{} «{}»", b.id, b.original)).collect::<Vec<_>>().join("\n");
    assert!(!frame.blocks.iter().any(|b| b.original.contains("creative way") || b.translation.contains("creative way")),
        "the translation of the previous dialogue is still drawn over the new text:\n{report}");
}
