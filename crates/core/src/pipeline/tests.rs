//! Tests of the pipeline as a whole (ticks, commands, both display modes) and the mocks they share.
use crate::sync::LockExt;
use super::*;
use crate::capture::CaptureError;
use crate::layout::engine::EDGE_REACH;
use super::state::MAX_REACH_UP;
use crate::ocr::OcrError;
use crate::settings::{NormRect, WindowKey};
use crate::translate::TranslateError;
use image::{DynamicImage, Rgba, RgbaImage};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MockCapture(Mutex<u8>);
impl Capture for MockCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
    async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> {
        // Светлое «слово» на тёмном фоне; значение сдвигает его, меняя текст в кадре.
        let x0 = *self.0.lock_recover() as u32 / 4;
        let mut img = RgbaImage::from_pixel(100, 50, Rgba([20, 20, 20, 255]));
        for y in 20..30 { for x in x0..x0 + 20 { img.put_pixel(x, y, Rgba([250, 250, 250, 255])); } }
        Ok(DynamicImage::ImageRgba8(img))
    }
}
struct MockOcr(Mutex<String>, Arc<AtomicUsize>);
impl Ocr for MockOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
        self.1.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.lock_recover().clone())
    }
}

struct MockTr(Arc<AtomicUsize>);
impl Translate for MockTr {
    async fn translate(&self, _: &Settings, t: &str, _: &str, _: &str) -> Result<String, TranslateError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(format!("RU:{t}"))
    }
}

fn settings() -> Settings {
    { let mut value = Settings::default(); value.capture.window = Some(WindowKey { uuid: "u".into(), resource_class: "g".into(), caption: "g".into(), program: String::new() }); value.capture.region = Some(NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }); value.recognition.debounce_ms = 100; value.recognition.interval_ms = 50; value }
}

/// Events other than stage statuses.
fn drain(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if !matches!(e, Event::Status(_)) { v.push(e); }
    }
    v
}

#[tokio::test]
async fn events_carry_the_generation_read_before_the_tick() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let p = Pipeline::new(cap, MockOcr(Mutex::new("Where are you going?".into()), ocr_n), MockTr(tr_n));
    let (_settings_tx, settings_rx) = watch::channel(settings());
    let (_running_tx, running_rx) = watch::channel(false);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let generation = Arc::new(AtomicU64::new(7));
    let task = tokio::spawn(p.run(settings_rx, running_rx, cmd_rx, generation.clone(), tx));
    cmd_tx.send(Cmd::TranslateOnce).unwrap();
    let first = loop { if let (g, Event::Translation { .. }) = rx.recv().await.unwrap() { break g; } };
    assert_eq!(first, 7);
    // The owner changed the settings: later events are stamped with the new generation, so the
    // receiver can tell an old event that was still in flight from a current one.
    generation.store(8, Ordering::SeqCst);
    cmd_tx.send(Cmd::Reset).unwrap();
    cmd_tx.send(Cmd::TranslateOnce).unwrap();
    let second = loop { if let (g, Event::Translation { .. }) = rx.recv().await.unwrap() { break g; } };
    assert_eq!(second, 8);
    drop(cmd_tx);
    task.await.unwrap();
}

#[tokio::test]
async fn debounce_then_translate_once() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut p = Pipeline::new(cap.clone(), MockOcr(Mutex::new("Where are you going?".into()), ocr_n.clone()), MockTr(tr_n.clone()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let s = settings();
    let t0 = Instant::now();

    p.tick(&s, false, t0, &tx).await; // первый кадр — «изменился»
    p.tick(&s, false, t0 + Duration::from_millis(50), &tx).await; // стабилен, но debounce не прошёл
    assert_eq!(ocr_n.load(Ordering::SeqCst), 0);
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    assert_eq!(ocr_n.load(Ordering::SeqCst), 1);
    assert_eq!(drain(&mut rx), vec![Event::Translation { region_id: "subtitles".into(), region_name: "Субтитры".into(), original: "Where are you going?".into(), text: "RU:Where are you going?".into() }]);

    // Без изменений кадра OCR больше не запускается.
    p.tick(&s, false, t0 + Duration::from_millis(400), &tx).await;
    assert_eq!(ocr_n.load(Ordering::SeqCst), 1);
    assert_eq!(tr_n.load(Ordering::SeqCst), 1);
}

fn previews(events: Vec<Event>) -> Vec<OcrPreview> {
    events.into_iter().filter_map(|e| match e { Event::OcrPreview { preview, .. } => Some(*preview), _ => None }).collect()
}

#[tokio::test]
async fn ocr_preview_is_sent_only_while_enabled() {
    let flag = Arc::new(AtomicBool::new(false));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let counters = || (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (ocr_n, tr_n) = counters();
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Where are you going?".into()), ocr_n), MockTr(tr_n)).with_preview(flag.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let s = settings();
    let t0 = Instant::now();
    p.tick(&s, true, t0, &tx).await;
    assert!(previews(drain(&mut rx)).is_empty(), "closed preview costs nothing");
    flag.store(true, Ordering::Relaxed);
    p.tick(&s, true, t0 + Duration::from_millis(10), &tx).await;
    let found = previews(drain(&mut rx));
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].image.width(), found[0].image.height()), (100, 50));
    assert_eq!(found[0].original, "Where are you going?");
    assert_eq!(found[0].translation, "RU:Where are you going?");
    assert_eq!(found[0].boxes.len(), 1);
}

#[tokio::test]
async fn the_inspector_gets_stage_timings_and_the_region_phase() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Where are you going?".into()), ocr_n), MockTr(tr_n)).with_preview(flag);
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&settings(), true, Instant::now(), &tx).await;
    let found = previews(drain(&mut rx));
    assert_eq!(found[0].phase, Phase::Showing);
    let stages: Vec<&str> = found[0].timings.iter().map(|t| t.stage).collect();
    assert_eq!(stages, ["capture", "ocr", "translate"], "only the stages that ran, in pipeline order: {stages:?}");
    assert!(found[0].timings.iter().all(|t| t.samples == 1 && t.p50_ms <= t.p95_ms));
}

#[tokio::test]
async fn the_inspector_explains_what_the_layout_engine_decided() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    let found = previews(drain(&mut rx));
    let details = &found.last().expect("a preview for the field").boxes[0].details;
    for expected in ["тип блока", "шрифт:", "высота прописных", "фон:"] {
        assert!(details.iter().any(|d| d.starts_with(expected)), "{expected} in {details:?}");
    }
    assert!(found.last().unwrap().timings.iter().any(|t| t.stage == "layout"));
}

#[test]
fn timings_keep_a_window_and_report_percentiles() {
    let mut t = Timings::default();
    for ms in 1..=200u64 { t.record("ocr", Duration::from_millis(ms)); }
    let ocr = &t.summary()[0];
    assert_eq!((ocr.stage, ocr.samples, ocr.last_ms.round() as u32), ("ocr", Timings::KEEP, 200));
    // Only the last 128 samples (73..=200) count.
    assert!((ocr.p50_ms - 137.0).abs() <= 1.5, "p50 {}", ocr.p50_ms);
    assert!(ocr.p95_ms > 190.0 && ocr.p95_ms <= 200.0, "p95 {}", ocr.p95_ms);
    assert!(Timings::default().summary().is_empty());
}

#[tokio::test]
async fn region_phases_follow_the_cycle_and_failures_are_visible() {
    struct FailingCapture;
    impl Capture for FailingCapture {
        fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
        async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> { Err(CaptureError::WindowGone) }
    }
    let mut state = RegionState::default();
    assert_eq!(state.phase, Phase::WaitingFrame);
    state.enter("r", Phase::Debouncing);
    state.enter("r", Phase::Recognizing);
    assert_eq!(state.phase, Phase::Recognizing);
    let region = CaptureRegion::default();
    let (tx, _rx) = mpsc::unbounded_channel();
    let now = Instant::now();
    state.fail(&region, "x".into(), now, false, &tx);
    assert_eq!(state.phase, Phase::RetryWait);
    state.fail(&region, "x".into(), now, true, &tx);
    assert_eq!(state.phase, Phase::Halted);
    assert!(Phase::Halted.label().contains("остановлен"));
    // A failing backend through the whole pipeline ends in a retry phase, not in silence.
    let mut p = Pipeline::new(FailingCapture, MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
    p.tick(&settings(), true, Instant::now(), &tx).await;
    assert_eq!(p.states["subtitles"].phase, Phase::RetryWait);
}

/// An engine that is only `confidence` percent sure about every line it reads.
struct SureOcr { text: String, confidence: f32 }
impl Ocr for SureOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok(self.text.clone()) }
    async fn recognize_detailed(&self, img: &DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        let lines = self.text.lines().enumerate().map(|(i, t)| crate::ocr::OcrLine {
            rect: crate::layout::CropRect::in_space(5.0, 5.0 + 20.0 * i as f32, img.width() as f32 - 10.0, 15.0), text: t.to_owned(), confidence: self.confidence }).collect();
        Ok(crate::ocr::OcrResult { text: self.text.clone(), lines, confidence: Some(self.confidence), engine: "mock", ..Default::default() })
    }
}

fn texts_of(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| match e { Event::Translation { text, .. } => Some(text.clone()), _ => None }).collect()
}

/// OCR that finishes after `delay` and counts the runs that got to the end.
struct LateOcr { delay: Duration, finished: Arc<AtomicUsize> }
impl Ocr for LateOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
        tokio::time::sleep(self.delay).await;
        self.finished.fetch_add(1, Ordering::SeqCst);
        Ok("Slow words here".into())
    }
}

#[tokio::test]
async fn ocr_of_a_frame_the_window_has_left_is_dropped() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let finished = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(cap.clone(), LateOcr { delay: Duration::from_millis(900), finished: finished.clone() }, MockTr(Arc::new(AtomicUsize::new(0))));
    let s = settings();
    let (tx, mut rx) = mpsc::unbounded_channel();
    // The picture changes 300 ms into the recognition.
    let mover = cap.clone();
    tokio::spawn(async move { tokio::time::sleep(Duration::from_millis(300)).await; *mover.0.lock_recover() = 160; });
    p.tick(&s, true, Instant::now(), &tx).await;
    let events = drain(&mut rx);
    assert_eq!(finished.load(Ordering::SeqCst), 0, "the stale OCR was cancelled, not awaited");
    assert!(texts_of(&events).is_empty(), "nothing is shown for the old frame");
    assert_eq!(p.states["subtitles"].phase, Phase::WaitingFrame);
    assert!(p.states["subtitles"].dirty_since.is_some(), "the new picture is read after it settles");
    assert_eq!(p.states["subtitles"].stale, 1);
}

#[tokio::test]
async fn ocr_of_a_still_picture_is_not_interrupted() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let finished = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(cap, LateOcr { delay: Duration::from_millis(400), finished: finished.clone() }, MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&settings(), true, Instant::now(), &tx).await;
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert_eq!(texts_of(&drain(&mut rx)).len(), 1);
    assert_eq!(p.states["subtitles"].stale, 0);
}

/// An engine that reads well only with binarization on.
struct PickyOcr;
impl Ocr for PickyOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok(String::new()) }
    async fn recognize_detailed(&self, _: &DynamicImage, s: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        let (text, confidence) = if s.recognition.binarize && !s.recognition.auto_invert { ("Open the door\nTake the key", 85.0) } else { ("| ~", 30.0) };
        let lines = text.lines().enumerate().map(|(i, t)| crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(0.0, 20.0 * i as f32, 50.0, 15.0), text: t.to_owned(), confidence }).collect();
        Ok(crate::ocr::OcrResult { text: text.into(), lines, confidence: Some(confidence), engine: "mock", ..Default::default() })
    }
}

#[tokio::test]
async fn auto_tune_reports_the_preset_that_reads_the_most() {
    let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), PickyOcr, MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.auto_tune(&settings(), &tx).await;
    let tuned = drain(&mut rx).into_iter().find_map(|e| match e { Event::AutoTune { results, best, .. } => Some((results, best)), _ => None }).expect("a result");
    assert_eq!(tuned.0.len(), crate::ocr::filter::PRESETS.len());
    assert_eq!(tuned.0[tuned.1].name, "Бинаризация");
    assert_eq!(tuned.0[tuned.1].score, 170.0);
    assert_eq!(tuned.0[0].score, 0.0, "the raw frame gave only noise");
}

/// Lines far apart (separate texts), as the engine would report a menu.
struct MenuOcr(Mutex<String>);
impl Ocr for MenuOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok(self.0.lock_recover().clone()) }
    async fn recognize_detailed(&self, _: &DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        let text = self.0.lock_recover().clone();
        let lines = text.lines().enumerate().map(|(i, t)| crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(0.0, 40.0 * i as f32, 90.0, 15.0), text: t.to_owned(), confidence: 90.0 }).collect();
        Ok(crate::ocr::OcrResult { text, lines, confidence: Some(90.0), engine: "mock", ..Default::default() })
    }
}

#[test]
fn paragraphs_follow_the_gaps_between_lines() {
    let line = |y: f32, t: &str| crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(0.0, y, 80.0, 15.0), text: t.into(), confidence: 90.0 };
    let wrapped = crate::ocr::OcrResult { text: String::new(), lines: vec![line(0.0, "I will not"), line(18.0, "go there"), line(60.0, "Continue")], confidence: None, engine: "mock", ..Default::default() };
    assert_eq!(paragraphs(&wrapped, 1.8), vec!["I will not go there".to_string(), "Continue".to_string()]);
    assert_eq!(paragraphs(&wrapped, 4.0), vec!["I will not go there Continue".to_string()], "a larger factor keeps more together");
    assert_eq!(paragraphs(&crate::ocr::OcrResult::text_only("Just text".into(), "x"), 1.8), vec!["Just text".to_string()]);
}

#[test]
fn a_sentence_split_by_a_gap_is_translated_whole() {
    let line = |y: f32, t: &str| crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(0.0, y, 80.0, 15.0), text: t.into(), confidence: 90.0 };
    let r = crate::ocr::OcrResult { text: String::new(), lines: vec![line(0.0, "Welcome to the village,"), line(60.0, "traveller."), line(120.0, "Exit"), line(180.0, "Options")], confidence: None, engine: "mock", ..Default::default() };
    assert_eq!(paragraphs(&r, 1.8), vec!["Welcome to the village, traveller.".to_string(), "Exit".to_string(), "Options".to_string()]);
}

#[tokio::test]
async fn changes_only_translates_the_new_paragraphs_and_reuses_the_rest() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), MenuOcr(Mutex::new("Main menu\nContinue\nExit".into())), MockTr(calls.clone()));
    let s = { let mut value = settings(); value.translation.changes_only = true; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3, "three separate texts the first time");
    assert_eq!(texts_of(&drain(&mut rx)), vec!["RU:Main menu\nRU:Continue\nRU:Exit".to_string()]);
    // Only the middle item changes.
    *p.io.ocr.0.lock_recover() = "Main menu\nLoad game\nExit".into();
    p.tick(&s, true, Instant::now(), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 4, "one new request for the one new paragraph");
    assert_eq!(texts_of(&drain(&mut rx)), vec!["RU:Main menu\nRU:Load game\nRU:Exit".to_string()]);
}

#[tokio::test]
async fn auto_tune_keeps_a_manual_choice_unless_a_preset_is_clearly_better() {
    let tune = |binarize: bool| async move {
        let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), PickyOcr, MockTr(Arc::new(AtomicUsize::new(0))));
        let s = { let mut value = settings(); value.recognition.binarize = binarize; value.recognition.contrast = 20; value };
        let (tx, mut rx) = mpsc::unbounded_channel();
        p.auto_tune(&s, &tx).await;
        drain(&mut rx).into_iter().find_map(|e| match e { Event::AutoTune { results, best, .. } => Some((results, best)), _ => None }).unwrap()
    };
    // The user's own binarize + contrast 20 already reads well: it stays (it is the first candidate).
    let (results, best) = tune(true).await;
    assert_eq!((best, results[0].name), (0, "Текущие настройки"));
    assert_eq!(results[0].filters.contrast, 20, "the manual contrast is not overwritten");
    // Contrast 20 alone reads nothing: a preset wins.
    let (results, best) = tune(false).await;
    assert_eq!(results[best].name, "Бинаризация");
}

#[tokio::test]
async fn a_paused_pipeline_does_not_read_the_window_until_it_is_resumed() {
    struct Counting(Arc<AtomicUsize>);
    impl Capture for Counting {
        fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
        async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 50, Rgba([20, 20, 20, 255]))))
        }
    }
    let grabs = Arc::new(AtomicUsize::new(0));
    let paused = Arc::new(AtomicBool::new(true));
    let pipeline = Pipeline::new(Counting(grabs.clone()), SureOcr { text: "x".into(), confidence: 90.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_pause(paused.clone());
    let (_settings_tx, settings_rx) = watch::channel(settings());
    let (_running_tx, running_rx) = watch::channel(true);
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(pipeline.run(settings_rx, running_rx, cmd_rx, Arc::default(), ev_tx));
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(grabs.load(Ordering::SeqCst), 0, "paused: nothing is captured");
    paused.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(grabs.load(Ordering::SeqCst) > 0, "resumed: the window is read again");
    task.abort();
}

/// Records the filters and the threshold each reading was made with; a reading takes `delay` and is only
/// recorded if it is allowed to finish.
struct RecordingOcr { delay: Duration, finished: Arc<Mutex<Vec<(bool, u32)>>> }
impl Ocr for RecordingOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok(String::new()) }
    async fn recognize_detailed(&self, _: &DynamicImage, s: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        tokio::time::sleep(self.delay).await;
        self.finished.lock_recover().push((s.recognition.binarize, s.recognition.minimum_confidence));
        Ok(crate::ocr::OcrResult { text: "Some words here".into(), lines: Vec::new(), confidence: Some(90.0), engine: "mock", ..Default::default() })
    }
}

#[tokio::test]
async fn changed_settings_reach_the_next_frame_without_a_restart_and_drop_the_reading_in_progress() {
    let finished = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Pipeline::new(MockCapture(Mutex::new(10)), RecordingOcr { delay: Duration::from_millis(500), finished: finished.clone() }, MockTr(Arc::new(AtomicUsize::new(0))));
    let (settings_tx, settings_rx) = watch::channel(settings());
    let (_running_tx, running_rx) = watch::channel(true);
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(pipeline.run(settings_rx, running_rx, cmd_rx, Arc::default(), ev_tx));
    // The first reading starts (debounce 100 ms) and is half way through when the filter is switched on. Nobody sends
    // a `Cmd::Reset`: the pipeline notices the change itself.
    tokio::time::sleep(Duration::from_millis(350)).await;
    settings_tx.send_modify(|s| { s.recognition.binarize = true; s.recognition.minimum_confidence = 45; });
    tokio::time::sleep(Duration::from_millis(1800)).await;
    task.abort();
    let readings = finished.lock_recover().clone();
    assert!(!readings.is_empty(), "the pipeline went on reading");
    assert!(readings.iter().all(|r| *r == (true, 45)), "no reading with the old settings reached its end: {readings:?}");
}

#[tokio::test]
async fn a_setting_that_does_not_affect_recognition_does_not_interrupt_a_reading() {
    let finished = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Pipeline::new(MockCapture(Mutex::new(10)), RecordingOcr { delay: Duration::from_millis(400), finished: finished.clone() }, MockTr(Arc::new(AtomicUsize::new(0))));
    let (settings_tx, settings_rx) = watch::channel(settings());
    let (_running_tx, running_rx) = watch::channel(true);
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(pipeline.run(settings_rx, running_rx, cmd_rx, Arc::default(), ev_tx));
    tokio::time::sleep(Duration::from_millis(300)).await;
    settings_tx.send_modify(|s| s.translation_window.opacity = 0.5);
    tokio::time::sleep(Duration::from_millis(700)).await;
    task.abort();
    assert!(!finished.lock_recover().is_empty(), "the reading in progress was not thrown away by a cosmetic change");
}

#[tokio::test]
async fn unsure_text_is_ignored_and_explained() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 18.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
    let s = { let mut value = settings(); value.recognition.minimum_confidence = 30; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    let events = drain(&mut rx);
    assert!(texts_of(&events).is_empty(), "noise is not translated");
    let preview = previews(events);
    assert_eq!(preview[0].boxes.len(), 1);
    assert!(preview[0].boxes[0].details.iter().any(|d| d.contains("отброшено") && d.contains("18%") && d.contains("30%")), "{:?}", preview[0].boxes[0].details);
    assert_eq!(p.states["subtitles"].phase, Phase::WaitingFrame);
}

#[tokio::test]
async fn preview_carries_confidence_and_the_filtered_frame_only_when_a_filter_is_on() {
    let run = |binarize: bool| async move {
        let flag = Arc::new(AtomicBool::new(true));
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 18.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
        let s = { let mut value = settings(); value.recognition.binarize = binarize; value };
        let (tx, mut rx) = mpsc::unbounded_channel();
        p.tick(&s, true, Instant::now(), &tx).await;
        previews(drain(&mut rx))
    };
    let plain = run(false).await;
    assert!(plain[0].filtered.is_none());
    assert_eq!(plain[0].boxes[0].confidence, Some(18.0));
    let filtered = run(true).await;
    let frame = filtered[0].filtered.as_ref().expect("filtered frame");
    assert!(filtered[0].filters.binarize && frame.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
}

#[tokio::test]
async fn sure_text_is_translated_with_its_lines_in_the_inspector() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, SureOcr { text: "Where are\nyou going".into(), confidence: 91.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&settings(), true, Instant::now(), &tx).await;
    let events = drain(&mut rx);
    assert_eq!(texts_of(&events), ["RU:Where are you going"]);
    let preview = previews(events);
    let boxes = &preview[0].boxes;
    assert_eq!((boxes.len(), boxes[0].original.as_str(), boxes[1].original.as_str()), (2, "Where are", "you going"), "one box per line the engine found");
    assert!(boxes[0].details[0].contains("91%") && boxes[0].details[0].contains("mock"));
    assert_eq!(preview[0].translation, "RU:Where are you going");
}

#[tokio::test]
async fn the_confidence_check_can_be_turned_off() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 18.0 }, MockTr(Arc::new(AtomicUsize::new(0))));
    let s = { let mut value = settings(); value.recognition.minimum_confidence = 0; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    assert_eq!(texts_of(&drain(&mut rx)), ["RU:Whxre arx yoz"]);
}

/// Counts requests, answers after `delay`.
struct SlowTr { calls: Arc<AtomicUsize>, delay: Duration }
impl Translate for SlowTr {
    async fn translate(&self, _: &Settings, t: &str, _: &str, _: &str) -> Result<String, TranslateError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        Ok(format!("RU:{t}"))
    }
}

#[tokio::test]
async fn a_cancelled_tick_does_not_lose_its_translation_request() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))),
        SlowTr { calls: calls.clone(), delay: Duration::from_millis(300) });
    let (tx, mut rx) = mpsc::unbounded_channel();
    let s = settings();
    // The tick is cancelled while the request is on its way (new command, changed settings...).
    let started = Instant::now();
    let _ = tokio::time::timeout(Duration::from_millis(100), p.tick(&s, true, started, &tx)).await;
    assert!(drain(&mut rx).is_empty(), "cancelled before the answer");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // The next tick asks for the same phrase: it joins the running request, no second one.
    p.tick(&s, true, started + Duration::from_millis(150), &tx).await;
    assert_eq!(texts_of(&drain(&mut rx)), ["RU:Hello there"]);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one request for the phrase");
    // Afterwards the answer is in the cache.
    tokio::time::sleep(Duration::from_millis(20)).await;
    p.tick(&s, true, started + Duration::from_millis(500), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(p.io.inflight.lock_recover().is_empty(), "finished requests are forgotten");
}

#[tokio::test]
async fn the_same_phrase_asked_twice_at_once_is_one_request() {
    let calls = Arc::new(AtomicUsize::new(0));
    let p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))),
        SlowTr { calls: calls.clone(), delay: Duration::from_millis(100) });
    let s = settings();
    let (a, b) = tokio::join!(p.io.translate_shared(&s, "Hello", "en", "ru"), p.io.translate_shared(&s, "Hello", "en", "ru"));
    assert_eq!((a.unwrap(), b.unwrap()), ("RU:Hello".to_string(), "RU:Hello".to_string()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // Another language pair is another request.
    let _ = p.io.translate_shared(&s, "Hello", "en", "de").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// Answers a batch in one call: counts `translate_many` and `translate` calls separately; "Echo" is returned as is.
struct BatchTr { many: Arc<AtomicUsize>, single: Arc<AtomicUsize>, sent: Arc<Mutex<Vec<Vec<String>>>> }
impl Translate for BatchTr {
    async fn translate(&self, _: &Settings, t: &str, _: &str, _: &str) -> Result<String, TranslateError> {
        self.single.fetch_add(1, Ordering::SeqCst);
        Ok(format!("RU:{t}"))
    }
    async fn translate_many(&self, _: &Settings, texts: &[String], _: &str, _: &str) -> Vec<Result<String, TranslateError>> {
        self.many.fetch_add(1, Ordering::SeqCst);
        self.sent.lock_recover().push(texts.to_vec());
        texts.iter().map(|t| Ok(if t == "Echo the source text" { t.clone() } else { format!("RU:{t}") })).collect()
    }
}

type SentBatches = Arc<Mutex<Vec<Vec<String>>>>;
type BatchPipeline = Pipeline<Arc<MockCapture>, MockOcr, BatchTr>;
fn batch_pipeline() -> (BatchPipeline, Arc<AtomicUsize>, Arc<AtomicUsize>, SentBatches) {
    let (many, single, sent) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
    let p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))),
        BatchTr { many: many.clone(), single: single.clone(), sent: sent.clone() });
    (p, many, single, sent)
}

#[tokio::test]
async fn bergamot_gets_all_pending_texts_in_one_batch_call() {
    let (p, many, single, sent) = batch_pipeline();
    let mut s = settings();
    s.translation.service = crate::settings::TranslationService::Bergamot;
    let texts = vec!["Start game", "Where are you going?", "Hello there"];
    let out = p.io.translate_texts(&s, texts.clone(), "en", "ru").await;
    assert_eq!((many.load(Ordering::SeqCst), single.load(Ordering::SeqCst)), (1, 0));
    for t in texts { assert_eq!(out[t].as_ref().unwrap(), &format!("RU:{t}")); }
    assert_eq!(sent.lock_recover()[0], ["Start game", "Where are you going?", "Hello there"], "order is kept");
    // Everything is cached now: no further call.
    let again = p.io.translate_texts(&s, vec!["Start game", "Hello there"], "en", "ru").await;
    assert_eq!(again["Hello there"].as_ref().unwrap(), "RU:Hello there");
    assert_eq!(many.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cached_texts_are_left_out_of_the_batch_and_a_rejected_one_is_not_cached() {
    let (p, many, single, sent) = batch_pipeline();
    let mut s = settings();
    s.translation.service = crate::settings::TranslationService::Bergamot;
    p.io.cache.lock_recover().put(&cache_source(&s, "en"), "ru", "Known line", "Известная".into());
    let out = p.io.translate_batch_shared(&s, &["Known line", "Echo the source text", "Open the door", "Open the door"], "en", "ru").await;
    assert_eq!(many.load(Ordering::SeqCst), 1);
    assert_eq!(sent.lock_recover()[0], ["Echo the source text", "Open the door"]);
    assert_eq!(out["Known line"].as_ref().unwrap(), "Известная");
    assert_eq!(out["Open the door"].as_ref().unwrap(), "RU:Open the door");
    assert!(matches!(&out["Echo the source text"], Err(e) if matches!(**e, TranslateError::Rejected(_))), "{:?}", out["Echo the source text"]);
    assert_eq!(out.len(), 3);
    assert!(p.io.cached(&s, "en", "ru", "Echo the source text").is_none());
    assert!(p.io.cached(&s, "en", "ru", "Open the door").is_some());
    assert_eq!(single.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn other_services_and_single_texts_keep_one_request_per_text() {
    let (p, many, single, _) = batch_pipeline();
    let s = settings();
    assert_ne!(s.translation.service, crate::settings::TranslationService::Bergamot);
    let out = p.io.translate_texts(&s, vec!["One", "Two", "Three"], "en", "ru").await;
    assert_eq!((many.load(Ordering::SeqCst), single.load(Ordering::SeqCst)), (0, 3));
    assert_eq!(out["Two"].as_ref().unwrap(), "RU:Two");
    let mut s = s;
    s.translation.service = crate::settings::TranslationService::Bergamot;
    p.io.translate_texts(&s, vec!["Lonely"], "en", "ru").await;
    assert_eq!((many.load(Ordering::SeqCst), single.load(Ordering::SeqCst)), (0, 4), "one text is not a batch");
    // Only one text is missing: the cached one does not make a batch of it.
    p.io.translate_texts(&s, vec!["Lonely", "Another"], "en", "ru").await;
    assert_eq!((many.load(Ordering::SeqCst), single.load(Ordering::SeqCst)), (0, 5));
}

#[tokio::test]
async fn translate_changes_sends_new_paragraphs_to_bergamot_as_one_batch() {
    let (p, many, single, _) = batch_pipeline();
    let mut s = settings();
    s.translation.service = crate::settings::TranslationService::Bergamot;
    let parts: Vec<String> = ["Header", "New line", "Header"].map(String::from).to_vec();
    let text = p.io.translate_changes(&s, &parts, "en", "ru").await.unwrap();
    assert_eq!(text, "RU:Header\nRU:New line\nRU:Header");
    assert_eq!((many.load(Ordering::SeqCst), single.load(Ordering::SeqCst)), (1, 0));
}

#[tokio::test]
async fn fields_that_fail_differently_each_get_their_own_message() {
    /// Fails with a different error depending on the phrase.
    struct Picky;
    impl Translate for Picky {
        async fn translate(&self, _: &Settings, t: &str, _: &str, _: &str) -> Result<String, TranslateError> {
            if t.contains("name") { Err(TranslateError::BadResponse) } else { Err(TranslateError::RateLimited { retry_after: 30 }) }
        }
    }
    /// Two fields read as different texts.
    struct TwoTexts(AtomicUsize);
    impl Ocr for TwoTexts {
        async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
            Ok(if self.0.fetch_add(1, Ordering::SeqCst).is_multiple_of(2) { "the name field".into() } else { "a long line of dialogue".into() })
        }
    }
    let mut p = Pipeline::new(Arc::new(SceneCapture), TwoTexts(AtomicUsize::new(0)), Picky);
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    let message = drain(&mut rx).into_iter().find_map(|e| match e { Event::Error { message, .. } => Some(message), _ => None }).expect("an error");
    assert!(message.contains("«the name field»") && message.contains("неожиданный ответ"), "{message}");
    assert!(message.contains("«a long line of dialogue»") && message.contains("429"), "{message}");
}

#[test]
fn an_error_names_the_service_and_the_phrase_it_is_about() {
    let s = Settings::default();
    assert_eq!(translation_context(&s, "Where  are\nyou?"), "Google Translate, «Where are you?»");
    let long = translation_context(&s, &"word ".repeat(30));
    assert!(long.ends_with("…»") && long.chars().count() < 70, "{long}");
    let yandex = { let mut value = Settings::default(); value.translation.service = crate::settings::TranslationService::Yandex; value };
    assert!(translation_context(&yandex, "Hi").starts_with("Yandex Translate"));
}

#[tokio::test]
async fn the_message_names_service_phrase_and_reason() {
    struct Limited;
    impl Translate for Limited {
        async fn translate(&self, _: &Settings, _: &str, _: &str, _: &str) -> Result<String, TranslateError> { Err(TranslateError::RateLimited { retry_after: 60 }) }
    }
    let mut p = Pipeline::new(Arc::new(SceneCapture), MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), Limited);
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    let message = drain(&mut rx).into_iter().find_map(|e| match e { Event::Error { message, .. } => Some(message), _ => None }).expect("an error");
    assert!(message.contains("Google Translate, «Hello there»") && message.contains("HTTP 429"), "service, phrase and the reason: {message}");
}

#[tokio::test]
async fn a_rejected_translation_is_not_cached_and_is_asked_again() {
    /// Echoes the source the first time, translates afterwards.
    struct Echoing(AtomicUsize);
    impl Translate for Echoing {
        async fn translate(&self, _: &Settings, t: &str, _: &str, _: &str) -> Result<String, TranslateError> {
            Ok(if self.0.fetch_add(1, Ordering::SeqCst) == 0 { t.to_owned() } else { format!("RU:{t}") })
        }
    }
    let p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), Echoing(AtomicUsize::new(0)));
    let s = settings();
    let error = p.io.translate_shared(&s, "Where are you going?", "en", "ru").await.unwrap_err();
    assert!(matches!(&*error, TranslateError::Rejected(_)) && !error.stops_automatic_retries(), "{error}");
    assert_eq!(p.io.cached(&s, "en", "ru", "Where are you going?"), None, "a rejected answer must not be cached");
    // Let the finished request leave the in-flight table, as the next tick would find it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(p.io.translate_shared(&s, "Where are you going?", "en", "ru").await.unwrap(), "RU:Where are you going?");
    assert_eq!(p.io.cached(&s, "en", "ru", "Where are you going?").as_deref(), Some("RU:Where are you going?"));
}

#[tokio::test]
async fn a_shared_error_reaches_everyone_who_waited() {
    struct Limited;
    impl Translate for Limited {
        async fn translate(&self, _: &Settings, _: &str, _: &str, _: &str) -> Result<String, TranslateError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Err(TranslateError::RateLimited { retry_after: 60 })
        }
    }
    let p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), Limited);
    let s = settings();
    let (a, b) = tokio::join!(p.io.translate_shared(&s, "Hi", "en", "ru"), p.io.translate_shared(&s, "Hi", "en", "ru"));
    for r in [a, b] { assert!(matches!(&*r.unwrap_err(), TranslateError::RateLimited { retry_after: 60 })); }
}

#[tokio::test]
async fn the_lines_of_the_engine_reach_the_layout_of_the_field() {
    /// One line exactly where the text is: the crop minus the margin the engine adds around a field.
    struct GeoOcr;
    impl Ocr for GeoOcr {
        async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok("Hello there".into()) }
        async fn recognize_detailed(&self, img: &DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
            let line = crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(3.0, 3.0, img.width() as f32 - 6.0, img.height() as f32 - 6.0), text: "Hello there".into(), confidence: 90.0 };
            Ok(crate::ocr::OcrResult { text: "Hello there".into(), lines: vec![line], confidence: Some(90.0), engine: "mock", ..Default::default() })
        }
    }
    let flag = Arc::new(AtomicBool::new(true));
    let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), GeoOcr, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    let events = drain(&mut rx);
    let block = events.iter().find_map(|e| match e { Event::Inplace { frame, .. } => frame.blocks.first().cloned(), _ => None }).expect("the field");
    assert!(block.lines_note.contains("согласны"), "the engine's line was matched with the block: {}", block.lines_note);
    let shown = previews(events);
    let details = &shown.last().unwrap().boxes[0].details;
    assert!(details.iter().any(|d| d.contains("согласны")), "the inspector shows where the structure came from: {details:?}");
    // The same field as plain data: its lines as the engine gave them (crop pixels), its box in that crop.
    let dto = shown.last().unwrap().boxes[0].block.as_ref().expect("a field that has been read carries its data");
    assert_eq!(dto.id, block.id);
    assert_eq!((dto.lines.len(), dto.lines[0].text.as_str(), dto.lines[0].confidence), (1, "Hello there", 90.0));
    assert!(!dto.font_family.is_empty() && dto.bbox.w > 0.0 && dto.bbox.h > 0.0);
    assert_eq!(dto.block_type, block.block_type);
    // The line the engine found lies inside the field's box in the same crop (both are relative to the crop's corner).
    assert!(dto.bbox.x <= dto.lines[0].bbox.x + 4.0 && dto.bbox.y <= dto.lines[0].bbox.y + 4.0, "{:?} vs {:?}", dto.bbox, dto.lines[0].bbox);
}

/// One block of three evenly spaced lines.
struct BlockCapture;
impl Capture for BlockCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
    async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> {
        let mut img = canvas(1000, 300, [25, 30, 40]);
        for y in [100u32, 134, 168] { draw_line(&mut img, 100, y, 30, 14, 4, 22, 3, [240, 240, 240], false); }
        Ok(DynamicImage::ImageRgba8(img))
    }
}

/// Reads a tall crop as a title and a body set far apart (two lines with a big gap), a short one as a single line.
struct GapOcr(Arc<AtomicUsize>);
impl Ocr for GapOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok("text".into()) }
    async fn recognize_detailed(&self, img: &DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let (w, h) = (img.width() as f32, img.height() as f32);
        let line = |y: f32, h: f32, text: &str| crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(3.0, y, w - 6.0, h), text: text.into(), confidence: 90.0 };
        let (text, lines) = if h > 60.0 { ("Title\nBody text".to_string(), vec![line(3.0, 22.0, "Title"), line(h - 25.0, 22.0, "Body text")]) }
            else { ("One part".to_string(), vec![line(3.0, h - 6.0, "One part")]) };
        Ok(crate::ocr::OcrResult { text, lines, confidence: Some(90.0), engine: "mock", ..Default::default() })
    }
}

#[tokio::test]
async fn a_field_that_ocr_shows_to_be_two_is_read_again_as_two_without_a_new_picture() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(Arc::new(BlockCapture), GapOcr(calls.clone()), MockTr(Arc::new(AtomicUsize::new(0))));
    let s = inplace_settings();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the merged block was read once");
    let drawn = |events: &[Event]| events.iter().filter_map(|e| match e { Event::Inplace { frame, .. } => Some(frame.blocks.len()), _ => None }).max().unwrap_or(0);
    assert_eq!(drawn(&drain(&mut rx)), 0, "nothing is drawn for a field that is about to be divided");
    // The picture has not changed and the region interval has not passed: the region is read again all the same.
    p.tick(&s, false, t0 + Duration::from_millis(160), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3, "one read for each part");
    assert_eq!(drawn(&drain(&mut rx)), 2, "the title and the body are two fields");
    // That was for one tick: a quiet picture is left alone afterwards.
    p.tick(&s, false, t0 + Duration::from_millis(200), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(p.illegal_transitions(), 0);
}

#[tokio::test]
async fn auto_pins_a_field_to_the_engine_that_was_needed() {
    /// `auto` ends up with the second engine for this text; a call that already names it gets it directly.
    struct AutoOcr(Arc<Mutex<Vec<String>>>, &'static str);
    impl Ocr for AutoOcr {
        async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok("Hello there".into()) }
        async fn recognize_detailed(&self, _: &DynamicImage, s: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
            self.0.lock_recover().push(s.recognition.engine.as_str().to_owned());
            Ok(crate::ocr::OcrResult { text: "Hello there".into(), lines: Vec::new(), confidence: Some(90.0), engine: self.1, ..Default::default() })
        }
    }
    for engine in ["paddleocr", "rapidocr", "meikiocr"] {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap.clone(), AutoOcr(seen.clone(), engine), MockTr(Arc::new(AtomicUsize::new(0))));
        let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value.recognition.engine = "auto".into(); value };
        let (tx, _rx) = mpsc::unbounded_channel();
        let t0 = Instant::now();
        p.tick(&s, false, t0, &tx).await;
        p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
        assert_eq!(*seen.lock_recover(), ["auto"], "the first read decides");
        // The text changed; the same field is read again, now straight with the engine that was needed.
        p.tick(&s, true, t0 + Duration::from_millis(300), &tx).await;
        assert_eq!(*seen.lock_recover(), ["auto", engine]);
        // Another engine setting is not touched by pins.
        let plain = { let mut value = s.clone(); value.recognition.engine = "tesseract".into(); value };
        p.tick(&plain, true, t0 + Duration::from_millis(500), &tx).await;
        assert_eq!(seen.lock_recover().last().map(String::as_str), Some("tesseract"));
    }
}

#[tokio::test]
async fn an_unsure_field_is_ignored_in_place_and_shown_in_the_inspector() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 12.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value.recognition.minimum_confidence = 30; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    let events = drain(&mut rx);
    assert!(!events.iter().any(|e| matches!(e, Event::Inplace { frame, .. } if !frame.blocks.is_empty())), "nothing is drawn over the original");
    let preview = previews(events);
    let note = preview.iter().flat_map(|p| p.boxes.iter()).flat_map(|b| b.details.iter()).find(|d| d.contains("отброшен"));
    assert!(note.is_some_and(|d| d.contains("12%")), "the inspector says why: {preview:?}");
}

#[test]
fn unsure_compares_with_the_threshold_and_ignores_engines_without_confidence() {
    assert_eq!(unsure(Some(20.0), 30), Some(20.0));
    assert_eq!(unsure(Some(30.0), 30), None, "at the threshold the text is used");
    assert_eq!(unsure(Some(5.0), 0), None, "0 turns it off");
    assert_eq!(unsure(None, 30), None, "an engine that does not report confidence is trusted");
}

#[tokio::test]
async fn ocr_preview_reports_an_empty_recognition() {
    let flag = Arc::new(AtomicBool::new(true));
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new(String::new()), ocr_n), MockTr(tr_n)).with_preview(flag);
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&settings(), true, Instant::now(), &tx).await;
    let found = previews(drain(&mut rx));
    assert_eq!(found.len(), 1);
    assert!(found[0].boxes.is_empty() && found[0].original.is_empty(), "the frame is shown even when nothing was read");
}

#[tokio::test]
async fn same_text_after_visual_change_not_retranslated() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut p = Pipeline::new(cap.clone(), MockOcr(Mutex::new("Hello there".into()), ocr_n.clone()), MockTr(tr_n.clone()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let s = settings();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    *cap.0.lock_recover() = 200; // визуальное изменение, текст тот же
    p.tick(&s, false, t0 + Duration::from_millis(200), &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(400), &tx).await;
    assert_eq!(ocr_n.load(Ordering::SeqCst), 2);
    assert_eq!(tr_n.load(Ordering::SeqCst), 1);
    assert_eq!(drain(&mut rx).len(), 1);
}

#[tokio::test]
async fn region_language_and_translation_override_use_distinct_cache_entries() {
    struct Languages(Arc<Mutex<Vec<(String, String)>>>);
    impl Translate for Languages {
        async fn translate(&self, s: &Settings, text: &str, src: &str, _: &str) -> Result<String, TranslateError> {
            self.0.lock_recover().push((s.recognition.language.clone(), src.to_owned()));
            Ok(format!("{src}:{text}"))
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))),
        MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), Languages(seen.clone()));
    let mut s = settings();
    s.capture.regions[0].rect = s.capture.region;
    s.capture.regions[0].recognition_language = "jpn+eng".into();
    let (tx, _rx) = mpsc::unbounded_channel();
    p.tick(&s, true, Instant::now(), &tx).await;
    s.translation.source_language = crate::settings::TranslationSourceLanguage::Explicit("fr".into());
    p.reset();
    p.tick(&s, true, Instant::now(), &tx).await;
    s.translation.source_language = crate::settings::TranslationSourceLanguage::RecognitionLanguage;
    p.reset();
    p.tick(&s, true, Instant::now(), &tx).await;
    assert_eq!(*seen.lock_recover(), [("jpn+eng".into(), "ja".into()), ("jpn+eng".into(), "fr".into())]);
}

#[tokio::test]
async fn changed_text_uses_cache_on_return() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let ocr = MockOcr(Mutex::new("First line".into()), ocr_n.clone());
    let mut p = Pipeline::new(cap.clone(), ocr, MockTr(tr_n.clone()));
    let (tx, _rx) = mpsc::unbounded_channel();
    let s = settings();
    let t0 = Instant::now();
    let step = |ms| t0 + Duration::from_millis(ms);
    p.tick(&s, false, step(0), &tx).await;
    p.tick(&s, false, step(150), &tx).await;
    *p.io.ocr.0.lock_recover() = "Totally different".into();
    *cap.0.lock_recover() = 200;
    p.tick(&s, false, step(200), &tx).await;
    p.tick(&s, false, step(400), &tx).await;
    *p.io.ocr.0.lock_recover() = "First line".into();
    *cap.0.lock_recover() = 20;
    p.tick(&s, false, step(450), &tx).await;
    p.tick(&s, false, step(650), &tx).await;
    assert_eq!(tr_n.load(Ordering::SeqCst), 2, "третий текст берётся из кэша");
}

#[tokio::test]
async fn window_mode_sends_one_translation_per_region() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&settings(), false, t0, &tx).await;
    p.tick(&settings(), false, t0 + Duration::from_millis(150), &tx).await;
    assert!(matches!(drain(&mut rx).as_slice(), [Event::Translation { .. }]));
}

/// A backend that cannot place text over the window (like the portal one).
struct NoInplace(Arc<MockCapture>);
impl Capture for NoInplace {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::PORTAL }
    async fn grab(&self, w: &WindowKey, r: NormRect) -> Result<DynamicImage, CaptureError> { self.0.grab(w, r).await }
}

#[tokio::test]
async fn a_backend_without_inplace_reads_the_region_as_a_whole() {
    let cap = NoInplace(Arc::new(MockCapture(Mutex::new(10))));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    assert!(matches!(drain(&mut rx).as_slice(), [Event::Translation { text, .. }] if text == "RU:Hello there"),
        "the whole region is translated for the translation window, no fields are searched");
}

/// An engine whose reading and confidence the test changes between ticks.
struct FlipOcr(Mutex<(String, f32)>);
impl Ocr for FlipOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok(self.0.lock_recover().0.clone()) }
    async fn recognize_detailed(&self, img: &DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
        let (text, confidence) = self.0.lock_recover().clone();
        let lines = vec![crate::ocr::OcrLine { rect: crate::layout::CropRect::in_space(5.0, 5.0, img.width() as f32 - 10.0, 15.0), text: text.clone(), confidence }];
        Ok(crate::ocr::OcrResult { text, lines, confidence: Some(confidence), engine: "mock", ..Default::default() })
    }
}

#[tokio::test]
async fn a_weak_lookalike_flip_of_a_solid_label_is_neither_translated_nor_drawn() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let tr_n = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(cap, FlipOcr(Mutex::new(("Level 15".into(), 92.0))), MockTr(tr_n.clone()));
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    let shown = |events: Vec<Event>| events.into_iter().find_map(|e| match e { Event::Inplace { frame, .. } => frame.blocks.first().map(|b| b.translation.clone()), _ => None });
    assert_eq!(shown(drain(&mut rx)).as_deref(), Some("RU:Level 15"));

    // The picture is read again and the engine now sees "is" instead of "15", unsure of it: the same label.
    *p.io.ocr.0.lock_recover() = ("Level is".into(), 55.0);
    p.tick(&s, true, t0 + Duration::from_millis(300), &tx).await;
    assert_eq!(shown(drain(&mut rx)), None, "nothing is redrawn");
    assert_eq!(tr_n.load(Ordering::SeqCst), 1, "and nothing is sent to the translator");
    assert!(p.states["subtitles"].field_ocr.values().any(|d| d.contains("похожие символы")), "the inspector says why");

    // Sure of the new reading, the engine is believed.
    *p.io.ocr.0.lock_recover() = ("Level is".into(), 91.0);
    p.tick(&s, true, t0 + Duration::from_millis(450), &tx).await;
    assert_eq!(shown(drain(&mut rx)).as_deref(), Some("RU:Level is"));
    assert_eq!(tr_n.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn inplace_fields_are_translated_once_and_unchanged_fields_are_not_resent() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut p = Pipeline::new(cap.clone(), MockOcr(Mutex::new("Hello there".into()), ocr_n.clone()), MockTr(tr_n.clone()));
    let s = { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, false, t0, &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
    let first = match drain(&mut rx).as_slice() {
        [Event::Inplace { frame, .. }] => frame.blocks.clone(),
        events => panic!("expected the field: {events:?}"),
    };
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].translation, "RU:Hello there");
    assert!(!first[0].font.family.is_empty(), "font selected for the field");

    // «Повторить» распознаёт поле заново, но тот же текст не переводится и не пересылается.
    p.tick(&s, true, t0 + Duration::from_millis(300), &tx).await;
    assert!(drain(&mut rx).is_empty());
    assert_eq!((ocr_n.load(Ordering::SeqCst), tr_n.load(Ordering::SeqCst)), (2, 1));

    // Текст сдвинулся: поле найдено заново, перевод взят из кэша.
    *cap.0.lock_recover() = 200;
    p.tick(&s, false, t0 + Duration::from_millis(400), &tx).await;
    p.tick(&s, false, t0 + Duration::from_millis(600), &tx).await;
    assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Inplace { .. })));
    assert_eq!(tr_n.load(Ordering::SeqCst), 1, "no second request to the translator");

    // Неудачное распознавание не стирает поле.
    p.io.ocr.0.lock_recover().clear();
    *cap.0.lock_recover() = 120;
    p.tick(&s, true, t0 + Duration::from_millis(800), &tx).await;
    assert!(drain(&mut rx).iter().all(|e| !matches!(e, Event::Inplace { frame, .. } if frame.blocks.is_empty())));
}

#[tokio::test]
async fn rate_limit_stops_automatic_retries_in_both_display_modes() {
    struct Limited(Arc<AtomicUsize>);
    impl Translate for Limited {
        async fn translate(&self, _: &Settings, _: &str, _: &str, _: &str) -> Result<String, TranslateError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(TranslateError::RateLimited { retry_after: 120 })
        }
    }
    for display in [TranslationDisplayMode::Window, TranslationDisplayMode::Inplace] {
        let count = Arc::new(AtomicUsize::new(0));
        let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))),
            MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), Limited(count.clone()));
        let s = { let mut value = settings(); value.display_mode = display; value };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let now = Instant::now();
        p.tick(&s, true, now, &tx).await;
        assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Error { terminal: true, message, .. } if message.contains("429"))), "{display:?}");
        p.tick(&s, false, now + Duration::from_secs(10), &tx).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "{display:?}: no automatic retry");
    }
}

#[tokio::test]
async fn missing_bergamot_engine_stops_retries_in_both_display_modes() {
    struct MissingEngine(Arc<AtomicUsize>);
    impl Translate for MissingEngine {
        async fn translate(&self, _: &Settings, _: &str, _: &str, _: &str) -> Result<String, TranslateError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(TranslateError::EngineUnavailable("Движок Bergamot отсутствует".into()))
        }
    }
    for display in [TranslationDisplayMode::Window, TranslationDisplayMode::Inplace] {
        let count = Arc::new(AtomicUsize::new(0));
        let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))),
            MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), MissingEngine(count.clone()));
        let mut s = settings();
        s.display_mode = display;
        s.translation.service = crate::settings::TranslationService::Bergamot;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let now = Instant::now();
        p.tick(&s, true, now, &tx).await;
        assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Error { terminal: true, message, .. } if message.contains("Bergamot отсутствует"))));
        p.tick(&s, false, now + Duration::from_secs(10), &tx).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "{display:?}: no automatic retry");
    }
}

// ── Several fields in one scene ──
use crate::layout::testing::{canvas, draw_line};

/// A scene with three separate lines of text (three independent fields).
struct SceneCapture;
impl Capture for SceneCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
    async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> {
        let mut img = canvas(1000, 300, [25, 30, 40]);
        for (i, y) in [40u32, 120, 200].into_iter().enumerate() {
            draw_line(&mut img, 100, y, 20 + i * 4, 14, 4, 22, 3, [240, 240, 240], false);
        }
        Ok(DynamicImage::ImageRgba8(img))
    }
}

/// OCR that takes time and records how many calls were in flight at once.
struct SlowOcr { delay: Duration, active: Arc<AtomicUsize>, peak: Arc<AtomicUsize>, calls: Arc<AtomicUsize>, same_text: bool }
impl Ocr for SlowOcr {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
        let n = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(n, Ordering::SeqCst);
        let k = self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(if self.same_text { "Hello there".into() } else { format!("Line number {k}") })
    }
}
fn slow(delay_ms: u64, same_text: bool) -> (SlowOcr, Arc<AtomicUsize>) {
    let peak = Arc::new(AtomicUsize::new(0));
    (SlowOcr { delay: Duration::from_millis(delay_ms), active: Arc::new(AtomicUsize::new(0)), peak: peak.clone(), calls: Arc::new(AtomicUsize::new(0)), same_text }, peak)
}
fn inplace_settings() -> Settings { { let mut value = settings(); value.display_mode = TranslationDisplayMode::Inplace; value } }

/// A window of 1000×500 with a speaker name at rows 180–202 and two lines of dialogue below; a grab returns the
/// part of the window `rect` asks for, and every rect asked for is recorded.
struct WindowCapture(Arc<Mutex<Vec<NormRect>>>);
impl Capture for WindowCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
    async fn grab(&self, _: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        self.0.lock_recover().push(rect);
        let mut window = canvas(1000, 500, [25, 30, 40]);
        draw_line(&mut window, 100, 180, 6, 14, 4, 22, 3, [240, 220, 120], false);
        for y in [240u32, 274] { draw_line(&mut window, 100, y, 30, 14, 4, 22, 3, [240, 240, 240], false); }
        let (x, y) = ((rect.x * 1000.0).round() as u32, (rect.y * 500.0).round() as u32);
        let (w, h) = ((rect.w * 1000.0).round() as u32, (rect.h * 500.0).round() as u32);
        Ok(DynamicImage::ImageRgba8(image::imageops::crop_imm(&window, x, y, w.min(1000 - x), h.min(500 - y)).to_image()))
    }
}

#[tokio::test]
async fn a_name_cut_by_the_top_edge_of_the_region_is_captured_whole_and_the_overlay_follows() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut p = Pipeline::new(WindowCapture(asked.clone()), SureOcr { text: "Mika".into(), confidence: 90.0 }, MockTr(Arc::new(AtomicUsize::new(0))));
    let mut s = inplace_settings();
    // The region was drawn through the middle of the name.
    let drawn = NormRect { x: 0.0, y: 0.38, w: 1.0, h: 0.62 };
    s.capture.region = Some(drawn);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&s, true, t0, &tx).await;
    let reach = p.states.values().next().unwrap().reach_up;
    assert!(reach > 0.02 && reach <= MAX_REACH_UP, "the capture is to start higher: {reach}");
    // The next tick reads at once, from higher up.
    p.tick(&s, false, t0 + Duration::from_millis(10), &tx).await;
    let last = *asked.lock_recover().last().unwrap();
    assert!(last.y < 180.0 / 500.0 && (last.y + last.h - 1.0).abs() < 1e-9, "the name is inside the capture now: {last:?}");
    let frames: Vec<Box<crate::layout::engine::InplaceFrame>> = drain(&mut rx).into_iter().filter_map(|e| match e { Event::Inplace { frame, .. } => Some(frame), _ => None }).collect();
    let frame = frames.last().expect("fields");
    assert_eq!(frame.capture, Some(last), "the overlay is placed by what was captured");
    let rows_above = (drawn.y - last.y) * frame.frame.1 as f64 / last.h;
    assert!(frame.blocks.iter().any(|b| (b.text_rect.y as f64) < rows_above && b.text_rect.y > EDGE_REACH), "the whole name is a field: {:?}",
        frame.blocks.iter().map(|b| b.text_rect).collect::<Vec<_>>());
    // Off: the drawn region is captured as it is.
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut p = Pipeline::new(WindowCapture(asked.clone()), SureOcr { text: "Mika".into(), confidence: 90.0 }, MockTr(Arc::new(AtomicUsize::new(0))));
    s.appearance.inplace.extend_up = false;
    p.tick(&s, true, t0, &tx).await;
    p.tick(&s, true, t0 + Duration::from_millis(10), &tx).await;
    assert!(asked.lock_recover().iter().all(|r| *r == drawn));
    assert!(drain(&mut rx).iter().all(|e| !matches!(e, Event::Inplace { frame, .. } if frame.capture.is_some())));
}

#[tokio::test]
async fn bounded_runs_everything_with_a_limit() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let done = bounded((0..10u64).collect(), 3, |n| {
        let (active, peak) = (active.clone(), peak.clone());
        async move {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10 - n)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            n
        }
    }).await;
    let mut sorted = done.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..10).collect::<Vec<_>>(), "every item is processed exactly once");
    assert_eq!(peak.load(Ordering::SeqCst), 3);
    assert_ne!(done, sorted, "results arrive in completion order");
    assert!(bounded(Vec::<u8>::new(), 3, |_| async { 0u8 }).await.is_empty());
}

#[tokio::test]
async fn fields_of_one_scene_are_recognised_concurrently_but_bounded() {
    let (ocr, peak) = slow(40, false);
    let tr = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(Arc::new(SceneCapture), ocr, MockTr(tr.clone()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&inplace_settings(), false, t0, &tx).await;
    p.tick(&inplace_settings(), false, t0 + Duration::from_millis(150), &tx).await;
    let frame = drain(&mut rx).into_iter().find_map(|e| match e { Event::Inplace { frame, .. } => Some(frame), _ => None }).expect("fields");
    assert_eq!(frame.blocks.len(), 3, "three fields translated");
    assert!(frame.blocks.iter().all(|b| b.translation.starts_with("RU:Line number")));
    // Concurrency is measured by the number of readings in flight at once, not by the clock: the tick also lays the
    // fields out, and how long that takes depends on the machine's load.
    let peak = peak.load(Ordering::SeqCst);
    assert!((2..=FIELD_CONCURRENCY).contains(&peak), "peak concurrency {peak}");
}

#[tokio::test]
async fn identical_texts_in_one_scene_are_translated_once() {
    let (ocr, _) = slow(5, true);
    let tr = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(Arc::new(SceneCapture), ocr, MockTr(tr.clone()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&inplace_settings(), false, t0, &tx).await;
    p.tick(&inplace_settings(), false, t0 + Duration::from_millis(150), &tx).await;
    let frame = drain(&mut rx).into_iter().find_map(|e| match e { Event::Inplace { frame, .. } => Some(frame), _ => None }).expect("fields");
    assert_eq!(frame.blocks.len(), 3);
    assert_eq!(tr.load(Ordering::SeqCst), 1, "one request for three identical originals");
}

#[tokio::test]
async fn a_cancelled_tick_keeps_the_engine_and_its_locked_fonts() {
    let (ocr, _) = slow(10, false);
    let mut p = Pipeline::new(Arc::new(SceneCapture), ocr, MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, _rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&inplace_settings(), false, t0, &tx).await;
    p.tick(&inplace_settings(), false, t0 + Duration::from_millis(150), &tx).await;
    let fonts = |p: &Pipeline<_, _, _>| -> Vec<(u64, String)> {
        let engine = p.states["subtitles"].inplace.as_ref().expect("engine kept in the region state");
        engine.tracker().blocks().iter().filter_map(|b| b.font.as_ref().map(|f| (b.id, f.family.clone()))).collect()
    };
    let before = fonts(&p);
    assert_eq!(before.len(), 3, "fonts locked after the first scan");
    // A forced scan is cancelled while OCR is running (as `run` does on a new command).
    p.io.ocr.delay = Duration::from_secs(5);
    let cancelled = tokio::time::timeout(Duration::from_millis(60), p.tick(&inplace_settings(), true, t0 + Duration::from_millis(400), &tx)).await;
    assert!(cancelled.is_err(), "the tick was cancelled");
    assert_eq!(fonts(&p), before, "the engine and its font decisions survive a cancelled tick");
}

#[tokio::test]
async fn a_field_whose_recognition_failed_is_recognised_again() {
    struct Flaky { fail: std::sync::atomic::AtomicBool, calls: Arc<AtomicUsize> }
    impl Ocr for Flaky {
        async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
            let k = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) { Err(OcrError::Failed("boom".into())) } else { Ok(format!("Text {k}")) }
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let mut p = Pipeline::new(Arc::new(SceneCapture), Flaky { fail: false.into(), calls: calls.clone() }, MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = Instant::now();
    p.tick(&inplace_settings(), false, t0, &tx).await;
    p.tick(&inplace_settings(), false, t0 + Duration::from_millis(150), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    drain(&mut rx);
    // A forced rescan fails: the error is reported, the shown fields stay.
    p.io.ocr.fail.store(true, Ordering::SeqCst);
    p.tick(&inplace_settings(), true, t0 + Duration::from_millis(300), &tx).await;
    assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Error { message, .. } if message.contains("boom"))));
    // The failed fields are due again, not forgotten.
    p.io.ocr.fail.store(false, Ordering::SeqCst);
    let before = calls.load(Ordering::SeqCst);
    p.tick(&inplace_settings(), true, t0 + Duration::from_secs(10), &tx).await;
    assert_eq!(calls.load(Ordering::SeqCst) - before, 3, "all three fields recognised again");
}

/// Reads "Hello there" unless told to fail.
struct Switch(std::sync::atomic::AtomicBool);
impl Ocr for Switch {
    async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
        if self.0.load(Ordering::SeqCst) { Err(OcrError::Failed("boom".into())) } else { Ok("Hello there".into()) }
    }
}

#[tokio::test]
async fn real_flows_never_leave_the_state_machine_whole_region() {
    let cap = Arc::new(MockCapture(Mutex::new(10)));
    let mut p = Pipeline::new(cap.clone(), Switch(false.into()), MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (s, t0) = (settings(), Instant::now());
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    // A normal cycle: the picture appears, settles, is read and translated; then nothing changes.
    p.tick(&s, false, at(0), &tx).await;
    p.tick(&s, false, at(50), &tx).await;
    p.tick(&s, false, at(150), &tx).await;
    p.tick(&s, false, at(400), &tx).await;
    // The picture changes again, settles and is read again.
    *cap.0.lock_recover() = 200;
    p.tick(&s, false, at(600), &tx).await;
    p.tick(&s, false, at(800), &tx).await;
    // The engine starts failing: a forced read fails, retries are scheduled and finally given up.
    p.io.ocr.0.store(true, Ordering::SeqCst);
    p.tick(&s, true, at(1_000), &tx).await;
    p.tick(&s, false, at(1_100), &tx).await;
    p.tick(&s, false, at(4_000), &tx).await;
    p.tick(&s, false, at(70_000), &tx).await;
    assert_eq!(p.states["subtitles"].phase, Phase::Halted);
    p.tick(&s, false, at(71_000), &tx).await;
    // A manual retry after the engine recovered.
    p.io.ocr.0.store(false, Ordering::SeqCst);
    p.tick(&s, true, at(72_000), &tx).await;
    assert_eq!(p.states["subtitles"].phase, Phase::Showing);
    drain(&mut rx);
    assert_eq!(p.illegal_transitions(), 0, "the whole-region flow stays inside the state machine");
}

#[tokio::test]
async fn real_flows_never_leave_the_state_machine_in_place() {
    let mut p = Pipeline::new(Arc::new(SceneCapture), Switch(false.into()), MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (s, t0) = (inplace_settings(), Instant::now());
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    p.tick(&s, false, at(0), &tx).await;
    p.tick(&s, false, at(150), &tx).await;
    p.tick(&s, false, at(400), &tx).await;
    p.io.ocr.0.store(true, Ordering::SeqCst);
    p.tick(&s, true, at(600), &tx).await;
    p.tick(&s, false, at(80_000), &tx).await;
    p.io.ocr.0.store(false, Ordering::SeqCst);
    p.tick(&s, true, at(81_000), &tx).await;
    drain(&mut rx);
    assert_eq!(p.illegal_transitions(), 0, "the in-place flow stays inside the state machine");
}

#[tokio::test]
async fn no_window_reports_status() {
    let cap = Arc::new(MockCapture(Mutex::new(0)));
    let mut p = Pipeline::new(cap, MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
    let (tx, mut rx) = mpsc::unbounded_channel();
    p.tick(&Settings::default(), false, Instant::now(), &tx).await;
    assert!(matches!(rx.try_recv(), Ok(Event::Status(_))));
}
