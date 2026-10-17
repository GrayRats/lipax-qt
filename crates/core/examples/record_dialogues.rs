//! Maintainers only: records real RapidOCR readings for a SEQUENCE of frames shown to one engine, the way
//! `tests/inplace/rust/dialogue_change.rs` replays them (700 ms between scans, at most four scans per frame, the
//! speaker/dialogue fields of the previous frame still lingering). Readings go to
//! `tests/inplace/fixtures/<fixture>-local-rapidocr.json`, replacing what was recorded before.
//!   record_dialogues CrossxMars.exe-live-d1 CrossxMars.exe-live-d2 CrossxMars.exe-live-d3
use lipa_core::{
    layout::{Rect, engine::{InplaceEngine, LinesVerdict}},
    ocr::rapid::RapidOcr,
    settings::{InplaceBackgroundMode, Settings},
};

#[tokio::main]
async fn main() {
    let fixtures: Vec<String> = std::env::args().skip(1).collect();
    assert!(!fixtures.is_empty(), "fixture names");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures");
    let mut s = Settings::default();
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
    s.recognition.engine = "rapidocr".into();
    s.recognition.language = "eng".into();
    s.recognition.rapid_threads = 2;
    s.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(lipa_core::ocr::rapid_models::cache_root(), None);
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let mut step = 0u64;
    for fixture in &fixtures {
        let image = image::open(dir.join(format!("{fixture}.png"))).unwrap();
        let mut history = serde_json::json!({"ocr_history": [], "source": fixture,
            "provenance": "Real production RapidOCR readings recorded by `record_dialogues` for the jobs the replay of a dialogue sequence needs; replay input, NOT manually annotated ground truth."});
        for local in 0..4u64 {
            let jobs = e.begin(image.clone(), &s, now + std::time::Duration::from_millis(700 * step), false);
            step += 1;
            for job in jobs {
                let existing = history["ocr_history"].as_array().unwrap().iter().rev().find(|r| {
                    let rect: Rect = serde_json::from_value(r["detector_rect"].clone()).unwrap();
                    rect.iou(&job.rect) >= 0.99
                }).cloned();
                let record = match existing {
                    Some(r) => r,
                    None => {
                        let result = lipa_core::pipeline::read_field(&rapid, &job.image, &s).await.unwrap();
                        let spans: Vec<_> = result.lines.iter().map(|l| serde_json::json!({
                            "confidence": l.confidence, "text": l.text,
                            "rect": {"x": l.rect.x + job.origin.0 as f32, "y": l.rect.y + job.origin.1 as f32, "w": l.rect.w, "h": l.rect.h}})).collect();
                        let record = serde_json::json!({"scan": local, "detector_rect": job.rect, "raw_original": lipa_core::text::normalize(&result.text),
                            "original": lipa_core::text::normalize(&result.text), "confidence": result.confidence, "spans": spans, "engine": "rapidocr"});
                        eprintln!("[{fixture}] scan {local} {:?}: {:?}", job.rect, record["raw_original"]);
                        history["ocr_history"].as_array_mut().unwrap().push(record.clone());
                        record
                    }
                };
                let result = lipa_core::ocr::OcrResult {
                    text: record["raw_original"].as_str().unwrap().into(),
                    lines: record["spans"].as_array().unwrap().iter().map(|span| {
                        let rect: Rect = serde_json::from_value(span["rect"].clone()).unwrap();
                        lipa_core::ocr::OcrLine { rect: lipa_core::layout::CropRect::in_space(rect.x - job.origin.0 as f32, rect.y - job.origin.1 as f32, rect.w, rect.h),
                            text: span["text"].as_str().unwrap().into(), confidence: span["confidence"].as_f64().unwrap() as f32 }
                    }).collect(),
                    confidence: record["confidence"].as_f64().map(|c| c as f32),
                    engine: "rapidocr-replay",
                    ..Default::default()
                };
                let accepted = result.confidence.is_none_or(|c| c >= s.effective_minimum_confidence() as f32) && lipa_core::text::is_meaningful(&result.text);
                if accepted {
                    let o = e.observe_ocr_result(job.id, &result, job.origin, &s);
                    let settled = o.verdict == LinesVerdict::Normal && lipa_core::text::is_meaningful(&o.text);
                    let translation = format!("RU {}", o.text);
                    e.complete(job.id, settled.then_some((o.text, translation)), &s);
                } else {
                    e.reject_ocr(job.id, &s);
                }
            }
            e.finish(&s);
            if !e.needs_rescan() { break; }
        }
        std::fs::write(dir.join(format!("{fixture}-local-rapidocr.json")), serde_json::to_string_pretty(&history).unwrap()).unwrap();
    }
}
