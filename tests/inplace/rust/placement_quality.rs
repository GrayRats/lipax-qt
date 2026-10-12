use lipa_core::layout::{
    FontWeight, Script, WrapMode,
    fit::{ApproxMeasure, FitInput, FontSpec, TextMeasure, fit_translation_to_box},
};
mod qt_measure;

#[test]
fn local_contrast_reconstructs_the_complete_rhodes_dialogue() {
    use lipa_core::layout::{Rect, block_detector::BlockDetector};
    let image = image::load_from_memory(include_bytes!("../fixtures/rhodes-source.png")).unwrap().into_rgba8();
    let mut mask = BlockDetector::ink_mask(&image);
    let (blocks, _) = BlockDetector::default().detect_with_local(&image, &mut mask);
    let dialogue = Rect::new(327.0, 105.0, 1109.0, 133.0);
    assert!(blocks.iter().any(|b| b.rect.intersection(&dialogue) / dialogue.area() > 0.9),
        "local glyphs must reconstruct the full paragraph before deduplication: {:?}", blocks.iter().map(|b| b.rect).collect::<Vec<_>>());
}

#[test]
fn italic_shop_word_stays_at_the_end_of_its_ocr_row() {
    use lipa_core::layout::{Rect, ocr_geometry::{OcrField, OcrSpan}};
    let field = OcrField::new(1, vec![
        OcrSpan::new(Rect::new(596.0, 201.0, 690.0, 59.0), "Spend the coins you've collected", 98.0).unwrap(),
        OcrSpan::new(Rect::new(1094.0, 244.0, 181.0, 66.0), "Shop!", 98.0).unwrap(),
        OcrSpan::new(Rect::new(648.0, 250.0, 455.0, 60.0), "while playing in the", 98.0).unwrap(),
    ], 51.0).unwrap();
    assert_eq!(field.text(), "Spend the coins you've collected while playing in the Shop!");
}

#[test]
fn amiya_partial_crop_is_owned_by_the_complete_dialogue() {
    use lipa_core::layout::{Rect, ocr_geometry::{OcrField, OcrSpan}};
    let observed: Vec<serde_json::Value> = serde_json::from_str(include_str!("../fixtures/amiya-fragment-ocr.json")).unwrap();
    let fields: Vec<_> = observed.iter().map(|record| {
        let spans = record["spans"].as_array().unwrap().iter().map(|span| {
            OcrSpan::new(serde_json::from_value::<Rect>(span["rect"].clone()).unwrap(),
                span["text"].as_str().unwrap(), span["confidence"].as_f64().unwrap() as f32).unwrap()
        }).collect();
        OcrField::new(record["id"].as_u64().unwrap(), spans, 26.0).unwrap()
    }).collect();
    assert!(!fields[0].duplicate_of(&fields[1]), "without crop provenance, retain the conservative small-label check");
    // Exact crop made by engine.begin for this observed 413px detector component.
    let partial = fields[0].clone().with_crop_bounds(Rect::new(554.0, 390.0, 428.0, 621.0));
    assert!(partial.duplicate_of(&fields[1]), "same source glyphs cut at the OCR input border");
}

#[test]
fn split_ocr_fields_are_scheduled_and_all_parts_are_read_again() {
    use lipa_core::{layout::engine::{InplaceEngine, LinesVerdict}, settings::Settings,
        ocr::{OcrResult, OcrLine}, layout::CropRect};
    let observation: serde_json::Value = serde_json::from_str(include_str!("../fixtures/shop-ocr-observation.json")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pixels = image::open(root.join(observation["source"].as_str().unwrap())).unwrap();
    let settings = Settings::default();
    let now = std::time::Instant::now();
    let mut engine = InplaceEngine::new();
    let jobs = engine.begin(pixels.clone(), &settings, now, false);
    let expected: lipa_core::layout::Rect = serde_json::from_value(observation["detector_rect"].clone()).unwrap();
    let job = jobs.into_iter().find(|j| j.rect.iou(&expected) > 0.99).unwrap();
    let origin = (observation["origin"][0].as_u64().unwrap() as u32, observation["origin"][1].as_u64().unwrap() as u32);
    let lines: Vec<OcrLine> = observation["spans"].as_array().unwrap().iter().map(|span| {
        let rect: lipa_core::layout::Rect = serde_json::from_value(span["rect"].clone()).unwrap();
        OcrLine { rect: CropRect::in_space(rect.x-origin.0 as f32, rect.y-origin.1 as f32, rect.w, rect.h),
            text: span["text"].as_str().unwrap().into(), confidence: span["confidence"].as_f64().unwrap() as f32 }
    }).collect();
    let result = OcrResult { text: lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" "), lines,
        confidence: Some(98.0), ..Default::default() };
    assert_eq!(engine.observe_ocr_result(job.id, &result, origin, &settings).verdict, LinesVerdict::Split);
    assert!(engine.needs_rescan(), "split and join requests both require a new OCR pass");
    let jobs = engine.begin(pixels, &settings, now + std::time::Duration::from_millis(100), false);
    let instruction = lipa_core::layout::Rect::new(596.0, 201.0, 690.0, 109.0);
    let footer = lipa_core::layout::Rect::new(496.0, 809.0, 627.0, 99.0);
    for source in [instruction, footer] {
        assert!(jobs.iter().any(|j| j.rect.intersection(&source) / source.area() > 0.9),
            "OCR-confirmed text must survive splitting: {source:?}, jobs: {:?}", jobs.iter().map(|j| j.rect).collect::<Vec<_>>());
    }
}

fn input(width: f32, height: f32) -> FitInput<'static> {
    FitInput {
        text: "Перевод персонажами становится читаемым",
        width, height,
        font: FontSpec { family: "Inter".into(), weight: FontWeight::Normal, italic: false },
        manual_px: None, cap_height_px: 10.0, min_px: 14.0, max_px: 14.0,
        source_lines: 1, manual_line_height: Some(1.0), source_line_px: None,
        letter_spacing: 0.0, letter_spacing_manual: true,
        manual_wrap: Some(WrapMode::WordWrap), script: Script::Cyrillic,
        condensed_family: None,
    }
}

#[test]
fn shorter_than_one_line_reports_zero_visible_lines() {
    let fit = fit_translation_to_box(&input(200.0, 3.0), &ApproxMeasure);
    assert!(fit.overflow);
    assert_eq!(fit.max_lines, 0, "a clipped glyph fragment is not a rendered line");
}

#[test]
#[ignore = "requires LIPAX_QT_MEASURE_SERVER: verifies Qt's real word-boundary/anywhere fallback"]
fn qt_character_columns_are_not_successful_word_wrapping() {
    let measure = qt_measure::QtMeasure::from_env().expect("Qt measurement helper");
    let small = input(22.0, 900.0);
    let fit = fit_translation_to_box(&small, &measure);
    assert!(fit.overflow, "Qt can stack characters vertically, but complete words do not fit");
    let words = measure.measure("персонажами", &small.font, 14.0, 0.0, f32::MAX, WrapMode::NoWrap);
    let fit = fit_translation_to_box(&input(words.width + 1.0, 900.0), &measure);
    assert!(!fit.overflow, "widening enough for a complete word allows normal wrapping");
}

#[test]
#[ignore = "requires LIPAX_QT_MEASURE_SERVER"]
fn explicit_character_wrapping_keeps_its_semantics() {
    let measure = qt_measure::QtMeasure::from_env().expect("Qt measurement helper");
    let mut small = input(22.0, 900.0);
    small.manual_wrap = Some(WrapMode::WrapAnywhere);
    assert!(!fit_translation_to_box(&small, &measure).overflow);
}

#[test]
fn a_viewport_shorter_than_a_glyph_does_not_publish_an_empty_replacement() {
    use lipa_core::{
        capture::kwin::WindowGeometry,
        layout::{Rect, engine::{InplaceEngine, InplaceFrame},
            place::{PlacementCache, RegionInput, Viewport, place_regions}},
        settings::{NormRect, Settings},
    };
    let mut pixels = image::RgbaImage::from_pixel(400, 180, image::Rgba([0, 0, 0, 255]));
    for column in 0..12 {
        for y in 60..90 { for x in 40 + column * 20..50 + column * 20 {
            pixels.put_pixel(x, y, image::Rgba([255; 4]));
        }}
    }
    let settings = Settings::default();
    let mut engine = InplaceEngine::new();
    let jobs = engine.begin(image::DynamicImage::ImageRgba8(pixels), &settings, std::time::Instant::now(), false);
    for job in jobs { engine.complete(job.id, Some(("source text".into(), "Перевод".into())), &settings); }
    let mut block = engine.finish(&settings).unwrap().blocks.remove(0);
    block.text_rect = Rect::new(40.0, 0.0, 100.0, 3.0);
    block.background.rect = block.text_rect;
    block.background.image = None;
    let frame = InplaceFrame { frame: (400, 3), blocks: vec![block], undrawable: vec![], new_translations: vec![] };
    let region = RegionInput { id: "small", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
    let result = place_regions(&[region], &Viewport::window(WindowGeometry::from([0.0, 0.0, 400.0, 3.0])),
        &settings.appearance.inplace, &Default::default(), &ApproxMeasure, &mut PlacementCache::default());
    assert!(result.placed.is_empty(), "a background with no readable line must not hide the source");
    assert_eq!(result.dropped.len(), 1);
    assert!(result.dropped[0].reason.contains("строк"));
}
