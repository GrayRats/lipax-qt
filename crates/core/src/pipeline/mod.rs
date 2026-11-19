//! Independent region state, bounded retries and a 60-second operation deadline.
use crate::sync::LockExt;
use crate::{cache::TranslationCache, layout::engine::InplaceFrame, capture::Capture, detect::ChangeDetector, ocr::Ocr,
    settings::{Settings, CaptureRegion, TranslationDisplayMode}, text, translate::{Translate, TranslateError}};
use futures_util::{FutureExt, StreamExt, stream::FuturesUnordered};
use std::{collections::HashMap, sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};
use tokio::{sync::{mpsc, watch}, time::MissedTickBehavior};

mod inplace;
mod preview;
mod state;
pub use inplace::read_field;
pub use preview::{OcrPreview, PreviewBox};
use preview::line_boxes;
pub use state::Phase;
use state::{ERROR_BUDGET, RegionState};

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
/// Под непрерывно меняющимся текстом (подвижный фон, анимация) OCR всё равно запускается с этим интервалом.
const MAX_UNSTABLE: Duration = Duration::from_secs(2);

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
    /// Result of auto-tuning the image filters: every preset with its score and the index of the best one.
    AutoTune { region_id: String, results: Vec<TuneResult>, best: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TuneResult {
    pub name: &'static str,
    pub score: f32,
    pub filters: crate::ocr::filter::Preprocess,
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

/// The recognised text as paragraphs, split by the rule that splits in-place fields (`layout::split`): a new
/// paragraph starts where the pitch between lines (centre to centre) is larger than `k` mean line heights,
/// `k` being `appearance.inplace.line_gap_factor`. A paragraph that stops mid-sentence (no closing punctuation)
/// and is followed by one that starts in lower case is joined to it: a sentence is translated whole.
/// An engine without geometry gives one paragraph.
fn paragraphs(result: &crate::ocr::OcrResult, k: f32) -> Vec<String> {
    let mut lines: Vec<&crate::ocr::OcrLine> = result.lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    if lines.is_empty() { return vec![text::normalize(&result.text)]; }
    lines.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<&crate::ocr::OcrLine> = None;
    for line in lines {
        let joined = last.is_some_and(|p| {
            let pitch = (line.rect.y + line.rect.h / 2.0) - (p.rect.y + p.rect.h / 2.0);
            pitch <= k * (p.rect.h + line.rect.h) / 2.0
        });
        match out.last_mut() {
            Some(current) if joined => { current.push('\n'); current.push_str(&line.text); }
            _ => out.push(line.text.clone()),
        }
        last = Some(line);
    }
    let ends_sentence = |p: &str| p.trim_end().chars().next_back().is_some_and(|c| matches!(c, '.' | '!' | '?' | '…' | ':' | ';' | '"' | '”' | '»' | ')' | '。' | '！' | '？'));
    let mut merged: Vec<String> = Vec::new();
    for paragraph in out.into_iter().map(|p| text::normalize(&p)).filter(|p| !p.is_empty()) {
        let continues = paragraph.chars().next().is_some_and(char::is_lowercase);
        match merged.last_mut() {
            Some(previous) if continues && !ends_sentence(previous) => { previous.push(' '); previous.push_str(&paragraph); }
            _ => merged.push(paragraph),
        }
    }
    merged
}

/// What the in-place engine would read: the text blocks of the frame with the margin it gives its OCR jobs
/// (at most `MAX_TUNE_FIELDS`, the largest first). A frame with no block is read whole.
const MAX_TUNE_FIELDS: usize = 4;
fn field_crops(frame: &image::DynamicImage, s: &Settings) -> Vec<image::DynamicImage> {
    use crate::layout::block_detector::BlockDetector;
    let image = frame.to_rgba8();
    let mut mask = BlockDetector::ink_mask(&image);
    let detector = BlockDetector { line_gap_factor: s.appearance.inplace.line_gap_factor, max_blocks: s.appearance.inplace.max_blocks };
    let mut blocks = detector.detect_with_local(&image, &mut mask).0;
    blocks.sort_by(|a, b| (b.rect.w * b.rect.h).total_cmp(&(a.rect.w * a.rect.h)));
    let (fw, fh) = (image.width() as f32, image.height() as f32);
    let crops: Vec<_> = blocks.iter().take(MAX_TUNE_FIELDS).map(|b| {
        let pad = (0.25 * b.line_height()).max(3.0);
        let (x, y, w, h) = b.rect.expand(pad, fw, fh).pixels(image.width(), image.height());
        image::DynamicImage::ImageRgba8(image::imageops::crop_imm(&image, x, y, w, h).to_image())
    }).collect();
    if crops.is_empty() { vec![frame.clone()] } else { crops }
}

/// Which service and which phrase an error is about: every waiter of a shared request gets its own message.
fn cache_source(s: &Settings, src: &str) -> String {
    let service = match s.translation.service {
        crate::settings::TranslationService::Custom => format!("custom:{}", s.translation.custom_url),
        crate::settings::TranslationService::Bergamot => format!("bergamot:{}:{}:{:?}", s.translation.bergamot_binary, s.translation.bergamot_models_dir, s.translation.bergamot_model_paths),
        other => format!("{other:?}"),
    };
    format!("{service}\u{1}{src}")
}

fn translation_context(s: &Settings, text: &str) -> String {
    let service = match s.translation.service {
        crate::settings::TranslationService::Google => "Google Translate",
        crate::settings::TranslationService::Yandex => "Yandex Translate",
        crate::settings::TranslationService::Custom => "свой API",
        crate::settings::TranslationService::DeepL => "DeepL",
        crate::settings::TranslationService::Microsoft => "Microsoft Translator",
        crate::settings::TranslationService::Bergamot => "Bergamot",
    };
    let mut excerpt: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect();
    if text.chars().count() > 40 { excerpt.push('…'); }
    format!("{service}, «{excerpt}»")
}

/// A translation in flight. Every holder gets the same result; the error is shared too.
type PendingTranslation = futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<String, Arc<TranslateError>>>>;
/// At most this many different phrases are fetched at once; more are fetched without sharing.
const MAX_INFLIGHT: usize = 32;
/// While OCR runs, the window is looked at this often to see whether the picture has moved on.
const STALE_PROBE: Duration = Duration::from_millis(150);
/// An animated background must not keep OCR from ever finishing: after this many dropped runs in a row
/// the next one is not interrupted.
const MAX_STALE: u8 = 2;
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
    /// Set by the owner while nobody can see the result (the game window is in the background and the overlay is
    /// hidden): automatic ticks are skipped, so no frames are captured, read or translated. A command still runs.
    paused: Arc<AtomicBool>,
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Pipeline<C, O, T> {
    pub fn new(capture: C, ocr: O, translator: T) -> Self {
        Self { io: Io { capture, ocr, translator: Arc::new(translator), cache: Arc::new(std::sync::Mutex::new(TranslationCache::new(512))), inflight: Arc::default(), preview: Arc::default(), timings: Timings::default(), last_status: String::new() }, states: HashMap::new(), paused: Arc::default() }
    }
    /// The flag is owned by the caller: switching it costs nothing while the preview is closed.
    pub fn with_preview(mut self, flag: Arc<AtomicBool>) -> Self { self.io.preview = flag; self }
    pub fn with_pause(mut self, flag: Arc<AtomicBool>) -> Self { self.paused = flag; self }
    pub fn reset(&mut self) { self.states.clear(); self.io.last_status.clear(); }

    pub async fn tick(&mut self, s: &Settings, force: bool, now: Instant, out: &dyn Sink) {
        let regions = s.capture_regions();
        let Self { io, states, .. } = self;
        if s.capture.window.is_none() || regions.is_empty() { io.status("Выберите окно и включите область", out); return; }
        states.retain(|id, _| regions.iter().any(|r| &r.id == id));
        for region in regions {
            // The state stays in the map across every `await`: `run` may cancel this future
            // (new command, stop), and a state removed for the duration would be lost with it —
            // including the engine with its locked fonts.
            let state = states.entry(region.id.clone()).or_default();
            if force { state.recovered(); }
            let mut effective = s.clone();
            effective.capture.region = region.rect;
            effective.recognition.debounce_ms = region.debounce_ms;
            for (value, local) in [(&mut effective.recognition.language, &region.recognition_language), (&mut effective.translation.target_language, &region.target_language)] {
                if !local.is_empty() { *value = local.clone(); }
            }
            if let Some(engine) = &region.engine { effective.recognition.engine = engine.clone(); }
            io.tick_region(&effective, &region, state, force, now, out).await;
        }
    }

    /// Reads the current frame of the active area with the current filters and with every preset at once and
    /// reports which recognised the most text (`ocr::filter::score`). The first candidate is the user's own
    /// setting, so a manual choice is replaced only by a preset that is clearly better. In the in-place mode the
    /// engine reads the fields, not the area: the candidates are scored on the same crops. Does not touch the
    /// settings: the owner applies the answer.
    pub async fn auto_tune(&mut self, s: &Settings, out: &dyn Sink) {
        let io = &mut self.io;
        let regions = s.capture_regions();
        let Some(region) = regions.iter().find(|r| r.id == s.capture.active_region).or(regions.first()).cloned() else { io.status("Выберите окно и включите область", out); return };
        let (Some(window), Some(rect)) = (s.capture.window.as_ref(), region.rect) else { io.status("Выберите окно и область", out); return };
        io.status("Подбор фильтров…", out);
        let frame = match io.capture.grab(window, rect).await {
            Ok(frame) => frame,
            Err(e) => { io.status(&format!("Подбор фильтров: ошибка захвата ({e})"), out); return }
        };
        let inplace = s.display_mode == TranslationDisplayMode::Inplace && io.capture.capabilities(window, s).inplace_translation;
        let images = if inplace { field_crops(&frame, s) } else { vec![frame] };
        let started = Instant::now();
        let current = crate::ocr::filter::Preprocess::of(&s.recognition);
        let mut candidates: Vec<(&'static str, crate::ocr::filter::Preprocess)> = vec![("Текущие настройки", current)];
        candidates.extend(crate::ocr::filter::PRESETS.iter().copied().filter(|(_, f)| *f != current));
        let ocr = &io.ocr;
        let images = &images;
        let runs = candidates.iter().map(|(name, filters)| {
            let mut candidate = s.clone();
            let r = &mut candidate.recognition;
            (r.binarize, r.auto_invert, r.contrast, r.sharpen) = (filters.binarize, filters.auto_invert, filters.contrast, filters.sharpen);
            // Each candidate is exactly what it says: the automatic choice would turn «no filters» into something else.
            r.auto_filters = false;
            // «auto» would start Python for every candidate; the filters are judged on the fast engine.
            if r.engine == "auto" { r.engine = "tesseract".into(); }
            async move {
                let mut score = 0.0;
                for image in images { score += ocr.recognize_detailed(image, &candidate).await.map(|r| crate::ocr::filter::score(&r)).unwrap_or(0.0); }
                (*name, *filters, score)
            }
        });
        let results: Vec<TuneResult> = futures_util::future::join_all(runs).await.into_iter()
            .map(|(name, filters, score)| TuneResult { name, filters, score })
            .collect();
        let best = crate::ocr::filter::pick_best(&results.iter().map(|r| r.score).collect::<Vec<_>>());
        tracing::debug!(target: "pipeline.autotune", elapsed_ms = started.elapsed().as_millis() as u64, best = results[best].name, scores = ?results.iter().map(|r| r.score).collect::<Vec<_>>(), fields = images.len(), "auto-tune finished");
        io.timings.record("autotune", started.elapsed());
        out.send(Event::AutoTune { region_id: region.id.clone(), results, best });
    }

    /// Marks the new settings as seen and, if they change what is recognised or shown, starts the pipeline over.
    /// `true`: it was started over.
    fn apply_settings_change(&mut self, settings: &mut watch::Receiver<Settings>, seen_key: &mut String) -> bool {
        let key = settings.borrow_and_update().processing_key();
        if key == *seen_key { return false; }
        *seen_key = key;
        tracing::debug!(target: "pipeline.settings", "settings changed: the pipeline starts over with the new ones");
        self.reset();
        true
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
    pub async fn run(mut self, mut settings: watch::Receiver<Settings>, mut running: watch::Receiver<bool>, mut cmds: mpsc::UnboundedReceiver<Cmd>,
                     generation: Arc<AtomicU64>, out: mpsc::UnboundedSender<(u64, Event)>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut pending = None;
        // What the running pipeline was built for: a change of any setting that decides what is recognised or shown
        // (`Settings::processing_key`) starts it over, with or without a `Cmd::Reset` from the owner.
        let mut seen_key = settings.borrow().processing_key();
        loop {
            let command = if let Some(c) = pending.take() { c } else {
                tokio::select! {
                    biased;
                    c = cmds.recv() => match c { Some(c) => c, None => break },
                    _ = ticker.tick() => { if !*running.borrow() || !settings.borrow().translation.auto_translate || self.paused.load(Ordering::Relaxed) { continue; } Cmd::Auto },
                }
            };
            if command == Cmd::Reset { self.reset(); continue; }
            if command == Cmd::ReanalyzeFonts { self.reanalyze_fonts(); continue; }
            // Settings are looked at before every frame, not only when the pipeline starts. What the user asked for by hand
            // (a read now, a filter choice) is not dropped with the automatic tick the change restarts.
            if settings.has_changed().unwrap_or(false) && self.apply_settings_change(&mut settings, &mut seen_key)
                && !matches!(command, Cmd::TranslateOnce | Cmd::AutoTune) {
                continue;
            }
            let stamp = Stamped { tx: &out, generation: generation.load(Ordering::SeqCst) };
            let s = settings.borrow().clone();
            let _ = running.borrow_and_update();
            let (mut quit, mut restart) = (false, false);
            {
                let work = async { if command == Cmd::AutoTune { self.auto_tune(&s, &stamp).await } else { self.tick(&s, command == Cmd::TranslateOnce, Instant::now(), &stamp).await } };
                tokio::pin!(work);
                loop {
                    tokio::select! {
                        biased;
                        c = cmds.recv() => { match c { Some(c) => pending = Some(c), None => quit = true } break },
                        changed = running.changed() => { quit = changed.is_err(); break },
                        // The settings change while a frame is being read. A change that decides what is recognised or
                        // shown makes the reading one for settings that are gone: it is dropped. Any other (a colour,
                        // a position) leaves it alone and the wait goes on.
                        changed = settings.changed() => {
                            if changed.is_err() { quit = true; break }
                            let key = settings.borrow_and_update().processing_key();
                            if key != seen_key { seen_key = key; restart = true; break }
                        },
                        _ = &mut work => break,
                    }
                }
            }
            if quit { break; }
            if restart {
                tracing::debug!(target: "pipeline.settings", "settings changed while a frame was being read: the reading is dropped, the pipeline starts over");
                self.reset();
            }
        }
    }
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Io<C, O, T> {
    fn cached(&self, s: &Settings, src: &str, dst: &str, text: &str) -> Option<String> {
        self.cache.lock_recover().get(&cache_source(s, src), dst, text)
    }

    /// Translate `text`, joining a request for the same phrase that is already running. The
    /// request is carried by its own task, not by the tick that asked: if that tick is cancelled
    /// (new command, changed settings, a new frame) the answer still arrives, lands in the cache
    /// and serves whoever asks for the phrase next, instead of being thrown away and fetched again.
    /// The same phrase in two places at once (or the same line shown again a moment later) is one
    /// request. The error is shared as well, so a rate limit reaches everyone who waited for it.
    async fn translate_shared(&self, s: &Settings, text: &str, src: &str, dst: &str) -> Result<String, Arc<TranslateError>> {
        // The service is part of the key: another translator may answer differently.
        let key = format!("{}\u{1}{dst}\u{1}{text}", cache_source(s, src));
        let pending = {
            let mut running = self.inflight.lock_recover();
            if let Some(found) = running.get(&key) { found.clone() } else {
                let (translator, settings, cache) = (self.translator.clone(), s.clone(), self.cache.clone());
                let (text, src, dst) = (text.to_owned(), src.to_owned(), dst.to_owned());
                let request = async move {
                    // An answer that fails the structural check is an error like any other: not cached, and
                    // for DeepL/Microsoft it goes to the Google fallback below.
                    let first = translator.translate(&settings, &text, &src, &dst).await.and_then(|translated| {
                        crate::translate::guard::check(&text, &translated, &src, &dst).map(|()| translated).map_err(|reason| {
                            tracing::warn!(service = ?settings.translation.service, %reason, "Ответ переводчика отклонён");
                            TranslateError::Rejected(reason.to_string())
                        })
                    });
                    match first {
                        Ok(translated) => {
                            cache.lock_recover().put(&cache_source(&settings, &src), &dst, &text, translated.clone());
                            Ok(translated)
                        }
                        Err(error) if matches!(settings.translation.service, crate::settings::TranslationService::DeepL | crate::settings::TranslationService::Microsoft)
                            && !matches!(&error, TranslateError::NotConfigured(_)) => {
                            tracing::warn!(service = ?settings.translation.service, error = %error, "Переводчик недоступен, используется Google Translate");
                            let mut fallback = settings.clone();
                            fallback.translation.service = crate::settings::TranslationService::Google;
                            let fallback_src = cache_source(&fallback, &src);
                            let cached = cache.lock_recover().get(&fallback_src, &dst, &text);
                            if let Some(translated) = cached { return Ok(translated); }
                            let translated = translator.translate(&fallback, &text, &src, &dst).await.map_err(Arc::new)?;
                            if let Err(reason) = crate::translate::guard::check(&text, &translated, &src, &dst) {
                                tracing::warn!(%reason, "Ответ запасного переводчика отклонён");
                                return Err(Arc::new(TranslateError::Rejected(reason.to_string())));
                            }
                            cache.lock_recover().put(&fallback_src, &dst, &text, translated.clone());
                            Ok(translated)
                        }
                        Err(error) => Err(Arc::new(error)),
                    }
                }.boxed().shared();
                // Beyond the limit the phrase is still fetched, just not shared or carried on its own.
                if running.len() < MAX_INFLIGHT {
                    running.insert(key.clone(), request.clone());
                    let (driver, inflight) = (request.clone(), self.inflight.clone());
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(ORPHAN_REQUEST_LIMIT, driver).await;
                        inflight.lock_recover().remove(&key);
                    });
                }
                request
            }
        };
        pending.await
    }

    /// Translate distinct texts, one entry per text. With the local Bergamot engine and more than one text that is not
    /// cached, they go to the engine as ONE batch (one process start instead of one per text, see
    /// [`Io::translate_batch_shared`]); every other case is [`Io::translate_shared`] per text, `FIELD_CONCURRENCY` at a time.
    async fn translate_texts(&self, s: &Settings, texts: Vec<&str>, src: &str, dst: &str) -> HashMap<String, Result<String, Arc<TranslateError>>> {
        let batch = matches!(s.translation.service, crate::settings::TranslationService::Bergamot)
            && texts.iter().filter(|t| self.cached(s, src, dst, t).is_none()).collect::<std::collections::HashSet<_>>().len() > 1;
        if batch { return self.translate_batch_shared(s, &texts, src, dst).await; }
        bounded(texts, FIELD_CONCURRENCY, |text| async move { (text.to_owned(), self.translate_shared(s, text, src, dst).await) })
            .await.into_iter().collect()
    }

    /// The texts that are not cached are translated by one `translate_many` call; the rest come from the cache. Each answer
    /// passes the same structural check as in [`Io::translate_shared`]: a rejected one is an error and is not cached. Like a
    /// shared request, the batch is carried by its own task, so a cancelled tick does not lose the answers: they land in the cache.
    async fn translate_batch_shared(&self, s: &Settings, texts: &[&str], src: &str, dst: &str) -> HashMap<String, Result<String, Arc<TranslateError>>> {
        let mut out: HashMap<String, Result<String, Arc<TranslateError>>> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        for &text in texts {
            if out.contains_key(text) || missing.iter().any(|m| m == text) { continue; }
            match self.cached(s, src, dst, text) {
                Some(t) => { out.insert(text.to_owned(), Ok(t)); }
                None => missing.push(text.to_owned()),
            }
        }
        if missing.is_empty() { return out; }
        let (translator, settings, cache) = (self.translator.clone(), s.clone(), self.cache.clone());
        let (src, dst) = (src.to_owned(), dst.to_owned());
        let sent = missing.clone();
        let task = tokio::spawn(async move {
            let answers = match tokio::time::timeout(ORPHAN_REQUEST_LIMIT, translator.translate_many(&settings, &sent, &src, &dst)).await {
                Ok(answers) => answers,
                Err(_) => return sent.iter().map(|_| Err(Arc::new(TranslateError::Local("пакетный перевод не ответил вовремя".into())))).collect::<Vec<_>>(),
            };
            let mut answers = answers.into_iter();
            sent.iter().map(|text| {
                let answer = answers.next().unwrap_or(Err(TranslateError::BadResponse));
                answer.and_then(|translated| {
                    crate::translate::guard::check(text, &translated, &src, &dst).map(|()| translated).map_err(|reason| {
                        tracing::warn!(service = ?settings.translation.service, %reason, "Ответ переводчика отклонён");
                        TranslateError::Rejected(reason.to_string())
                    })
                }).inspect(|translated| {
                    cache.lock_recover().put(&cache_source(&settings, &src), &dst, text, translated.clone());
                }).map_err(Arc::new)
            }).collect::<Vec<_>>()
        });
        let results = match task.await {
            Ok(results) => results,
            Err(e) => { let e = Arc::new(TranslateError::Local(e.to_string())); missing.iter().map(|_| Err(e.clone())).collect() }
        };
        out.extend(missing.into_iter().zip(results));
        out
    }

    /// Awaits `work` (OCR of `frame`) but gives it up as soon as the window shows another picture: a result
    /// for a frame that is gone would draw old text over new, or draw the same text twice. Dropping the
    /// future stops the engine (the process is killed with it). `Err(())`: the picture changed.
    /// Without a `probe` (the limit of dropped runs is reached) or a window to look at, it is just `work`.
    async fn unless_stale<R>(&self, s: &Settings, region: &CaptureRegion, probe: Option<ChangeDetector>, work: impl Future<Output = R>) -> Result<R, ()> {
        let (Some(window), Some(rect), Some(mut probe)) = (s.capture.window.as_ref(), region.rect, probe) else { return Ok(work.await) };
        tokio::pin!(work);
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + STALE_PROBE, STALE_PROBE);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                done = &mut work => return Ok(done),
                _ = ticker.tick() => {}
            }
            // The look at the window is itself awaited beside the work: a reading that finishes while the capture is
            // under way is taken at once, not after the capture (which can take as long as KWin or the portal needs).
            tokio::select! {
                biased;
                done = &mut work => return Ok(done),
                next = self.capture.grab(window, rect) => {
                    // A failed grab says nothing about the picture: the work goes on.
                    if let Ok(next) = next && probe.changed(&next, s.recognition.sensitivity, Instant::now()) { return Err(()); }
                }
            }
        }
    }

    /// What the picture looked like when OCR started, to compare with it while OCR runs. Built only when OCR really
    /// starts and the tick has not already looked at the frame: the change detector of the region holds exactly this.
    fn seeded_probe(s: &Settings, frame: &image::DynamicImage) -> ChangeDetector {
        let mut probe = ChangeDetector::new();
        probe.changed(frame, s.recognition.sensitivity, Instant::now());
        probe
    }

    /// The OCR of a stale frame was dropped: the next tick looks at the new picture and, once it settles, reads it.
    fn dropped_stale(&mut self, region: &CaptureRegion, state: &mut RegionState, now: Instant, out: &dyn Sink) {
        state.stale += 1;
        state.dirty_since = Some(now);
        state.dirty_first.get_or_insert(now);
        tracing::debug!(target: "pipeline.ocr", region = %region.id, dropped = state.stale, "OCR dropped: the picture changed while it was read");
        self.status("Кадр изменился, распознавание отменено", out);
    }

    /// «Translate changes only»: each paragraph is translated on its own, so what was translated before (a static
    /// header, a button that stays) comes from the cache and only the new paragraphs are sent. Joined with `\n`.
    async fn translate_changes(&self, s: &Settings, parts: &[String], src: &str, dst: &str) -> Result<String, Arc<TranslateError>> {
        let sent = parts.iter().filter(|p| self.cached(s, src, dst, p).is_none()).count();
        let done: Vec<Result<String, Arc<TranslateError>>> = if matches!(s.translation.service, crate::settings::TranslationService::Bergamot) && sent > 1 {
            let answers = self.translate_texts(s, parts.iter().map(String::as_str).collect(), src, dst).await;
            parts.iter().map(|part| answers.get(part.as_str()).cloned().unwrap_or_else(|| Err(Arc::new(TranslateError::BadResponse)))).collect::<Vec<_>>()
        } else {
            let all = parts.iter().map(|part| async move {
                match self.cached(s, src, dst, part) {
                    Some(t) => Ok(t),
                    None => self.translate_shared(s, part, src, dst).await,
                }
            });
            futures_util::future::join_all(all).await
        };
        tracing::debug!(target: "pipeline.translate", paragraphs = parts.len(), sent, "translate changes only");
        done.into_iter().collect::<Result<Vec<_>, _>>().map(|t| t.join("\n"))
    }

    fn status(&mut self, text: &str, out: &dyn Sink) {
        if self.last_status != text {
            tracing::debug!(stage = text, "Состояние обработки");
            self.last_status = text.to_string();
            out.send(Event::Status(text.to_string()));
        }
    }

    async fn tick_region(&mut self, s: &Settings, region: &CaptureRegion, state: &mut RegionState, force: bool, now: Instant, out: &dyn Sink) {
        // The backend decides whether the text can be found and drawn over in place; otherwise
        // the region is read as a whole and the translation goes to the translation window.
        let can_inplace = s.capture.window.as_ref().is_some_and(|w| self.capture.capabilities(w, s).inplace_translation);
        let inplace = s.display_mode == TranslationDisplayMode::Inplace && can_inplace;
        if !inplace { state.inplace = None; }
        // The capture of this tick: the drawn region, or started higher for a line its top edge cut. Everything below
        // (capture, the check for a stale reading, the fields) works on it; the overlay is placed by it.
        let extended = state.captured(s, region, inplace);
        let drawn = region;
        let region = extended.as_ref().unwrap_or(region);
        // Поменяли оформление «поверх оригинала»: перестроить без нового кадра и OCR.
        if let Some(mut frame) = state.inplace.as_mut().and_then(|e| e.restyle(s)) {
            frame.capture = state.engine_capture;
            out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
        }
        // Taken at once: it is for exactly one tick.
        let rescan = std::mem::take(&mut state.rescan);
        // A regrouping asks for a new reading, not for another request to a service that has stopped the automatic retries.
        if state.halted && !force { return; }
        if !force && !rescan {
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
        let frame = stage!(self.capture.grab(s.capture.window.as_ref().unwrap(), region.rect.unwrap()), "Ошибка захвата");
        tracing::debug!(target: "pipeline.capture", region = %region.id, width = frame.width(), height = frame.height(), inplace, "capture frame received");
        self.timings.record("capture", grabbed.elapsed());
        let mut detected = false;
        state.unsettled = false;
        if !force && !retry && !rescan {
            detected = true;
            let detecting = Instant::now();
            let changed = state.detector.changed(&frame, s.recognition.sensitivity, now);
            self.timings.record("detect", detecting.elapsed());
            if changed {
                state.dirty_since = Some(now);
                state.dirty_first.get_or_insert(now);
            }
            let settled = !changed && state.dirty_since.is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_millis(s.recognition.debounce_ms));
            let overdue = state.dirty_first.is_some_and(|t| now.saturating_duration_since(t) >= MAX_UNSTABLE);
            if !settled && !overdue {
                state.enter(&region.id, if state.dirty_since.is_some() { Phase::Debouncing } else { Phase::WaitingFrame });
                if self.last_status.is_empty() { self.status("Ожидание текста", out); }
                return;
            }
            state.dirty_since = None;
            state.dirty_first = None;
            state.unsettled = !settled;
            // В кадре нет ничего похожего на текст: OCR не запускаем. Поля «поверх оригинала»
            // всё равно обновляются: пропавший текст должен погаснуть.
            if !state.detector.has_text() && !inplace {
                state.enter(&region.id, Phase::WaitingFrame);
                self.status("Ожидание текста", out); return;
            }
        }
        if inplace {
            let rows_above = match (drawn.rect, region.rect) {
                (Some(d), Some(c)) if c.h > 0.0 => (frame.height() as f64 * (d.y - c.y) / c.h) as f32,
                _ => 0.0,
            };
            self.inplace_tick(s, region, state, frame, detected, force, now, out, rows_above).await;
            return;
        }
        state.enter(&region.id, Phase::Recognizing);
        self.status("Распознавание окна…", out);
        let recognizing = Instant::now();
        let probe = (state.stale < MAX_STALE).then(|| if detected { state.detector.clone() } else { Self::seeded_probe(s, &frame) });
        let result = match tokio::time::timeout_at(deadline, self.unless_stale(s, region, probe, self.ocr.recognize_detailed(&frame, s))).await {
            Ok(Ok(Ok(value))) => value,
            Ok(Ok(Err(e))) => { state.fail(region, format!("Ошибка OCR: {e}"), now + started.elapsed(), false, out); return; },
            Ok(Err(())) => {
                state.enter(&region.id, Phase::WaitingFrame);
                self.dropped_stale(region, state, now, out);
                return;
            },
            Err(_) => { state.fail(region, "Ошибка OCR: нет ответа за 60 с".into(), now + started.elapsed(), true, out); return; },
        };
        state.stale = 0;
        self.timings.record("ocr", recognizing.elapsed());
        let original = text::normalize(&result.text);
        let doubt = unsure(result.confidence, s.effective_minimum_confidence());
        if !text::is_meaningful(&original) || doubt.is_some() {
            state.enter(&region.id, Phase::WaitingFrame);
            let note = doubt.map(|c| format!("отброшено: уверенность {c:.0}% ниже порога {}%", s.effective_minimum_confidence()));
            self.send_preview(s, region, &frame, line_boxes(&result, (0, 0), note.as_deref()), original.clone(), String::new(), Phase::WaitingFrame, out);
            state.settle(region, out);
            match doubt {
                Some(c) => { tracing::debug!(target: "pipeline.ocr", region = %region.id, confidence = c, min = s.effective_minimum_confidence(), "OCR ignored: low confidence"); self.status(&format!("OCR: низкая уверенность ({c:.0}%), текст пропущен"), out) }
                None => self.status("OCR: текст не обнаружен", out),
            }
            return;
        }
        if !force && text::similarity(&original, &state.last_text) >= SAME_TEXT_RATIO { state.enter(&region.id, Phase::Showing); state.settle(region, out); self.status("Ожидание текста", out); return; }
        let src = s.translation_source_language();
        let translated = match self.cached(s, src, &s.translation.target_language, &original) {
            Some(t) => t,
            None => {
                state.enter(&region.id, Phase::Translating);
                self.status("Перевод…", out);
                let translating = Instant::now();
                let parts = if s.translation.changes_only { paragraphs(&result, s.appearance.inplace.line_gap_factor) } else { Vec::new() };
                let request = async {
                    if parts.len() > 1 { self.translate_changes(s, &parts, src, &s.translation.target_language).await }
                    else { self.translate_shared(s, &original, src, &s.translation.target_language).await }
                };
                match tokio::time::timeout_at(deadline, request).await {
                    Ok(Ok(t)) => { self.timings.record("translate", translating.elapsed()); t },
                    Ok(Err(e)) => {
                        let stop = e.stops_automatic_retries();
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
            boxes.push(PreviewBox { rect: whole, original: original.clone(), translation: translated.clone(), details: Vec::new(), block: None, confidence: result.confidence });
        }
        self.send_preview(s, region, &frame, boxes, original.clone(), translated.clone(), Phase::Showing, out);
        state.last_text = original.clone();
        state.recovered();
        self.last_status = "Перевод обновлён".into();
        out.send(Event::Translation { region_id: region.id.clone(), region_name: region.name.clone(), original, text: translated });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd { Reset, TranslateOnce, Auto, ReanalyzeFonts, AutoTune }

#[cfg(test)]
mod tests;
