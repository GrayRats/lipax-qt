//! Independent region state, bounded retries and a 60-second operation deadline.
use crate::{cache::TranslationCache, layout::engine::{InplaceEngine, InplaceFrame}, capture::Capture, detect::ChangeDetector, ocr::Ocr,
    settings::{Settings, RegionProfile, TranslationDisplay}, tesseract::primary_lang, text, translate::{Translate, TranslateError, tess_to_iso}};
use futures_util::{FutureExt, StreamExt, stream::FuturesUnordered};
use std::{collections::HashMap, sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};
use tokio::{sync::{mpsc, watch}, time::MissedTickBehavior};
const SAME_TEXT_RATIO: f32 = 0.92;
/// Сколько полей одной сцены распознаются/переводятся одновременно: достаточно для имени, реплики
/// и пары кнопок и не заваливает Tesseract процессами и API перевода запросами.
const FIELD_CONCURRENCY: usize = 3;

/// Выполняет `work` для каждого элемента, держа в полёте не больше `limit` будущих; результаты —
/// в порядке завершения. Написано вручную поверх `FuturesUnordered`: связка `buffer_unordered` с
/// замыканием, захватывающим ссылки, ломает вывод `Send` для будущего, которое уходит в `spawn`.
async fn bounded<I, Fut, R>(items: Vec<I>, limit: usize, mut work: impl FnMut(I) -> Fut) -> Vec<R>
where Fut: std::future::Future<Output = R> {
    let mut items = items.into_iter();
    let mut running = FuturesUnordered::new();
    let mut done = Vec::new();
    loop {
        while running.len() < limit.max(1) {
            match items.next() { Some(item) => running.push(work(item)), None => break }
        }
        match running.next().await { Some(result) => done.push(result), None => break }
    }
    done
}
const ERROR_BUDGET: Duration = Duration::from_secs(60);
/// Под непрерывно меняющимся текстом (подвижный фон, анимация) OCR всё равно запускается с этим интервалом.
const MAX_UNSTABLE: Duration = Duration::from_secs(2);
/// How long the inspector keeps showing a field whose text was ignored as unsure.
const IGNORED_SHOWN: Duration = Duration::from_secs(3);

/// Where the pipeline sends its events: a plain channel (tests) or a generation-stamped one (`run`).
pub trait Sink: Send + Sync {
    fn send(&self, event: Event);
}

impl Sink for mpsc::UnboundedSender<Event> {
    fn send(&self, event: Event) { let _ = mpsc::UnboundedSender::send(self, event); }
}

struct Stamped<'a> {
    tx: &'a mpsc::UnboundedSender<(u64, Event)>,
    generation: u64,
}

impl Sink for Stamped<'_> {
    fn send(&self, event: Event) { let _ = self.tx.send((self.generation, event)); }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Status(String),
    Translation { region_id: String, region_name: String, original: String, text: String },
    Error { region_id: String, message: String, terminal: bool },
    /// Режим «поверх оригинала»: поля области изменились (неизменное не пересылается).
    Inplace { region_id: String, region_name: String, frame: Box<InplaceFrame> },
    /// A failing region works again without producing a new translation.
    Cleared { region_id: String },
    /// What the last OCR run saw and found; sent only while the preview window is open.
    OcrPreview { region_id: String, region_name: String, preview: Box<OcrPreview> },
}

/// One recognized block (or the whole frame in the translation-window mode), px of the region frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewBox {
    pub rect: crate::layout::Rect,
    pub original: String,
    pub translation: String,
    /// What the layout engine decided for this block and why (type, font and its confidence, background);
    /// empty in the translation-window mode, which does not analyse blocks.
    pub details: Vec<String>,
}

/// Where a region is in its cycle. Every change is logged (`pipeline.phase`), so a stuck region
/// or a surprising transition can be read from the log; the inspector shows the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Nothing changed on screen (also: no text found).
    #[default]
    WaitingFrame,
    /// The picture changed; waiting until it stays still.
    Debouncing,
    Recognizing,
    Translating,
    /// The translation is on screen.
    Showing,
    /// An error; the next attempt is scheduled.
    RetryWait,
    /// Automatic retries stopped (budget spent or rate limit); a manual retry is needed.
    Halted,
}

impl Phase {
    /// Every phase, for tables and tests.
    pub const ALL: [Phase; 7] = [Self::WaitingFrame, Self::Debouncing, Self::Recognizing, Self::Translating, Self::Showing, Self::RetryWait, Self::Halted];

    /// The state machine of a region. A cycle goes
    /// `WaitingFrame → Debouncing → Recognizing → Translating → Showing`, and from `Showing` back to
    /// waiting or to the next change. Reading and translating cannot be skipped into: nothing goes
    /// to `Translating` except from `Recognizing`, and `Recognizing` is entered from a quiet or a
    /// stable state, never from a failed one except by a retry. Any phase can fail into
    /// `RetryWait` (a delay is scheduled) or `Halted` (retries are over, a manual retry is needed);
    /// those two lead back only through `Recognizing` (the retry) or by the picture settling.
    pub fn allows(self, to: Phase) -> bool {
        use Phase::*;
        if self == to || matches!(to, RetryWait | Halted) { return true; }
        match self {
            WaitingFrame => matches!(to, Debouncing | Recognizing | Showing),
            Debouncing => matches!(to, WaitingFrame | Recognizing | Showing),
            Recognizing => matches!(to, Translating | Showing | WaitingFrame),
            Translating => matches!(to, Showing | WaitingFrame),
            Showing => matches!(to, WaitingFrame | Debouncing | Recognizing),
            RetryWait => matches!(to, WaitingFrame | Debouncing | Recognizing | Showing),
            // Only a manual retry leaves it, and that reads at once (no waiting for the picture to settle).
            Halted => matches!(to, WaitingFrame | Recognizing | Showing),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingFrame => "ожидание кадра",
            Self::Debouncing => "ждёт, пока текст остановится",
            Self::Recognizing => "распознавание",
            Self::Translating => "перевод",
            Self::Showing => "перевод показан",
            Self::RetryWait => "повтор после ошибки",
            Self::Halted => "автоповтор остановлен",
        }
    }
}

/// Duration of one pipeline stage over the last samples, ms.
#[derive(Debug, Clone, PartialEq)]
pub struct StageTiming {
    pub stage: &'static str,
    pub last_ms: f32,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub samples: usize,
}

/// Rolling durations per stage. Recording costs one `Instant` read per stage, so it is always on;
/// the summary is built only for the inspector.
#[derive(Default)]
struct Timings {
    stages: HashMap<&'static str, std::collections::VecDeque<f32>>,
}

impl Timings {
    const KEEP: usize = 128;
    /// In the order of the pipeline.
    const ORDER: [&'static str; 5] = ["capture", "detect", "ocr", "layout", "translate"];

    fn record(&mut self, stage: &'static str, took: Duration) {
        let samples = self.stages.entry(stage).or_default();
        if samples.len() == Self::KEEP { samples.pop_front(); }
        samples.push_back(took.as_secs_f32() * 1000.0);
    }

    fn summary(&self) -> Vec<StageTiming> {
        Self::ORDER.iter().filter_map(|stage| {
            let samples = self.stages.get(stage).filter(|s| !s.is_empty())?;
            let mut sorted: Vec<f32> = samples.iter().copied().collect();
            sorted.sort_by(f32::total_cmp);
            let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q).round() as usize];
            Some(StageTiming { stage, last_ms: *samples.back()?, p50_ms: at(0.5), p95_ms: at(0.95), samples: sorted.len() })
        }).collect()
    }
}

/// `Some(confidence)` if the engine is less sure than the user allows: the text is not used.
fn unsure(confidence: Option<f32>, min_confidence: u32) -> Option<f32> {
    confidence.filter(|c| min_confidence > 0 && *c < min_confidence as f32)
}

/// One preview box per line the engine found; `origin` moves the crop to frame coordinates.
fn line_boxes(result: &crate::ocr::OcrResult, origin: (u32, u32), note: Option<&str>) -> Vec<PreviewBox> {
    result.lines.iter().map(|l| {
        let mut details = vec![format!("уверенность OCR ({}): {:.0}%", result.engine, l.confidence)];
        details.extend(note.map(str::to_owned));
        PreviewBox { rect: l.rect.in_frame(origin), original: l.text.clone(), translation: String::new(), details }
    }).collect()
}

/// The first error and the other distinct ones of the same scene, at most three, in one message.
fn join_messages(first: &str, notes: &[String]) -> String {
    let mut all: Vec<&str> = vec![first];
    all.extend(notes.iter().map(String::as_str).filter(|n| *n != first));
    let more = all.len().saturating_sub(3);
    all.truncate(3);
    let mut text = all.join(" · ");
    if more > 0 { text.push_str(&format!(" · и ещё {more}")); }
    text
}

/// Which service and which phrase an error is about: every waiter of a shared request gets its own message.
fn translation_context(s: &Settings, text: &str) -> String {
    let service = match s.translator {
        crate::settings::TranslatorKind::Google => "Google Translate",
        crate::settings::TranslatorKind::Yandex => "Yandex Translate",
        crate::settings::TranslatorKind::Custom => "свой API",
    };
    let mut excerpt: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect();
    if text.chars().count() > 40 { excerpt.push('…'); }
    format!("{service}, «{excerpt}»")
}

/// What the layout engine decided for a field, for the inspector.
fn describe(block: &crate::layout::engine::InplaceBlock) -> Vec<String> {
    let font = &block.font;
    let mut lines = vec![format!("тип блока: {:?}", block.block_type)];
    lines.push(if font.generic {
        format!("шрифт: подходящего нет, общий «{}» — поле не рисуется", font.family)
    } else {
        format!("шрифт: {} ({:?}), уверенность {:.2}", font.family, font.category, font.confidence)
    });
    if let Some(narrow) = &font.condensed_family { lines.push(format!("узкий вариант для длинного перевода: {narrow}")); }
    lines.push(format!("высота прописных оригинала: {:.0} px кадра", block.style.cap_height_px));
    if !block.lines_note.is_empty() { lines.push(block.lines_note.clone()); }
    lines.push(format!("фон: {:?}", block.background.mode));
    lines
}

/// OCR input and result of one run: the frame, the blocks found in it and the texts.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrPreview {
    pub image: image::RgbaImage,
    pub boxes: Vec<PreviewBox>,
    pub original: String,
    pub translation: String,
    pub phase: Phase,
    pub timings: Vec<StageTiming>,
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
    phase: Phase,
    /// Transitions the state machine does not allow (always 0 unless the pipeline has a bug).
    illegal: u32,
    /// `auto` engine: fields for which Tesseract was not enough read straight with PaddleOCR from
    /// then on (the second engine costs time only once, not on every change of the text).
    field_engine: HashMap<u64, &'static str>,
    /// Inspector: what the engine said about each field the last time it was read.
    field_ocr: HashMap<u64, String>,
    /// Inspector: fields whose text was ignored as unsure, with when; shown for a few seconds.
    ignored: HashMap<u64, (Instant, PreviewBox)>,
    /// Inspector: boxes of the last frame sent to the screen.
    preview_boxes: Vec<PreviewBox>,
}
impl RegionState {
    fn enter(&mut self, region_id: &str, phase: Phase) {
        if self.phase == phase { return; }
        if self.phase.allows(phase) {
            tracing::debug!(target: "pipeline.phase", region = region_id, from = ?self.phase, to = ?phase, "region phase");
        } else {
            // A transition outside the table is a pipeline bug. Keep the last valid phase so the
            // state machine remains trustworthy, and expose the violation through logs/inspector.
            self.illegal += 1;
            tracing::error!(target: "pipeline.phase", region = region_id, from = ?self.phase, to = ?phase, "illegal region phase transition");
            return;
        }
        self.phase = phase;
    }
    fn fail(&mut self, region: &RegionProfile, reason: String, now: Instant, timeout: bool, out: &dyn Sink) {
        let first = *self.first_error.get_or_insert(now);
        self.failures += 1;
        self.reason = reason;
        self.halted = timeout || now.saturating_duration_since(first) >= ERROR_BUDGET;
        let delay = Duration::from_secs((1u64 << self.failures.min(5)).min(30));
        self.retry_at = (!self.halted).then_some((now + delay).min(first + ERROR_BUDGET));
        self.enter(&region.id, if self.halted { Phase::Halted } else { Phase::RetryWait });
        let message = if self.halted {
            format!("{} ({}). Автоповтор остановлен — нажмите «Повторить»", self.reason, region.name)
        } else { format!("{} ({}). Повтор через {} с", self.reason, region.name, delay.as_secs()) };
        // Log before the channel/Qt queue: the error survives a closed or unavailable GUI.
        tracing::error!(region = %region.id, terminal = self.halted, "{message}");
        out.send(Event::Error { region_id: region.id.clone(), message, terminal: self.halted });
    }
    fn settle(&mut self, region: &RegionProfile, out: &dyn Sink) {
        if self.failures > 0 { out.send(Event::Cleared { region_id: region.id.clone() }); }
        self.recovered();
    }
    fn recovered(&mut self) { self.failures = 0; self.first_error = None; self.retry_at = None; self.halted = false; self.reason.clear(); }
}

/// A translation in flight. Every holder gets the same result; the error is shared too.
type PendingTranslation = futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<String, Arc<TranslateError>>>>;
/// At most this many different phrases are fetched at once; more are fetched without sharing.
const MAX_INFLIGHT: usize = 32;
/// A request that nobody waits for any more is given up after this long.
const ORPHAN_REQUEST_LIMIT: Duration = Duration::from_secs(120);

/// Всё, что тик использует кроме состояния областей. Отдельная структура нужна, чтобы тик мог
/// держать `&mut RegionState` из карты, а не вынимать состояние на время `await`.
struct Io<C, O, T> {
    capture: C, ocr: O,
    /// Shared with the tasks that carry requests: they outlive the tick that started them.
    translator: Arc<T>,
    cache: Arc<std::sync::Mutex<TranslationCache>>,
    /// Translations being fetched right now, by key; see [`Io::translate_shared`].
    inflight: Arc<std::sync::Mutex<HashMap<String, PendingTranslation>>>,
    /// The OCR preview window is open: frames and blocks are copied into `Event::OcrPreview`.
    preview: Arc<AtomicBool>,
    timings: Timings,
    /// Last stage status sent; repeats are suppressed so idle ticks stay silent.
    last_status: String,
}

pub struct Pipeline<C, O, T> {
    io: Io<C, O, T>,
    states: HashMap<String, RegionState>,
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Pipeline<C, O, T> {
    pub fn new(capture: C, ocr: O, translator: T) -> Self {
        Self { io: Io { capture, ocr, translator: Arc::new(translator), cache: Arc::new(std::sync::Mutex::new(TranslationCache::new(512))), inflight: Arc::default(), preview: Arc::default(), timings: Timings::default(), last_status: String::new() }, states: HashMap::new() }
    }
    /// The flag is owned by the caller: switching it costs nothing while the preview is closed.
    pub fn with_preview(mut self, flag: Arc<AtomicBool>) -> Self { self.io.preview = flag; self }
    pub fn reset(&mut self) { self.states.clear(); self.io.last_status.clear(); }

    pub async fn tick(&mut self, s: &Settings, force: bool, now: Instant, out: &dyn Sink) {
        let regions = s.capture_regions();
        let Self { io, states } = self;
        if s.window.is_none() || regions.is_empty() { io.status("Выберите окно и включите область", out); return; }
        states.retain(|id, _| regions.iter().any(|r| &r.id == id));
        for region in regions {
            // The state stays in the map across every `await`: `run` may cancel this future
            // (new command, stop), and a state removed for the duration would be lost with it —
            // including the engine with its locked fonts.
            let state = states.entry(region.id.clone()).or_default();
            if force { state.recovered(); }
            let mut effective = s.clone();
            effective.region = region.rect;
            effective.debounce_ms = region.debounce_ms;
            for (value, local) in [(&mut effective.source_lang, &region.source_lang), (&mut effective.target_lang, &region.target_lang), (&mut effective.ocr_engine, &region.ocr_engine)] {
                if !local.is_empty() { *value = local.clone(); }
            }
            io.tick_region(&effective, &region, state, force, now, out).await;
        }
    }

    /// How many transitions outside the state machine happened over the life of this pipeline.
    pub fn illegal_transitions(&self) -> u32 {
        self.states.values().map(|s| s.illegal).sum()
    }

    /// Пользователь попросил определить шрифты полей заново.
    pub fn reanalyze_fonts(&mut self) {
        for state in self.states.values_mut() {
            if let Some(e) = state.inplace.as_mut() { e.reanalyze_fonts(); }
        }
    }

    /// `generation` is bumped by the owner whenever the processing settings change (window, area,
    /// language, engine, ...). Every event is stamped with the generation read *before* the
    /// settings of its tick, so an event from before a change can never look current: the
    /// receiver drops it. A cancelled tick already stops its own work; this covers the events
    /// that were sent but not yet delivered.
    pub async fn run(mut self, settings: watch::Receiver<Settings>, mut running: watch::Receiver<bool>, mut cmds: mpsc::UnboundedReceiver<Cmd>,
                     generation: Arc<AtomicU64>, out: mpsc::UnboundedSender<(u64, Event)>) {
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
            let stamp = Stamped { tx: &out, generation: generation.load(Ordering::SeqCst) };
            let s = settings.borrow().clone();
            let _ = running.borrow_and_update();
            tokio::select! {
                biased;
                c = cmds.recv() => { match c { Some(c) => pending = Some(c), None => break } },
                changed = running.changed() => { if changed.is_err() { break; } },
                _ = self.tick(&s, command == Cmd::TranslateOnce, Instant::now(), &stamp) => {},
            }
        }
    }
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Io<C, O, T> {
    fn cached(&self, src: &str, dst: &str, text: &str) -> Option<String> {
        self.cache.lock().unwrap().get(src, dst, text)
    }

    /// Translate `text`, joining a request for the same phrase that is already running. The
    /// request is carried by its own task, not by the tick that asked: if that tick is cancelled
    /// (new command, changed settings, a new frame) the answer still arrives, lands in the cache
    /// and serves whoever asks for the phrase next, instead of being thrown away and fetched again.
    /// The same phrase in two places at once (or the same line shown again a moment later) is one
    /// request. The error is shared as well, so a rate limit reaches everyone who waited for it.
    async fn translate_shared(&self, s: &Settings, text: &str, src: &str, dst: &str) -> Result<String, Arc<TranslateError>> {
        // The service is part of the key: another translator may answer differently.
        let service = if s.translator == crate::settings::TranslatorKind::Custom { format!("custom:{}", s.custom_url) } else { format!("{:?}", s.translator) };
        let key = format!("{service}\u{1}{src}\u{1}{dst}\u{1}{text}");
        let pending = {
            let mut running = self.inflight.lock().unwrap();
            if let Some(found) = running.get(&key) { found.clone() } else {
                let (translator, settings, cache) = (self.translator.clone(), s.clone(), self.cache.clone());
                let (text, src, dst) = (text.to_owned(), src.to_owned(), dst.to_owned());
                let request = async move {
                    let result = translator.translate(&settings, &text, &src, &dst).await.map_err(Arc::new);
                    if let Ok(translated) = &result { cache.lock().unwrap().put(&src, &dst, &text, translated.clone()); }
                    result
                }.boxed().shared();
                // Beyond the limit the phrase is still fetched, just not shared or carried on its own.
                if running.len() < MAX_INFLIGHT {
                    running.insert(key.clone(), request.clone());
                    let (driver, inflight) = (request.clone(), self.inflight.clone());
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(ORPHAN_REQUEST_LIMIT, driver).await;
                        inflight.lock().unwrap().remove(&key);
                    });
                }
                request
            }
        };
        pending.await
    }

    fn status(&mut self, text: &str, out: &dyn Sink) {
        if self.last_status != text {
            tracing::debug!(stage = text, "Состояние обработки");
            self.last_status = text.to_string();
            out.send(Event::Status(text.to_string()));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_preview(&self, region: &RegionProfile, frame: &image::DynamicImage, boxes: Vec<PreviewBox>, original: String, translation: String, phase: Phase, out: &dyn Sink) {
        if !self.preview.load(Ordering::Relaxed) { return; }
        let preview = OcrPreview { image: frame.to_rgba8(), boxes, original, translation, phase, timings: self.timings.summary() };
        out.send(Event::OcrPreview { region_id: region.id.clone(), region_name: region.name.clone(), preview: Box::new(preview) });
    }

    async fn tick_region(&mut self, s: &Settings, region: &RegionProfile, state: &mut RegionState, force: bool, now: Instant, out: &dyn Sink) {
        // The backend decides whether the text can be found and drawn over in place; otherwise
        // the region is read as a whole and the translation goes to the translation window.
        let can_inplace = s.window.as_ref().is_some_and(|w| self.capture.capabilities(w, s).inplace_overlay);
        let inplace = s.translation_display == TranslationDisplay::Inplace && can_inplace;
        if !inplace { state.inplace = None; }
        // Поменяли оформление «поверх оригинала»: перестроить без нового кадра и OCR.
        if let Some(frame) = state.inplace.as_mut().and_then(|e| e.restyle(s)) {
            out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
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
        let grabbed = Instant::now();
        let frame = stage!(self.capture.grab(s.window.as_ref().unwrap(), region.rect.unwrap()), "Ошибка захвата");
        self.timings.record("capture", grabbed.elapsed());
        if !force && !retry {
            let detecting = Instant::now();
            let changed = state.detector.changed(&frame, s.sensitivity, now);
            self.timings.record("detect", detecting.elapsed());
            if changed {
                state.dirty_since = Some(now);
                state.dirty_first.get_or_insert(now);
            }
            let settled = !changed && state.dirty_since.is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_millis(s.debounce_ms));
            let overdue = state.dirty_first.is_some_and(|t| now.saturating_duration_since(t) >= MAX_UNSTABLE);
            if !settled && !overdue {
                state.enter(&region.id, if state.dirty_since.is_some() { Phase::Debouncing } else { Phase::WaitingFrame });
                if self.last_status.is_empty() { self.status("Ожидание текста", out); }
                return;
            }
            state.dirty_since = None;
            state.dirty_first = None;
            // В кадре нет ничего похожего на текст: OCR не запускаем. Поля «поверх оригинала»
            // всё равно обновляются: пропавший текст должен погаснуть.
            if !state.detector.has_text() && !inplace {
                state.enter(&region.id, Phase::WaitingFrame);
                self.status("Ожидание текста", out); return;
            }
        }
        if inplace {
            self.inplace_tick(s, region, state, frame, force, now, out).await;
            return;
        }
        state.enter(&region.id, Phase::Recognizing);
        self.status("Распознавание окна…", out);
        let recognizing = Instant::now();
        let result = stage!(self.ocr.recognize_detailed(&frame, s), "Ошибка OCR");
        self.timings.record("ocr", recognizing.elapsed());
        let original = text::normalize(&result.text);
        let doubt = unsure(result.confidence, s.ocr_min_confidence);
        if !text::is_meaningful(&original) || doubt.is_some() {
            state.enter(&region.id, Phase::WaitingFrame);
            let note = doubt.map(|c| format!("отброшено: уверенность {c:.0}% ниже порога {}%", s.ocr_min_confidence));
            self.send_preview(region, &frame, line_boxes(&result, (0, 0), note.as_deref()), original.clone(), String::new(), Phase::WaitingFrame, out);
            state.settle(region, out);
            match doubt {
                Some(c) => { tracing::debug!(target: "pipeline.ocr", region = %region.id, confidence = c, min = s.ocr_min_confidence, "OCR ignored: low confidence"); self.status(&format!("OCR: низкая уверенность ({c:.0}%), текст пропущен"), out) }
                None => self.status("OCR: текст не обнаружен", out),
            }
            return;
        }
        if !force && text::similarity(&original, &state.last_text) >= SAME_TEXT_RATIO { state.enter(&region.id, Phase::Showing); state.settle(region, out); self.status("Ожидание текста", out); return; }
        let src = tess_to_iso(primary_lang(&s.source_lang));
        let translated = match self.cached(src, &s.target_lang, &original) {
            Some(t) => t,
            None => {
                state.enter(&region.id, Phase::Translating);
                self.status("Перевод…", out);
                let translating = Instant::now();
                match tokio::time::timeout_at(deadline, self.translate_shared(s, &original, src, &s.target_lang)).await {
                    Ok(Ok(t)) => { self.timings.record("translate", translating.elapsed()); t },
                    Ok(Err(e)) => {
                        let stop = matches!(&*e, TranslateError::RateLimited { .. });
                        state.fail(region, format!("Ошибка перевода ({}): {e}", translation_context(s, &original)), now + started.elapsed(), stop, out);
                        return;
                    }
                    Err(_) => { state.fail(region, "Ошибка перевода: нет ответа за 60 с".into(), now + started.elapsed(), true, out); return; }
                }
            }
        };
        state.enter(&region.id, Phase::Showing);
        // Lines the engine found; an engine without geometry gives one box over the whole frame.
        let mut boxes = line_boxes(&result, (0, 0), None);
        if boxes.is_empty() {
            let whole = crate::layout::Rect::new(0.0, 0.0, frame.width() as f32, frame.height() as f32);
            boxes.push(PreviewBox { rect: whole, original: original.clone(), translation: translated.clone(), details: Vec::new() });
        }
        self.send_preview(region, &frame, boxes, original.clone(), translated.clone(), Phase::Showing, out);
        state.last_text = original.clone();
        state.recovered();
        self.last_status = "Перевод обновлён".into();
        out.send(Event::Translation { region_id: region.id.clone(), region_name: region.name.clone(), original, text: translated });
    }

    /// «Поверх оригинала»: каждое поле распознаётся и переводится отдельно, и только если его
    /// содержимое изменилось. Поля одной сцены обрабатываются конкурентно (не более
    /// `FIELD_CONCURRENCY` одновременно): задержка сцены из имени, реплики и двух кнопок — это
    /// самая медленная операция, а не их сумма. Одинаковые тексты (две кнопки «OK») переводятся один раз.
    ///
    /// Тик можно отменить в любой точке `await`: движок остаётся в состоянии области, а подпись
    /// содержимого поля запоминается только в `complete`, поэтому незавершённое поле на следующем
    /// кадре распознаётся снова.
    #[allow(clippy::too_many_arguments)]
    async fn inplace_tick(&mut self, s: &Settings, region: &RegionProfile, state: &mut RegionState, frame: image::DynamicImage,
                          force: bool, now: Instant, out: &dyn Sink) {
        // The engine takes the frame; the preview needs its own copy, made only while it is open.
        let preview_image = self.preview.load(Ordering::Relaxed).then(|| frame.to_rgba8());
        let engine = state.inplace.get_or_insert_with(Default::default);
        let laying_out = Instant::now();
        let jobs = engine.begin(frame, s, now, force);
        self.timings.record("layout", laying_out.elapsed());
        let budget = state.first_error.map(|first| ERROR_BUDGET.saturating_sub(now.saturating_duration_since(first))).unwrap_or(ERROR_BUDGET);
        let deadline = tokio::time::Instant::now() + budget;
        let src = tess_to_iso(primary_lang(&s.source_lang)).to_string();
        // The first failure decides the status; fields that did succeed are still shown.
        let mut failure: Option<(String, bool)> = None;
        // Every distinct error of the scene, not only the first: two fields failing for different
        // reasons are two messages.
        let mut notes: Vec<String> = Vec::new();

        // 1. OCR of the changed fields.
        let mut recognized = Vec::with_capacity(jobs.len());
        let mut origins: HashMap<u64, (crate::layout::Rect, (u32, u32))> = HashMap::new();
        let mut newly_ignored = false;
        if !jobs.is_empty() {
            state.enter(&region.id, Phase::Recognizing);
            self.status("Распознавание окна…", out);
            let ocr = &self.ocr;
            let recognizing = Instant::now();
            origins = jobs.iter().map(|j| (j.id, (j.rect, j.origin))).collect();
            let pinned: HashMap<u64, Settings> = if s.ocr_engine == "auto" {
                jobs.iter().filter(|j| state.field_engine.get(&j.id) == Some(&"paddleocr"))
                    .map(|j| (j.id, Settings { ocr_engine: "paddleocr".into(), ..s.clone() })).collect()
            } else { HashMap::new() };
            let pinned = &pinned;
            let all = bounded(jobs, FIELD_CONCURRENCY, |job| {
                let settings = pinned.get(&job.id).unwrap_or(s);
                async move { (job.id, ocr.recognize_detailed(&job.image, settings).await) }
            });
            match tokio::time::timeout_at(deadline, all).await {
                Ok(done) => { self.timings.record("ocr", recognizing.elapsed()); recognized = done },
                Err(_) => failure = Some(("Ошибка OCR: нет ответа за 60 с".into(), true)),
            }
        }
        // 2. Cached translations are applied at once; the rest wait for the translator.
        let engine = state.inplace.get_or_insert_with(Default::default);
        let mut pending: Vec<(u64, String)> = Vec::new();
        for (id, result) in recognized {
            if s.ocr_engine == "auto" {
                match &result {
                    Ok(r) if r.engine == "paddleocr" => {
                        if state.field_engine.len() >= 64 { state.field_engine.clear(); }
                        if state.field_engine.insert(id, "paddleocr").is_none() {
                            tracing::debug!(target: "pipeline.ocr", region = %region.id, field = id, "OCR auto: the field is read with PaddleOCR from now on");
                        }
                    }
                    // Tesseract was sure this time, or the pinned engine failed: decide afresh next time.
                    _ => { state.field_engine.remove(&id); }
                }
            }
            match result {
                Err(e) => {
                    let message = format!("Ошибка OCR: {e}");
                    if !notes.contains(&message) { notes.push(message.clone()); }
                    failure.get_or_insert((message, false));
                }
                Ok(result) => {
                    let original = text::normalize(&result.text);
                    let (rect, origin) = origins.get(&id).copied().unwrap_or_default();
                    if let Some(c) = unsure(result.confidence, s.ocr_min_confidence) {
                        // The engine is not sure what it read: keep the field as it was, do not translate noise.
                        tracing::debug!(target: "pipeline.ocr", region = %region.id, field = id, confidence = c, min = s.ocr_min_confidence, "OCR ignored: low confidence");
                        let details = vec![format!("OCR ({}) отброшен: уверенность {c:.0}% ниже порога {}%", result.engine, s.ocr_min_confidence)];
                        state.ignored.insert(id, (now, PreviewBox { rect, original: original.clone(), translation: String::new(), details }));
                        newly_ignored = true;
                        engine.complete(id, None, s);
                        continue;
                    }
                    state.ignored.remove(&id);
                    // The lines the engine found describe the structure of the field better than the picture does.
                    let lines: Vec<crate::layout::Rect> = result.lines.iter().map(|l| l.rect.in_frame(origin)).collect();
                    engine.observe_ocr_lines(id, &lines, s);
                    state.field_ocr.insert(id, match result.confidence {
                        Some(c) => format!("OCR ({}): уверенность {c:.0}%, строк {}", result.engine, result.lines.len().max(1)),
                        None => format!("OCR ({}): уверенность не сообщается", if result.engine.is_empty() { "?" } else { result.engine }),
                    });
                    if !text::is_meaningful(&original) { engine.complete(id, None, s); continue; }
                    match engine.cached_translation(id, &original).or_else(|| self.cached(&src, &s.target_lang, &original)) {
                        Some(t) => engine.complete(id, Some((original, t)), s),
                        None => pending.push((id, original)),
                    }
                }
            }
        }
        // 3. Translation of the distinct texts.
        if !pending.is_empty() && !matches!(failure, Some((_, true))) {
            state.enter(&region.id, Phase::Translating);
            self.status("Перевод…", out);
            let translating = Instant::now();
            let mut distinct: Vec<&str> = pending.iter().map(|(_, t)| t.as_str()).collect();
            distinct.sort_unstable();
            distinct.dedup();
            let io = &*self;
            let (src_ref, target) = (src.as_str(), s.target_lang.as_str());
            let all = bounded(distinct, FIELD_CONCURRENCY, |text| async move { (text, io.translate_shared(s, text, src_ref, target).await) });
            let translated: HashMap<String, Result<String, Arc<TranslateError>>> = match tokio::time::timeout_at(deadline, all).await {
                Ok(done) => { self.timings.record("translate", translating.elapsed()); done.into_iter().map(|(t, r)| (t.to_owned(), r)).collect() },
                Err(_) => { failure.get_or_insert(("Ошибка перевода: нет ответа за 60 с".into(), true)); HashMap::new() }
            };
            let engine = state.inplace.get_or_insert_with(Default::default);
            for (id, original) in pending {
                match translated.get(&original) {
                    Some(Ok(t)) => { engine.complete(id, Some((original, t.clone())), s); }
                    Some(Err(e)) => {
                        let stop = matches!(&**e, TranslateError::RateLimited { .. });
                        let message = format!("Ошибка перевода ({}): {e}", translation_context(s, &original));
                        if !notes.contains(&message) { notes.push(message.clone()); }
                        // A rate limit stops automatic retries and wins over an earlier soft failure.
                        match &failure { Some((_, true)) => {}, _ if stop => failure = Some((message, true)), None => failure = Some((message, false)), _ => {} }
                    }
                    None => {}
                }
            }
        }
        let finishing = Instant::now();
        let result = state.inplace.get_or_insert_with(Default::default).finish(s);
        self.timings.record("layout", finishing.elapsed());
        if failure.is_none() { state.enter(&region.id, if result.as_ref().is_some_and(|f| !f.blocks.is_empty()) { Phase::Showing } else { Phase::WaitingFrame }); }
        if let Some(frame) = &result {
            let known = &state.field_ocr;
            state.preview_boxes = frame.blocks.iter().map(|b| {
                let mut details = describe(b);
                details.extend(known.get(&b.id).cloned());
                PreviewBox { rect: b.text_rect, original: b.original.clone(), translation: b.translation.clone(), details }
            }).collect();
        }
        if let Some(image) = preview_image.filter(|_| result.is_some() || newly_ignored) {
            state.ignored.retain(|_, (when, _)| now.saturating_duration_since(*when) < IGNORED_SHOWN);
            let mut boxes = state.preview_boxes.clone();
            boxes.extend(state.ignored.values().map(|(_, b)| b.clone()));
            let join = |f: fn(&PreviewBox) -> &str| boxes.iter().map(f).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            let (original, translation) = (join(|b| &b.original), join(|b| &b.translation));
            let preview = OcrPreview { image, boxes, original, translation, phase: state.phase, timings: self.timings.summary() };
            out.send(Event::OcrPreview { region_id: region.id.clone(), region_name: region.name.clone(), preview: Box::new(preview) });
        }
        if let Some(frame) = result {
            self.last_status = "Перевод обновлён".into();
            out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
        }
        match failure {
            Some((first, terminal)) => state.fail(region, join_messages(&first, &notes), now, terminal, out),
            None => state.settle(region, out),
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
        fn capabilities(&self, _: &WindowKey, _: &Settings) -> crate::capture::CaptureCapabilities { crate::capture::CaptureCapabilities::KWIN }
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
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
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
    fn the_state_machine_allows_exactly_the_documented_transitions() {
        use Phase::*;
        let allowed: &[(Phase, Phase)] = &[
            (WaitingFrame, Debouncing), (WaitingFrame, Recognizing), (WaitingFrame, Showing),
            (Debouncing, WaitingFrame), (Debouncing, Recognizing), (Debouncing, Showing),
            (Recognizing, Translating), (Recognizing, Showing), (Recognizing, WaitingFrame),
            (Translating, Showing), (Translating, WaitingFrame),
            (Showing, WaitingFrame), (Showing, Debouncing), (Showing, Recognizing),
            (RetryWait, WaitingFrame), (RetryWait, Debouncing), (RetryWait, Recognizing), (RetryWait, Showing),
            (Halted, WaitingFrame), (Halted, Recognizing), (Halted, Showing),
        ];
        for from in Phase::ALL {
            for to in Phase::ALL {
                let expected = from == to || matches!(to, RetryWait | Halted) || allowed.contains(&(from, to));
                assert_eq!(from.allows(to), expected, "{from:?} -> {to:?}");
            }
        }
        // The ones that matter most: nothing skips reading, and translating is never entered cold.
        assert!(!WaitingFrame.allows(Translating) && !Showing.allows(Translating) && !Halted.allows(Translating) && !RetryWait.allows(Translating));
        assert!(!Halted.allows(Debouncing), "a halted region leaves only through a manual retry");
    }

    #[test]
    fn an_illegal_transition_is_counted_and_rejected() {
        let mut state = RegionState::default();
        state.enter("r", Phase::Translating);
        assert_eq!((state.illegal, state.phase), (1, Phase::WaitingFrame));
        state.enter("r", Phase::Recognizing);
        state.enter("r", Phase::Showing);
        assert_eq!(state.illegal, 1);
        assert_eq!(state.phase, Phase::Showing);
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
        let region = RegionProfile::default();
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
            Ok(crate::ocr::OcrResult { text: self.text.clone(), lines, confidence: Some(self.confidence), engine: "mock" })
        }
    }

    fn texts_of(events: &[Event]) -> Vec<String> {
        events.iter().filter_map(|e| match e { Event::Translation { text, .. } => Some(text.clone()), _ => None }).collect()
    }

    #[tokio::test]
    async fn unsure_text_is_ignored_and_explained() {
        let flag = Arc::new(AtomicBool::new(true));
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 18.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
        let s = Settings { ocr_min_confidence: 30, ..settings() };
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
        let s = Settings { ocr_min_confidence: 0, ..settings() };
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
        assert!(p.io.inflight.lock().unwrap().is_empty(), "finished requests are forgotten");
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

    #[test]
    fn several_errors_of_one_scene_are_all_reported_up_to_three() {
        let notes: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
        assert_eq!(join_messages("a", &notes), "a · b · c · и ещё 2");
        assert_eq!(join_messages("only", &[]), "only");
        assert_eq!(join_messages("a", &["a".into(), "b".into()]), "a · b", "no repeats");
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
                Ok(if self.0.fetch_add(1, Ordering::SeqCst) % 2 == 0 { "the name field".into() } else { "a long line of dialogue".into() })
            }
        }
        let mut p = Pipeline::new(Arc::new(SceneCapture), TwoTexts(AtomicUsize::new(0)), Picky);
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
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
        let yandex = Settings { translator: crate::settings::TranslatorKind::Yandex, ..Settings::default() };
        assert!(translation_context(&yandex, "Hi").starts_with("Yandex Translate"));
    }

    #[tokio::test]
    async fn the_message_names_service_phrase_and_reason() {
        struct Limited;
        impl Translate for Limited {
            async fn translate(&self, _: &Settings, _: &str, _: &str, _: &str) -> Result<String, TranslateError> { Err(TranslateError::RateLimited { retry_after: 60 }) }
        }
        let mut p = Pipeline::new(Arc::new(SceneCapture), MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), Limited);
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
        let (tx, mut rx) = mpsc::unbounded_channel();
        p.tick(&s, true, Instant::now(), &tx).await;
        let message = drain(&mut rx).into_iter().find_map(|e| match e { Event::Error { message, .. } => Some(message), _ => None }).expect("an error");
        assert!(message.contains("Google Translate, «Hello there»") && message.contains("HTTP 429"), "service, phrase and the reason: {message}");
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
                Ok(crate::ocr::OcrResult { text: "Hello there".into(), lines: vec![line], confidence: Some(90.0), engine: "mock" })
            }
        }
        let flag = Arc::new(AtomicBool::new(true));
        let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))), GeoOcr, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
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
    }

    #[tokio::test]
    async fn auto_pins_a_field_to_the_engine_that_was_needed() {
        /// `auto` ends up with PaddleOCR for this text; a call that already names PaddleOCR gets it directly.
        struct AutoOcr(Arc<Mutex<Vec<String>>>);
        impl Ocr for AutoOcr {
            async fn recognize(&self, _: &DynamicImage, _: &Settings) -> Result<String, OcrError> { Ok("Hello there".into()) }
            async fn recognize_detailed(&self, _: &DynamicImage, s: &Settings) -> Result<crate::ocr::OcrResult, OcrError> {
                self.0.lock().unwrap().push(s.ocr_engine.clone());
                Ok(crate::ocr::OcrResult { text: "Hello there".into(), lines: Vec::new(), confidence: Some(90.0), engine: "paddleocr" })
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap.clone(), AutoOcr(seen.clone()), MockTr(Arc::new(AtomicUsize::new(0))));
        let s = Settings { translation_display: TranslationDisplay::Inplace, ocr_engine: "auto".into(), ..settings() };
        let (tx, _rx) = mpsc::unbounded_channel();
        let t0 = Instant::now();
        p.tick(&s, false, t0, &tx).await;
        p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
        assert_eq!(*seen.lock().unwrap(), ["auto"], "the first read decides");
        // The text changed; the same field is read again, now straight with PaddleOCR.
        p.tick(&s, true, t0 + Duration::from_millis(300), &tx).await;
        assert_eq!(*seen.lock().unwrap(), ["auto", "paddleocr"]);
        // Another engine setting is not touched by pins.
        let plain = Settings { ocr_engine: "tesseract".into(), ..s.clone() };
        p.tick(&plain, true, t0 + Duration::from_millis(500), &tx).await;
        assert_eq!(seen.lock().unwrap().last().map(String::as_str), Some("tesseract"));
    }

    #[tokio::test]
    async fn an_unsure_field_is_ignored_in_place_and_shown_in_the_inspector() {
        let flag = Arc::new(AtomicBool::new(true));
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let mut p = Pipeline::new(cap, SureOcr { text: "Whxre arx yoz".into(), confidence: 12.0 }, MockTr(Arc::new(AtomicUsize::new(0)))).with_preview(flag);
        let s = Settings { translation_display: TranslationDisplay::Inplace, ocr_min_confidence: 30, ..settings() };
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
        *p.io.ocr.0.lock().unwrap() = "Totally different".into();
        *cap.0.lock().unwrap() = 200;
        p.tick(&s, false, step(200), &tx).await;
        p.tick(&s, false, step(400), &tx).await;
        *p.io.ocr.0.lock().unwrap() = "First line".into();
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
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let t0 = Instant::now();
        p.tick(&s, false, t0, &tx).await;
        p.tick(&s, false, t0 + Duration::from_millis(150), &tx).await;
        assert!(matches!(drain(&mut rx).as_slice(), [Event::Translation { text, .. }] if text == "RU:Hello there"),
            "the whole region is translated for the translation window, no fields are searched");
    }

    #[tokio::test]
    async fn inplace_fields_are_translated_once_and_unchanged_fields_are_not_resent() {
        let cap = Arc::new(MockCapture(Mutex::new(10)));
        let (ocr_n, tr_n) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let mut p = Pipeline::new(cap.clone(), MockOcr(Mutex::new("Hello there".into()), ocr_n.clone()), MockTr(tr_n.clone()));
        let s = Settings { translation_display: TranslationDisplay::Inplace, ..settings() };
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
        p.io.ocr.0.lock().unwrap().clear();
        *cap.0.lock().unwrap() = 120;
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
        for display in [TranslationDisplay::Window, TranslationDisplay::Inplace] {
            let count = Arc::new(AtomicUsize::new(0));
            let mut p = Pipeline::new(Arc::new(MockCapture(Mutex::new(10))),
                MockOcr(Mutex::new("Hello there".into()), Arc::new(AtomicUsize::new(0))), Limited(count.clone()));
            let s = Settings { translation_display: display, ..settings() };
            let (tx, mut rx) = mpsc::unbounded_channel();
            let now = Instant::now();
            p.tick(&s, true, now, &tx).await;
            assert!(drain(&mut rx).iter().any(|e| matches!(e, Event::Error { terminal: true, message, .. } if message.contains("429"))), "{display:?}");
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
    fn inplace_settings() -> Settings { Settings { translation_display: TranslationDisplay::Inplace, ..settings() } }

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
        let started = std::time::Instant::now();
        p.tick(&inplace_settings(), false, t0 + Duration::from_millis(150), &tx).await;
        let elapsed = started.elapsed();
        let frame = drain(&mut rx).into_iter().find_map(|e| match e { Event::Inplace { frame, .. } => Some(frame), _ => None }).expect("fields");
        assert_eq!(frame.blocks.len(), 3, "three fields translated");
        assert!(frame.blocks.iter().all(|b| b.translation.starts_with("RU:Line number")));
        let peak = peak.load(Ordering::SeqCst);
        assert!((2..=FIELD_CONCURRENCY).contains(&peak), "peak concurrency {peak}");
        assert!(elapsed < Duration::from_millis(3 * 40 + 200), "slower than sequential: {elapsed:?}");
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
        *cap.0.lock().unwrap() = 200;
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
}
