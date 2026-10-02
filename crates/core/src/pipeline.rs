//! Capture → Change Detection → debounce → OCR → сравнение текста → кэш → перевод.
//!
//! Цикл последовательный: пока идут OCR или перевод, новые тики не обрабатываются
//! (`MissedTickBehavior::Skip`), поэтому очередь не копится, а устаревшие кадры
//! отбрасываются сами — следующий кадр всегда берётся свежим.

use crate::cache::TranslationCache;
use crate::capture::Capture;
use crate::detect::ChangeDetector;
use crate::ocr::Ocr;
use crate::settings::Settings;
use crate::tesseract::primary_lang;
use crate::text;
use crate::translate::{Translate, tess_to_iso};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;

/// Текст считается прежним, если похожесть не ниже порога (шум OCR).
const SAME_TEXT_RATIO: f32 = 0.92;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Status(String),
    Translation { original: String, text: String },
    Error(String),
}

pub struct Pipeline<C, O, T> {
    capture: C,
    ocr: O,
    translator: T,
    detector: ChangeDetector,
    cache: TranslationCache,
    last_text: String,
    dirty_since: Option<Instant>,
}

impl<C: Capture, O: Ocr, T: Translate> Pipeline<C, O, T> {
    pub fn new(capture: C, ocr: O, translator: T) -> Self {
        Self {
            capture,
            ocr,
            translator,
            detector: ChangeDetector::new(),
            cache: TranslationCache::new(512),
            last_text: String::new(),
            dirty_since: None,
        }
    }

    /// Сбросить состояние (смена окна/области/языка).
    pub fn reset(&mut self) {
        self.detector.reset();
        self.last_text.clear();
        self.dirty_since = None;
    }

    /// Одна итерация. `force` — ручной режим: без change detection и debounce.
    pub async fn tick(&mut self, s: &Settings, force: bool, now: Instant, out: &mpsc::UnboundedSender<Event>) {
        let (Some(window), Some(region)) = (&s.window, s.region) else {
            let _ = out.send(Event::Status("Выберите окно и область".into()));
            return;
        };
        let frame = match self.capture.grab(window, region).await {
            Ok(f) => f,
            Err(e) => {
                let _ = out.send(Event::Error(e.to_string()));
                return;
            }
        };

        if !force {
            if self.detector.changed(&frame, s.sensitivity) {
                // Кадр ещё меняется (анимация появления текста) — ждём стабилизации.
                self.dirty_since = Some(now);
                return;
            }
            match self.dirty_since {
                Some(t) if now.duration_since(t) >= Duration::from_millis(s.debounce_ms) => {}
                _ => return,
            }
        }
        self.dirty_since = None;

        let raw = match self.ocr.recognize(&frame, s).await {
            Ok(t) => t,
            Err(e) => {
                let _ = out.send(Event::Error(e.to_string()));
                return;
            }
        };
        let original = text::normalize(&raw);
        if !text::is_meaningful(&original) {
            return;
        }
        if !force && text::similarity(&original, &self.last_text) >= SAME_TEXT_RATIO {
            return;
        }
        self.last_text = original.clone();

        let src = tess_to_iso(primary_lang(&s.source_lang));
        let dst = &s.target_lang;
        let translated = match self.cache.get(src, dst, &original) {
            Some(t) => t,
            None => match self.translator.translate(s, &original, src, dst).await {
                Ok(t) => {
                    self.cache.put(src, dst, &original, t.clone());
                    t
                }
                Err(e) => {
                    // Не запоминаем неудачный текст, чтобы следующая попытка повторила перевод.
                    self.last_text.clear();
                    let _ = out.send(Event::Error(e.to_string()));
                    return;
                }
            },
        };
        let _ = out.send(Event::Translation { original, text: translated });
    }

    /// Долгоживущая задача. Авто-режим работает, пока `running == true` и включён
    /// `auto_translate`; ручной перевод и сброс приходят командами.
    pub async fn run(
        mut self,
        settings: watch::Receiver<Settings>,
        running: watch::Receiver<bool>,
        mut cmds: mpsc::UnboundedReceiver<Cmd>,
        out: mpsc::UnboundedSender<Event>,
    ) {
        let mut interval_ms = settings.borrow().interval_ms.max(50);
        let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let s = settings.borrow().clone();
                    if s.interval_ms.max(50) != interval_ms {
                        interval_ms = s.interval_ms.max(50);
                        ticker = tokio::time::interval(Duration::from_millis(interval_ms));
                        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    }
                    if *running.borrow() && s.auto_translate {
                        self.tick(&s, false, Instant::now(), &out).await;
                    }
                }
                cmd = cmds.recv() => match cmd {
                    None => break,
                    Some(Cmd::Reset) => self.reset(),
                    Some(Cmd::TranslateOnce) => {
                        let s = settings.borrow().clone();
                        self.tick(&s, true, Instant::now(), &out).await;
                    }
                },
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    /// Сбросить состояние (сменились окно, область или язык).
    Reset,
    /// Ручной режим: один проход без change detection и debounce.
    TranslateOnce,
}

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
            let v = *self.0.lock().unwrap();
            Ok(DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 50, Rgba([v, v, v, 255]))))
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
            ..Settings::default()
        }
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<Event> {
        let mut v = Vec::new();
        while let Ok(e) = rx.try_recv() {
            v.push(e);
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
        assert_eq!(drain(&mut rx), vec![Event::Translation { original: "Where are you going?".into(), text: "RU:Where are you going?".into() }]);

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
    async fn no_window_reports_status() {
        let cap = Arc::new(MockCapture(Mutex::new(0)));
        let mut p = Pipeline::new(cap, MockOcr(Mutex::new(String::new()), Arc::new(AtomicUsize::new(0))), MockTr(Arc::new(AtomicUsize::new(0))));
        let (tx, mut rx) = mpsc::unbounded_channel();
        p.tick(&Settings::default(), false, Instant::now(), &tx).await;
        assert!(matches!(drain(&mut rx)[0], Event::Status(_)));
    }
}
