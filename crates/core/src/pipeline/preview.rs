//! The inspector: what the last OCR run saw, what it found and what the layout engine decided, for the
//! preview window.
use super::{Event, Io, Phase, Sink, StageTiming};
use crate::{capture::Capture, layout::engine::EDGE_REACH, ocr::Ocr, settings::{CaptureRegion, Settings}, translate::Translate};
use std::sync::atomic::Ordering;

/// One recognized block (or the whole frame in the translation-window mode), px of the region frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewBox {
    pub rect: crate::layout::Rect,
    pub original: String,
    pub translation: String,
    /// What the layout engine decided for this block and why (type, font and its confidence, background);
    /// empty in the translation-window mode, which does not analyse blocks.
    pub details: Vec<String>,
    /// The field as plain data (lines, box in the crop, font, colour, type, spacing); only for fields of the
    /// in-place mode that have been read.
    pub block: Option<crate::layout::dto::TextBlock>,
    /// 0–100, as the engine reported it for this block; `None` if it does not.
    pub confidence: Option<f32>,
}

/// The frame as the engine sees it after the filters of the settings; `None` if none is on.
pub(super) fn filtered_frame(s: &Settings, frame: &image::DynamicImage) -> (Option<image::RgbaImage>, bool) {
    // What the engine will see, automatic choice included (and whether the program added to the user's filters).
    let (filters, added) = crate::ocr::filter::FilterPlan::of(&s.recognition).resolve(frame);
    ((!filters.is_identity()).then(|| filters.apply(frame).to_rgba8()), added)
}

/// «binarization + inversion», for the inspector: what the engine did to the picture before reading it.
fn describe_filters(f: &crate::ocr::filter::Preprocess) -> String {
    let mut parts = Vec::new();
    if f.auto_invert { parts.push("инверсия тёмного кадра".to_owned()); }
    if f.contrast != 0 { parts.push(format!("контраст {}", f.contrast)); }
    if f.sharpen { parts.push("резкость".into()); }
    if f.binarize { parts.push("бинаризация Оцу".into()); }
    parts.join(" + ")
}

/// The inspector line about the filters of a reading; none when the picture was read as it was.
pub(super) fn filters_note(result: &crate::ocr::OcrResult) -> Option<String> {
    (!result.filters.is_identity()).then(|| format!("фильтры ({}): {}", if result.automatic_filters { "подобраны автоматически" } else { "заданы вручную" }, describe_filters(&result.filters)))
}

/// One preview box per line the engine found; `origin` moves the crop to frame coordinates.
pub(super) fn line_boxes(result: &crate::ocr::OcrResult, origin: (u32, u32), note: Option<&str>) -> Vec<PreviewBox> {
    result.lines.iter().map(|l| {
        let mut details = vec![format!("уверенность OCR ({}): {:.0}%", result.engine, l.confidence)];
        details.extend(filters_note(result));
        details.extend(note.map(str::to_owned));
        PreviewBox { rect: l.rect.in_frame(origin), original: l.text.clone(), translation: String::new(), details, block: None, confidence: Some(l.confidence) }
    }).collect()
}

/// The inspector note for a field cut by the edge of the region: a speaker name cut at the top reads as another word
/// ("Mika" as "Muka"), and only a larger region helps.
fn edge_note(rect: &crate::layout::Rect, frame: (u32, u32)) -> Option<String> {
    let top = rect.y <= EDGE_REACH;
    let bottom = rect.bottom() >= frame.1 as f32 - EDGE_REACH;
    let edge = match (top, bottom) {
        (true, true) => "верхний и нижний края",
        (true, false) => "верхний край",
        (false, true) => "нижний край",
        (false, false) => return None,
    };
    let side = match (top, bottom) { (true, true) => "вверх и вниз", (true, false) => "вверх", _ => "вниз" };
    Some(format!("текст упирается в {edge} области: буквы могли обрезаться и читаться неверно — расширьте область {side}"))
}

/// What the layout engine decided for a field, for the inspector.
pub(super) fn describe(block: &crate::layout::engine::InplaceBlock, frame: (u32, u32)) -> Vec<String> {
    let font = &block.font;
    let mut lines = vec![format!("тип блока: {:?}", block.block_type)];
    lines.push(if font.generic {
        format!("шрифт: подходящего нет, общий «{}» — поле не рисуется", font.family)
    } else {
        format!("шрифт: ближайший доступный — {} (оценка категории {:?}), уверенность подбора {:.2}; точный шрифт оригинала не определяется", font.family, font.category, font.confidence)
    });
    if let Some(narrow) = &font.condensed_family { lines.push(format!("узкий вариант для длинного перевода: {narrow}")); }
    lines.push(format!("высота прописных оригинала: {:.0} px кадра", block.style.cap_height_px));
    if !block.lines_note.is_empty() { lines.push(block.lines_note.clone()); }
    lines.extend(edge_note(&block.text_rect, frame));
    lines.push(format!("фон: {:?}", block.background.mode));
    lines
}

/// OCR input and result of one run: the frame, the blocks found in it and the texts.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrPreview {
    pub image: image::RgbaImage,
    /// The frame after the filters of the recognition settings, if any is on.
    pub filtered: Option<image::RgbaImage>,
    /// The threshold in force, for colouring the confidence of the boxes.
    pub minimum_confidence: u32,
    pub filters: crate::ocr::filter::Preprocess,
    /// The program added filters of its own to this frame (the user's `filters` are what the settings say).
    pub filters_auto: bool,
    pub filter_noise: bool,
    pub boxes: Vec<PreviewBox>,
    pub original: String,
    pub translation: String,
    pub phase: Phase,
    pub timings: Vec<StageTiming>,
}

impl<C: Capture, O: Ocr, T: Translate + 'static> Io<C, O, T> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn send_preview(&self, s: &Settings, region: &CaptureRegion, frame: &image::DynamicImage, boxes: Vec<PreviewBox>, original: String, translation: String, phase: Phase, out: &dyn Sink) {
        if !self.preview.load(Ordering::Relaxed) { return; }
        let (filtered, filters_auto) = filtered_frame(s, frame);
        let preview = OcrPreview { image: frame.to_rgba8(), filtered, filters_auto, minimum_confidence: s.effective_minimum_confidence(), filters: crate::ocr::filter::Preprocess::of(&s.recognition), filter_noise: s.recognition.filter_noise, boxes, original, translation, phase, timings: self.timings.summary() };
        out.send(Event::OcrPreview { region_id: region.id.clone(), region_name: region.name.clone(), preview: Box::new(preview) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inspector_says_what_was_done_to_the_picture() {
        use crate::ocr::{OcrResult, filter::Preprocess};
        let plain = OcrResult::default();
        assert_eq!(filters_note(&plain), None, "nothing to say about a picture read as it was");
        let auto = OcrResult { filters: Preprocess { binarize: true, auto_invert: true, ..Preprocess::default() }, automatic_filters: true, ..OcrResult::default() };
        let note = filters_note(&auto).unwrap();
        assert!(note.contains("автоматически") && note.contains("бинаризация") && note.contains("инверсия"), "{note}");
        let manual = OcrResult { filters: Preprocess { sharpen: true, contrast: 20, ..Preprocess::default() }, ..OcrResult::default() };
        let note = filters_note(&manual).unwrap();
        assert!(note.contains("вручную") && note.contains("резкость") && note.contains("контраст 20"), "{note}");
    }

    #[test]
    fn a_field_cut_by_the_edge_of_the_region_is_pointed_out() {
        use crate::layout::Rect;
        let frame = (800, 200);
        assert_eq!(edge_note(&Rect::new(40.0, 20.0, 300.0, 30.0), frame), None);
        let top = edge_note(&Rect::new(40.0, 0.0, 120.0, 18.0), frame).expect("a name cut at the top");
        assert!(top.contains("верхний край") && top.contains("вверх"), "{top}");
        let bottom = edge_note(&Rect::new(40.0, 170.0, 300.0, 29.5), frame).expect("a line cut at the bottom");
        assert!(bottom.contains("нижний край") && bottom.contains("вниз"), "{bottom}");
    }
}
