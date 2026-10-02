//! Где и как выглядит текст в кадре области: для режима «перевод поверх оригинала».
//!
//! Яркость кадра делится порогом Оцу на два класса; текст — меньший по площади («чернила»).
//! По проекциям чернил находятся границы блока и строки; из высоты строк — размер шрифта,
//! из толщины штрихов — жирность. Цвет текста — средний цвет чернил, цвет фона — среднее
//! остальных пикселей вокруг блока. Подложка — сильно уменьшенный фрагмент кадра: растянутый
//! со сглаживанием, он выглядит размытым и закрывает оригинал.

use image::{DynamicImage, RgbaImage, imageops::FilterType};

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// Блок текста в долях кадра: x, y, w, h.
    pub bbox: [f64; 4],
    /// Размер кадра, px: по нему QML переводит пиксели кадра в пиксели экрана.
    pub frame: (u32, u32),
    pub lines: u32,
    /// Размер шрифта оригинала в пикселях кадра.
    pub font_px: f64,
    pub bold: bool,
    /// Generic family estimate; a pixel image cannot identify the exact font file.
    pub font_family: String,
    pub text_color: [u8; 3],
    pub background_color: [u8; 3],
    /// Уменьшенный фрагмент кадра под блоком: при растяжении — размытая заливка.
    pub backdrop: RgbaImage,
}

/// Строка или столбец считается текстовым, если в нём столько чернил (доля от длины).
const MIN_INK: f64 = 0.01;
/// Поля вокруг блока, px кадра.
const PAD: u32 = 4;
/// Ширина подложки: чем меньше, тем сильнее размытие.
const BACKDROP_WIDTH: u32 = 24;

fn luma(p: &[u8]) -> u8 {
    ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8
}

/// Порог Оцу по гистограмме яркости.
fn otsu(hist: &[u64; 256], total: u64) -> u8 {
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut sum_b, mut w_b, mut best, mut threshold) = (0.0, 0u64, -1.0, 128u8);
    for (t, &c) in hist.iter().enumerate() {
        w_b += c;
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += t as f64 * c as f64;
        let (m_b, m_f) = (sum_b / w_b as f64, (sum - sum_b) / w_f as f64);
        let between = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
        if between > best { best = between; threshold = t as u8; }
    }
    threshold
}

/// Отрезки подряд идущих `true`: (начало, конец не включая).
fn runs(flags: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, &f) in flags.iter().chain(std::iter::once(&false)).enumerate() {
        match (f, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => { out.push((s, i)); start = None; }
            _ => {}
        }
    }
    out
}

pub fn analyze(img: &DynamicImage) -> Option<TextLayout> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w < 8 || h < 8 { return None; }
    let raw = rgba.as_raw();
    let mut hist = [0u64; 256];
    for p in raw.chunks_exact(4) { hist[luma(p) as usize] += 1; }
    let total = w as u64 * h as u64;
    let t = otsu(&hist, total);
    let dark = hist[..=t as usize].iter().sum::<u64>();
    // Текст занимает меньшую часть площади.
    let ink_is_dark = dark * 2 < total;
    let ink: Vec<bool> = raw.chunks_exact(4).map(|p| (luma(p) <= t) == ink_is_dark).collect();

    let (wu, hu) = (w as usize, h as usize);
    let row_ink: Vec<usize> = (0..hu).map(|y| ink[y * wu..(y + 1) * wu].iter().filter(|b| **b).count()).collect();
    let text_rows: Vec<bool> = row_ink.iter().map(|&c| c as f64 >= (MIN_INK * w as f64).max(2.0)).collect();
    let lines: Vec<_> = runs(&text_rows).into_iter().filter(|(a, b)| b - a >= 3).collect();
    let (y0, y1) = (lines.first()?.0, lines.last()?.1);
    let col_ink: Vec<usize> = (0..wu).map(|x| (y0..y1).filter(|&y| ink[y * wu + x]).count()).collect();
    let text_cols: Vec<bool> = col_ink.iter().map(|&c| c as f64 >= (MIN_INK * (y1 - y0) as f64).max(1.0)).collect();
    let x0 = text_cols.iter().position(|b| *b)?;
    let x1 = text_cols.iter().rposition(|b| *b)? + 1;

    // Цвета: чернила внутри блока и всё остальное в блоке с полями.
    let (bx0, by0) = (x0.saturating_sub(PAD as usize), y0.saturating_sub(PAD as usize));
    let (bx1, by1) = ((x1 + PAD as usize).min(wu), (y1 + PAD as usize).min(hu));
    let (mut ink_sum, mut ink_n, mut bg_sum, mut bg_n) = ([0u64; 3], 0u64, [0u64; 3], 0u64);
    let mut transitions = 0u64;
    for y in by0..by1 {
        for x in bx0..bx1 {
            let i = y * wu + x;
            let p = &raw[i * 4..i * 4 + 3];
            let (sum, n) = if ink[i] { (&mut ink_sum, &mut ink_n) } else { (&mut bg_sum, &mut bg_n) };
            for c in 0..3 { sum[c] += p[c] as u64; }
            *n += 1;
            if x > bx0 && ink[i] && !ink[i - 1] { transitions += 1; }
        }
    }
    let mean = |s: [u64; 3], n: u64| s.map(|v| (v / n.max(1)) as u8);

    // Размер шрифта: высота строки с выносными элементами примерно равна кеглю.
    let mut heights: Vec<usize> = lines.iter().map(|(a, b)| b - a).collect();
    heights.sort_unstable();
    let font_px = heights[heights.len() / 2] as f64 * 1.05;
    // Толщина штриха: средняя длина горизонтального отрезка чернил.
    let stroke = ink_n as f64 / transitions.max(1) as f64;
    let bold = stroke / font_px > 0.14;

    // Classify regular glyph spacing and prominent terminal strokes approximately.
    let glyphs = runs(&text_cols);
    let centers: Vec<f64> = glyphs.iter().map(|(a, b)| (a + b) as f64 / 2.0).collect();
    let gaps: Vec<f64> = centers.windows(2).map(|p| p[1] - p[0]).collect();
    let mean_gap = gaps.iter().sum::<f64>() / gaps.len().max(1) as f64;
    let variance = gaps.iter().map(|g| (g - mean_gap).powi(2)).sum::<f64>() / gaps.len().max(1) as f64;
    let (ly0, ly1) = lines[0];
    let middle = (ly0 + ly1) / 2;
    let serif_count = glyphs.iter().filter(|(a, b)| {
        let count = |y: usize| (*a..*b).filter(|x| ink[y * wu + x]).count();
        let mid = count(middle);
        mid > 0 && count(ly0) > mid * 3 / 2 && count(ly1 - 1) > mid * 3 / 2
    }).count();
    let font_family = if glyphs.len() >= 4 && serif_count * 2 >= glyphs.len() { "serif" }
        else if gaps.len() >= 5 && mean_gap > 0.0 && variance.sqrt() / mean_gap < 0.08 { "monospace" }
        else { "sans-serif" }.to_string();

    // Replace the original ink before blurring, so the old letters cannot show through.
    let bg = mean(bg_sum, bg_n);
    let mut cleaned = rgba;
    for y in by0..by1 {
        for x in bx0..bx1 {
            if ink[y * wu + x] { cleaned.put_pixel(x as u32, y as u32, image::Rgba([bg[0], bg[1], bg[2], 255])); }
        }
    }
    let crop = DynamicImage::ImageRgba8(cleaned).crop_imm(bx0 as u32, by0 as u32, (bx1 - bx0) as u32, (by1 - by0) as u32);
    let bw = BACKDROP_WIDTH.min(crop.width()).max(1);
    let bh = ((crop.height() as f64 * bw as f64 / crop.width() as f64).round() as u32).max(1);
    let backdrop = crop.resize_exact(bw, bh, FilterType::Triangle).to_rgba8();

    let (fw, fh) = (w as f64, h as f64);
    Some(TextLayout {
        bbox: [bx0 as f64 / fw, by0 as f64 / fh, (bx1 - bx0) as f64 / fw, (by1 - by0) as f64 / fh],
        frame: (w, h),
        lines: lines.len() as u32,
        font_px,
        bold,
        font_family,
        text_color: mean(ink_sum, ink_n),
        background_color: mean(bg_sum, bg_n),
        backdrop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Кадр w×h цвета `bg` со «строками текста»: штрихи `stroke` px с шагом `pitch`.
    fn frame(w: u32, h: u32, bg: [u8; 3], ink: [u8; 3], lines: &[(u32, u32)], x: (u32, u32), stroke: u32, pitch: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([bg[0], bg[1], bg[2], 255]));
        for &(top, height) in lines {
            for gx in (x.0..x.1).step_by(pitch as usize) {
                for y in top..top + height {
                    for dx in 0..stroke { img.put_pixel(gx + dx, y, Rgba([ink[0], ink[1], ink[2], 255])); }
                }
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn light_subtitle_on_dark_scene() {
        let img = frame(800, 200, [30, 40, 50], [250, 250, 240], &[(80, 28)], (200, 600), 3, 9);
        let l = analyze(&img).unwrap();
        assert_eq!(l.lines, 1);
        assert!((l.font_px - 29.4).abs() < 1.0, "font {}", l.font_px);
        assert!(l.text_color[0] > 230, "text colour {:?}", l.text_color);
        assert!(l.background_color[0] < 60, "background {:?}", l.background_color);
        let [x, y, w, h] = l.bbox;
        assert!((x * 800.0 - 196.0).abs() < 2.0 && (y * 200.0 - 76.0).abs() < 2.0, "bbox {:?}", l.bbox);
        assert!((w * 800.0 - 409.0).abs() < 4.0 && (h * 200.0 - 36.0).abs() < 2.0, "bbox {:?}", l.bbox);
        assert!(!l.bold);
        assert!(l.backdrop.width() <= BACKDROP_WIDTH);
    }

    #[test]
    fn dark_bold_text_on_light_box_with_two_lines() {
        let img = frame(600, 240, [235, 225, 200], [20, 20, 20], &[(40, 20), (80, 20)], (50, 500), 6, 14);
        let l = analyze(&img).unwrap();
        assert_eq!(l.lines, 2);
        assert!(l.text_color[0] < 40 && l.background_color[0] > 200);
        assert!(l.bold, "6 px strokes at ~21 px size are bold");
    }

    #[test]
    fn empty_frame_has_no_layout() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(300, 100, Rgba([90, 90, 90, 255])));
        assert!(analyze(&img).is_none());
    }
}
