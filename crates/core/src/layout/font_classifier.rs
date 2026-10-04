//! Признаки шрифта оригинала по пикселям поля.
//!
//! Классификатор проверен на настоящем тексте, а не на нарисованных прямоугольниках: фикстуры
//! `crates/core/tests/fixtures/fonts` — реальные шрифты с известными метками, а пороги ниже
//! подобраны по ним (дерево решений с кросс-валидацией по шрифтам, см. `tests/font_fixtures.rs`).
//!
//! Что измеряется (`FontFeatures`):
//! - глифы: компоненты строки, склеенные только при заметном перекрытии по горизонтали
//!   (точка над «i», акценты); склейка по зазору превращала слово в один «глиф»;
//! - толщина вертикального штриха и тонких горизонтальных (`contrast`): у шрифтов с засечками
//!   вертикали заметно толще горизонталей;
//! - «лапки» у одиночных штрихов («l», «I», «i»): ширина основания к ширине штриха;
//! - регулярность шага символов (моноширинность);
//! - пропорции букв (узкий шрифт) и толщина штриха к высоте прописных (насыщенность);
//! - наклон: сдвиг, при котором вертикали выстраиваются в самые резкие столбцы.
//!
//! Точное имя игрового шрифта не определяется: цель — подобрать похожую замену. Если признаков
//! мало, категория `Unknown` и низкая уверенность.

use super::block_detector::DetectedTextBlock;
use super::{FontCategory, FontWeight, InkMask, Rect, luma};
use image::RgbaImage;

/// Измеренные признаки блока. Хранятся в `FontAnalysis`: по ним видно, почему выбрана категория.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontFeatures {
    /// Глифов после склейки частей.
    pub glyphs: u32,
    /// Высота прописных/выносных, px кадра.
    pub cap_h: f32,
    /// Высота строчных к высоте прописных.
    pub x_ratio: f32,
    /// Ширина букв к высоте прописных (70-й процентиль, без узких «i», «l»).
    pub width_ratio: f32,
    /// Толщина вертикального штриха, px.
    pub stem_w: f32,
    /// Толщина тонких горизонтальных штрихов, px.
    pub bar_h: f32,
    /// Контраст штрихов: `stem_w / bar_h`.
    pub contrast: f32,
    /// Толщина штриха к высоте прописных.
    pub weight_ratio: f32,
    /// Основание одиночных штрихов к их толщине (1 — без лапок); 0 — таких глифов нет.
    pub foot: f32,
    pub foot_glyphs: u32,
    /// Отклонение шага символов от целого кратного (0 — идеально ровный, моноширинный).
    pub pitch_error: f32,
    /// Наклон по сдвигу, при котором столбцы резче всего.
    pub slant: f32,
    /// Выигрыш резкости столбцов при этом сдвиге относительно вертикали.
    pub slant_gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontAnalysis {
    pub category: FontCategory,
    pub weight: FontWeight,
    pub italic: bool,
    /// Толщина вертикального штриха, px кадра.
    pub stroke_px: f32,
    /// Высота прописных/выносных (высокие глифы строки), px кадра.
    pub cap_height_px: f32,
    /// Ширина букв к высоте прописных.
    pub width_ratio: f32,
    /// Наклон: смещение верха относительно низа, в долях высоты.
    pub slant: f32,
    pub monospace: bool,
    pub condensed: bool,
    /// 0.0–1.0.
    pub confidence: f32,
    pub features: FontFeatures,
}

#[derive(Default)]
pub struct FontClassifier;

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

fn percentile(v: &mut [f32], p: f32) -> Option<f32> {
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[((v.len() - 1) as f32 * p).round() as usize])
}

/// Части одной буквы (точка «i», акценты, «й») склеиваются по перекрытию по горизонтали.
/// Соседние буквы курсива перекрываются рамками слабо и остаются разными глифами.
fn merge_glyphs(mut parts: Vec<Rect>) -> Vec<Rect> {
    parts.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut out: Vec<Rect> = Vec::new();
    for p in parts {
        match out.last_mut() {
            Some(last) => {
                let overlap = last.right().min(p.right()) - p.x.max(last.x);
                if overlap >= 0.6 * last.w.min(p.w).max(1.0) { *last = last.union(&p); } else { out.push(p); }
            }
            None => out.push(p),
        }
    }
    out
}

/// Отклонение шага от целого кратного по центрам глифов строки.
fn pitch_error(glyphs: &[Rect]) -> Option<f32> {
    // Короткая строка ровна случайно: надёжно судить о шаге можно от восьми глифов.
    if glyphs.len() < 8 { return None; }
    let centers: Vec<f32> = glyphs.iter().map(|g| g.center().0).collect();
    let mut diffs: Vec<f32> = centers.windows(2).map(|p| p[1] - p[0]).filter(|d| *d > 1.0).collect();
    if diffs.len() < 5 { return None; }
    let mut sorted = diffs.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    // Шаг — типичное из меньших расстояний: пробелы и широкие буквы дают кратные ему.
    let pitch = sorted[sorted.len() / 4].max(1.0);
    diffs.iter_mut().for_each(|d| { let k = (*d / pitch).round().max(1.0); *d = (*d / pitch - k).abs(); });
    Some(diffs.iter().sum::<f32>() / diffs.len() as f32)
}

/// Наклон: сдвиг `x − s·(высота над основанием строки)` возвращает наклонённые буквы в
/// вертикаль; `s`, при котором гистограмма столбцов самая резкая, и есть наклон (метод
/// выпрямления). Возвращает `(s, выигрыш резкости к вертикали)`.
fn best_slant(mask: &InkMask, lines: &[&Rect]) -> (f32, f32) {
    const STEPS: i32 = 24; // s = -0.06 … 0.40, шаг 0.02
    let mut score = vec![0f64; STEPS as usize + 1];
    for line in lines {
        let (x0, x1) = (line.x as usize, (line.right() as usize + 1).min(mask.w));
        let (y0, y1) = (line.y as usize, (line.bottom() as usize + 1).min(mask.h));
        if x1 <= x0 || y1 <= y0 { continue; }
        let bins = (x1 - x0) + (y1 - y0) + 8;
        for (i, total) in score.iter_mut().enumerate() {
            let s = -0.06 + 0.02 * i as f32;
            let mut hist = vec![0u32; bins];
            for y in y0..y1 {
                let shift = -s * (y1 - y) as f32;
                for x in x0..x1 {
                    if mask.at(x, y) {
                        let b = ((x - x0) as f32 + shift).round() as i64 + 4 + (y1 - y0) as i64 / 2;
                        if let Some(c) = hist.get_mut(b.max(0) as usize) { *c += 1; }
                    }
                }
            }
            *total += hist.iter().map(|c| (*c as f64).powi(2)).sum::<f64>();
        }
    }
    let upright = score[3].max(1.0); // s = 0
    let (best, top) = score.iter().enumerate().fold((3usize, 0.0f64), |a, (i, v)| if *v > a.1 { (i, *v) } else { a });
    (-0.06 + 0.02 * best as f32, (top / upright) as f32)
}

/// Доля «чернил» в пикселе (0..1) по яркости между уровнем фона и уровнем чернил блока.
/// Бинарная маска теряет тонкие засечки и квантует толщины до целых пикселей; покрытие даёт
/// толщину штриха с точностью до долей пикселя.
struct Coverage {
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    data: Vec<f32>,
}

impl Coverage {
    fn new(img: &RgbaImage, mask: &InkMask, r: &Rect) -> Self {
        let (x0, y0) = ((r.x as usize).saturating_sub(2), (r.y as usize).saturating_sub(2));
        let (x1, y1) = (((r.right().ceil() as usize) + 2).min(mask.w), ((r.bottom().ceil() as usize) + 2).min(mask.h));
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let raw = img.as_raw();
        let l = |x: usize, y: usize| luma(&raw[(y * mask.w + x) * 4..(y * mask.w + x) * 4 + 4]) as f32;
        let (mut ink, mut bg) = (Vec::new(), Vec::new());
        for y in y0..y1 { for x in x0..x1 { if mask.at(x, y) { ink.push(l(x, y)) } else { bg.push(l(x, y)) } } }
        let level = |v: &mut Vec<f32>| median(v).unwrap_or(0.0);
        let (ink_l, bg_l) = (level(&mut ink), level(&mut bg));
        let span = ink_l - bg_l;
        let data = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| if span.abs() < 8.0 { if mask.at(x, y) { 1.0 } else { 0.0 } } else { ((l(x, y) - bg_l) / span).clamp(0.0, 1.0) })
            .collect();
        Self { x0, y0, w, h, data }
    }
    fn at(&self, x: usize, y: usize) -> f32 {
        if x < self.x0 || y < self.y0 || x >= self.x0 + self.w || y >= self.y0 + self.h { return 0.0; }
        self.data[(y - self.y0) * self.w + (x - self.x0)]
    }
    /// Толщины штрихов вдоль строки: сумма покрытия по сериям выше порога.
    fn row_widths(&self, y: usize, x0: usize, x1: usize) -> Vec<f32> {
        self.series((x0..x1).map(|x| self.at(x, y)))
    }
    fn column_widths(&self, x: usize, y0: usize, y1: usize) -> Vec<f32> {
        self.series((y0..y1).map(|y| self.at(x, y)))
    }
    fn series(&self, values: impl Iterator<Item = f32>) -> Vec<f32> {
        const THRESHOLD: f32 = 0.22;
        let (mut out, mut sum, mut open) = (Vec::new(), 0.0f32, false);
        for v in values {
            if v >= THRESHOLD { sum += v; open = true; } else if open { out.push(sum); sum = 0.0; open = false; }
        }
        if open { out.push(sum); }
        out
    }
}

impl FontClassifier {
    pub fn features(&self, img: &RgbaImage, mask: &InkMask, block: &DetectedTextBlock) -> FontFeatures {
        let cover = Coverage::new(img, mask, &block.rect);
        let line_rects: Vec<&Rect> = block.lines.iter().map(|l| &l.rect).collect();
        let per_line: Vec<Vec<Rect>> = block.lines.iter().map(|l| merge_glyphs(l.glyphs.clone())).collect();
        let glyphs: Vec<Rect> = per_line.iter().flatten().copied().collect();
        let mut heights: Vec<f32> = glyphs.iter().map(|g| g.h).collect();
        let cap_h = percentile(&mut heights, 0.85).unwrap_or(block.line_height()).max(1.0);
        let letters: Vec<&Rect> = glyphs.iter().filter(|g| g.h >= 0.55 * cap_h).collect();

        let mut x_heights: Vec<f32> = glyphs.iter().filter(|g| g.h >= 0.35 * cap_h && g.h < 0.85 * cap_h).map(|g| g.h).collect();
        let x_ratio = median(&mut x_heights).map(|h| h / cap_h).unwrap_or(0.0);
        let mut widths: Vec<f32> = letters.iter().filter(|g| g.w >= 0.3 * cap_h).map(|g| g.w / cap_h).collect();
        let width_ratio = percentile(&mut widths, 0.7).unwrap_or(0.0);

        // Горизонтальные серии в середине глифов — толщина вертикальных штрихов; вертикальные
        // серии во внутренних столбцах — тонкие горизонтальные штрихи (по покрытию, не по маске).
        let (mut stems, mut bars) = (Vec::new(), Vec::new());
        for g in &letters {
            let (x0, x1) = (g.x as usize, (g.right().ceil() as usize).min(mask.w));
            let (y0, y1) = (g.y as usize, (g.bottom().ceil() as usize).min(mask.h));
            for y in (y0 + (y1 - y0) / 4)..=(y1.saturating_sub((y1 - y0) / 4)).min(mask.h - 1) {
                stems.extend(cover.row_widths(y, x0, x1));
            }
            let inner = (x0 + (x1 - x0) / 5)..(x1.saturating_sub((x1 - x0) / 5));
            for x in inner {
                bars.extend(cover.column_widths(x, y0, y1));
            }
        }
        let stem_w = median(&mut stems).unwrap_or(1.0).max(0.5);
        let bar_h = percentile(&mut bars, 0.25).unwrap_or(1.0).max(0.3);

        // Лапки: одиночные штрихи («l», «I», «i») — единственная серия в середине, узкий глиф.
        let mut feet = Vec::new();
        for g in &letters {
            if g.w > 0.5 * g.h { continue; }
            let (x0, x1) = (g.x as usize, (g.right().ceil() as usize).min(mask.w));
            let (y0, y1) = (g.y as usize, (g.bottom().ceil() as usize).min(mask.h));
            if y1 <= y0 + 4 { continue; }
            let mid = cover.row_widths((y0 + y1) / 2, x0, x1);
            if mid.len() != 1 || mid[0] < 0.5 { continue; }
            // Основание — самая широкая из трёх нижних строк: тонкая засечка занимает одну строку.
            let widest = (y1.saturating_sub(3)..y1).map(|y| {
                let xs: Vec<usize> = (x0..x1).filter(|&x| cover.at(x, y) >= 0.22).collect();
                match (xs.first(), xs.last()) { (Some(a), Some(b)) => (b - a + 1) as f32, _ => 0.0 }
            }).fold(0.0f32, f32::max);
            feet.push(widest / mid[0]);
        }
        let foot_glyphs = feet.len() as u32;
        let foot = if feet.len() >= 2 { median(&mut feet).unwrap_or(0.0) } else { 0.0 };

        let errors: Vec<f32> = per_line.iter().filter_map(|l| pitch_error(l)).collect();
        let pitch_error = if errors.is_empty() { 1.0 } else { errors.iter().sum::<f32>() / errors.len() as f32 };
        let (slant, slant_gain) = best_slant(mask, &line_rects);
        FontFeatures { glyphs: glyphs.len() as u32, cap_h, x_ratio, width_ratio, stem_w, bar_h, contrast: stem_w / bar_h,
            weight_ratio: stem_w / cap_h, foot, foot_glyphs, pitch_error, slant, slant_gain }
    }

    pub fn classify(&self, img: &RgbaImage, mask: &InkMask, block: &DetectedTextBlock) -> FontAnalysis {
        let f = self.features(img, mask, block);
        decide(f)
    }
}

fn weight_from_ratio(r: f32) -> FontWeight {
    match r {
        // Границы подобраны по фикстурам: 300 → ≈0,09; 400 → ≈0,13; 700 → ≈0,21; 800 → ≈0,26.
        r if r < 0.05 => FontWeight::Thin,
        r if r < 0.08 => FontWeight::ExtraLight,
        r if r < 0.11 => FontWeight::Light,
        r if r < 0.155 => FontWeight::Normal,
        r if r < 0.17 => FontWeight::Medium,
        r if r < 0.19 => FontWeight::DemiBold,
        r if r < 0.235 => FontWeight::Bold,
        r if r < 0.29 => FontWeight::ExtraBold,
        _ => FontWeight::Black,
    }
}

/// Решение по признакам. Пороги выбраны по фикстурам на плато (соседние значения дают тот же
/// результат), а не по точечному максимуму; на шрифтах, которых не было при подборе (Arial,
/// Verdana, Times, Georgia, Courier), точность не ниже, чем на обучающем наборе.
///
/// 1. шаг символов ровный → моноширинный (`pitch_error` 0,05 против 0,19–0,23 у остальных);
/// 2. толщина вертикалей заметно больше горизонталей (`contrast` ≥ 1,8) → с засечками;
/// 3. широкое основание одиночных штрихов (`foot` ≥ 2,2): при ровной толщине — брусковые,
///    иначе с засечками;
/// 4. узкие буквы (`width_ratio` < 0,59) → узкий; иначе — без засечек.
///
/// CJK по пикселям не определяется: язык оригинала известен после OCR (см. `engine`).
fn decide(f: FontFeatures) -> FontAnalysis {
    const MONO_PITCH_ERROR: f32 = 0.11;
    const SERIF_CONTRAST: f32 = 1.8;
    const SLAB_CONTRAST: f32 = 1.5;
    const FLARED_FOOT: f32 = 2.2;
    const NARROW_WIDTH: f32 = 0.59;
    let enough = f.glyphs >= 4 && f.cap_h >= 6.0;
    let monospace = enough && f.pitch_error < MONO_PITCH_ERROR;
    let condensed = enough && f.width_ratio < NARROW_WIDTH;
    let category = if !enough { FontCategory::Unknown }
        else if monospace { FontCategory::Monospace }
        else if f.contrast >= SERIF_CONTRAST { FontCategory::Serif }
        else if f.foot >= FLARED_FOOT { if f.contrast < SLAB_CONTRAST { FontCategory::SlabSerif } else { FontCategory::Serif } }
        else if condensed { FontCategory::Condensed }
        else { FontCategory::SansSerif };
    // Уверенность растёт с числом глифов и падает, когда не хватило глифов для признака.
    let mut confidence = if enough { (f.glyphs as f32 / 12.0).min(1.0) * 0.8 + 0.2 } else { 0.1 };
    if enough && f.foot_glyphs < 2 { confidence *= 0.85; }
    if enough && f.pitch_error >= 1.0 { confidence *= 0.9; }
    FontAnalysis {
        category,
        weight: weight_from_ratio(f.weight_ratio),
        italic: f.slant >= 0.08 && f.slant_gain >= 1.03,
        stroke_px: f.stem_w,
        cap_height_px: f.cap_h,
        width_ratio: f.width_ratio,
        slant: f.slant,
        monospace,
        condensed,
        confidence,
        features: f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::block_detector::BlockDetector;
    use crate::layout::testing::*;
    use image::{Rgba, RgbaImage};

    fn analyse(img: &RgbaImage) -> FontAnalysis {
        let mask = BlockDetector::ink_mask(img);
        let blocks = BlockDetector::default().detect_text_blocks(img, &mask);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        FontClassifier.classify(img, &mask, &blocks[0])
    }

    #[test]
    fn slanted_glyphs_are_italic() {
        let mut img = canvas(900, 120, [10, 10, 10]);
        for i in 0..25u32 {
            let x0 = 20 + i * 24;
            // Наклонная палочка: верх смещён вправо на 40% высоты.
            for y in 0..24u32 {
                let shift = (24 - y) * 10 / 24;
                for dx in 0..3 { img.put_pixel(x0 + shift + dx, 40 + y, Rgba([255, 255, 255, 255])); }
            }
        }
        let a = analyse(&img);
        assert!(a.italic, "{a:?}");
    }

    #[test]
    fn too_few_glyphs_is_unknown() {
        let mut img = canvas(300, 80, [10, 10, 10]);
        draw_line(&mut img, 20, 20, 3, 14, 4, 22, 3, [255, 255, 255], false);
        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let a = FontClassifier.classify(&img, &mask, &blocks[0]);
        assert_eq!(a.category, FontCategory::Unknown);
        assert!(a.confidence < 0.3);
    }

    #[test]
    fn heavier_strokes_weigh_more() {
        let mut thin = canvas(900, 120, [20, 20, 30]);
        draw_line(&mut thin, 20, 40, 30, 14, 5, 24, 2, [250, 250, 250], false);
        let mut bold = canvas(900, 120, [20, 20, 30]);
        draw_line(&mut bold, 20, 40, 30, 16, 5, 24, 5, [250, 250, 250], false);
        assert!(analyse(&bold).weight > analyse(&thin).weight);
    }
}
