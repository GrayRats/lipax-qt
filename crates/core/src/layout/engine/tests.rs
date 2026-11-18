use super::*;
use crate::layout::font_database::tests::SAMPLE;
use crate::layout::font_database::InstalledFontDatabase;
use crate::layout::testing::*;
use crate::settings::{InplaceBackgroundMode, PropertyMode};
use std::time::Duration;

fn scene(shift: u32, name_color: [u8; 3]) -> DynamicImage {
    let mut img = canvas(1200, 400, [25, 30, 40]);
    draw_line(&mut img, 100 + shift, 100, 8, 14, 4, 22, 3, name_color, false);
    draw_line(&mut img, 100 + shift, 140, 40, 14, 4, 22, 3, [240, 240, 240], true);
    draw_line(&mut img, 100 + shift, 172, 30, 14, 4, 22, 3, [240, 240, 240], true);
    dynamic(img)
}

fn settings() -> Settings {
    { let mut value = Settings::default(); value.translation.target_language = "ru".into(); value.display_mode = crate::settings::TranslationDisplayMode::Inplace; value }
}

/// A character name one blank line above its dialogue, all one colour: the grouping alone makes one block.
/// `lines` dialogue lines, the name optional, everything moved right by `shift` px.
fn glued(lines: usize, name: bool, shift: u32) -> DynamicImage {
    let mut img = canvas(1200, 500, [25, 30, 40]);
    if name { draw_prose_line(&mut img, 100 + shift, 100, 8, 14, 4, 32, 3, [240, 240, 240]); }
    for i in 0..lines as u32 { draw_prose_line(&mut img, 100 + shift, 162 + 46 * i, 34 - 6 * i as usize, 14, 4, 32, 3, [240, 240, 240]); }
    dynamic(img)
}

/// Reads every job as the text of its field and translates it; returns the published frame, the jobs' ids and the new pairs.
fn read_all(e: &mut InplaceEngine, frame: DynamicImage, s: &Settings, now: Instant) -> (Vec<u64>, Option<InplaceFrame>) {
    let jobs = e.begin(frame, s, now, false);
    let ids: Vec<u64> = jobs.iter().map(|j| j.id).collect();
    for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), s); }
    (ids, e.finish(s))
}

#[test]
fn a_split_gives_two_fields_with_their_own_plates_and_stable_ids() {
    let s = settings();
    let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
    let (jobs, frame) = read_all(&mut e, glued(2, true, 0), &s, t0);
    assert_eq!(jobs.len(), 2, "the name and the dialogue are read separately");
    let blocks = frame.expect("both fields").blocks;
    let by_type = |t: TextBlockType| blocks.iter().find(|b| b.block_type == t).unwrap_or_else(|| panic!("{t:?} in {:?}", blocks.iter().map(|b| b.block_type).collect::<Vec<_>>())).clone();
    let (name, dialogue) = (by_type(TextBlockType::CharacterName), by_type(TextBlockType::Dialogue));
    // Each field has the plate of its own boundary, not the merged parent's: the name's plate stops above the dialogue.
    assert!(name.background.rect.bottom() <= dialogue.text_rect.y, "{:?} vs {:?}", name.background.rect, dialogue.text_rect);
    assert!(dialogue.background.rect.y >= name.text_rect.bottom(), "{:?} vs {:?}", dialogue.background.rect, name.text_rect);
    // The same picture again: the same fields, nothing re-read, nothing sent again.
    let (again, frame) = read_all(&mut e, glued(2, true, 0), &s, t0 + Duration::from_millis(200));
    assert!(again.is_empty() && frame.is_none(), "stable ids: no flicker, no repeated work");
}

#[test]
fn changing_one_field_keeps_the_font_lock_of_its_neighbour() {
    let s = settings();
    let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
    let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
    let before = first.expect("fields").blocks;
    let name_before = before.iter().find(|b| b.block_type == TextBlockType::CharacterName).unwrap().clone();
    // The dialogue gets a third line; the name is untouched.
    let (jobs, frame) = read_all(&mut e, glued(3, true, 0), &s, t0 + Duration::from_millis(300));
    assert_eq!(jobs.len(), 1, "only the changed dialogue is read again");
    assert!(jobs[0] != name_before.id);
    let name_after = frame.expect("the dialogue changed").blocks.into_iter().find(|b| b.id == name_before.id).expect("the name keeps its id");
    assert_eq!(name_after.font, name_before.font, "the font chosen for the name stays");
    assert_eq!(name_after.translation, name_before.translation);
}

#[test]
fn a_field_that_blinks_out_keeps_its_id_through_the_grace_period() {
    let s = settings();
    let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
    let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
    let name_id = first.expect("fields").blocks.iter().find(|b| b.block_type == TextBlockType::CharacterName).unwrap().id;
    // One scan without the name: it is still shown.
    let (_, frame) = read_all(&mut e, glued(2, false, 0), &s, t0 + Duration::from_millis(200));
    assert!(frame.is_none_or(|f| f.blocks.iter().any(|b| b.id == name_id)), "a short miss does not drop the field");
    // It comes back: the same id, read from memory (no new read).
    let (again, frame) = read_all(&mut e, glued(2, true, 0), &s, t0 + Duration::from_millis(400));
    assert!(again.is_empty(), "{again:?}");
    assert!(frame.is_none_or(|f| f.blocks.iter().any(|b| b.id == name_id)));
}

#[test]
fn a_move_alone_changes_the_geometry_not_the_translations() {
    let s = settings();
    let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
    let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
    assert_eq!(first.expect("fields").new_translations.len(), 2, "the first reading is new text");
    // The scene moves 6 px right: the fields follow, and since the text is the same no translation is new,
    // so neither the cache nor the history has anything to write.
    let (jobs, frame) = read_all(&mut e, glued(2, true, 6), &s, t0 + Duration::from_millis(200));
    assert!(jobs.is_empty(), "the text did not change");
    if let Some(frame) = frame {
        assert!(frame.new_translations.is_empty(), "{:?}", frame.new_translations);
        assert!(frame.blocks.iter().all(|b| b.text_rect.x >= 100.0 + 5.0), "the fields moved with the scene");
    }
}

/// One block of three lines, as dialogue text.
fn three_lines() -> DynamicImage {
    let mut img = canvas(1200, 400, [25, 30, 40]);
    for y in [100, 134, 168] { draw_line(&mut img, 100, y, 30, 14, 4, 22, 3, [240, 240, 240], false); }
    dynamic(img)
}

fn halves(r: &Rect) -> [Rect; 2] {
    [Rect::new(r.x, r.y, r.w, r.h / 2.0 - 2.0), Rect::new(r.x, r.y + r.h / 2.0, r.w, r.h / 2.0)]
}

#[test]
fn ocr_lines_decide_the_line_structure_when_they_lie_on_the_block() {
    let s = settings();
    let (frame, t0) = (three_lines(), Instant::now());
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(frame.clone(), &s, t0, false);
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    // The detector sees three lines; the engine that read the text found two.
    e.observe_ocr_lines(job.id, &halves(&job.rect), &s);
    e.complete(job.id, Some(("a b".into(), "в г".into())), &s);
    let block = e.finish(&s).expect("the field").blocks.remove(0);
    assert_eq!(block.style.source_lines, 2, "the OCR line count wins");
    assert!(block.lines_note.contains("2 по OCR") && block.lines_note.contains("детектор насчитал 3"), "{}", block.lines_note);
    // The decision is stable: the same picture again neither re-reads the field nor changes it.
    let again = e.begin(frame, &s, t0 + Duration::from_millis(200), false);
    assert!(again.is_empty());
    assert!(e.finish(&s).is_none(), "an OCR-decided structure must not make the field flap");
}

#[test]
fn ocr_lines_that_agree_with_the_detector_change_nothing_but_the_note() {
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(three_lines(), &s, Instant::now(), false);
    let job = &jobs[0];
    let third = job.rect.h / 3.0;
    let lines: Vec<Rect> = (0..3).map(|i| Rect::new(job.rect.x, job.rect.y + third * i as f32, job.rect.w, third - 2.0)).collect();
    e.observe_ocr_lines(job.id, &lines, &s);
    e.complete(job.id, Some(("a b c".into(), "в г д".into())), &s);
    let block = e.finish(&s).expect("the field").blocks.remove(0);
    assert_eq!(block.style.source_lines, 3);
    assert!(block.lines_note.contains("согласны"), "{}", block.lines_note);
}

/// The OCR lines of `job.rect` with every side moved by `by` pixels (negative: inwards).
fn lines_moved_by(rect: &Rect, by: f32) -> [Rect; 1] {
    [Rect::new(rect.x - by, rect.y - by, rect.w + 2.0 * by, rect.h + 2.0 * by)]
}

fn block_with(frame: DynamicImage, lines: impl Fn(&Rect) -> Vec<Rect>) -> (InplaceBlock, Rect) {
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(frame, &s, Instant::now(), false);
    let job = &jobs[0];
    e.observe_ocr_lines(job.id, &lines(&job.rect), &s);
    e.complete(job.id, Some(("a".into(), "б".into())), &s);
    (e.finish(&s).expect("the field").blocks.remove(0), job.rect)
}

#[test]
fn the_boundary_follows_the_ocr_lines_when_they_are_close_to_the_block() {
    let (plain, detected) = block_with(three_lines(), |_| Vec::new());
    assert_eq!(plain.text_rect, detected, "no OCR geometry: the detector's boundary");
    // The engine's lines are 4 px tighter on every side than the detected block.
    let (refined, _) = block_with(three_lines(), |r| lines_moved_by(r, -4.0).to_vec());
    assert!(refined.text_rect.w < detected.w - 5.0 && refined.text_rect.h < detected.h - 5.0, "{:?} vs {:?}", refined.text_rect, detected);
    assert!((refined.text_rect.x - (detected.x + 3.0)).abs() < 0.01, "tight box plus one pixel of margin: {:?}", refined.text_rect);
    assert!(refined.lines_note.contains("граница блока по OCR"), "{}", refined.lines_note);
    // The plate follows: it was restored again for the new boundary, not left at the old size.
    assert!(refined.background.rect.w < plain.background.rect.w && refined.background.rect.h < plain.background.rect.h,
        "{:?} vs {:?}", refined.background.rect, plain.background.rect);
}

#[test]
fn a_boundary_that_cuts_off_lines_is_not_taken() {
    // The engine read only the top two of three lines: its boundary would leave the third one
    // uncovered under the translation, so the detector's boundary stays.
    let (block, detected) = block_with(three_lines(), |r| vec![Rect::new(r.x, r.y, r.w, r.h * 0.66)]);
    assert_eq!(block.text_rect, detected, "{}", block.lines_note);
    assert!(block.lines_note.contains("граница блока по детектору"), "{}", block.lines_note);
}

#[test]
fn a_refined_boundary_is_stable_between_scans() {
    let s = settings();
    let (frame, t0) = (three_lines(), Instant::now());
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let job = e.begin(frame.clone(), &s, t0, false).remove(0);
    e.observe_ocr_lines(job.id, &lines_moved_by(&job.rect, -4.0), &s);
    e.complete(job.id, Some(("a".into(), "б".into())), &s);
    let first = e.finish(&s).expect("the field").blocks.remove(0);
    // The same picture is detected again (with the old, wider boundary): the refinement stays.
    assert!(e.begin(frame.clone(), &s, t0 + Duration::from_millis(200), false).is_empty());
    assert!(e.finish(&s).is_none(), "nothing changed, nothing is sent again");
    // A forced re-read gives back the same boundary and plate.
    let again = e.begin(frame, &s, t0 + Duration::from_millis(400), true).remove(0);
    e.observe_ocr_lines(again.id, &lines_moved_by(&again.rect, -4.0), &s);
    e.complete(again.id, Some(("a".into(), "б".into())), &s);
    let second = e.finish(&s).map_or(first.clone(), |f| f.blocks[0].clone());
    assert_eq!((second.text_rect, second.background.rect), (first.text_rect, first.background.rect));
}

#[test]
fn a_field_that_grows_in_place_gets_a_plate_of_its_new_size() {
    let s = settings();
    let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
    let (_, first) = read_all(&mut e, glued(2, false, 0), &s, t0);
    let before = first.expect("the field").blocks.remove(0);
    // A third line appears under the same top-left corner: the field does not move, its surroundings look the
    // same, but the plate that covered two lines must now cover three.
    let (jobs, frame) = read_all(&mut e, glued(3, false, 0), &s, t0 + Duration::from_millis(300));
    assert_eq!(jobs.len(), 1);
    let after = frame.expect("the field changed").blocks.remove(0);
    assert!(after.text_rect.h > before.text_rect.h + 20.0, "{:?} vs {:?}", after.text_rect, before.text_rect);
    assert!(after.background.rect.bottom() >= after.text_rect.bottom(), "the plate covers the whole text: {:?} / {:?}", after.background.rect, after.text_rect);
    assert!(after.background.rect.h > before.background.rect.h + 20.0, "and it was built again, not kept: {:?} vs {:?}", after.background.rect, before.background.rect);
}

/// What the OCR engine would report for the three-line block if it saw a title and a body set well apart:
/// two boxes 22 px tall at the top and the bottom of the block (a pitch of 3 line heights).
fn far_apart(rect: &Rect) -> Vec<Rect> {
    vec![Rect::new(rect.x, rect.y, rect.w, 22.0), Rect::new(rect.x, rect.bottom() - 22.0, rect.w, 22.0)]
}

#[test]
fn a_gap_only_the_ocr_engine_saw_splits_the_field_on_the_next_scan() {
    let s = settings();
    let (frame, t0) = (three_lines(), Instant::now());
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    // The detector sees one block of three lines (evenly spaced, nothing to cut)...
    let job = e.begin(frame.clone(), &s, t0, false).remove(0);
    // ...the engine's own lines show a title and a body far apart: the field is not to be published as it is.
    assert_eq!(e.observe_ocr_lines(job.id, &far_apart(&job.rect), &s), LinesVerdict::Split);
    // Not completed: the next scan hands the field out again, now divided at the remembered cut.
    let jobs = e.begin(frame.clone(), &s, t0 + Duration::from_millis(100), false);
    assert_eq!(jobs.len(), 2, "two fields where there was one");
    assert!(jobs[0].rect.bottom() <= jobs[1].rect.y, "the parts do not overlap: {:?} / {:?}", jobs[0].rect, jobs[1].rect);
    for j in &jobs { e.complete(j.id, Some((format!("t{}", j.id), format!("п{}", j.id))), &s); }
    let blocks = e.finish(&s).expect("both parts").blocks;
    assert_eq!(blocks.len(), 2);
    // Each part has its plate of its own boundary and the picture goes on being divided the same way.
    assert!(blocks[0].background.rect.bottom() <= blocks[1].text_rect.y, "{:?} / {:?}", blocks[0].background.rect, blocks[1].text_rect);
    assert!(e.begin(frame, &s, t0 + Duration::from_millis(300), false).is_empty(), "stable: nothing is read again");
    assert!(e.finish(&s).is_none(), "and nothing flickers");
}

/// An OCR reading of the given rows, each with its text, in frame coordinates.
fn reading_of(origin: (u32, u32), rows: &[(Rect, &str)]) -> crate::ocr::OcrResult {
    crate::ocr::OcrResult {
        text: rows.iter().map(|(_, t)| *t).collect::<Vec<_>>().join(" "),
        lines: rows.iter().map(|(r, t)| crate::ocr::OcrLine {
            rect: crate::layout::CropRect::in_space(r.x - origin.0 as f32, r.y - origin.1 as f32, r.w, r.h),
            text: (*t).into(), confidence: 98.0 }).collect(),
        confidence: Some(98.0),
        engine: "test",
        ..Default::default()
    }
}

#[test]
fn the_parts_of_a_cut_field_are_not_repeats_of_the_parent_that_gave_way() {
    let s = settings();
    let mut img = canvas(1200, 400, [25, 30, 40]);
    for y in [100, 134, 168, 202] { draw_line(&mut img, 100, y, 30, 14, 4, 22, 3, [240, 240, 240], false); }
    let (frame, t0) = (dynamic(img), Instant::now());
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    // The whole block is read and drawn first.
    let parent = e.begin(frame.clone(), &s, t0, false).remove(0);
    let lines: Vec<Rect> = e.snapshot.as_ref().unwrap().blocks[&parent.id].lines.iter().map(|l| l.rect).collect();
    assert_eq!(lines.len(), 4);
    let rows: Vec<(Rect, &str)> = lines.iter().copied().zip(["alpha one", "beta two", "gamma three", "delta four"]).collect();
    let o = e.observe_ocr_result(parent.id, &reading_of(parent.origin, &rows), parent.origin, &s);
    assert_eq!(o.verdict, LinesVerdict::Normal);
    e.complete(parent.id, Some((o.text, "целое".into())), &s);
    assert_eq!(e.finish(&s).expect("the whole field").blocks.len(), 1);
    // The block is then divided row by row (independent labels): the next scan finds the parts, not the parent.
    e.cuts.push(CutMemory { parent: parent.rect, partitions: lines.clone(), rows: lines.iter().map(|&r| vec![r]).collect(), seen: t0 });
    let parts = e.begin(frame, &s, t0 + Duration::from_millis(100), false);
    assert_eq!(parts.len(), 4, "{:?}", parts.iter().map(|j| j.rect).collect::<Vec<_>>());
    assert!(parts.iter().all(|j| j.id != parent.id), "the parts are new fields");
    for job in &parts {
        let row: Vec<(Rect, &str)> = rows.iter().copied().filter(|(r, _)| r.intersection(&job.rect) > 0.5 * r.area()).collect();
        assert_eq!(row.len(), 1);
        let o = e.observe_ocr_result(job.id, &reading_of(job.origin, &row), job.origin, &s);
        e.complete(job.id, Some((o.text, format!("часть {}", job.id))), &s);
    }
    // The parent that gave way to them is not drawn, so it cannot make them its repeats: every part is drawn.
    let out = e.finish(&s).expect("the parts");
    let mut drawn: Vec<u64> = out.blocks.iter().map(|b| b.id).collect();
    drawn.sort_unstable();
    let mut want: Vec<u64> = parts.iter().map(|j| j.id).collect();
    want.sort_unstable();
    assert_eq!(drawn, want, "undrawable: {:?}", out.undrawable.iter().map(|u| (u.id, u.reason)).collect::<Vec<_>>());
}

#[test]
fn a_split_is_planned_once_and_forgotten_with_the_scene() {
    let s = settings();
    let (frame, t0) = (three_lines(), Instant::now());
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let job = e.begin(frame.clone(), &s, t0, false).remove(0);
    assert_eq!(e.observe_ocr_lines(job.id, &far_apart(&job.rect), &s), LinesVerdict::Split);
    // The same merged block shown to the engine again while the memory stands: no second plan, no loop.
    assert_eq!(e.observe_ocr_lines(job.id, &far_apart(&job.rect), &s), LinesVerdict::Normal);
    // Long after, the scene is new: the memory is gone and the block is whole again.
    let jobs = e.begin(frame, &s, t0 + CUT_TTL + Duration::from_secs(1), false);
    assert_eq!(jobs.len(), 1, "forgotten");
    assert_eq!(jobs[0].rect, job.rect);
}

#[test]
fn evenly_spaced_ocr_lines_never_plan_a_split() {
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let job = e.begin(three_lines(), &s, Instant::now(), false).remove(0);
    let third = job.rect.h / 3.0;
    let lines: Vec<Rect> = (0..3).map(|i| Rect::new(job.rect.x, job.rect.y + third * i as f32, job.rect.w, third - 2.0)).collect();
    assert_eq!(e.observe_ocr_lines(job.id, &lines, &s), LinesVerdict::Normal);
    assert_eq!(e.observe_ocr_lines(job.id, &[], &s), LinesVerdict::Normal, "no geometry, no verdict");
}

#[test]
fn ocr_lines_somewhere_else_do_not_override_the_detector() {
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(three_lines(), &s, Instant::now(), false);
    let job = &jobs[0];
    let elsewhere = [Rect::new(job.rect.x, job.rect.y + 250.0, job.rect.w, 20.0)];
    e.observe_ocr_lines(job.id, &elsewhere, &s);
    e.complete(job.id, Some(("a".into(), "б".into())), &s);
    let block = e.finish(&s).expect("the field").blocks.remove(0);
    assert_eq!(block.style.source_lines, 3, "the detector's lines stay");
    assert!(block.lines_note.contains("не совпала"), "{}", block.lines_note);
    // An engine without geometry changes nothing at all.
    e.observe_ocr_lines(job.id, &[], &s);
}

fn run(e: &mut InplaceEngine, frame: &DynamicImage, s: &Settings, now: Instant) -> (usize, Option<InplaceFrame>) {
    let jobs = e.begin(frame.clone(), s, now, false);
    let n = jobs.len();
    for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), s); }
    (n, e.finish(s))
}

/// Where the time goes on a 2560×1080 frame with six lines of text:
/// cargo test --release -p lipa-core layout::engine::tests::stages -- --ignored --nocapture
#[test]
#[ignore]
fn stages() {
    let mut img = canvas(2560, 1080, [25, 30, 40]);
    for row in 0..6 { draw_line(&mut img, 200, 120 + row * 150, 60, 14, 4, 22, 3, [240, 240, 240], row % 2 == 0); }
    let dynamic_frame = dynamic(img.clone());
    let time = |name: &str, runs: u32, mut f: Box<dyn FnMut() + '_>| {
        let t = Instant::now();
        for _ in 0..runs { f(); }
        println!("{name:<28} {:>9.3} ms", t.elapsed().as_secs_f64() * 1000.0 / runs as f64);
    };
    time("to_rgba8 copy", 20, Box::new(|| { std::hint::black_box(dynamic_frame.to_rgba8()); }));
    time("ink_mask", 20, Box::new(|| { std::hint::black_box(BlockDetector::ink_mask(&img)); }));
    let mask = BlockDetector::ink_mask(&img);
    time("detect_text_blocks", 20, Box::new(|| { std::hint::black_box(BlockDetector::default().detect_text_blocks(&img, &mask)); }));
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let t0 = Instant::now();
    for j in e.begin(dynamic_frame.clone(), &s, t0, false) { e.complete(j.id, Some(("a".into(), "б".into())), &s); }
    let mut i = 0u64;
    time("begin (stable, whole)", 20, Box::new(|| { i += 1; e.begin(dynamic_frame.clone(), &s, t0 + Duration::from_millis(100 * i), false); }));
    for (name, took) in e.timings() { println!("  {name:<26} {:>9.3} ms", took.as_secs_f64() * 1000.0); }
}

/// cargo test --release -p lipa-core layout::engine::tests::speed -- --ignored --nocapture
#[test]
#[ignore]
fn speed() {
    let mut img = canvas(2560, 1080, [25, 30, 40]);
    for row in 0..6 { draw_line(&mut img, 200, 120 + row * 150, 60, 14, 4, 22, 3, [240, 240, 240], row % 2 == 0); }
    let frame = dynamic(img);
    let s = settings();
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
    let t0 = Instant::now();
    let jobs = e.begin(frame.clone(), &s, t0, false);
    for j in jobs { e.complete(j.id, Some(("a".into(), "б".into())), &s); }
    println!("first frame (detect + classify + background + fonts): {:?}", t0.elapsed());
    let runs = 10u32;
    let t1 = Instant::now();
    for i in 0..runs { e.begin(frame.clone(), &s, t0 + Duration::from_millis(100 * (i as u64 + 1)), false); e.finish(&s); }
    println!("stable frame: {:?}", t1.elapsed() / runs);
}

#[test]
fn font_is_selected_once_per_field_and_stays_locked() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
    let s = settings();
    let t0 = Instant::now();
    let (jobs, out) = run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
    assert_eq!(jobs, 2, "name and dialogue are recognised separately");
    let out = out.unwrap();
    assert_eq!(out.blocks.len(), 2);
    let fonts: HashMap<u64, String> = out.blocks.iter().map(|b| (b.id, b.font.family.clone())).collect();

    // Сдвиг на 2 px и новый перевод того же поля: id и шрифт те же, OCR не повторяется для неизменного.
    let jobs = e.begin(scene(2, [255, 200, 60]), &s, t0 + Duration::from_millis(500), false);
    assert!(jobs.is_empty(), "same content → no OCR");
    let dialogue = out.blocks.iter().find(|b| b.style.source_lines == 2).unwrap().id;
    e.complete(dialogue, Some(("text changed".into(), "гораздо более длинный перевод этой реплики".into())), &s);
    let again = e.finish(&s).unwrap();
    for b in &again.blocks { assert_eq!(fonts[&b.id], b.font.family, "font locked for field {}", b.id); }
    assert_eq!(again.new_translations.len(), 1);
    assert!(e.finish(&s).is_none(), "nothing changed → nothing retransmitted");
}

#[test]
fn background_mode_change_keeps_fonts_and_typography() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
    let mut s = settings();
    let first = run(&mut e, &scene(0, [255, 200, 60]), &s, Instant::now()).1.unwrap();
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TransparentOutline;
    let restyled = e.restyle(&s).unwrap();
    for (a, b) in first.blocks.iter().zip(&restyled.blocks) {
        assert_eq!((a.id, &a.font, &a.style), (b.id, &b.font, &b.style));
        assert_eq!(b.background.mode, super::super::background::BackgroundRenderMode::Transparent);
    }
    // Ручной трекинг меняет только его.
    s.appearance.inplace.letter_spacing = PropertyMode::Manual(2.0);
    let spaced = e.restyle(&s).unwrap();
    for (a, b) in restyled.blocks.iter().zip(&spaced.blocks) {
        assert_eq!(b.style.letter_spacing, 2.0);
        assert_eq!((&a.font, a.style.alignment, a.style.font_weight), (&b.font, b.style.alignment, b.style.font_weight));
    }
}

#[test]
fn failed_recognition_keeps_the_previous_state() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
    let s = settings();
    let t0 = Instant::now();
    let out = run(&mut e, &scene(0, [255, 200, 60]), &s, t0).1.unwrap();
    let jobs = e.begin(scene(0, [255, 200, 60]), &s, t0 + Duration::from_millis(300), true);
    for j in jobs { e.complete(j.id, None, &s); }
    assert!(e.finish(&s).is_none(), "the stable fields stay as they were");
    assert_eq!(e.tracker().blocks().iter().filter(|t| !t.translated_text.is_empty()).count(), out.blocks.len());
}

#[test]
fn a_changed_field_stays_due_until_it_is_completed() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let s = settings();
    let t0 = Instant::now();
    run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
    // The name changes colour and the dialogue is replaced by a different image; recognition
    // does not complete (it failed, or the tick was cancelled).
    let changed = scene(0, [60, 200, 255]);
    let first = e.begin(changed.clone(), &s, t0 + Duration::from_millis(200), false);
    assert!(!first.is_empty(), "the changed field needs recognising");
    let again = e.begin(changed.clone(), &s, t0 + Duration::from_millis(400), false);
    assert_eq!(again.len(), first.len(), "an unfinished field is handed out again, not forgotten");
    // Completing it (even as 'nothing recognised') settles it.
    for j in again { e.complete(j.id, None, &s); }
    assert!(e.begin(changed, &s, t0 + Duration::from_millis(600), false).is_empty());
}

/// A line a game prints letter by letter, `chars` letters of it so far.
fn typed(chars: usize) -> DynamicImage {
    let mut img = canvas(1200, 300, [25, 30, 40]);
    draw_prose_line(&mut img, 100, 120, chars, 14, 4, 32, 3, [240, 240, 240]);
    dynamic(img)
}

#[test]
fn a_line_still_being_typed_is_not_read_until_it_stops() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let s = settings();
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    // The frame never settled (the pipeline stopped waiting): the first look and every growth are held back.
    for (k, chars) in [6usize, 12, 18, 24, 30].into_iter().enumerate() {
        e.set_unsettled(true);
        let jobs = e.begin(typed(chars), &s, t0 + ms(500 * k as u64), false);
        assert!(jobs.is_empty(), "{chars} letters of a line that is still growing were handed to OCR");
    }
    // The text stopped growing: after the settling time it is read, once, whole.
    e.set_unsettled(true);
    let jobs = e.begin(typed(30), &s, t0 + ms(500 * 5), false);
    assert_eq!(jobs.len(), 1, "a finished line is read");
    for j in jobs { e.complete(j.id, Some(("text".into(), "перевод".into())), &s); }
    e.set_unsettled(true);
    assert!(e.begin(typed(30), &s, t0 + ms(500 * 6), false).is_empty(), "and not again");
}

/// A dialogue with its name, and two buttons in one row below it.
fn dialogue_with_buttons() -> DynamicImage {
    let mut img = canvas(1200, 500, [25, 30, 40]);
    draw_line(&mut img, 100, 100, 8, 14, 4, 22, 3, [255, 200, 60], false);
    draw_line(&mut img, 100, 140, 40, 14, 4, 22, 3, [240, 240, 240], false);
    draw_line(&mut img, 100, 172, 30, 14, 4, 22, 3, [240, 240, 240], false);
    draw_line(&mut img, 300, 380, 7, 14, 4, 22, 3, [240, 240, 240], false);
    draw_line(&mut img, 700, 380, 6, 14, 4, 22, 3, [240, 240, 240], false);
    dynamic(img)
}

#[test]
fn the_interface_of_the_game_is_left_alone_only_when_the_user_asked() {
    let t0 = Instant::now();
    let all = settings();
    let mut on = settings();
    on.appearance.inplace.skip_controls = true;
    // By default the name, the dialogue and both buttons are fields.
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(dialogue_with_buttons(), &all, t0, false);
    assert_eq!(jobs.len(), 4, "name, dialogue, two buttons");
    // With the option the two buttons never become fields: not read, not translated, not drawn.
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(dialogue_with_buttons(), &on, t0, false);
    assert_eq!(jobs.len(), 2, "only the name and the dialogue");
    for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), &on); }
    let frame = e.finish(&on).expect("the dialogue is published");
    assert_eq!(frame.blocks.len(), 2, "nothing is drawn over the buttons");
    assert!(frame.blocks.iter().all(|b| !matches!(b.block_type, TextBlockType::Button | TextBlockType::MenuItem)));
}

#[test]
fn text_cut_at_the_top_is_reported_and_the_strip_above_the_region_is_not_read() {
    let t0 = Instant::now();
    // A name whose upper part the frame cut (rows 0..12) over a line of dialogue.
    let mut img = canvas(800, 120, [25, 30, 40]);
    draw_line(&mut img, 60, 0, 6, 14, 4, 12, 3, [240, 220, 120], false);
    draw_line(&mut img, 60, 60, 30, 14, 4, 22, 3, [240, 240, 240], false);
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    assert_eq!(e.begin(dynamic(img.clone()), &settings(), t0, false).len(), 2);
    assert!(e.cut_at_top().is_some_and(|h| h >= 10.0), "{:?}", e.cut_at_top());
    // The same picture where the top 20 rows lie above the drawn region: the name there is no field.
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    e.set_reach_above(20.0);
    let jobs = e.begin(dynamic(img), &settings(), t0, false);
    assert_eq!(jobs.len(), 1, "only the dialogue: {:?}", jobs.iter().map(|j| j.rect).collect::<Vec<_>>());
    assert!(jobs[0].rect.y > 50.0);
    assert_eq!(e.cut_at_top(), None, "what the strip holds does not ask for more");
}

#[test]
fn the_name_of_the_speaker_is_left_alone_only_when_the_user_asked() {
    let t0 = Instant::now();
    let mut on = settings();
    on.appearance.inplace.skip_names = true;
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    assert_eq!(e.begin(dialogue_with_buttons(), &settings(), t0, false).len(), 4, "name, dialogue, two buttons");
    // The name goes, the dialogue and the buttons stay.
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = e.begin(dialogue_with_buttons(), &on, t0, false);
    assert_eq!(jobs.len(), 3, "dialogue and two buttons");
    for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), &on); }
    let frame = e.finish(&on).expect("published");
    assert!(frame.blocks.iter().all(|b| b.block_type != TextBlockType::CharacterName), "nothing is drawn over the name");
    // Both options together leave only the dialogue.
    on.appearance.inplace.skip_controls = true;
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    assert_eq!(e.begin(dialogue_with_buttons(), &on, t0, false).len(), 1);
}

/// A line of text over small checks (a pattern the exemplar search can copy).
fn checks_with_text() -> DynamicImage {
    let mut img = canvas(800, 240, [0, 0, 0]);
    for y in 0..240 { for x in 0..800 { let v = ((x / 6 + y / 6) % 2 * 40 + 50) as u8; img.put_pixel(x, y, image::Rgba([v, v, 110, 255])); } }
    draw_line(&mut img, 100, 100, 34, 14, 4, 26, 4, [255, 255, 255], false);
    dynamic(img)
}

#[test]
fn a_patterned_background_is_filled_quickly_and_refined_beside_the_frames() {
    let mut s = settings();
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let (ids, first) = read_all(&mut e, checks_with_text(), &s, Instant::now());
    assert!(!ids.is_empty(), "the text is found");
    let first = first.expect("the field is drawn at once, with the quick fill");
    assert!(first.blocks.iter().any(|b| b.background.refinable), "the exemplar search was left for later");
    assert!(e.refining());
    // The full fill arrives on its own: the layout is published again without a new frame.
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut refined = None;
    while Instant::now() < deadline {
        if let Some(frame) = e.restyle(&s) { refined = Some(frame); break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    let refined = refined.expect("the refined background was published");
    assert!(refined.blocks.iter().all(|b| !b.background.refinable), "nothing is left to refine");
    assert!(refined.blocks.iter().zip(&first.blocks).any(|(a, b)| a.revision > b.revision), "the field has a new revision");
    assert!(!e.refining());
    // And once nothing is pending there is nothing to publish.
    assert!(e.restyle(&s).is_none());
}

#[test]
fn a_smooth_background_is_final_at_once() {
    let mut s = settings();
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let (_, frame) = read_all(&mut e, dialogue_with_buttons(), &s, Instant::now());
    assert!(frame.unwrap().blocks.iter().all(|b| !b.background.refinable), "a flat background needs no second pass");
    assert!(!e.refining());
}

#[test]
fn a_settled_frame_and_a_forced_read_are_never_held() {
    let s = settings();
    let t0 = Instant::now();
    // The pipeline waited for the frame to settle: the first look reads at once, as it always did.
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    assert_eq!(e.begin(typed(30), &s, t0, false).len(), 1);
    // An unsettled frame whose text does not grow (an animated background) is read one frame later.
    let mut still = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    still.set_unsettled(true);
    assert!(still.begin(typed(30), &s, t0, false).is_empty(), "the first look waits one frame");
    still.set_unsettled(true);
    assert_eq!(still.begin(typed(30), &s, t0 + Duration::from_millis(500), false).len(), 1, "text that does not grow is read");
    // A read the user asked for is not held.
    let mut forced = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    forced.set_unsettled(true);
    assert_eq!(forced.begin(typed(30), &s, t0, true).len(), 1);
}

#[test]
fn explicit_reanalysis_selects_fonts_again() {
    let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
    let s = settings();
    let t0 = Instant::now();
    run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
    e.reanalyze_fonts();
    assert!(e.tracker().blocks().iter().all(|t| !t.font_selection_locked()));
    let jobs = e.begin(scene(0, [255, 200, 60]), &s, t0 + Duration::from_millis(200), false);
    assert!(jobs.is_empty(), "text is unchanged: no OCR needed");
    assert!(e.tracker().blocks().iter().all(|t| t.font_selection_locked()), "fonts chosen again from the last analysis");
    assert!(e.finish(&s).is_some());
}
