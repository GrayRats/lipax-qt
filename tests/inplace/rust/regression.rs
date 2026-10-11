use lipa_core::layout::Rect;
use lipa_core::layout::block_detector::{DetectedTextBlock, TextLine};
use lipa_core::layout::font_classifier::FontAnalysis;
use lipa_core::layout::font_matcher::FontSelection;
use lipa_core::layout::tracker::TextBlockTracker;
use std::time::Instant;

fn block(x: f32, y: f32, w: f32, h: f32, count: usize) -> DetectedTextBlock {
    let lines = (0..count)
        .map(|i| TextLine {
            rect: Rect::new(x, y + i as f32 * h / count as f32, w, h / count as f32),
            glyphs: vec![],
            ink_color: [255; 3],
        })
        .collect();
    DetectedTextBlock {
        rect: Rect::new(x, y, w, h),
        lines,
        block_type: lipa_core::layout::TextBlockType::Unknown,
        ink_color: [255; 3],
    }
}

#[test]
fn zero_sized_image_has_no_pixel_crop() {
    assert_eq!(Rect::new(0.0, 0.0, 10.0, 10.0).pixels(0, 4), (0, 0, 0, 0));
    assert_eq!(Rect::new(0.0, 0.0, 10.0, 10.0).pixels(4, 0), (0, 0, 0, 0));
}

#[test]
fn empty_block_has_no_glyph_evidence_or_line_height() {
    use lipa_core::layout::{FontCategory, block_detector::BlockDetector, font_classifier::FontClassifier};
    let image = image::RgbaImage::new(20, 20);
    let mask = BlockDetector::ink_mask(&image);
    let empty = block(0.0, 0.0, 0.0, 0.0, 0);
    assert_eq!(empty.line_height(), 0.0);
    let analysis = FontClassifier.classify(&image, &mask, &empty);
    assert_eq!(analysis.category, FontCategory::Unknown);
    assert_eq!(analysis.features.glyphs, 0);
    assert_eq!(analysis.cap_height_px, 0.0);
}

#[test]
fn ocr_line_order_duplicates_and_invalid_boxes_do_not_change_typography() {
    use lipa_core::layout::{block_detector::BlockDetector, font_classifier::FontClassifier, typography::TypographyEstimator};
    let image = image::RgbaImage::new(400, 200);
    let detected = block(20.0, 20.0, 300.0, 100.0, 3);
    let analysis = FontClassifier.classify(&image, &BlockDetector::ink_mask(&image), &detected);
    let valid = [Rect::new(20.0, 20.0, 280.0, 20.0), Rect::new(20.0, 52.0, 180.0, 20.0), Rect::new(20.0, 84.0, 220.0, 20.0)];
    let expected = TypographyEstimator.estimate_from(&detected, &analysis, [0; 3], 400.0, Some(&valid));
    let messy = [valid[2], valid[1], valid[0], valid[1], Rect::new(f32::NAN, 0.0, 1.0, 1.0), Rect::new(0.0, 0.0, 0.0, 20.0), Rect::new(0.0, 0.0, 20.0, -1.0)];
    let actual = TypographyEstimator.estimate_from(&detected, &analysis, [0; 3], 400.0, Some(&messy));
    assert_eq!(actual, expected);
    assert_eq!(actual.line_height.absolute_px, 32.0);
}

#[test]
fn invalid_ocr_geometry_keeps_detector_typography() {
    use lipa_core::layout::{block_detector::BlockDetector, font_classifier::FontClassifier, typography::TypographyEstimator};
    let image = image::RgbaImage::new(400, 200);
    let detected = block(20.0, 20.0, 300.0, 100.0, 3);
    let analysis = FontClassifier.classify(&image, &BlockDetector::ink_mask(&image), &detected);
    let expected = TypographyEstimator.estimate(&detected, &analysis, [0; 3], 400.0);
    let actual = TypographyEstimator.estimate_from(&detected, &analysis, [0; 3], 400.0, Some(&[Rect::new(0.0, f32::INFINITY, 4.0, 5.0)]));
    assert_eq!(actual, expected);
}

#[test]
fn engine_normalizes_ocr_geometry_before_refining_a_block() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let mut image = image::RgbaImage::from_pixel(400, 180, image::Rgba([0, 0, 0, 255]));
    for column in 0..12 {
        for y in 60..90 {
            for x in 40 + column * 20..50 + column * 20 {
                image.put_pixel(x, y, image::Rgba([255; 4]));
            }
        }
    }
    let settings = Settings::default();
    let mut engine = InplaceEngine::new();
    let jobs = engine.begin(image::DynamicImage::ImageRgba8(image), &settings, Instant::now(), false);
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    engine.complete(job.id, Some(("Hello".into(), "Привет".into())), &settings);
    engine.observe_ocr_lines(job.id, &[job.rect, Rect::new(f32::NAN, 0.0, 1.0, 1.0), job.rect], &settings);
    let frame = engine.finish(&settings).unwrap();
    assert_eq!(frame.blocks.len(), 1);
    assert_eq!(frame.blocks[0].style.source_lines, 1);
    assert!(frame.blocks[0].lines_note.contains("OCR и детектор согласны"));
    assert!([frame.blocks[0].text_rect.x, frame.blocks[0].text_rect.y, frame.blocks[0].text_rect.w, frame.blocks[0].text_rect.h].iter().all(|v| v.is_finite()));
}

#[test]
fn diagonal_contact_and_a_bridge_join_components_but_a_pixel_gap_does_not() {
    use lipa_core::layout::block_detector::BlockDetector;
    let mut image = image::RgbaImage::from_pixel(180, 80, image::Rgba([0, 0, 0, 255]));
    // Two diagonal strokes: 8-connectivity joins each stroke, not the gap.
    for offset in [10, 35] {
        for y in 20..32 {
            image.put_pixel(offset + y - 20, y, image::Rgba([255; 4]));
        }
    }
    // Two disjoint runs in one row are both touched by a later bridge.
    for y in 20..32 {
        for x in [55, 65] { image.put_pixel(x, y, image::Rgba([255; 4])); }
    }
    for x in 55..=65 { image.put_pixel(x, 32, image::Rgba([255; 4])); }
    let found = BlockDetector::default().detect_text_blocks(&image, &BlockDetector::ink_mask(&image));
    let mut glyphs: Vec<_> = found.iter().flat_map(|b| &b.lines).flat_map(|l| &l.glyphs).copied().collect();
    glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(glyphs, [Rect::new(10.0, 20.0, 12.0, 12.0), Rect::new(35.0, 20.0, 12.0, 12.0), Rect::new(55.0, 20.0, 11.0, 13.0)]);
}

#[test]
fn slant_measurement_separates_upright_from_italic_strokes_at_several_sizes() {
    use lipa_core::layout::{block_detector::BlockDetector, font_classifier::FontClassifier};
    for height in [16, 24, 40] {
        for italic in [false, true] {
            let mut image = image::RgbaImage::from_pixel(1000, 120, image::Rgba([0, 0, 0, 255]));
            for column in 0..20 {
                for y in 0..height {
                    let shift = if italic { (height - y) * 3 / 10 } else { 0 };
                    for dx in 0..3 { image.put_pixel(20 + column * (height + 6) + shift + dx, 30 + y, image::Rgba([255; 4])); }
                }
            }
            let mask = BlockDetector::ink_mask(&image);
            let blocks = BlockDetector::default().detect_text_blocks(&image, &mask);
            assert_eq!(blocks.len(), 1);
            let analysis = FontClassifier.classify(&image, &mask, &blocks[0]);
            assert_eq!(analysis.italic, italic, "height={height}, {analysis:?}");
            if italic { assert!((analysis.slant - 0.3).abs() <= 0.06); }
            else { assert!(analysis.slant.abs() <= 0.02); }
        }
    }
}

#[test]
fn disjoint_clip_stays_at_nearest_boundary() {
    let bounds = Rect::new(-50.0, 10.0, 100.0, 80.0);
    for x in [-500.0, 500.0] {
        for y in [-500.0, 500.0] {
            let clipped = Rect::new(x, y, 5.0, 5.0).clipped_to(&bounds);
            assert!(bounds.contains(&clipped));
            assert_eq!(clipped.area(), 0.0);
        }
    }
}

#[test]
fn typography_change_requires_two_confident_observations() {
    use lipa_core::layout::{FontCategory, FontWeight};
    let mut tracker = TextBlockTracker::default();
    let now = Instant::now();
    let id = tracker.update(&[block(0.0, 0.0, 200.0, 30.0, 1)], now)[0].id;
    let t = tracker.get_mut(id).unwrap();
    let old = FontAnalysis {
        category: FontCategory::SansSerif,
        weight: FontWeight::Normal,
        italic: false,
        stroke_px: 2.0,
        cap_height_px: 20.0,
        width_ratio: 0.6,
        slant: 0.0,
        monospace: false,
        condensed: false,
        confidence: 0.9,
        features: Default::default(),
    };
    t.observe_font(old.clone(), now);
    t.font = Some(FontSelection::generic(old.category));
    let changed = FontAnalysis {
        weight: FontWeight::Bold,
        ..old.clone()
    };
    t.observe_font(changed.clone(), now);
    assert!(t.font.is_some());
    t.observe_font(old.clone(), now); // a noisy observation must not accumulate
    t.observe_font(changed.clone(), now);
    assert!(t.font.is_some());
    t.observe_font(changed, now);
    assert!(t.font.is_none());
    assert_eq!(t.font_analysis.as_ref().unwrap().weight, FontWeight::Bold);
    t.font = Some(FontSelection::generic(old.category));
    t.observe_font(
        FontAnalysis {
            confidence: 0.2,
            ..old
        },
        now,
    );
    assert!(t.font.is_some(), "weak evidence does not reset the family");
}

#[test]
fn matching_confidence_never_invents_evidence() {
    use lipa_core::layout::{FontCategory, FontWeight, Script, TextBlockType};
    use lipa_core::layout::{font_database::InstalledFontDatabase, font_matcher::FontMatcher};
    let db = InstalledFontDatabase::bundled();
    for confidence in [0.0, 0.05, 0.2, 0.8, 1.0] {
        let analysis = FontAnalysis {
            category: FontCategory::SansSerif,
            weight: FontWeight::Normal,
            italic: false,
            stroke_px: 2.0,
            cap_height_px: 20.0,
            width_ratio: 0.6,
            slant: 0.0,
            monospace: false,
            condensed: false,
            confidence,
            features: Default::default(),
        };
        let selected = FontMatcher.select_font(
            &analysis,
            Script::Latin,
            TextBlockType::UiLabel,
            &db,
            &Default::default(),
        );
        assert!(!selected.generic);
        assert!(selected.confidence <= confidence);
    }
}

#[test]
fn connected_component_order_is_repeatable() {
    use lipa_core::layout::block_detector::BlockDetector;
    let mut image = image::RgbaImage::from_pixel(640, 240, image::Rgba([0, 0, 0, 255]));
    for row in 0..6 {
        for column in 0..25 {
            let x = 10 + column * 20;
            let y = 10 + row * 35 + column % 3;
            for py in y..y + 12 + column % 5 {
                for px in x..x + 7 + column % 4 {
                    image.put_pixel(px, py, image::Rgba([255, 255, 255, 255]));
                }
            }
        }
    }
    let detector = BlockDetector::default();
    let mask = BlockDetector::ink_mask(&image);
    let describe = || format!("{:?}", detector.detect_text_blocks(&image, &mask));
    let expected = describe();
    for _ in 0..30 {
        assert_eq!(describe(), expected);
    }
}

#[test]
fn background_panel_does_not_become_a_giant_glyph() {
    use lipa_core::layout::block_detector::BlockDetector;
    let mut image = image::RgbaImage::from_pixel(800, 600, image::Rgba([0, 0, 0, 255]));
    for y in 0..350 {
        for x in 0..500 {
            image.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
        }
    }
    for i in 0..12 {
        for y in 450..480 {
            for x in 20 + i * 20..30 + i * 20 {
                if x < 23 + i * 20 || x >= 27 + i * 20 || (463..466).contains(&y) {
                    image.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
                }
            }
        }
    }
    let mask = BlockDetector::ink_mask(&image);
    let blocks = BlockDetector::default().detect_text_blocks(&image, &mask);
    assert!(
        !blocks.is_empty(),
        "the text below the panel remains detectable"
    );
    assert!(
        blocks.iter().all(|b| b.rect.y >= 450.0),
        "panel must not consume the text-block budget"
    );
}

#[test]
fn long_translation_stays_inside_custom_capture_on_negative_monitor() {
    use lipa_core::layout::{
        DesktopRect,
        engine::InplaceEngine,
        fit::ApproxMeasure,
        place::{PlacementCache, RegionInput, Viewport, place_regions},
    };
    use lipa_core::{
        capture::kwin::WindowGeometry,
        settings::{NormRect, Settings},
    };
    let mut image = image::RgbaImage::from_pixel(400, 200, image::Rgba([0, 0, 0, 255]));
    for i in 0..12 {
        for y in 150..180 {
            for x in 140 + i * 20..150 + i * 20 {
                if x < 143 + i * 20 || x >= 147 + i * 20 || (163..166).contains(&y) {
                    image.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
                }
            }
        }
    }
    let settings = Settings::default();
    let mut engine = InplaceEngine::new();
    for job in engine.begin(
        image::DynamicImage::ImageRgba8(image),
        &settings,
        Instant::now(),
        false,
    ) {
        engine.complete(
            job.id,
            Some((
                "original".into(),
                "Очень длинный перевод с переносом строк. ".repeat(10),
            )),
            &settings,
        );
    }
    let frame = engine.finish(&settings).unwrap();
    let rect = NormRect {
        x: 0.6,
        y: 0.6,
        w: 0.4,
        h: 0.3,
    };
    let region = RegionInput {
        id: "crop",
        rect,
        frame: &frame,
    };
    let view = Viewport::new(
        WindowGeometry::from([-1800.0, 80.0, 1200.0, 800.0]),
        vec![DesktopRect::in_space(-1920.0, 0.0, 1920.0, 1080.0)],
    );
    let result = place_regions(
        &[region],
        &view,
        &settings.appearance.inplace,
        &Default::default(),
        &ApproxMeasure,
        &mut PlacementCache::default(),
    );
    assert!(!result.placed.is_empty());
    for placed in result.placed {
        let [x, y, w, h] = placed.box_rect;
        assert!(
            x >= -1e-6 && y >= -1e-6 && x + w <= 1.000001 && y + h <= 1.000001,
            "translation plate must respect the selected capture: {:?}",
            placed.box_rect
        );
    }
}

#[test]
fn nested_repeat_of_the_same_ocr_text_does_not_block_its_owner() {
    use lipa_core::{capture::kwin::WindowGeometry, settings::{NormRect, Settings}};
    use lipa_core::layout::{DesktopRect, engine::InplaceEngine, fit::ApproxMeasure,
        place::{PlacementCache, RegionInput, Viewport, place_regions}};
    let mut image = image::RgbaImage::from_pixel(400, 200, image::Rgba([0, 0, 0, 255]));
    for column in 0..12 {
        for y in 60..90 { for x in 40 + column * 20..50 + column * 20 {
            image.put_pixel(x, y, image::Rgba([255; 4]));
        } }
    }
    let settings = Settings::default();
    let mut engine = InplaceEngine::new();
    let jobs = engine.begin(image::DynamicImage::ImageRgba8(image), &settings, Instant::now(), false);
    engine.complete(jobs[0].id, Some(("Morning, Ella. We have the right to be angry!".into(), "Доброе утро!".into())), &settings);
    let mut frame = engine.finish(&settings).unwrap();
    let mut fragment = frame.blocks[0].clone();
    fragment.id = 900;
    fragment.original = "the right to be angry!".into();
    frame.blocks.push(fragment);
    let view = Viewport::new(WindowGeometry::from([0.0, 0.0, 400.0, 200.0]),
        vec![DesktopRect::in_space(0.0, 0.0, 400.0, 200.0)]);
    let run = |frame: &lipa_core::layout::engine::InplaceFrame| {
        place_regions(&[RegionInput { id: "game", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame }],
            &view, &settings.appearance.inplace, &Default::default(), &ApproxMeasure, &mut PlacementCache::default())
    };
    let mut single = frame.clone();
    single.blocks.pop();
    let expected_box = run(&single).placed[0].box_rect;
    let result = run(&frame);
    assert_eq!(result.placed[0].box_rect, expected_box, "a duplicate must not force the owner into a fallback label");
    assert_eq!(result.placed.len(), 1, "a confirmed repeated fragment must not block its owner");
    assert_eq!(result.placed[0].block_id, frame.blocks[0].id);
    assert_eq!(result.dropped.len(), 1);
    // Real SAVE/LOAD OCR can produce a nested three-letter suffix such as OAD.
    frame.blocks[0].original = "Quick SAVE LOAD".into();
    frame.blocks[1].original = "OAD".into();
    let short = run(&frame);
    assert_eq!(short.placed[0].box_rect, expected_box);
    assert_eq!(short.placed.len(), 1);
    assert_eq!(short.dropped[0].reason, "повторный OCR того же текста");
    // Geometric overlap alone is not proof that an unrelated label may be covered.
    frame.blocks[1].original = "Independent HUD label".into();
    assert!(run(&frame).dropped.iter().all(|d| d.reason != "повторный OCR того же текста"));
    // The same text at another position remains an independent field.
    frame.blocks[1].original = frame.blocks[0].original.clone();
    frame.blocks[1].text_rect.y += 80.0;
    assert_eq!(run(&frame).placed.len(), 2);
}

#[test]
fn ocr_word_fragments_share_a_line_but_distant_labels_do_not() {
    use lipa_core::layout::{typography::TypographyEstimator, block_detector::BlockDetector, font_classifier::FontClassifier};
    let image = image::RgbaImage::new(500, 200);
    let detected = block(20.0, 20.0, 400.0, 60.0, 2);
    let analysis = FontClassifier.classify(&image, &BlockDetector::ink_mask(&image), &detected);
    let boxes = [Rect::new(20.0, 20.0, 80.0, 20.0), Rect::new(105.0, 22.0, 100.0, 20.0), Rect::new(20.0, 52.0, 180.0, 20.0)];
    let result = TypographyEstimator.estimate_from(&detected, &analysis, [0; 3], 500.0, Some(&boxes));
    assert_eq!(result.lines, 2, "word fragments must not inflate source line count");
    assert_eq!(result.line_height.absolute_px, 32.0);
    let distant = [boxes[0], Rect::new(350.0, 20.0, 80.0, 20.0)];
    assert_eq!(TypographyEstimator.estimate_from(&detected, &analysis, [0; 3], 500.0, Some(&distant)).lines, 2);
}

#[test]
fn ocr_refinement_does_not_turn_separated_rows_into_overlapping_sources() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let mut image = image::RgbaImage::from_pixel(400, 200, image::Rgba([0, 0, 0, 255]));
    // A small text and larger text remain independently detected fields.
    for (top, height) in [(40, 20), (66, 32)] {
        for column in 0..12 { for y in top..top+height { for x in 40+column*20..50+column*20 {
            image.put_pixel(x, y, image::Rgba([255; 4]));
        } } }
    }
    let settings = Settings::default();
    let mut engine = InplaceEngine::new();
    let jobs = engine.begin(image::DynamicImage::ImageRgba8(image.clone()), &settings, Instant::now(), false);
    assert_eq!(jobs.len(), 2);
    for job in &jobs {
        engine.observe_ocr_lines(job.id, &[Rect::new(job.rect.x, job.rect.y-5.0, job.rect.w, job.rect.h+10.0)], &settings);
        engine.complete(job.id, Some((format!("Independent row {}", job.id), "Перевод".into())), &settings);
    }
    let frame = engine.finish(&settings).unwrap();
    assert_eq!(frame.blocks.len(), 2);
    assert_eq!(frame.blocks[0].text_rect.intersection(&frame.blocks[1].text_rect), 0.0);
    assert!(engine.begin(image::DynamicImage::ImageRgba8(image), &settings,
        Instant::now() + std::time::Duration::from_millis(100), false).is_empty());
    for block in &frame.blocks {
        assert_eq!(engine.tracker().get(block.id).unwrap().text_rect(), block.text_rect,
            "cached OCR geometry must not accumulate padding or jitter on the next frame");
    }
}
