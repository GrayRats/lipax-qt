//! Offline diagnostics, not an OCR accuracy benchmark. Input must be a PNG.
use lipa_core::layout::{
    Script, block_detector::BlockDetector, font_classifier::FontClassifier,
    font_database::InstalledFontDatabase, font_matcher::FontMatcher,
};
use std::time::Instant;
mod qt_measure;
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let path = std::env::args().nth(1).expect("PNG path");
    let image = image::open(&path).expect("image").into_rgba8();
    let db = InstalledFontDatabase::bundled();
    let mut times = vec![];
    let mut result = vec![];
    for _ in 0..5 {
        let t = Instant::now();
        let mask = BlockDetector::ink_mask(&image);
        let blocks = BlockDetector::default().detect_text_blocks(&image, &mask);
        let detection = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let analysis: Vec<_> = blocks
            .iter()
            .map(|b| FontClassifier.classify(&image, &mask, b))
            .collect();
        let glyph = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        result = blocks.iter().zip(&analysis).map(|(b,a)| {
            let f = FontMatcher.select_font(a, Script::Cyrillic, b.block_type, &db, &Default::default());
            assert!(a.confidence.is_finite() && f.confidence.is_finite());
            assert!(b.rect.x >= 0.0 && b.rect.y >= 0.0 && b.rect.right() <= image.width() as f32 && b.rect.bottom() <= image.height() as f32);
            serde_json::json!({"rect":b.rect,"font":f.family,"confidence":f.confidence,"cap_height":a.cap_height_px,"category":format!("{:?}",a.category)})
        }).collect();
        times.push([detection, glyph, t.elapsed().as_secs_f64() * 1000.0]);
    }
    use lipa_core::layout::{
        engine::InplaceEngine,
        fit::ApproxMeasure,
        place::{PlacementCache, RegionInput, Viewport, place_regions},
    };
    use lipa_core::{
        capture::kwin::WindowGeometry,
        ocr::{Ocr, rapid::RapidOcr, rapid_models},
        settings::{NormRect, Settings},
    };
    let mut settings = Settings::default();
    let ocr_blocks = std::env::args().any(|arg| arg == "--ocr-blocks");
    let language = std::env::args()
        .find_map(|arg| arg.strip_prefix("--lang=").map(str::to_owned))
        .unwrap_or_else(|| "eng".into());
    if ocr_blocks {
        settings.recognition.engine = "rapidocr".into();
        settings.recognition.language = language.clone();
    }
    settings.recognition.rapid_threads = 2;
    settings.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(
        std::env::var_os("LIPAX_RAPID_TEST_MODELS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(rapid_models::cache_root),
        std::env::var_os("LIPAX_RAPID_TEST_ORT").map(std::path::PathBuf::from),
    );
    let translation = std::env::args()
        .find_map(|arg| arg.strip_prefix("--translation=").map(str::to_owned))
        .unwrap_or_else(|| "Перевод длиннее оригинала. Hello 日本語".into());
    let translations: std::collections::HashMap<String, String> = std::env::args()
        .find_map(|arg| arg.strip_prefix("--translations=").map(str::to_owned))
        .map(|path| {
            serde_json::from_slice(&std::fs::read(path).expect("translation fixture"))
                .expect("translation map")
        })
        .unwrap_or_default();
    let mut engine = InplaceEngine::with_fonts(db);
    let now = Instant::now();
    let t = Instant::now();
    let mut jobs = engine.begin(
        image::DynamicImage::ImageRgba8(image.clone()),
        &settings,
        now,
        false,
    );
    let first_frame_ms = t.elapsed().as_secs_f64() * 1000.0;
    let mut block_ocr = Vec::new();
    let mut ocr_history = Vec::new();
    let mut scan_index = 0;
    let mut final_frame = None;
    let mut finish_times = Vec::new();
    loop {
        for job in jobs {
            if ocr_blocks {
                let started = Instant::now();
                let result = rapid
                    .recognize_detailed(&job.image, &settings)
                    .await
                    .expect("block RapidOCR");
                let ocr_ms = started.elapsed().as_secs_f64() * 1000.0;
                let result = lipa_core::ocr::filter::clean(
                    result,
                    settings.effective_minimum_confidence(),
                    settings.recognition.filter_noise,
                );
                let raw_original = lipa_core::text::normalize(&result.text);
                let lines: Vec<_> = result
                    .lines
                    .iter()
                    .map(|line| line.rect.in_frame(job.origin))
                    .collect();
                let confidence_accepted = result
                    .confidence
                    .is_none_or(|c| c >= settings.effective_minimum_confidence() as f32)
                    && lipa_core::text::is_meaningful(&raw_original);
                let started = Instant::now();
                let observation = confidence_accepted
                    .then(|| engine.observe_ocr_result(job.id, &result, job.origin, &settings));
                let observation_ms = started.elapsed().as_secs_f64() * 1000.0;
                let original = observation
                    .as_ref()
                    .map_or_else(|| raw_original.clone(), |o| o.text.clone());
                let accepted = confidence_accepted && lipa_core::text::is_meaningful(&original);
                let split = observation
                    .is_some_and(|o| o.verdict == lipa_core::layout::engine::LinesVerdict::Split);
                if !accepted {
                    engine.reject_ocr(job.id, &settings);
                } else {
                    engine.complete(
                        job.id,
                        (accepted && !split).then(|| {
                            (
                                original.clone(),
                                translations.get(&original).unwrap_or(&translation).clone(),
                            )
                        }),
                        &settings,
                    );
                }
                let record = serde_json::json!({"id":job.id,"scan":scan_index,"detector_rect":job.rect,"crop_origin":job.origin,
                "ocr_ms":ocr_ms,"observation_ms":observation_ms,"original":original,"raw_original":raw_original,"confidence":result.confidence,
                "accepted":accepted && !split,"split":split,"lines":lines,
                "spans":result.lines.iter().map(|line| serde_json::json!({"rect":line.rect.in_frame(job.origin),"text":line.text,"confidence":line.confidence})).collect::<Vec<_>>(),
                "filters":format!("{:?}",result.filters),"automatic_filters":result.automatic_filters});
                ocr_history.push(record.clone());
                block_ocr.retain(|old: &serde_json::Value| old["id"] != record["id"]);
                block_ocr.push(record);
            } else {
                engine.complete(
                    job.id,
                    Some((format!("fixture {}", job.id), translation.clone())),
                    &settings,
                );
            }
        }
        let started = Instant::now();
        if let Some(frame) = engine.finish(&settings) {
            final_frame = Some(frame);
        }
        finish_times.push(started.elapsed().as_secs_f64() * 1000.0);
        if !engine.needs_rescan() {
            break;
        }
        scan_index += 1;
        assert!(
            scan_index < 3,
            "OCR regrouping did not settle after three scans"
        );
        jobs = engine.begin(
            image::DynamicImage::ImageRgba8(image.clone()),
            &settings,
            now + std::time::Duration::from_millis(100 * scan_index),
            false,
        );
    }
    let frame = final_frame.expect("frame");
    let region = RegionInput {
        id: "test",
        rect: NormRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        frame: &frame,
    };
    let window = std::env::args()
        .find_map(|arg| {
            arg.strip_prefix("--window=").map(|s| {
                let values: Vec<f64> = s
                    .split(',')
                    .map(|v| v.parse().expect("window coordinate"))
                    .collect();
                let values: [f64; 4] = values.try_into().expect("four window coordinates");
                assert!(values.iter().all(|v| v.is_finite()) && values[2] > 0.0 && values[3] > 0.0);
                WindowGeometry::from(values)
            })
        })
        .unwrap_or_else(|| {
            WindowGeometry::from([0.0, 0.0, image.width() as f64, image.height() as f64])
        });
    let screens = std::env::args()
        .find_map(|arg| {
            arg.strip_prefix("--screens=").map(|s| {
                let values: Vec<[f32; 4]> = serde_json::from_str(s).expect("screen rectangles");
                values
                    .into_iter()
                    .map(|[x, y, w, h]| lipa_core::layout::DesktopRect::in_space(x, y, w, h))
                    .collect()
            })
        })
        .unwrap_or_default();
    let view = Viewport::new(window, screens);
    let mut backgrounds = std::collections::HashMap::new();
    if let Some(directory) = std::env::var_os("LIPAX_DIAGNOSTICS_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("diagnostics directory");
        for block in &frame.blocks {
            if let Some(image) = &block.background.image {
                let output = directory.join(format!("background-{}.png", block.id));
                image.save(&output).expect("background");
                backgrounds.insert(
                    ("test".into(), block.id),
                    format!("file://{}", output.display()),
                );
            }
        }
    }
    let mut cache = PlacementCache::default();
    let qt_measure = qt_measure::QtMeasure::from_env();
    let measure: &dyn lipa_core::layout::fit::TextMeasure = qt_measure
        .as_ref()
        .map(|qt| qt as &dyn lipa_core::layout::fit::TextMeasure)
        .unwrap_or(&ApproxMeasure);
    let t = Instant::now();
    let placements = place_regions(
        &[region],
        &view,
        &settings.appearance.inplace,
        &backgrounds,
        measure,
        &mut cache,
    );
    let placement_ms = t.elapsed().as_secs_f64() * 1000.0;
    let region = RegionInput {
        id: "test",
        rect: NormRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        frame: &frame,
    };
    let t = Instant::now();
    let repeat = place_regions(
        &[region],
        &view,
        &settings.appearance.inplace,
        &backgrounds,
        measure,
        &mut cache,
    );
    let cached_placement_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(placements.placed, repeat.placed);
    let t = Instant::now();
    let repeated_jobs = engine.begin(
        image::DynamicImage::ImageRgba8(image.clone()),
        &settings,
        now + std::time::Duration::from_millis(100 * (scan_index + 1)),
        false,
    );
    let repeat_frame_ms = t.elapsed().as_secs_f64() * 1000.0;
    if !ocr_blocks {
        assert!(repeated_jobs.is_empty(), "identical frame requires no OCR");
    }
    let ocr = if std::env::args().any(|a| a == "--ocr") {
        let t = Instant::now();
        let mut ocr_settings = settings.clone();
        ocr_settings.recognition.engine = "rapidocr".into();
        ocr_settings.recognition.language = language.clone();
        let result = rapid
            .recognize_detailed(
                &image::DynamicImage::ImageRgba8(image.clone()),
                &ocr_settings,
            )
            .await
            .expect("RapidOCR");
        Some(
            serde_json::json!({"engine":result.engine,"language":language,"latency_ms":t.elapsed().as_secs_f64()*1000.0,"text":result.text,
            "lines":result.lines.iter().map(|l| serde_json::json!({"text":l.text,"rect":l.rect,"confidence":l.confidence})).collect::<Vec<_>>()}),
        )
    } else {
        None
    };
    println!(
        "{}",
        serde_json::json!({"image":path,"size":[image.width(),image.height()],"timings_ms":times,"blocks":result,"placed":placements.placed,"dropped":placements.dropped.len(),"dropped_details":placements.dropped,
        "peak_memory":std::fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l| l.starts_with("VmHWM:")).unwrap_or("unavailable"),"finish_ms":finish_times,"first_frame_ms":first_frame_ms,"repeat_frame_ms":repeat_frame_ms,"placement_ms":placement_ms,
        "cached_placement_ms":cached_placement_ms,"fits":cache.fits,"hits":cache.hits,"ocr":ocr,"block_ocr":block_ocr,"ocr_history":ocr_history,"ocr_scans":scan_index+1,"repeat_ocr_jobs":repeated_jobs.len(),
        "frame_blocks":frame.blocks.iter().map(|b| serde_json::json!({"id":b.id,"text_rect":b.text_rect,"source_lines":b.style.source_lines,"lines_note":b.lines_note})).collect::<Vec<_>>(),
        "minimum_confidence":settings.effective_minimum_confidence(),
        "translation_input":if ocr_blocks { "deterministic translation of accepted real OCR blocks" } else { "deterministic translation of every detection candidate; candidates can include scenery" },
        "measurement":if qt_measure.is_some() { "Qt: shared production helper, test IPC adapter" } else { "ApproxMeasure: diagnostic placement only" }})
    );
}
