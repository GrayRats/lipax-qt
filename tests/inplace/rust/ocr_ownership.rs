use lipa_core::layout::{
    Rect,
    ocr_geometry::{OcrField, OcrSpan},
};
fn field(id: u64, rows: &[(f32, f32, f32, f32, &str)], cap: f32) -> OcrField {
    OcrField::new(
        id,
        rows.iter()
            .map(|&(x, y, w, h, text)| OcrSpan::new(Rect::new(x, y, w, h), text, 98.0).unwrap())
            .collect(),
        cap,
    )
    .unwrap()
}
#[test]
fn overlapping_partial_ocr_readings_belong_to_the_complete_paragraph() {
    let owner = field(
        1,
        &[
            (20., 40., 180., 25., "Morning, Ella."),
            (205., 40., 260., 25., "Sometimes I wonder"),
            (
                20.,
                75.,
                460.,
                25.,
                "and-forth gets tiresome and, quite frankly, I'm upset.",
            ),
            (20., 110., 250., 25., "but I have the right to be angry!"),
        ],
        20.,
    );
    let partial = field(
        2,
        &[
            (22., 41., 177., 25., "Morning, Ella."),
            (207., 41., 255., 25., "Sometimes I wonder"),
            (
                21.,
                76.,
                455.,
                25.,
                "and-forth gets tiresome and, guite frankly, I'm upset.",
            ),
        ],
        20.,
    );
    assert!(partial.duplicate_of(&owner));
    assert!(!owner.duplicate_of(&partial));
    let suffix = field(
        3,
        &[(21., 111., 248., 25., "but I have the right to be angry!")],
        20.,
    );
    assert!(suffix.duplicate_of(&owner));
}
#[test]
fn independent_text_and_repeated_text_at_another_position_are_not_suppressed() {
    let owner = field(
        1,
        &[(
            20.,
            40.,
            300.,
            25.,
            "Morning Ella we have the right to be angry",
        )],
        20.,
    );
    let unrelated = field(2, &[(22., 41., 298., 25., "Independent HUD label")], 20.);
    assert!(!unrelated.duplicate_of(&owner));
    let other_position = field(3, &[(20., 140., 300., 25., owner.text())], 20.);
    assert!(!other_position.duplicate_of(&owner));
}
#[test]
fn partially_overlapping_independent_labels_and_repeated_words_stay_separate() {
    // Neighbouring HUD labels whose OCR boxes overlap by a third: different words, neither owns the other.
    let sound = field(1, &[(20., 40., 120., 22., "Sound")], 20.);
    let volume = field(2, &[(100., 41., 130., 22., "Volume 80")], 20.);
    assert!(!sound.duplicate_of(&volume) && !volume.duplicate_of(&sound));
    // The same word twice in one row (two buttons): independent, however alike.
    let yes_a = field(3, &[(20., 40., 70., 22., "Yes")], 20.);
    let yes_b = field(4, &[(120., 40., 70., 22., "Yes")], 20.);
    assert!(!yes_a.duplicate_of(&yes_b) && !yes_b.duplicate_of(&yes_a));
    // A field whose text is a different word set inside the same rows is not a duplicate even when the
    // rectangles coincide: only text *and* geometry together prove a repeated reading.
    let long = field(5, &[(20., 40., 300., 22., "Fuel heating status: nominal")], 20.);
    let other = field(6, &[(25., 41., 100., 22., "Engine status")], 20.);
    assert!(!other.duplicate_of(&long));
    // Deterministic: asking again, in either order, gives the same answers.
    for _ in 0..3 {
        assert!(!sound.duplicate_of(&volume) && !yes_a.duplicate_of(&yes_b));
    }
}
#[test]
fn duplicate_parent_spans_cannot_fake_complete_coverage() {
    let owner = field(
        1,
        &[
            (0., 0., 40., 20., "Hello world"),
            (0., 0., 40., 20., "Hello world"),
        ],
        16.,
    );
    let child = field(2, &[(0., 0., 50., 20., "Hello")], 16.);
    assert!(child.duplicate_of(&owner)); // exactly 80%, acceptable
    let uncovered = field(3, &[(0., 0., 70., 20., "Hello")], 16.);
    assert!(!uncovered.duplicate_of(&owner));
}
#[test]
fn clipped_glyph_evidence_does_not_suppress_a_genuine_short_label() {
    let owner = field(
        1,
        &[(
            0.,
            40.,
            600.,
            40.,
            "and forth gets tiresome and quite frankly more than it should",
        )],
        30.,
    );
    let clipped = field(2, &[(100., 53., 20., 15., "Ia")], 41.);
    assert!(clipped.duplicate_of(&owner));
    let genuine_short = field(3, &[(100., 50., 20., 20., "HP")], 16.);
    assert!(!genuine_short.duplicate_of(&owner));
}
#[test]
fn geometry_and_confidence_are_validated_at_the_boundary() {
    for rect in [
        Rect::new(f32::NAN, 0., 20., 20.),
        Rect::new(0., 0., 0., 20.),
        Rect::new(0., 0., 20., -1.),
    ] {
        assert!(OcrSpan::new(rect, "Hello", 98.).is_none());
    }
    for confidence in [f32::NAN, -1., 101.] {
        assert!(OcrSpan::new(Rect::new(0., 0., 20., 20.), "Hello", confidence).is_none());
    }
    assert!(OcrSpan::new(Rect::new(0., 0., 20., 20.), "...", 98.).is_none());
    assert!(OcrField::new(1, vec![], 20.).is_none());
}
#[test]
fn partial_word_crops_request_a_fresh_combined_reading_not_a_guessed_name() {
    let first = field(1, &[(55., 697., 108., 62., "Mik")], 41.);
    let second = field(2, &[(139., 717., 45., 32., "ca")], 29.);
    assert!(first.needs_join(&second));
    assert!(!second.duplicate_of(&first));
    let distinct = field(3, &[(400., 717., 45., 32., "ca")], 29.);
    assert!(!first.needs_join(&distinct));
    let different_size = field(4, &[(139., 717., 45., 32., "HP")], 10.);
    assert!(!first.needs_join(&different_size));
}
#[test]
fn shared_hud_text_with_extra_source_pixels_needs_another_crop() {
    let owner = field(
        1,
        &[
            (574., 455., 161., 15., "Exoskeleton management:"),
            (771., 456., 40., 13., "active"),
            (701., 486., 136., 13., "122.115.122.33:E444"),
        ],
        9.,
    );
    let partial = field(
        2,
        &[
            (703., 457., 31., 10., "nent:"),
            (773., 457., 36., 10., "active"),
            (702., 485., 172., 15., "122.:115.122.33:E444:1123"),
        ],
        9.,
    );
    assert!(
        !partial.duplicate_of(&owner),
        "extra IP digits cannot be silently lost"
    );
    assert!(partial.needs_join(&owner));
}
#[test]
fn a_low_confidence_paragraph_cannot_own_a_reliable_reading() {
    let weak = OcrField::new(
        1,
        vec![OcrSpan::new(Rect::new(0., 0., 100., 20.), "Hello world", 40.).unwrap()],
        16.,
    )
    .unwrap();
    let reliable = field(2, &[(0., 0., 100., 20., "Hello")], 16.);
    assert!(!reliable.duplicate_of(&weak));
}

#[test]
fn a_source_fields_label_and_value_columns_share_rows_without_a_false_line_pitch() {
    let hud = field(
        1,
        &[
            (20., 40., 100., 12., "Fuel heating status:"),
            (220., 41., 55., 11., "nominal"),
            (20., 60., 120., 12., "Exoskeleton management:"),
            (230., 61., 45., 11., "active"),
        ],
        9.,
    );
    assert_eq!(
        hud.rows(),
        [
            Rect::new(20., 40., 255., 12.),
            Rect::new(20., 60., 255., 12.)
        ]
    );
    let another = field(2, &[(400., 40., 100., 12., "Damage")], 9.);
    assert!(!hud.needs_join(&another));
    assert!(!another.duplicate_of(&hud));
}

fn frame(width: u32) -> image::DynamicImage {
    let mut image = image::RgbaImage::from_pixel(width, 180, image::Rgba([0, 0, 0, 255]));
    for column in 0..12 {
        for y in 60..90 {
            for x in 40 + column * 20..50 + column * 20 {
                image.put_pixel(x, y, image::Rgba([255; 4]));
            }
        }
    }
    image::DynamicImage::ImageRgba8(image)
}
fn reading(origin: (u32, u32), rows: &[(Rect, &str, f32)]) -> lipa_core::ocr::OcrResult {
    lipa_core::ocr::OcrResult {
        text: rows
            .iter()
            .map(|(_, text, _)| *text)
            .collect::<Vec<_>>()
            .join(" "),
        lines: rows
            .iter()
            .map(|(r, text, confidence)| lipa_core::ocr::OcrLine {
                rect: lipa_core::layout::CropRect::in_space(
                    r.x - origin.0 as f32,
                    r.y - origin.1 as f32,
                    r.w,
                    r.h,
                ),
                text: (*text).into(),
                confidence: *confidence,
            })
            .collect(),
        confidence: Some(98.0),
        engine: "recorded-test-input",
        ..Default::default()
    }
}

#[test]
fn crop_margins_and_weak_spans_do_not_pollute_the_source_translation() {
    use lipa_core::{
        layout::engine::{InplaceEngine, LinesVerdict},
        settings::Settings,
    };
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let jobs = e.begin(frame(400), &s, std::time::Instant::now(), false);
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    let rect = Rect::new(50., 62., 180., 25.);
    let result = reading(
        job.origin,
        &[
            (Rect::new(50., 5., 80., 20.), "Speaker", 98.),
            (rect, "Hello world", 98.),
            (Rect::new(100., 65., 20., 20.), "noise", 0.),
        ],
    );
    let observation = e.observe_ocr_result(job.id, &result, job.origin, &s);
    assert_eq!(observation.text, "Hello world");
    assert_eq!(observation.verdict, LinesVerdict::Normal);
    e.complete(job.id, Some((observation.text, "Привет, мир".into())), &s);
    let output = e.finish(&s).unwrap();
    assert_eq!(output.blocks.len(), 1);
    assert_eq!(output.blocks[0].text_rect, Rect::new(49., 61., 182., 27.));
}

#[test]
fn validated_source_geometry_and_revision_stay_stable_on_the_next_frame() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let job = &jobs[0];
    let o = e.observe_ocr_result(
        job.id,
        &reading(
            job.origin,
            &[(Rect::new(50., 62., 180., 25.), "Hello world", 98.)],
        ),
        job.origin,
        &s,
    );
    e.complete(job.id, Some((o.text, "Привет, мир".into())), &s);
    let expected = e.finish(&s).unwrap();
    assert!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(100),
            false
        )
        .is_empty()
    );
    let next = e.finish(&s);
    if let Some(next) = next {
        assert_eq!(next.blocks, expected.blocks);
    }
    assert_eq!(
        e.tracker().get(job.id).unwrap().text_rect(),
        expected.blocks[0].text_rect
    );
}

#[test]
fn switching_to_text_only_ocr_restores_detector_geometry_and_background() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let job = &jobs[0];
    let o = e.observe_ocr_result(
        job.id,
        &reading(
            job.origin,
            &[(Rect::new(50., 62., 180., 25.), "Hello world", 98.)],
        ),
        job.origin,
        &s,
    );
    e.complete(job.id, Some((o.text, "Привет, мир".into())), &s);
    e.finish(&s).unwrap();
    let o = e.observe_ocr_result(
        job.id,
        &lipa_core::ocr::OcrResult::text_only("Hello again".into(), "test"),
        job.origin,
        &s,
    );
    e.complete(job.id, Some((o.text, "Снова привет".into())), &s);
    let output = e.finish(&s).unwrap();
    assert_eq!(output.blocks[0].text_rect, job.rect);
    let t = e.tracker().get(job.id).unwrap();
    assert_eq!(t.background_for, job.rect);
    assert!(t.ocr_lines.is_none());
    assert!(t.ocr_bounds.is_none());
}

#[test]
fn frame_resize_invalidates_crop_geometry_even_when_the_ink_is_unchanged() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let job = &jobs[0];
    let o = e.observe_ocr_result(
        job.id,
        &reading(
            job.origin,
            &[(Rect::new(50., 62., 180., 25.), "Hello world", 98.)],
        ),
        job.origin,
        &s,
    );
    e.complete(job.id, Some((o.text, "Привет, мир".into())), &s);
    e.finish(&s).unwrap();
    let resized = e.begin(
        frame(420),
        &s,
        now + std::time::Duration::from_millis(100),
        false,
    );
    assert_eq!(
        resized.len(),
        1,
        "frame dimensions are part of the OCR cache key"
    );
    assert!(e.tracker().get(job.id).unwrap().ocr_bounds.is_none());
}

#[test]
fn duplicate_ownership_is_invariant_under_coordinate_translation_and_uniform_scaling() {
    for scale in [0.25, 1., 1.5, 3., 10.] {
        for (dx, dy) in [(-300., -90.), (0., 0.), (2000., 800.)] {
            let owner = field(
                1,
                &[(
                    dx + 20. * scale,
                    dy + 40. * scale,
                    300. * scale,
                    25. * scale,
                    "Hello world paragraph",
                )],
                20. * scale,
            );
            let child = field(
                2,
                &[(
                    dx + 22. * scale,
                    dy + 41. * scale,
                    90. * scale,
                    24. * scale,
                    "Hello",
                )],
                20. * scale,
            );
            assert!(
                child.duplicate_of(&owner),
                "scale={scale}, origin={dx},{dy}"
            );
            assert!(!owner.duplicate_of(&child));
        }
    }
}

fn replay_game(process: &str) -> lipa_core::layout::engine::InplaceFrame {
    use lipa_core::{
        layout::engine::{InplaceEngine, LinesVerdict},
        settings::Settings,
    };
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures");
    let image = image::open(fixtures.join(format!("{process}.png"))).unwrap();
    let history: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixtures.join(format!("{process}-rapidocr.json"))).unwrap(),
    )
    .unwrap();
    let supplement_path = fixtures.join(format!("{process}-local-rapidocr.json"));
    let supplement: serde_json::Value = if supplement_path.exists() {
        serde_json::from_slice(&std::fs::read(supplement_path).unwrap()).unwrap()
    } else { serde_json::json!({"ocr_history": []}) };
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let mut last = None;
    let mut joins = 0;
    for scan in 0..3 {
        let jobs = e.begin(
            image.clone(),
            &s,
            now + std::time::Duration::from_millis(100 * scan),
            false,
        );
        for job in jobs {
            let record = history["ocr_history"]
                .as_array()
                .unwrap()
                .iter()
                .chain(supplement["ocr_history"].as_array().unwrap())
                .find(|r| {
                    let rect: Rect = serde_json::from_value(r["detector_rect"].clone()).unwrap();
                    r["scan"] == scan && rect.iou(&job.rect) >= 0.99
                })
                .unwrap_or_else(|| panic!("changed detector geometry needs a new observed OCR replay: {process}, scan={scan}, job={:?}", job.rect));
            let result = lipa_core::ocr::OcrResult {
                text: record["raw_original"].as_str().unwrap().into(),
                lines: record["spans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|span| {
                        let rect: Rect = serde_json::from_value(span["rect"].clone()).unwrap();
                        lipa_core::ocr::OcrLine {
                            rect: lipa_core::layout::CropRect::in_space(
                                rect.x - job.origin.0 as f32,
                                rect.y - job.origin.1 as f32,
                                rect.w,
                                rect.h,
                            ),
                            text: span["text"].as_str().unwrap().into(),
                            confidence: span["confidence"].as_f64().unwrap() as f32,
                        }
                    })
                    .collect(),
                confidence: record["confidence"].as_f64().map(|c| c as f32),
                engine: "rapidocr-replay",
                ..Default::default()
            };
            let accepted = result
                .confidence
                .is_none_or(|c| c >= s.effective_minimum_confidence() as f32)
                && lipa_core::text::is_meaningful(&result.text);
            if accepted {
                let o = e.observe_ocr_result(job.id, &result, job.origin, &s);
                let settled =
                    o.verdict == LinesVerdict::Normal && lipa_core::text::is_meaningful(&o.text);
                e.complete(
                    job.id,
                    settled.then(|| (o.text, "Проверочный перевод".into())),
                    &s,
                );
            } else {
                e.reject_ocr(job.id, &s);
            }
        }
        if let Some(f) = e.finish(&s) {
            last = Some(f);
        }
        if !e.needs_rescan() {
            let last = last.unwrap();
            let repeated = e.begin(
                image.clone(),
                &s,
                now + std::time::Duration::from_millis(100 * (scan + 1)),
                false,
            );
            assert!(
                repeated.is_empty(),
                "settled and confirmed-empty fields must not rerun OCR on an identical frame"
            );
            if process.starts_with("Cross") {
                assert_eq!(joins, 1, "name and HUD unions settle on the next scan");
            }
            return last;
        }
        joins += 1;
    }
    panic!("source regrouping kept requesting OCR on an identical frame");
}

#[test]
fn crossxmars_replay_reads_union_pixels_again_before_replacing_name_hud_and_dialogue() {
    let output = replay_game("CrossxMars.exe-129055");
    assert!(output.blocks.iter().any(|b| b.original == "Mika"));
    let hud = output
        .blocks
        .iter()
        .find(|b| b.original.starts_with("Fuel heating status:"))
        .unwrap();
    assert!(
        hud.original.contains(":1123"),
        "a high-IoU union must invalidate the smaller cached crop"
    );
    assert_eq!(
        hud.style.source_lines, 7,
        "label/value columns share baselines"
    );
    assert_eq!(hud.style.alignment, lipa_core::layout::TextAlignment::Left);
    let dialogue = output
        .blocks
        .iter()
        .find(|b| b.original.starts_with("Morning, Ella."))
        .unwrap();
    assert!(
        dialogue
            .original
            .ends_with("but I have the right to be angry!")
    );
    assert_eq!(dialogue.style.source_lines, 3);
    assert!(
        !output
            .blocks
            .iter()
            .any(|b| b.original == "Mik" || b.original == "ca" || b.original == "Ia")
    );
}

#[test]
fn engine_exe_replay_reads_the_three_line_dialogue_as_one_field_and_suppresses_load_fragment() {
    // Re-recorded after the detector stopped cutting the third line off the paragraph: the three lines
    // have one pitch, one colour and one alignment, so one field (one translation unit) is correct.
    let output = replay_game("engine.exe-100656");
    let dialogue = output
        .blocks
        .iter()
        .find(|b| b.original.starts_with("Considering the amount"))
        .unwrap();
    assert!(dialogue.original.contains("king-sized bed"));
    assert_eq!(dialogue.style.source_lines, 3);
    // No other drawn field overlaps the dialogue's source pixels.
    assert!(!output.blocks.iter().any(|b| !std::ptr::eq(b, dialogue) && b.text_rect.intersection(&dialogue.text_rect) > 0.0));
    assert!(!output.blocks.iter().any(|b| b.original == "OAD"));
    assert!(
        output
            .undrawable
            .iter()
            .any(|b| b.original == "OAD" && b.reason == "повторный OCR того же текста")
    );
}

#[test]
fn confirmed_empty_ocr_is_cached_briefly_but_forced_scans_and_timeout_retry_it() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let id = jobs[0].id;
    e.reject_ocr(id, &s);
    assert!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(100),
            false
        )
        .is_empty()
    );
    assert_eq!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(200),
            true
        )
        .len(),
        1
    );
    assert_eq!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_secs(5),
            false
        )
        .len(),
        1
    );
}

#[test]
fn failed_or_cancelled_initial_ocr_does_not_create_a_negative_cache_entry() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let id = jobs[0].id;
    assert_eq!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(100),
            false
        )
        .len(),
        1
    );
    e.complete(id, None, &s); // failure hysteresis, not a confirmed empty reading
    assert_eq!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(200),
            false
        )
        .len(),
        1
    );
}

#[test]
fn recognition_policy_and_frame_resize_invalidate_negative_ocr_cache() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let mut s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    let id = jobs[0].id;
    e.reject_ocr(id, &s);
    s.recognition.language = "rus".into();
    assert_eq!(
        e.begin(
            frame(400),
            &s,
            now + std::time::Duration::from_millis(100),
            false
        )
        .len(),
        1
    );
    e.reject_ocr(id, &s);
    assert_eq!(
        e.begin(
            frame(420),
            &s,
            now + std::time::Duration::from_millis(200),
            false
        )
        .len(),
        1
    );
}

#[test]
fn changed_source_pixels_invalidate_negative_ocr_cache_immediately() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let now = std::time::Instant::now();
    let jobs = e.begin(frame(400), &s, now, false);
    e.reject_ocr(jobs[0].id, &s);
    let mut changed = frame(400).into_rgba8();
    for pixel in changed.pixels_mut() {
        if pixel.0[0] > 0 {
            *pixel = image::Rgba([120, 120, 120, 255]);
        }
    }
    assert_eq!(
        e.begin(
            image::DynamicImage::ImageRgba8(changed),
            &s,
            now + std::time::Duration::from_millis(100),
            false
        )
        .len(),
        1
    );
}

#[test]
fn a_late_result_for_an_older_frame_does_not_mark_the_newer_pixels_as_read() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let t = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let old = e.begin(frame(400), &s, t, false);
    let id = old[0].id;
    // A newer, different frame arrives while the first reading is still running.
    let mut newer = frame(400).into_rgba8();
    for p in newer.pixels_mut() { if p.0[0] > 0 { *p = image::Rgba([180, 180, 180, 255]); } }
    let newer = image::DynamicImage::ImageRgba8(newer);
    let fresh = e.begin(newer.clone(), &s, t + ms(100), false);
    assert_eq!(fresh.len(), 1, "the changed pixels are due for reading");
    // The old reading completes late. It must not remember the newer picture as handled.
    e.complete(id, None, &s);
    assert_eq!(
        e.begin(newer.clone(), &s, t + ms(200), false).len(),
        1,
        "the newer frame was never read: it is still due"
    );
    // Once the newer frame itself is handled, an identical frame needs no reading.
    e.reject_ocr(id, &s);
    assert!(e.begin(newer, &s, t + ms(300), false).is_empty());
}

#[test]
fn a_negative_cache_entry_lives_until_its_deadline_and_a_frame_without_text_costs_nothing() {
    use lipa_core::{layout::engine::InplaceEngine, settings::Settings};
    let s = Settings::default();
    let mut e = InplaceEngine::new();
    let t = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let id = e.begin(frame(400), &s, t, false)[0].id;
    e.reject_ocr(id, &s);
    assert!(e.begin(frame(400), &s, t + ms(4_900), false).is_empty(), "inside the 5 s lifetime: no reading");
    assert_eq!(e.begin(frame(400), &s, t + ms(5_000), false).len(), 1, "at the deadline it is read again");
    // A frame with nothing to read: no candidate, no job, however often it repeats.
    let blank = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(400, 180, image::Rgba([10, 10, 10, 255])));
    let mut fresh = InplaceEngine::new();
    for i in 0..5 { assert!(fresh.begin(blank.clone(), &s, t + ms(100 * i), false).is_empty()); }
}

#[test]
fn a_dialogue_line_cut_in_two_by_the_detector_is_joined_and_not_left_as_overlapping_parts() {
    // Recorded on CrossxMars.exe (tests/inplace/fixtures/CrossxMars.exe-live-d1): the detector took the right part of
    // the first line as one block and the left part of both lines as another, so that the words "now that was a
    // creative way to" are read twice, from the same pixels, by two fields that overlap.
    let right = field(6, &[(184., 31., 1249., 45., "now that was a creative way to tell me I'm being rude! So did you figure")], 28.);
    let left = field(7, &[
        (43., 32., 156., 39., "Whoa -r"),
        (180., 35., 549., 39., "now that was a creative way to"),
        (41., 84., 645., 47., "out how I should spend my time-off?"),
    ], 28.);
    assert!(!left.duplicate_of(&right) && !right.duplicate_of(&left), "neither owns the other: each has text the other lacks");
    assert!(left.needs_join(&right) && right.needs_join(&left), "the shared row asks for one combined reading");
    // Independent texts that merely touch are not joined.
    let other = field(8, &[(41., 84., 645., 47., "Open the door and take the key"), (41., 130., 400., 40., "Then leave quickly")], 28.);
    assert!(!other.needs_join(&right));
}
