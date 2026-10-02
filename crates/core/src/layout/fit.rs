//! Подгонка перевода в прямоугольник поля.
//!
//! Измерение текста — через трейт [`TextMeasure`]: в приложении его реализуют метрики Qt
//! (`QFontMetricsF`) для выбранного шрифта, в тестах — простая модель. Порядок:
//!
//! 1. начальный кегль — оценка по оригиналу (или ручной);
//! 2. перенос: по словам, для CJK — по символам; одна строка, если оригинал был в одну и
//!    перевод помещается;
//! 3. уменьшение межстрочного интервала (если он автоматический);
//! 4. умеренное сужение трекинга (не больше 4% кегля, только автоматического);
//! 5. постепенное уменьшение кегля до минимума;
//! 6. узкий вариант шрифта, если разрешён и установлен;
//! 7. явное усечение с многоточием: текст никогда не выходит за пределы поля.

use super::{FontWeight, Script, WrapMode};

#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub weight: FontWeight,
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub width: f32,
    pub height: f32,
    pub lines: u32,
    /// Естественный шаг строк шрифта при этом кегле.
    pub line_spacing: f32,
}

pub trait TextMeasure {
    /// Размер текста при ширине `width` (перенос по `wrap`), кегле `px` и трекинге `letter_spacing`.
    fn measure(&self, text: &str, font: &FontSpec, px: f32, letter_spacing: f32, width: f32, wrap: WrapMode) -> Measured;
    /// Высота прописных к кеглю для шрифта (≈0.7).
    fn cap_ratio(&self, font: &FontSpec) -> f32;
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitInput<'a> {
    pub text: &'a str,
    /// Внутренняя область поля (без полей), px экрана.
    pub width: f32,
    pub height: f32,
    pub font: FontSpec,
    /// Ручной кегль; иначе — по высоте прописных оригинала (`cap_height_px`, px экрана).
    pub manual_px: Option<f32>,
    pub cap_height_px: f32,
    pub min_px: f32,
    pub max_px: f32,
    pub source_lines: u32,
    /// Ручной множитель межстрочного интервала.
    pub manual_line_height: Option<f32>,
    /// Расстояние между строками оригинала, px экрана.
    pub source_line_px: Option<f32>,
    pub letter_spacing: f32,
    pub letter_spacing_manual: bool,
    pub manual_wrap: Option<WrapMode>,
    pub script: Script,
    /// Узкий вариант того же склада, если разрешён и установлен.
    pub condensed_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    pub family: String,
    pub font_px: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub wrap_mode: WrapMode,
    /// Сколько строк помещается: для усечения при `WrapMode::Elide`.
    pub max_lines: u32,
    pub overflow: bool,
}

/// Высота текста с множителем межстрочного интервала.
fn text_height(m: &Measured, line_height: f32) -> f32 {
    if m.lines <= 1 { m.height } else { m.height + (m.lines - 1) as f32 * m.line_spacing * (line_height - 1.0) }
}

pub fn fit_translation_to_box(input: &FitInput, measure: &dyn TextMeasure) -> FitResult {
    let (w, h) = (input.width.max(1.0), input.height.max(1.0));
    let start = input.manual_px.unwrap_or_else(|| input.cap_height_px / measure.cap_ratio(&input.font).max(0.3))
        .clamp(input.min_px, input.max_px.max(input.min_px));
    let fits = |m: &Measured, lh: f32| m.width <= w + 0.5 && text_height(m, lh) <= h + 0.5;
    // Перенос: CJK — по символам; одна строка, если и оригинал в одну и перевод помещается.
    let mut wrap = input.manual_wrap.unwrap_or_else(|| {
        if input.script.is_cjk() { return WrapMode::WrapAnywhere; }
        let single = measure.measure(input.text, &input.font, start, input.letter_spacing, w, WrapMode::NoWrap);
        if input.source_lines <= 1 && single.width <= w + 0.5 { WrapMode::NoWrap } else { WrapMode::WordWrap }
    });
    let natural_lh = |px: f32, font: &FontSpec| {
        let m = measure.measure("Hg", font, px, 0.0, f32::MAX, WrapMode::NoWrap);
        match (input.manual_line_height, input.source_line_px) {
            (Some(v), _) => v,
            (None, Some(src)) if m.line_spacing > 0.0 => (src / m.line_spacing).clamp(0.85, 2.0),
            _ => 1.0,
        }
    };
    let mut line_height = natural_lh(start, &input.font);
    let mut spacing = input.letter_spacing;
    let try_size = |family: &FontSpec, px: f32, lh: f32, ls: f32, wrap: WrapMode| {
        let m = measure.measure(input.text, family, px, ls, w, wrap);
        (fits(&m, lh), m)
    };
    let done = |family: &FontSpec, px: f32, lh: f32, ls: f32, wrap: WrapMode, overflow: bool, m: &Measured| FitResult {
        family: family.family.clone(), font_px: px, line_height: lh, letter_spacing: ls, wrap_mode: wrap,
        max_lines: ((h / (m.line_spacing * lh).max(1.0)).floor() as u32).max(1), overflow,
    };

    // 1–2. Начальный кегль с выбранным переносом; одна строка не поместилась — перенос по словам.
    let (ok, m) = try_size(&input.font, start, line_height, spacing, wrap);
    if ok { return done(&input.font, start, line_height, spacing, wrap, false, &m); }
    if wrap == WrapMode::NoWrap && input.manual_wrap.is_none() {
        wrap = WrapMode::WordWrap;
        let (ok, m) = try_size(&input.font, start, line_height, spacing, wrap);
        if ok { return done(&input.font, start, line_height, spacing, wrap, false, &m); }
    }
    // 3. Межстрочный интервал (автоматический) — до 0.9.
    if input.manual_line_height.is_none() && line_height > 0.9 {
        line_height = (line_height - 0.15).max(0.9);
        let (ok, m) = try_size(&input.font, start, line_height, spacing, wrap);
        if ok { return done(&input.font, start, line_height, spacing, wrap, false, &m); }
    }
    // 4. Трекинг (автоматический) — не уже −4% кегля: дальше текст портится, лучше уменьшить кегль.
    if !input.letter_spacing_manual {
        spacing = (spacing - 0.03 * start).max(-0.04 * start);
        let (ok, m) = try_size(&input.font, start, line_height, spacing, wrap);
        if ok { return done(&input.font, start, line_height, spacing, wrap, false, &m); }
    }
    // 5–6. Кегль вниз (если не ручной), затем узкий вариант.
    let families: Vec<FontSpec> = std::iter::once(input.font.clone())
        .chain(input.condensed_family.iter().map(|f| FontSpec { family: f.clone(), ..input.font.clone() }))
        .collect();
    for family in &families {
        let mut px = start;
        loop {
            let ls = if input.letter_spacing_manual { spacing } else { spacing.max(-0.04 * px) };
            let (ok, m) = try_size(family, px, line_height, ls, wrap);
            if ok { return done(family, px, line_height, ls, wrap, false, &m); }
            if input.manual_px.is_some() || px <= input.min_px { break; }
            px = (px - (px * 0.06).max(0.5)).max(input.min_px);
        }
    }
    // 7. Не помещается и на минимуме: явное усечение в пределах поля.
    let px = if input.manual_px.is_some() { start } else { input.min_px };
    let m = measure.measure(input.text, &input.font, px, spacing, w, wrap);
    done(&input.font, px, line_height, spacing, WrapMode::Elide, true, &m)
}

/// Простая модель для тестов и для случая, когда метрики Qt недоступны: ширина символа 0.55
/// кегля (CJK — 1.0), шаг строк 1.2 кегля, перенос по словам или символам.
pub struct ApproxMeasure;

impl TextMeasure for ApproxMeasure {
    fn measure(&self, text: &str, _font: &FontSpec, px: f32, ls: f32, width: f32, wrap: WrapMode) -> Measured {
        let cw = |c: char| if (c as u32) >= 0x2E80 { px } else { 0.55 * px } + ls;
        let line_spacing = 1.2 * px;
        let words: Vec<String> = match wrap {
            WrapMode::WrapAnywhere => text.chars().map(String::from).collect(),
            _ => text.split(' ').map(String::from).collect(),
        };
        let sep = if wrap == WrapMode::WrapAnywhere { 0.0 } else { cw(' ') };
        let (mut lines, mut cur, mut widest) = (1u32, 0.0f32, 0.0f32);
        for (i, word) in words.iter().enumerate() {
            let ww: f32 = word.chars().map(cw).sum();
            let add = if i == 0 || cur == 0.0 { ww } else { sep + ww };
            if matches!(wrap, WrapMode::WordWrap | WrapMode::WrapAnywhere) && cur > 0.0 && cur + add > width {
                widest = widest.max(cur);
                lines += 1;
                cur = ww;
            } else {
                cur += add;
            }
        }
        widest = widest.max(cur);
        Measured { width: widest, height: lines as f32 * line_spacing, lines, line_spacing }
    }
    fn cap_ratio(&self, _font: &FontSpec) -> f32 {
        0.7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str, w: f32, h: f32) -> FitInput<'_> {
        FitInput {
            text, width: w, height: h,
            font: FontSpec { family: "PT Serif".into(), weight: FontWeight::Normal, italic: false },
            manual_px: None, cap_height_px: 14.0, min_px: 8.0, max_px: 96.0, source_lines: 1,
            manual_line_height: None, source_line_px: None, letter_spacing: 0.0, letter_spacing_manual: false,
            manual_wrap: None, script: Script::Cyrillic, condensed_family: None,
        }
    }

    #[test]
    fn short_translation_keeps_original_size_on_one_line() {
        let r = fit_translation_to_box(&input("Привет", 300.0, 30.0), &ApproxMeasure);
        assert_eq!((r.font_px, r.wrap_mode, r.overflow), (20.0, WrapMode::NoWrap, false));
    }

    #[test]
    fn long_translation_wraps_then_shrinks_but_stays_inside() {
        let text = "Это очень длинный перевод который не помещается в исходный блок текста целиком никак";
        let r = fit_translation_to_box(&input(text, 300.0, 60.0), &ApproxMeasure);
        assert!(!r.overflow);
        assert_eq!(r.wrap_mode, WrapMode::WordWrap);
        assert!(r.font_px < 20.0 && r.font_px >= 8.0, "{r:?}");
        let m = ApproxMeasure.measure(text, &input(text, 0.0, 0.0).font, r.font_px, r.letter_spacing, 300.0, r.wrap_mode);
        assert!(m.width <= 300.5 && text_height(&m, r.line_height) <= 60.5, "fits: {m:?}");
        assert!(r.letter_spacing >= -0.04 * r.font_px - 1e-3, "no destructive tracking");
    }

    #[test]
    fn impossible_fit_elides_at_minimum_size() {
        let text = "слово ".repeat(200);
        let r = fit_translation_to_box(&input(&text, 100.0, 20.0), &ApproxMeasure);
        assert!(r.overflow);
        assert_eq!((r.wrap_mode, r.font_px), (WrapMode::Elide, 8.0));
        assert!(r.max_lines >= 1);
    }

    #[test]
    fn cjk_wraps_by_character_and_manual_values_are_kept() {
        let mut i = input("这是一个很长的中文翻译文本需要换行", 200.0, 100.0);
        i.script = Script::ChineseSimplified;
        assert_eq!(fit_translation_to_box(&i, &ApproxMeasure).wrap_mode, WrapMode::WrapAnywhere);
        let mut m = input("Привет", 300.0, 40.0);
        m.manual_px = Some(30.0);
        m.manual_line_height = Some(1.4);
        let r = fit_translation_to_box(&m, &ApproxMeasure);
        assert_eq!((r.font_px, r.line_height), (30.0, 1.4));
    }

    #[test]
    fn condensed_family_is_tried_before_giving_up() {
        struct Narrow;
        impl TextMeasure for Narrow {
            fn measure(&self, t: &str, f: &FontSpec, px: f32, ls: f32, w: f32, wrap: WrapMode) -> Measured {
                let mut m = ApproxMeasure.measure(t, f, px, ls, w, wrap);
                if f.family.contains("Condensed") { m.width *= 0.5; }
                m
            }
            fn cap_ratio(&self, _: &FontSpec) -> f32 { 0.7 }
        }
        // Начальный кегль 20 (14 / 0.7): 25 символов не помещаются в 220 px даже на минимуме 18.
        let mut i = input("Длинныйпереводбезпробелов", 220.0, 30.0);
        i.min_px = 18.0;
        i.condensed_family = Some("Roboto Condensed".into());
        let r = fit_translation_to_box(&i, &Narrow);
        assert_eq!(r.family, "Roboto Condensed");
        assert!(!r.overflow);
    }
}
