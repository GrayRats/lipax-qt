//! The per-field stages of `InplaceEngine::begin`: signatures, typing, OCR state, font, background, typography.

use super::*;

/// Уменьшенная яркость поля 32×8: сравнение показывает, изменился ли его текст.
pub(super) fn content_signature(img: &RgbaImage, r: &Rect) -> Vec<u8> {
    let (x, y, w, h) = r.pixels(img.width(), img.height());
    let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let small = image::imageops::resize(&crop, 32, 8, FilterType::Triangle);
    small.pixels().map(|p| luma(&p.0)).collect()
}

/// A coarse picture of where the letters of a field are: the ink mask of its rectangle on a 48×12 grid. Unlike the
/// luma `content_signature` it does not move with the game's background, only with the text itself.
pub(super) fn ink_signature(mask: &InkMask, r: &Rect) -> Vec<bool> {
    const GX: usize = 48; const GY: usize = 12;
    let (x0, y0, w, h) = r.pixels(mask.w as u32, mask.h as u32);
    let (x0, y0, w, h) = (x0 as usize, y0 as usize, w as usize, h as usize);
    let mut count = vec![0u32; GX * GY];
    let mut total = vec![0u32; GX * GY];
    for dy in 0..h { for dx in 0..w {
        let cell = (dy * GY / h) * GX + dx * GX / w;
        total[cell] += 1;
        if mask.at(x0 + dx, y0 + dy) { count[cell] += 1; }
    } }
    count.iter().zip(&total).map(|(c, t)| *t > 0 && *c as f32 > 0.12 * *t as f32).collect()
}

/// Pixels of ink in the rectangle: unlike the grid of `ink_signature` it does not depend on the size of the rectangle,
/// so it can be compared between frames while the text (and its block) grows.
pub(super) fn ink_area(mask: &InkMask, r: &Rect) -> u32 {
    let (x0, y0, w, h) = r.pixels(mask.w as u32, mask.h as u32);
    (0..h as usize).map(|dy| (0..w as usize).filter(|dx| mask.at(x0 as usize + dx, y0 as usize + dy)).count() as u32).sum()
}

/// A field still being typed is read this long after its ink last grew, at most `TYPING_MAX` from the first growth.
pub(super) const TYPING_MAX: std::time::Duration = std::time::Duration::from_secs(8);

/// Share of the cells that hold ink in either picture and in only one of them.
pub(super) fn ink_distance(a: &[bool], b: &[bool]) -> f32 {
    if a.len() != b.len() { return 1.0; }
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    if either == 0 { return 0.0; }
    a.iter().zip(b).filter(|(x, y)| x != y).count() as f32 / either as f32
}

/// Beyond this distance the text under a field is another text.
pub(super) const INK_STALE: f32 = 0.4;

pub(super) fn signature_changed(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || a.is_empty() { return true; }
    let mad = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u32).sum::<u32>() as f32 / a.len() as f32;
    mad > 10.0
}

pub(super) fn color_distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (*x as f32 - y as f32).powi(2)).sum::<f32>().sqrt()
}

/// Признаки поля для выбора шрифта. Пиксели CJK-текста не отличить от моноширинного (ровный шаг)
/// и не классифицировать по засечкам, поэтому, если OCR прочитал CJK, категория берётся
/// по языку оригинала: с засечками (минтё) при высоком контрасте штрихов, иначе — готика.
pub(super) fn analysis_for(tracker: &TextBlockTracker, id: u64) -> Option<FontAnalysis> {
    let t = tracker.get(id)?;
    let mut a = tracker.analysis_with_peers(id)?;
    if t.detected_language.is_cjk() {
        a.category = if a.features.contrast >= 1.8 { FontCategory::CjkSerif } else { FontCategory::CjkSans };
        a.monospace = false;
    }
    Some(a)
}

pub(super) fn preferences(s: &Settings) -> FontPreferences {
    let category_overrides = s.appearance.inplace.font_overrides.iter()
        .filter_map(|(k, v)| serde_json::from_value::<FontCategory>(serde_json::Value::String(k.clone())).ok().map(|c| (c, v.clone())))
        .collect();
    FontPreferences { category_overrides, preferred: s.appearance.inplace.preferred_fonts.clone() }
}

/// What every field of one `begin` is prepared from: the frame, its ink mask and blocks, and what changed since the last
/// frame.
pub(super) struct FrameContext<'a> {
    pub(super) image: &'a RgbaImage,
    pub(super) mask: &'a InkMask,
    pub(super) detected: &'a [DetectedTextBlock],
    pub(super) now: Instant,
    pub(super) s: &'a Settings,
    pub(super) fw: f32,
    pub(super) fh: f32,
    pub(super) frame_changed: bool,
    pub(super) recognition_changed: bool,
    pub(super) unsettled: bool,
    pub(super) force: bool,
    pub(super) bg_changed: bool,
}

/// Stage 5a of `begin`: the content and ink signatures of a field; a translated field whose text changed is made stale.
/// Returns the signatures and the time they took.
pub(super) fn field_signatures(t: &mut TrackedTextBlock, d: &DetectedTextBlock, frame: &FrameContext, ocr_dirty: &mut bool)
    -> (Vec<u8>, Vec<bool>, std::time::Duration) {
    let mut took = std::time::Duration::ZERO;
    let started = Instant::now();
    let signature = content_signature(frame.image, &d.rect);
    took += started.elapsed();
    // The text under a translated field changed (a new dialogue): the old translation goes at once, not
    // when the new one is ready.
    let started = Instant::now();
    let ink = ink_signature(frame.mask, &d.rect);
    took += started.elapsed();
    if !t.translated_text.is_empty() && !t.stale && !t.ink_signature.is_empty() && ink_distance(&ink, &t.ink_signature) > INK_STALE {
        tracing::debug!(target: "inplace.stale", block_id = t.id, distance = ink_distance(&ink, &t.ink_signature), "the text under the field changed: its translation is withdrawn until the new text is read");
        t.stale = true;
        t.revision += 1;
        *ocr_dirty = true;
    }
    (signature, ink, took)
}

/// Stage 5b of `begin`: whether the text of a field is still being typed (it is then not read on an unsettled frame).
/// Returns that and the time the ink count took.
pub(super) fn field_typing(t: &mut TrackedTextBlock, d: &DetectedTextBlock, frame: &FrameContext) -> (bool, std::time::Duration) {
    // A field seen for the first time on a frame that never settled may be half printed: it waits one frame to
    // show whether it grows.
    let unseen = t.typing.unseen();
    let started = Instant::now();
    t.typing.observe(ink_area(frame.mask, &d.rect), frame.now);
    let took = started.elapsed();
    let settle = std::time::Duration::from_millis(frame.s.recognition.debounce_ms.max(300));
    let holding = t.typing.holding(frame.now, settle, TYPING_MAX);
    (frame.unsettled && !frame.force && (unseen || holding), took)
}

/// Stage 5c of `begin`: an earlier reading of a field is dropped when its source changed, or moved with the field; then
/// where its glyphs are and whether it is due for OCR.
pub(super) fn field_ocr_state(t: &mut TrackedTextBlock, d: &DetectedTextBlock, lh: f32, signature: &[u8], frame: &FrameContext,
    joins: &[JoinMemory], ocr_dirty: &mut bool) -> (Rect, bool) {
    let mut ocr_invalidated = false;
    if let Some(old) = t.ocr_detector_rect {
        // A union can retain the old track id and almost the same sampled signature.
        // Its new source pixels still require a complete reading, even at high IoU.
        let regrouped = old != d.rect && joins.iter().any(|m| {
            let union = m.members.iter().skip(1).fold(m.members[0], |r, b| r.union(b));
            union.iou(&d.rect) >= 0.8 && m.members.iter().any(|r| r.iou(&old) >= 0.5)
        });
        if frame.frame_changed || frame.recognition_changed || regrouped || old.iou(&d.rect) < 0.8 || signature_changed(signature, &t.content_signature) {
            t.ocr_field = None;
            t.ocr_detector_rect = None;
            t.ocr_owner = None;
            t.ocr_bounds = None;
            t.refined_rect = None;
            t.ocr_lines = None;
            *ocr_dirty = true;
            ocr_invalidated = true;
        } else if old != d.rect {
            let (dx,dy) = (d.rect.x-old.x,d.rect.y-old.y);
            if let Some(field) = &mut t.ocr_field { field.translate(dx,dy); }
            for rect in [&mut t.ocr_bounds, &mut t.refined_rect].into_iter().flatten() { rect.x += dx; rect.y += dy; }
            if let Some(rows) = &mut t.ocr_lines { for rect in rows { rect.x += dx; rect.y += dy; } }
            t.ocr_detector_rect = Some(d.rect);
            *ocr_dirty = true;
        }
    }
    // Содержимое поля изменилось — распознать заново.
    // Where the glyphs are: the OCR lines' boundary while it agrees with the detected block.
    // Cached bounds already include the OCR ink margin: validate, do not pad again.
    let rect = t.ocr_field.as_ref().map(|f| t.ocr_bounds.unwrap_or_else(|| {
            let r = f.bounds(); Rect::new(r.x-1.0,r.y-1.0,r.w+2.0,r.h+2.0)
        }))
        .or_else(|| t.ocr_bounds.filter(|b| b.iou(&d.rect) >= LINES_AGREE_IOU
        && refine_bounds(&[*b], &d.rect, lh).is_some())
        .map(|r| bounded_refinement(r, &d.rect, frame.detected.iter().map(|b| b.rect)))).unwrap_or(d.rect);
    t.refined_rect = (rect != d.rect).then_some(rect);
    let empty_due = t.rejected_ocr_at.map_or_else(|| t.original_text.is_empty(), |last|
        frame.now.saturating_duration_since(last) >= REJECTED_OCR_TTL);
    let due = frame.force || frame.frame_changed || frame.recognition_changed || ocr_invalidated || empty_due || signature_changed(signature, &t.content_signature);
    (rect, due)
}

/// Stage 5d of `begin`: the appearance of a field is probed again every 500 ms. Returns the time the probe took.
pub(super) fn probe_font(t: &mut TrackedTextBlock, d: &DetectedTextBlock, frame: &FrameContext) -> std::time::Duration {
    // Probe appearance periodically even when OCR reads identical text. Keep the family
    // stable until two confident probes agree; expensive classification is not per frame.
    if t.font_analysis.is_none() || t.last_font_probe.is_none_or(|last|
        frame.now.saturating_duration_since(last) >= std::time::Duration::from_millis(500)) {
        let started = Instant::now();
        t.observe_font(FontClassifier.classify(frame.image, frame.mask, d), frame.now);
        return started.elapsed();
    }
    std::time::Duration::ZERO
}

/// Stage 6 of `begin`: the plate of a field (over `rect`, where its glyphs are) is computed again only when its
/// surroundings or its size changed or the mode did; a patterned one is finished on its own thread. Returns the time
/// it took.
pub(super) fn field_background(t: &mut TrackedTextBlock, d: &DetectedTextBlock, rect: Rect, lh: f32, frame: &FrameContext,
    refining_threads: &Arc<std::sync::atomic::AtomicUsize>, refinements: &mut HashMap<u64, (Rect, std::sync::mpsc::Receiver<BackgroundResult>)>)
    -> std::time::Duration {
    let (image, mask, s) = (frame.image, frame.mask, frame.s);
    let mut took = std::time::Duration::ZERO;
    // Фон — только если он заметно изменился (подвижная сцена) или сменили режим.
    let margin = BackgroundAnalyzer::margin(lh);
    let started = Instant::now();
    let bg_sig = BackgroundAnalyzer::signature(image, &rect, margin);
    took += started.elapsed();
    let moved = (t.previous_rect.x - t.current_rect.x).abs() > 3.0 || (t.previous_rect.y - t.current_rect.y).abs() > 3.0;
    // A field that grew, shrank or was divided in place needs a plate of its new size, even if it did not move
    // and its surroundings look the same.
    let reshaped = [(t.background_for.x, rect.x), (t.background_for.y, rect.y), (t.background_for.w, rect.w), (t.background_for.h, rect.h)]
        .iter().any(|(old, new)| (old - new).abs() > 3.0);
    if t.background.is_none() || frame.bg_changed || moved || reshaped || t.background_signature.is_none_or(|old| color_distance(old, bg_sig) > 18.0) {
        let started = Instant::now();
        let analysis = BackgroundAnalyzer.analyze(image, mask, &rect, margin);
        // On a patterned background the full fill (an exemplar search, up to a few hundred ms) is computed on
        // its own thread; the quick one is drawn meanwhile and replaced when it is ready.
        let quick = BackgroundInpainter.render_quick(image, mask, &rect, lh, analysis.clone(), &s.appearance.inplace, d.block_type);
        use std::sync::atomic::Ordering::SeqCst;
        if quick.refinable && refining_threads.load(SeqCst) < MAX_REFINEMENTS {
            let (frame, ink, settings, block) = (image.clone(), mask.clone(), s.appearance.inplace.clone(), d.block_type);
            let (sender, receiver) = std::sync::mpsc::channel();
            let running = refining_threads.clone();
            running.fetch_add(1, SeqCst);
            let guard = RefiningThread(running.clone());
            let spawned = std::thread::Builder::new().name("lipax-background".into()).spawn(move || {
                let _guard = guard;
                let _ = sender.send(BackgroundInpainter.render(&frame, &ink, &rect, lh, analysis, &settings, block));
            });
            // A thread that could not be started has dropped its guard with the closure: the count is right.
            if spawned.is_ok() { refinements.insert(t.id, (rect, receiver)); }
        }
        let mut quick = quick;
        // No thread to finish it (too many at once, or none could be started): the quick fill is final.
        if !refinements.contains_key(&t.id) { quick.refinable = false; }
        t.background = Some(quick);
        took += started.elapsed();
        t.background_for = rect;
        t.background_signature = Some(bg_sig);
        t.revision += 1;
    }
    took
}

/// Stage 7 of `begin`: the typography of a field is estimated again for a new field and when its size or number of
/// lines changed noticeably. Returns the time it took.
pub(super) fn field_typography(t: &mut TrackedTextBlock, d: &DetectedTextBlock, frame: &FrameContext) -> std::time::Duration {
    // Типографика — для нового поля и при заметной смене размера или числа строк.
    let resized = (t.typography_rect.w - d.rect.w).abs() > 0.1 * d.rect.w || (t.typography_rect.h - d.rect.h).abs() > 0.1 * d.rect.h;
    // The lines the OCR engine found count while they still lie on the detected block.
    if t.ocr_field.is_none() && t.ocr_lines.as_ref().is_some_and(|lines| !lines_agree(lines, &d.rect)) {
        t.ocr_lines = None;
        t.ocr_bounds = None;
        t.refined_rect = None;
        t.lines_note = "строки по детектору: геометрия OCR больше не совпадает с блоком".into();
    }
    let expected_lines = t.ocr_lines.as_ref().map_or(d.lines.len(), Vec::len);
    let relined = t.typography.as_ref().is_some_and(|e| e.lines as usize != expected_lines);
    if t.typography.is_none() || resized || relined {
        let started = Instant::now();
        let bg = t.background.as_ref().map(|b| b.color).unwrap_or([0; 3]);
        t.typography = Some(TypographyEstimator.estimate_from(d, t.font_analysis.as_ref().unwrap(), bg, frame.fw, t.ocr_lines.as_deref()));
        t.typography_rect = d.rect;
        t.revision += 1;
        return started.elapsed();
    }
    std::time::Duration::ZERO
}

/// Stage 8 of `begin`: the OCR job of a field, its block with a margin cropped from the frame. Returns the job and the
/// crop (frame pixels).
pub(super) fn ocr_job(id: u64, d: &DetectedTextBlock, lh: f32, frame: &FrameContext) -> (OcrJob, Rect) {
    let image = frame.image;
    let (x, y, w, h) = d.rect.expand((0.25 * lh).max(3.0), frame.fw, frame.fh).pixels(image.width(), image.height());
    let job = OcrJob { id, rect: d.rect, origin: (x, y), image: DynamicImage::ImageRgba8(image::imageops::crop_imm(image, x, y, w, h).to_image()) };
    (job, Rect::new(x as f32, y as f32, w as f32, h as f32))
}
