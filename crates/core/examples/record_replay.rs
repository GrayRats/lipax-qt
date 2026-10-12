//! Maintainers only: records the real RapidOCR readings that the replay in `tests/inplace/rust/ocr_ownership.rs`
//! is missing after a detector change, into `tests/inplace/fixtures/<process>-local-rapidocr.json`.
//!   record_replay CrossxMars.exe-129055
use lipa_core::{
    layout::{Rect, engine::{InplaceEngine, LinesVerdict}},
    ocr::rapid::RapidOcr,
    settings::Settings,
};

#[tokio::main]
async fn main() {
    let process = std::env::args().nth(1).expect("process-pid");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures");
    let image = image::open(dir.join(format!("{process}.png"))).unwrap();
    let read = |name: &str| serde_json::from_slice::<serde_json::Value>(&std::fs::read(dir.join(name)).unwrap_or_else(|_| b"{\"ocr_history\":[]}".to_vec())).unwrap();
    let history = read(&format!("{process}-rapidocr.json"));
    let supplement_path = dir.join(format!("{process}-local-rapidocr.json"));
    let mut supplement = read(&format!("{process}-local-rapidocr.json"));
    let mut s = Settings::default();
    s.recognition.engine = "rapidocr".into();
    s.recognition.language = "eng".into();
    s.recognition.rapid_threads = 2;
    s.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(lipa_core::ocr::rapid_models::cache_root(), None);
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    for scan in 0..3u64 {
        let jobs = e.begin(image.clone(), &s, now + std::time::Duration::from_millis(100 * scan), false);
        for job in jobs {
            let find = |h: &serde_json::Value| h["ocr_history"].as_array().unwrap().iter().find(|r| {
                let rect: Rect = serde_json::from_value(r["detector_rect"].clone()).unwrap();
                r["scan"] == scan && rect.iou(&job.rect) >= 0.99
            }).cloned();
            let record = match find(&history).or_else(|| find(&supplement)) {
                Some(r) => r,
                None => {
                    let result = lipa_core::pipeline::read_field(&rapid, &job.image, &s).await.unwrap();
                    let spans: Vec<_> = result.lines.iter().map(|l| serde_json::json!({
                        "confidence": l.confidence, "text": l.text,
                        "rect": {"x": l.rect.x + job.origin.0 as f32, "y": l.rect.y + job.origin.1 as f32, "w": l.rect.w, "h": l.rect.h}})).collect();
                    let record = serde_json::json!({"scan": scan, "detector_rect": job.rect, "raw_original": lipa_core::text::normalize(&result.text),
                        "original": lipa_core::text::normalize(&result.text), "confidence": result.confidence, "spans": spans, "engine": "rapidocr"});
                    eprintln!("recorded scan {scan} {:?}: {:?}", job.rect, record["raw_original"]);
                    supplement["ocr_history"].as_array_mut().unwrap().push(record.clone());
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
                e.complete(job.id, settled.then(|| (o.text, "Проверочный перевод".into())), &s);
            } else {
                e.reject_ocr(job.id, &s);
            }
        }
        e.finish(&s);
        if !e.needs_rescan() { break; }
    }
    supplement["source"] = serde_json::json!(process);
    supplement["provenance"] = serde_json::json!("Real production RapidOCR readings (with the second chances for faint crops) of the jobs that the replay needed after the colour-plane local-contrast detector change; replay input, NOT manually annotated ground truth.");
    std::fs::write(supplement_path, serde_json::to_string_pretty(&supplement).unwrap()).unwrap();
}
