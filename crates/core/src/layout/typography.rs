//! Типографика поля: каждое свойство оценивается отдельно и отдельно заменяется ручным
//! значением из настроек (`PropertyMode`). Ручной шрифт не отключает автоматический размер,
//! ручной трекинг — автоматическое выравнивание и т. д.
//!
//! Здесь оценки в пикселях кадра. Кегль по высоте прописных переводят в пиксели экрана
//! уже с реальными метриками выбранного шрифта (Qt `QFontMetricsF`) — в приложении.

use super::block_detector::DetectedTextBlock;
use super::font_classifier::FontAnalysis;
use super::font_matcher::FontSelection;
use super::{FontWeight, Padding, Rect, TextAlignment, WrapMode, contrast_ratio, parse_hex_color};
use crate::settings::InplaceSettings;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHeightEstimate {
    /// Расстояние между строками, px кадра.
    pub absolute_px: f32,
    /// К высоте прописных (кегль ещё не известен): переводится в множитель при подгонке.
    pub proportional: f32,
    pub confidence: f32,
}

/// Автоматические оценки по оригиналу, px кадра.
#[derive(Debug, Clone, PartialEq)]
pub struct TypographyEstimate {
    pub cap_height_px: f32,
    pub lines: u32,
    pub line_height: LineHeightEstimate,
    pub weight: FontWeight,
    pub italic: bool,
    pub alignment: TextAlignment,
    pub letter_spacing_px: f32,
    pub text_color: [u8; 3],
    /// Цвет оригинала хорошо отличается от фона и пригоден для перевода.
    pub text_color_reliable: bool,
    pub padding: Padding,
}

/// Что получает отрисовка: ручные значения пользователя или автоматические оценки.
/// `None` у размера и межстрочного интервала — автоматический подбор при подгонке.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslationTextStyle {
    pub font_family: String,
    /// Ручной кегль, px экрана.
    pub font_size: Option<f32>,
    /// Высота прописных оригинала, px кадра: из неё и метрик шрифта — автоматический кегль.
    pub cap_height_px: f32,
    pub font_weight: FontWeight,
    pub italic: bool,
    /// Ручной множитель или `None` — по расстоянию между строками оригинала.
    pub line_height: Option<f32>,
    pub line_height_px: Option<f32>,
    /// Трекинг: ручной — px экрана, автоматический — px кадра (`letter_spacing_manual`).
    pub letter_spacing: f32,
    pub letter_spacing_manual: bool,
    pub alignment: TextAlignment,
    /// `None` — выбирается при подгонке по языку, длине и числу строк оригинала.
    pub wrap_mode: Option<WrapMode>,
    pub text_color: [u8; 3],
    /// Поля: ручные — px экрана, автоматические — px кадра (`padding_manual`).
    pub padding: Padding,
    pub padding_manual: bool,
    pub source_lines: u32,
    pub min_font_size: f32,
    pub max_font_size: f32,
    pub allow_condensed: bool,
}

#[derive(Default)]
pub struct TypographyEstimator;

fn spread(v: &[f32]) -> f32 {
    if v.len() < 2 { return 0.0; }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
}


impl TypographyEstimator {
    /// `frame_w` — ширина кадра области: одиночную строку выравнивают по её положению в кадре.
    pub fn estimate(&self, block: &DetectedTextBlock, analysis: &FontAnalysis, background: [u8; 3], frame_w: f32) -> TypographyEstimate {
        self.estimate_from(block, analysis, background, frame_w, None)
    }

    /// Like `estimate`, but the structure of the lines (how many, the step between them, which edge
    /// they share) comes from `ocr_lines` when given: the engine that read the text knows its
    /// lines better than the picture analysis. The detector still supplies everything else
    /// (letter gaps, colour) and is the fallback when the engine gave no geometry.
    pub fn estimate_from(&self, block: &DetectedTextBlock, analysis: &FontAnalysis, background: [u8; 3], frame_w: f32, ocr_lines: Option<&[Rect]>) -> TypographyEstimate {
        let lines: Vec<&Rect> = match ocr_lines {
            Some(found) if !found.is_empty() => found.iter().collect(),
            _ => block.lines.iter().map(|l| &l.rect).collect(),
        };
        let cap = analysis.cap_height_px.max(1.0);
        // Межстрочный: медиана расстояний между верхами соседних строк.
        let mut steps: Vec<f32> = lines.windows(2).map(|p| p[1].y - p[0].y).filter(|d| *d > 0.0).collect();
        steps.sort_by(|a, b| a.total_cmp(b));
        let line_height = match steps.get(steps.len() / 2) {
            Some(&step) => LineHeightEstimate { absolute_px: step, proportional: step / cap, confidence: if steps.len() >= 2 { 0.9 } else { 0.6 } },
            None => LineHeightEstimate { absolute_px: block.line_height() * 1.2, proportional: 1.2 * block.line_height() / cap, confidence: 0.2 },
        };
        // Выравнивание — по геометрии строк: какой край у них совпадает лучше всего.
        let alignment = if lines.len() >= 2 {
            let lefts: Vec<f32> = lines.iter().map(|r| r.x).collect();
            let centers: Vec<f32> = lines.iter().map(|r| r.center().0).collect();
            let rights: Vec<f32> = lines.iter().map(|r| r.right()).collect();
            let (l, c, r) = (spread(&lefts), spread(&centers), spread(&rights));
            if l <= c && l <= r { TextAlignment::Left } else if c <= r { TextAlignment::Center } else { TextAlignment::Right }
        } else {
            let r = block.rect;
            let (left_gap, right_gap) = (r.x, frame_w - r.right());
            if (left_gap - right_gap).abs() <= 0.08 * frame_w { TextAlignment::Center }
            else if right_gap < left_gap * 0.5 { TextAlignment::Right } else { TextAlignment::Left }
        };
        // Трекинг: межбуквенный просвет к высоте прописных сверх обычного (~0.15).
        let mut gaps: Vec<f32> = Vec::new();
        for l in &block.lines {
            let mut g = l.glyphs.clone();
            g.sort_by(|a, b| a.x.total_cmp(&b.x));
            gaps.extend(g.windows(2).map(|p| p[1].x - p[0].right()).filter(|d| *d >= 0.0 && *d < 0.6 * cap));
        }
        gaps.sort_by(|a, b| a.total_cmp(b));
        let letter_spacing_px = gaps.get(gaps.len() / 2).map(|g| {
            let extra = g - 0.15 * cap;
            if extra.abs() < 0.04 * cap { 0.0 } else { extra.clamp(-0.05 * cap, 0.2 * cap) }
        }).unwrap_or(0.0);
        let reliable = contrast_ratio(block.ink_color, background) >= 2.5;
        let text_color = if reliable { block.ink_color } else if contrast_ratio([255; 3], background) >= contrast_ratio([0; 3], background) { [255; 3] } else { [0; 3] };
        TypographyEstimate {
            cap_height_px: cap,
            lines: lines.len() as u32,
            line_height,
            weight: analysis.weight,
            italic: analysis.italic,
            alignment,
            letter_spacing_px,
            text_color,
            text_color_reliable: reliable,
            padding: Padding::uniform((0.3 * block.line_height()).clamp(2.0, 16.0)),
        }
    }

    /// Ручные значения пользователя поверх автоматических — по каждому свойству отдельно.
    pub fn resolve(&self, est: &TypographyEstimate, font: &FontSelection, s: &InplaceSettings) -> TranslationTextStyle {
        let weight = s.font_weight.resolve(est.weight).closest(&font.available_weights);
        TranslationTextStyle {
            font_family: s.font_family.resolve(font.family.clone()),
            font_size: s.font_size.manual().copied(),
            cap_height_px: est.cap_height_px,
            font_weight: weight,
            italic: s.italic.resolve(est.italic),
            line_height: s.line_height.manual().copied(),
            line_height_px: (est.lines > 1 && est.line_height.confidence >= 0.5).then_some(est.line_height.absolute_px),
            letter_spacing: s.letter_spacing.resolve(est.letter_spacing_px),
            letter_spacing_manual: s.letter_spacing.manual().is_some(),
            alignment: s.alignment.resolve(est.alignment),
            wrap_mode: s.wrap_mode.manual().copied(),
            text_color: s.text_color.manual().and_then(|c| parse_hex_color(c)).unwrap_or(est.text_color),
            padding: s.padding.resolve(est.padding),
            padding_manual: s.padding.manual().is_some(),
            source_lines: est.lines,
            min_font_size: s.minimum_font_size,
            max_font_size: s.maximum_font_size,
            allow_condensed: s.allow_condensed_fallback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::block_detector::BlockDetector;
    use crate::layout::font_classifier::FontClassifier;
    use crate::layout::testing::*;
    use crate::layout::{FontCategory, TextBlockType};
    use crate::settings::PropertyMode;

    fn estimate(img: &image::RgbaImage) -> (TypographyEstimate, DetectedTextBlock) {
        let mask = BlockDetector::ink_mask(img);
        let blocks = BlockDetector::default().detect_text_blocks(img, &mask);
        let b = blocks.into_iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area())).unwrap();
        let a = FontClassifier.classify(img, &mask, &b);
        (TypographyEstimator.estimate(&b, &a, [20, 20, 30], img.width() as f32), b)
    }

    #[test]
    fn multiline_left_aligned_dialogue() {
        let mut img = canvas(1000, 300, [20, 20, 30]);
        draw_line(&mut img, 60, 60, 40, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 60, 94, 28, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 60, 128, 34, 14, 4, 22, 3, [240, 240, 240], false);
        let (e, b) = estimate(&img);
        assert_eq!(b.lines.len(), 3);
        assert_eq!(e.alignment, TextAlignment::Left);
        assert!((e.line_height.absolute_px - 34.0).abs() < 1.5, "{e:?}");
        assert!(e.text_color_reliable && e.text_color[0] > 200);
    }

    #[test]
    fn centred_lines_and_unreadable_colour() {
        let mut img = canvas(1000, 300, [20, 20, 30]);
        draw_line(&mut img, 200, 60, 40, 14, 4, 22, 3, [40, 40, 52], false);
        draw_line(&mut img, 290, 94, 30, 14, 4, 22, 3, [40, 40, 52], false);
        let (e, _) = estimate(&img);
        assert_eq!(e.alignment, TextAlignment::Center);
        assert!(!e.text_color_reliable);
        assert_eq!(e.text_color, [255, 255, 255], "readable colour against the dark background");
    }

    #[test]
    fn manual_values_replace_only_their_property() {
        let est = TypographyEstimate { cap_height_px: 20.0, lines: 2, line_height: LineHeightEstimate { absolute_px: 30.0, proportional: 1.5, confidence: 0.9 },
            weight: FontWeight::DemiBold, italic: true, alignment: TextAlignment::Center, letter_spacing_px: 1.0, text_color: [200, 200, 200],
            text_color_reliable: true, padding: Padding::uniform(4.0) };
        let font = FontSelection { family: "PT Serif".into(), category: FontCategory::Serif, italic_available: true,
            available_weights: vec![FontWeight::Normal, FontWeight::Bold], generic: false, condensed_family: None, confidence: 0.9 };
        let mut s = InplaceSettings::default();
        let auto = TypographyEstimator.resolve(&est, &font, &s);
        assert_eq!((auto.font_family.as_str(), auto.font_weight, auto.italic), ("PT Serif", FontWeight::Bold, true), "DemiBold → closest available");
        assert_eq!(auto.line_height_px, Some(30.0));
        s.letter_spacing = PropertyMode::Manual(3.0);
        s.font_family = PropertyMode::Manual("Inter".into());
        let r = TypographyEstimator.resolve(&est, &font, &s);
        assert_eq!((r.font_family.as_str(), r.letter_spacing, r.letter_spacing_manual), ("Inter", 3.0, true));
        assert_eq!((r.alignment, r.italic, r.font_size), (TextAlignment::Center, true, None), "the rest stays automatic");
        let _ = TextBlockType::Unknown;
    }
}
