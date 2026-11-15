//! The in-place mode: every field of a region is read and translated on its own, and only when it changed.
use super::preview::{describe, filtered_frame, filters_note};
use super::state::{ERROR_BUDGET, MAX_REACH_UP};
use super::{bounded, translation_context, unsure, Event, Io, OcrPreview, Phase, PreviewBox, RegionState, Sink, FIELD_CONCURRENCY, MAX_STALE};
use crate::{capture::Capture, ocr::Ocr, settings::{CaptureRegion, Settings}, text, translate::{Translate, TranslateError}};
use std::{collections::HashMap, sync::{Arc, atomic::Ordering}, time::{Duration, Instant}};

/// How long the inspector keeps showing a field whose text was ignored as unsure.
const IGNORED_SHOWN: Duration = Duration::from_secs(3);

/// The lines of a reading cover most of the width of the crop it was made from.
fn spans_the_crop(reading: &crate::ocr::OcrResult, width: u32) -> bool {
    if reading.lines.is_empty() { return false; }
    let left = reading.lines.iter().map(|l| l.rect.x).fold(f32::MAX, f32::min);
    let right = reading.lines.iter().map(|l| l.rect.x + l.rect.w).fold(f32::MIN, f32::max);
    (right - left) >= 0.55 * width as f32
}

/// One field of the in-place mode. A small crop that came back empty or unsure is read again in the variants of
/// `filter::faint_variants`; the best reading wins, and one that is no better than the first is not used.
pub async fn read_field<O: Ocr>(ocr: &O, image: &image::DynamicImage, settings: &Settings) -> Result<crate::ocr::OcrResult, crate::ocr::OcrError> {
    use crate::ocr::filter;
    let first = ocr.recognize_detailed(image, settings).await?;
    let floor = settings.effective_minimum_confidence();
    let weak = first.text.trim().is_empty() || unsure(first.confidence, floor).is_some();
    // Only small crops: a large field that reads as nothing is scenery, not faint text.
    if !weak || image.height() > 120 || image.width() > 900 { return Ok(first); }
    let mut best = first;
    for (variant, factor) in filter::faint_variants(image) {
        let Ok(mut reading) = ocr.recognize_detailed(&variant, settings).await else { continue };
        filter::rescale_lines(&mut reading, factor);
        // A fragment (one word of a longer line, or a mesh read as letters) must not replace the field.
        if !spans_the_crop(&reading, image.width()) || reading.confidence.is_some_and(|c| c < 80.0) { continue; }
        if std::ptr::eq(filter::better_reading(&best, &reading, floor), &reading) { best = reading; }
    }
    Ok(best)
}

/// The first error and the other distinct ones of the same scene, at most three, in one message.
pub(super) fn join_messages(first: &str, notes: &[String]) -> String {
    let mut all: Vec<&str> = vec![first];
    all.extend(notes.iter().map(String::as_str).filter(|n| *n != first));
    let more = all.len().saturating_sub(3);
    all.truncate(3);
    let mut text = all.join(" · ");
    if more > 0 { text.push_str(&format!(" · и ещё {more}")); }
    text
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Io<C, O, T> {
    /// «Поверх исходного текста»: каждое поле распознаётся и переводится отдельно, и только если его
    /// содержимое изменилось. Поля одной сцены обрабатываются конкурентно (не более
    /// `FIELD_CONCURRENCY` одновременно): задержка сцены из имени, реплики и двух кнопок — это
    /// самая медленная операция, а не их сумма. Одинаковые тексты (две кнопки «OK») переводятся один раз.
    ///
    /// Тик можно отменить в любой точке `await`: движок остаётся в состоянии области, а подпись
    /// содержимого поля запоминается только в `complete`, поэтому незавершённое поле на следующем
    /// кадре распознаётся снова.
    ///
    /// `rows_above`: the top rows of `frame` that lie above the drawn region (the capture was extended upwards).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn inplace_tick(&mut self, s: &Settings, region: &CaptureRegion, state: &mut RegionState, frame: image::DynamicImage, detected: bool,
                          force: bool, now: Instant, out: &dyn Sink, rows_above: f32) {
        // The engine takes the frame; the preview needs its own copy, made only while it is open.
        let preview_image = self.preview.load(Ordering::Relaxed).then(|| frame.to_rgba8());
        // What the picture looked like when OCR started; the engine takes `frame` itself.
        // Only a tick that did not look at the frame yet (forced, rescan) needs its own reference, and such a tick reads.
        let seeded = (!detected && state.stale < MAX_STALE).then(|| Self::seeded_probe(s, &frame));
        let engine = state.inplace.get_or_insert_with(Default::default);
        let laying_out = Instant::now();
        engine.set_unsettled(state.unsettled);
        engine.set_reach_above(rows_above);
        state.engine_capture = (rows_above > 0.0).then_some(region.rect).flatten();
        let frame_height = frame.height().max(1) as f64;
        let jobs = engine.begin(frame, s, now, force);
        // The top edge cut a line (a speaker name over the dialogue): next time the capture starts higher by its height
        // and the region is read again at once. Not above the window, and not farther than `MAX_REACH_UP`.
        if let (true, Some(line), Some(drawn), Some(captured)) = (s.appearance.inplace.extend_up, engine.cut_at_top(), state.reach_for, region.rect) {
            let per_row = captured.h / frame_height;
            let wanted = (state.reach_up + (1.25 * line as f64 + 4.0) * per_row).min(drawn.y).min(MAX_REACH_UP);
            if wanted > state.reach_up + 0.5 * per_row {
                tracing::info!(target: "pipeline.inplace", region = %region.id, from = state.reach_up, to = wanted, line_px = line,
                    "text cut by the top edge of the region: the capture starts higher");
                state.reach_up = wanted;
                state.rescan = true;
            }
        }
        tracing::debug!(target: "pipeline.inplace", region = %region.id, stage = "detection", ocr_jobs = jobs.len(), "inplace frame analyzed");
        self.timings.record("layout", laying_out.elapsed());
        let budget = state.first_error.map(|first| ERROR_BUDGET.saturating_sub(now.saturating_duration_since(first))).unwrap_or(ERROR_BUDGET);
        let deadline = tokio::time::Instant::now() + budget;
        let src = s.translation_source_language().to_string();
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
            let pinned: HashMap<u64, Settings> = if s.recognition.engine == "auto" {
                jobs.iter().filter_map(|j| state.field_engine.get(&j.id).map(|engine| (j.id, *engine)))
                    .map(|(id, engine)| (id, { let mut value = s.clone(); value.recognition.engine = engine.into(); value })).collect()
            } else { HashMap::new() };
            let pinned = &pinned;
            let all = bounded(jobs, FIELD_CONCURRENCY, |job| {
                let settings = pinned.get(&job.id).unwrap_or(s);
                async move { (job.id, read_field(ocr, &job.image, settings).await) }
            });
            let probe = (state.stale < MAX_STALE).then(|| seeded.unwrap_or_else(|| state.detector.clone()));
            match tokio::time::timeout_at(deadline, self.unless_stale(s, region, probe, all)).await {
                Ok(Ok(done)) => { state.stale = 0; self.timings.record("ocr", recognizing.elapsed()); recognized = done },
                // The fields stay due: their signatures are stored only when a field is completed.
                Ok(Err(())) => self.dropped_stale(region, state, now, out),
                Err(_) => failure = Some(("Ошибка OCR: нет ответа за 60 с".into(), true)),
            }
        }
        // 2. Cached translations are applied at once; the rest wait for the translator.
        let engine = state.inplace.get_or_insert_with(Default::default);
        tracing::debug!(target: "pipeline.inplace", region = %region.id, stage = "ocr", results = recognized.len(), "OCR results received");
        let mut pending: Vec<(u64, String)> = Vec::new();
        for (id, result) in recognized {
            if s.recognition.engine == "auto" {
                match &result {
                    // The second opinion of `auto` (MeikiOCR for Japanese, RapidOCR, or PaddleOCR in its place) was needed: it reads the field from now on.
                    Ok(r) if matches!(r.engine, "paddleocr" | "rapidocr" | "meikiocr") => {
                        if state.field_engine.len() >= 64 { state.field_engine.clear(); }
                        if state.field_engine.insert(id, r.engine) != Some(r.engine) {
                            tracing::debug!(target: "pipeline.ocr", region = %region.id, field = id, engine = r.engine, "OCR auto: the field is read with this engine from now on");
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
                    let mut original = text::normalize(&result.text);
                    let (rect, origin) = origins.get(&id).copied().unwrap_or_default();
                    if let Some(c) = unsure(result.confidence, s.effective_minimum_confidence()) {
                        // The engine is not sure what it read: keep the field as it was, do not translate noise.
                        tracing::debug!(target: "pipeline.ocr", region = %region.id, field = id, confidence = c, min = s.effective_minimum_confidence(), "OCR ignored: low confidence");
                        let details = vec![format!("OCR ({}) отброшен: уверенность {c:.0}% ниже порога {}%", result.engine, s.effective_minimum_confidence())];
                        state.ignored.insert(id, (now, PreviewBox { rect, original: original.clone(), translation: String::new(), details, block: None, confidence: result.confidence }));
                        newly_ignored = true;
                        engine.reject_ocr(id, s);
                        continue;
                    }
                    state.ignored.remove(&id);
                    // A short label that only changed a look-alike glyph (15 → is) on a weak reading is the same
                    // label: the solid reading stays, nothing is translated or redrawn again. Decided before the
                    // lines are observed: observing them renews the field and would redraw it.
                    if let Some(before) = engine.field_text(id)
                        && crate::ocr::lookalike::keeps_earlier(before, state.field_confidence.get(&id).copied(), &original, result.confidence) {
                        tracing::debug!(target: "pipeline.ocr", region = %region.id, field = id, kept = before, ignored = %original, "OCR flip of look-alike glyphs ignored: the earlier reading was solid");
                        state.field_ocr.insert(id, format!("OCR ({}): «{original}» — похожие символы, оставлено «{before}»", result.engine));
                        engine.reject_ocr(id, s);
                        continue;
                    }
                    // The lines the engine found describe the structure of the field better than the picture does.
                    let observation = engine.observe_ocr_result(id, &result, origin, s);
                    original = observation.text;
                    if observation.verdict == crate::layout::engine::LinesVerdict::Split {
                        // Read the corrected split/union crops on the next tick before translating.
                        state.rescan = true;
                        state.field_ocr.remove(&id);
                        continue;
                    }
                    state.field_lines.insert(id, (origin, result.lines.clone()));
                    let read = match result.confidence {
                        Some(c) => format!("OCR ({}): уверенность {c:.0}%, строк {}", result.engine, result.lines.len().max(1)),
                        None => format!("OCR ({}): уверенность не сообщается", if result.engine.is_empty() { "?" } else { result.engine }),
                    };
                    state.field_ocr.insert(id, match filters_note(&result) { Some(note) => format!("{read}; {note}"), None => read });
                    if !text::is_meaningful(&original) { engine.reject_ocr(id, s); continue; }
                    match result.confidence {
                        Some(c) => {
                            if state.field_confidence.len() >= 256 { state.field_confidence.clear(); }
                            state.field_confidence.insert(id, c);
                        }
                        None => { state.field_confidence.remove(&id); }
                    }
                    match engine.cached_translation(id, &original).or_else(|| self.cached(s, &src, &s.translation.target_language, &original)) {
                        Some(t) => engine.complete(id, Some((original, t)), s),
                        None => pending.push((id, original)),
                    }
                }
            }
        }
        // Crops queued for regrouping are read again before translation: never concatenate guesses.
        if engine.needs_rescan() { state.rescan = true; }
        pending.retain(|(id,_)| !engine.is_regrouping(*id));
        // 3. Translation of the distinct texts.
        if !pending.is_empty() && !matches!(failure, Some((_, true))) {
            state.enter(&region.id, Phase::Translating);
            self.status("Перевод…", out);
            let translating = Instant::now();
            let mut distinct: Vec<&str> = pending.iter().map(|(_, t)| t.as_str()).collect();
            distinct.sort_unstable();
            distinct.dedup();
            let io = &*self;
            let (src_ref, target) = (src.as_str(), s.translation.target_language.as_str());
            let all = io.translate_texts(s, distinct, src_ref, target);
            let translated: HashMap<String, Result<String, Arc<TranslateError>>> = match tokio::time::timeout_at(deadline, all).await {
                Ok(done) => { self.timings.record("translate", translating.elapsed()); done },
                Err(_) => { failure.get_or_insert(("Ошибка перевода: нет ответа за 60 с".into(), true)); HashMap::new() }
            };
            let engine = state.inplace.get_or_insert_with(Default::default);
            for (id, original) in pending {
                match translated.get(&original) {
                    Some(Ok(t)) => { engine.complete(id, Some((original, t.clone())), s); }
                    Some(Err(e)) => {
                        let stop = e.stops_automatic_retries();
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
            let (known, read) = (&state.field_ocr, &state.field_lines);
            state.preview_boxes = frame.blocks.iter().map(|b| {
                let mut details = describe(b, frame.frame);
                details.extend(known.get(&b.id).cloned());
                let block = read.get(&b.id).map(|(origin, lines)| crate::layout::dto::TextBlock::of_field(b, lines, *origin));
                PreviewBox { rect: b.text_rect, original: b.original.clone(), translation: b.translation.clone(), details, block, confidence: read.get(&b.id).filter(|(_, l)| !l.is_empty()).map(|(_, l)| l.iter().map(|x| x.confidence).sum::<f32>() / l.len() as f32) }
            }).collect();
        }
        if let Some(image) = preview_image.filter(|_| result.is_some() || newly_ignored) {
            state.ignored.retain(|_, (when, _)| now.saturating_duration_since(*when) < IGNORED_SHOWN);
            let mut boxes = state.preview_boxes.clone();
            boxes.extend(state.ignored.values().map(|(_, b)| b.clone()));
            let join = |f: fn(&PreviewBox) -> &str| boxes.iter().map(f).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            let (original, translation) = (join(|b| &b.original), join(|b| &b.translation));
            let (filtered, filters_auto) = filtered_frame(s, &image::DynamicImage::ImageRgba8(image.clone()));
            let preview = OcrPreview { image, filtered, filters_auto, minimum_confidence: s.effective_minimum_confidence(), filters: crate::ocr::filter::Preprocess::of(&s.recognition), filter_noise: s.recognition.filter_noise, boxes, original, translation, phase: state.phase, timings: self.timings.summary() };
            out.send(Event::OcrPreview { region_id: region.id.clone(), region_name: region.name.clone(), preview: Box::new(preview) });
        }
        if let Some(mut frame) = result {
            // A capture other than the drawn region (extended upwards) tells the overlay where its pixels lie.
            frame.capture = state.engine_capture;
            tracing::debug!(target: "pipeline.inplace", region = %region.id, stage = "event_emitted", blocks = frame.blocks.len(), undrawable = frame.undrawable.len(), new_translations = frame.new_translations.len(), "Event::Inplace emitted");
            self.last_status = "Перевод обновлён".into();
            out.send(Event::Inplace { region_id: region.id.clone(), region_name: region.name.clone(), frame: Box::new(frame) });
        }
        match failure {
            Some((first, terminal)) => state.fail(region, join_messages(&first, &notes), now, terminal, out),
            None => state.settle(region, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_errors_of_one_scene_are_all_reported_up_to_three() {
        let notes: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
        assert_eq!(join_messages("a", &notes), "a · b · c · и ещё 2");
        assert_eq!(join_messages("only", &[]), "only");
        assert_eq!(join_messages("a", &["a".into(), "b".into()]), "a · b", "no repeats");
    }

    /// Reads nothing from a crop as it is; enlarged ×3 it reads a full line, at the geometry of the enlarged crop.
    struct FaintOcr;
    impl Ocr for FaintOcr {
        async fn recognize(&self, img: &image::DynamicImage, s: &Settings) -> Result<String, crate::ocr::OcrError> {
            Ok(self.recognize_detailed(img, s).await?.text)
        }
        async fn recognize_detailed(&self, img: &image::DynamicImage, _: &Settings) -> Result<crate::ocr::OcrResult, crate::ocr::OcrError> {
            use crate::{layout::CropRect, ocr::{OcrLine, OcrResult}};
            if img.width() < 300 { return Ok(OcrResult::default()); }
            let line = OcrLine { rect: CropRect::in_space(3.0, 3.0, img.width() as f32 - 6.0, 12.0), text: "Relative velocity calculator".into(), confidence: 95.0 };
            Ok(OcrResult { text: line.text.clone(), lines: vec![line], confidence: Some(95.0), engine: "rapidocr", ..OcrResult::default() })
        }
    }

    #[tokio::test]
    async fn a_faint_crop_is_read_again_and_its_geometry_goes_back_to_the_crop() {
        let crop = image::DynamicImage::new_rgb8(120, 16);
        let reading = read_field(&FaintOcr, &crop, &Settings::default()).await.unwrap();
        assert_eq!(reading.text, "Relative velocity calculator");
        let line = &reading.lines[0];
        assert!((line.rect.w - 118.0).abs() < 0.01 && (line.rect.h - 4.0).abs() < 0.01, "divided by the enlargement: {line:?}");
    }

    #[tokio::test]
    async fn a_large_empty_crop_is_not_read_again() {
        // Scenery that reads as nothing stays nothing: only small crops get the second chances.
        let reading = read_field(&FaintOcr, &image::DynamicImage::new_rgb8(200, 400), &Settings::default()).await.unwrap();
        assert!(reading.text.is_empty() || reading.lines.len() == 1);
        let tall = image::DynamicImage::new_rgb8(100, 400);
        assert!(read_field(&FaintOcr, &tall, &Settings::default()).await.unwrap().text.is_empty());
    }
}
