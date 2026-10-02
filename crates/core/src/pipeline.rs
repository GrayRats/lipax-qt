//! Independent region state, bounded retries and a 60-second operation deadline.
use crate::{cache::TranslationCache, layout::engine::{InplaceEngine, InplaceFrame}, capture::Capture, detect::ChangeDetector, ocr::Ocr,
    settings::{Settings, RegionProfile}, tesseract::primary_lang, text, translate::{Translate, tess_to_iso}};
use std::{collections::HashMap, time::{Duration, Instant}};
use tokio::{sync::{mpsc, watch}, time::MissedTickBehavior};
const SAME_TEXT_RATIO: f32 = 0.92;
const ERROR_BUDGET: Duration = Duration::from_secs(60);
/// Под непрерывно меняющимся текстом (подвижный фон, анимация) OCR всё равно запускается с этим интервалом.
const MAX_UNSTABLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Status(String),
    Translation { region_id: String, region_name: String, original: String, text: String },
    Error { region_id: String, message: String, terminal: bool },
    /// Режим «поверх оригинала»: поля области изменились (неизменное не пересылается).
    Inplace { region_id: String, region_name: String, frame: Box<InplaceFrame> },
    /// A failing region works again without producing a new translation.
    Cleared { region_id: String },
}

#[derive(Default)]
struct RegionState {
    detector: ChangeDetector,
    last_text: String,
    dirty_since: Option<Instant>,
    /// Первое изменение после последнего OCR.
    dirty_first: Option<Instant>,
    last_attempt: Option<Instant>,
    retry_at: Option<Instant>,
    first_error: Option<Instant>,
    failures: u32,
    halted: bool,
    reason: String,
    /// Движок «поверх оригинала»: поля, их шрифты и фон живут, пока жива область.
    inplace: Option<InplaceEngine>,
}
impl RegionState {
    fn fail(&mut self, region: &RegionProfile, reason: String, now: Instant, timeout: bool, out: &mpsc::UnboundedSender<Event>) {
        let first = *self.first_error.get_or_insert(now);
        self.failures += 1;
        self.reason = reason;
        self.halted = timeout || now.saturating_duration_since(first) >= ERROR_BUDGET;
        let delay = Duration::from_secs((1u64 << self.failures.min(5)).min(30));
        self.retry_at = (!self.halted).then_some((now + delay).min(first + ERROR_BUDGET));
        let message = if self.halted {
            format!("{} ({}). Автоповтор остановлен — нажмите «Повторить»", self.reason, region.name)
        } else { format!("{} ({}). Повтор через {} с", self.reason, region.name, delay.as_secs()) };
        let _ = out.send(Event::Error { region_id: region.id.clone(), message, terminal: self.halted });
    }
    fn settle(&mut self, region: &RegionProfile, out: &mpsc::UnboundedSender<Event>) {
        if self.failures > 0 { let _ = out.send(Event::Cleared { region_id: region.id.clone() }); }
        self.recovered();
    }
    fn recovered(&mut self) { self.failures = 0; self.first_error = None; self.retry_at = None; self.halted = false; self.reason.clear(); }
}

pub struct Pipeline<C, O, T> {
    capture: C, ocr: O, translator: T,
    states: HashMap<String, RegionState>,
    cache: TranslationCache,
    /// Last stage status sent; repeats are suppressed so idle ticks stay silent.
    last_status: String,
}
impl<C: Capture, O: Ocr, T: Translate> Pipeline<C, O, T> {
    pub fn new(capture: C, ocr: O, translator: T) -> Self {
        Self { capture, ocr, translator, states: HashMap::new(), cache: TranslationCache::new(512), last_status: String::new() }
    }
    pub fn reset(&mut self) { self.states.clear(); self.last_status.clear(); }

    fn status(&mut self, text: &str, out: &mpsc::UnboundedSender<Event>) {
        if self.last_status != text {
            self.last_status = text.to_string();
            let _ = out.send(Event::Status(text.to_string()));
        }
    }

    pub async fn tick(&mut self, s: &Settings, force: bool, now: Instant, out: &mpsc::UnboundedSender<Event>) {
        let regions = s.capture_regions();
        if s.window.is_none() || regions.is_empty() { self.status("Выберите окно и включите область", out); return; }
        self.states.retain(|id, _| regions.iter().any(|r| &r.id == id));
        for region in regions {
            let mut state = self.states.remove(&region.id).unwrap_or_default();
            if force { state.recovered(); }
            let mut effective = s.clone();
            effective.region = region.rect;
            effective.debounce_ms = region.debounce_ms;
            for (value, local) in [(&mut effective.source_lang, &region.source_lang), (&mut effective.target_lang, &region.target_lang), (&mut effective.ocr_engine, &region.ocr_engine)] {
                if !local.is_empty() { *value = local.clone(); }
            }
            self.tick_region(&effective, &region, &mut state, force, now, out).await;
            self.states.insert(region.id, state);
        }
    }

    async fn tick_region(&mut self, s: &Settings, region: &RegionProfile, state: &mut RegionState, force: bool, now: Instant, out: &mpsc::UnboundedSender<Event>) {
        let inplace = s.translation_display == "inplace";
        if !inplace { state.inplace = None; }
        // Поменяли оформление «поверх оригинала»: перестроить без нового кадра и OCR.
        if let Some(frame) = state.inplace.as_mut().and_then(|e| e.restyle(s)) {
            let _ = out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
        }
        if !force {
            if state.halted { return; }
            if let Some(first) = state.first_error
                && now.saturating_duration_since(first) >= ERROR_BUDGET {
                    state.fail(region, state.reason.clone(), now, true, out); return;
                }
            if state.retry_at.is_some_and(|t| now < t) { return; }
            if state.retry_at.is_none() && state.last_attempt.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(region.interval_ms)) { return; }
        }
        state.last_attempt = Some(now);
        let retry = state.retry_at.is_some();
        let budget = state.first_error.map(|first| ERROR_BUDGET.saturating_sub(now.saturating_duration_since(first))).unwrap_or(ERROR_BUDGET);
        let deadline = tokio::time::Instant::now() + budget;
        let started = Instant::now();
        macro_rules! stage {
            ($future:expr, $name:expr) => {
                match tokio::time::timeout_at(deadline, $future).await {
                    Ok(Ok(value)) => value,
                    Ok(Err(e)) => { state.fail(region, format!("{}: {e}", $name), now + started.elapsed(), false, out); return; },
                    Err(_) => { state.fail(region, format!("{}: нет ответа за 60 с", $name), now + started.elapsed(), true, out); return; },
                }
            }
        }
        let frame = stage!(self.capture.grab(s.window.as_ref().unwrap(), region.rect.unwrap()), "Ошибка захвата");
        if !force && !retry {
            let changed = state.detector.changed(&frame, s.sensitivity, now);
            if changed {
                state.dirty_since = Some(now);
                state.dirty_first.get_or_insert(now);
            }
            let settled = !changed && state.dirty_since.is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_millis(s.debounce_ms));
            let overdue = state.dirty_first.is_some_and(|t| now.saturating_duration_since(t) >= MAX_UNSTABLE);
            if !settled && !overdue {
                if self.last_status.is_empty() { self.status("Ожидание текста", out); }
                return;
            }
            state.dirty_since = None;
            state.dirty_first = None;
            // В кадре нет ничего похожего на текст: OCR не запускаем. Поля «поверх оригинала»
            // всё равно обновляются: пропавший текст должен погаснуть.
            if !state.detector.has_text() && !inplace {
                self.status("Ожидание текста", out); return;
            }
        }
        if inplace {
            self.inplace_tick(s, region, state, &frame, force, now, out).await;
            return;
        }
        self.status("Распознавание окна…", out);
        let raw = stage!(self.ocr.recognize(&frame, s), "Ошибка OCR");
        let original = text::normalize(&raw);
        if !text::is_meaningful(&original) { state.settle(region, out); self.status("OCR: текст не обнаружен", out); return; }
        if !force && text::similarity(&original, &state.last_text) >= SAME_TEXT_RATIO { state.settle(region, out); self.status("Ожидание текста", out); return; }
        let src = tess_to_iso(primary_lang(&s.source_lang));
        let translated = match self.cache.get(src, &s.target_lang, &original) {
            Some(t) => t,
            None => {
                self.status("Перевод…", out);
                let t = stage!(self.translator.translate(s, &original, src, &s.target_lang), "Ошибка перевода");
                self.cache.put(src, &s.target_lang, &original, t.clone()); t
            }
        };
        state.last_text = original.clone();
        state.recovered();
        self.last_status = "Перевод обновлён".into();
        let _ = out.send(Event::Translation { region_id: region.id.clone(), region_name: region.name.clone(), original, text: translated });
    }

    /// «Поверх оригинала»: каждое поле распознаётся и переводится отдельно, и только если его
    /// содержимое изменилось. Движок живёт в состоянии области и не теряется при ошибке.
    #[allow(clippy::too_many_arguments)]
    async fn inplace_tick(&mut self, s: &Settings, region: &RegionProfile, state: &mut RegionState, frame: &image::DynamicImage,
                          force: bool, now: Instant, out: &mpsc::UnboundedSender<Event>) {
        let mut engine = state.inplace.take().unwrap_or_default();
        let jobs = engine.begin(frame, s, now, force);
        let budget = state.first_error.map(|first| ERROR_BUDGET.saturating_sub(now.saturating_duration_since(first))).unwrap_or(ERROR_BUDGET);
        let deadline = tokio::time::Instant::now() + budget;
        let src = tess_to_iso(primary_lang(&s.source_lang)).to_string();
        let mut failure = None;
        for job in jobs {
            self.status("Распознавание окна…", out);
            let raw = match tokio::time::timeout_at(deadline, self.ocr.recognize(&job.image, s)).await {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => { failure = Some((format!("Ошибка OCR: {e}"), false)); break; }
                Err(_) => { failure = Some(("Ошибка OCR: нет ответа за 60 с".into(), true)); break; }
            };
            let original = text::normalize(&raw);
            if !text::is_meaningful(&original) { engine.complete(job.id, None, s); continue; }
            let translated = match engine.cached_translation(job.id, &original).or_else(|| self.cache.get(&src, &s.target_lang, &original)) {
                Some(t) => t,
                None => {
                    self.status("Перевод…", out);
                    match tokio::time::timeout_at(deadline, self.translator.translate(s, &original, &src, &s.target_lang)).await {
                        Ok(Ok(t)) => { self.cache.put(&src, &s.target_lang, &original, t.clone()); t }
                        Ok(Err(e)) => { failure = Some((format!("Ошибка перевода: {e}"), false)); break; }
                        Err(_) => { failure = Some(("Ошибка перевода: нет ответа за 60 с".into(), true)); break; }
                    }
                }
            };
            engine.complete(job.id, Some((original, translated)), s);
        }
        let result = engine.finish(s);
        state.inplace = Some(engine);
        if let Some(frame) = result {
            self.last_status = "Перевод обновлён".into();
            let _ = out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
        }
        match failure {
            Some((reason, timeout)) => state.fail(region, reason, now, timeout, out),
            None => state.settle(region, out),
        }
    }

    /// Пользователь попросил определить шрифты полей заново.
    pub fn reanalyze_fonts(&mut self) {
        for state in self.states.values_mut() {
            if let Some(e) = state.inplace.as_mut() { e.reanalyze_fonts(); }
        }
    }

    pub async fn run(mut self, settings: watch::Receiver<Settings>, mut running: watch::Receiver<bool>, mut cmds: mpsc::UnboundedReceiver<Cmd>, out: mpsc::UnboundedSender<Event>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut pending = None;
        loop {
            let command = if let Some(c) = pending.take() { c } else {
                tokio::select! {
                    biased;
                    c = cmds.recv() => match c { Some(c) => c, None => break },
                    _ = ticker.tick() => { if !*running.borrow() || !settings.borrow().auto_translate { continue; } Cmd::Auto },
                }
            };
            if command == Cmd::Reset { self.reset(); continue; }
            if command == Cmd::ReanalyzeFonts { self.reanalyze_fonts(); continue; }
            let s = settings.borrow().clone();
            let _ = running.borrow_and_update();
            tokio::select! {
                biased;
                c = cmds.recv() => { match c { Some(c) => pending = Some(c), None => break } },
                changed = running.changed() => { if changed.is_err() { break; } },
                _ = self.tick(&s, command == Cmd::TranslateOnce, Instant::now(), &out) => {},
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd { Reset, TranslateOnce, Auto, ReanalyzeFonts }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::CaptureError;
    use crate::ocr::OcrError;
    use crate::settings::{NormRect, WindowKey};
    use crate::translate::TranslateError;
    use image::{DynamicImage, Rgba, RgbaImage};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockCapture(Mutex<u8>);
    impl Capture for MockCapture {
        async fn grab(&self, _: &WindowKey, _: NormRect) -> Result<DynamicImage, CaptureError> {
            // Светлое «слово» на тёмном фоне; значение сдвигает его, меняя текст в кадре.
            let x0 = *self.0.lock().unwrap() as u32 / 4;
            let mut img = RgbaImage::from_pixel(100, 50, Rgba([20, 20, 20, 255]));
            for y in 20..30 { for x in x0..x0 + 20 { img.put_pixel(x, y, Rgba([250, 250, 250, 255])); } }
            Ok(DynamicImage::ImageRgba8(img))
        }
    }
    struct MockOcr(Mutex<String>, Arc<AtomicUsize>);
    impl Ocr for MockOcr {
        async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> {
            self.1.fetch_add(1, Ordering::SeqCst);
            Ok(self.0.lock().unwrap().clone())
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
        Settings {
            window: Some(WindowKey { uuid: "u".into(), resource_class: "g".into(), caption: "g".into() }),
            region: Some(NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }),
            debounce_ms: 100,
            interval_ms: 50,
            ..Settings::default()
        }
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
        *cap.0.lock().unwrap() = 200; // визуальное изменение, текст тот же
        p.tick(&s, false, t0 + Duration::from_millis(200), &tx).await;
        p.tick(&s, false, t0 + Duration::from_millis(400), &tx).await;
        assert_eq!(ocr_n.load(Ordering::SeqCst), 2);
        assert_eq!(tr_n.load(Ordering::SeqCst), 1);
        assert_eq!(drain(&mut rx).len(), 1);
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
        *p.ocr.0.lock().unwrap() = "Totally different".into();
        *cap.0.lock().unwrap() = 200;
        p.tick(&s, false, step(200), &tx).await;
        p.tick(&s, false, step(400), &tx).await;
        *p.ocr.0.lock().unwrap() = "First line".into();
        *cap.0.lock().unwrap() = 20;
        p.tick(&s, false, step(450), &tx).await;
        p.tick(&s, false, step(650), &tx).await;
        assert_eq!(tr_n.load(Ordering::SeqCst), 2, "третий текст берётся из кэша");
    }

    #[tokio::test]
    async fn overlay_mode_sends_one_translation_per_region() {
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap, MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let t0 = Instant::now();
        p.tick(&settings(), false, t0, &tx).await;
        p.tick(&settings(), false, t0 + Duration::from_millis(150), &tx).await;
        assert!(matches!(drain(&mut rx).as_slice(), [Event::Translation { .. }]));
    }

    #[tokio::test]
    async fn inplace_fields_are_translated_once_and_unchanged_fields_are_not_resent() {
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let mut p = Pipeline::new(cap.clone(), MockOcr(Mutex::new("Hello there".into()), ocr_n.clone()), MockTr(tr_n.clone()));
        let s = Settings { translation_display: "inplace".into(), ..settings() };
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
        *cap.0.lock().unwrap() = 200;
        p.tick(&s, false, t0 + Duration::from_millis(400), &tx).await;
        p.tick(&s, false, t0 + Duration::from_millis(600), &tx).await;
        assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Inplace { .. })));
        assert_eq!(tr_n.load(Ordering::SeqCst), 1, "no second request to the translator");

        // Неудачное распознавание не стирает поле.
        p.ocr.0.lock().unwrap().clear();
        *cap.0.lock().unwrap() = 120;
        p.tick(&s, true, t0 + Duration::from_millis(800), &tx).await;
        assert!(drain(&mut rx).iter().all(|e| !matches!(e, Event::Inplace { frame, .. } if frame.blocks.is_empty())));
    }

    #[tokio::test]
    async fn no_window_reports_status() {
        let cap = Arc::new(MockCapture(Mutex::new(0)));
        let mut p = Pipeline::new(cap, MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
        let (tx, mut rx) = mpsc::unbounded_channel();
        p.tick(&Settings::default(), false, Instant::now(), &tx).await;
        assert!(matches!(rx.try_recv(), Ok(Event::Status(_))));
    }
}
