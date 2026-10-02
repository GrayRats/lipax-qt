//! Признаки шрифта оригинала по пикселям поля.
//!
//! Измеряются толщина штриха и её разброс (контраст), высота прописных, засечки и их толщина,
//! наклон, равномерность шага (моноширинность), пропорции глифов (узкий шрифт, квадратные
//! глифы CJK). Точное имя игрового шрифта не определяется: цель — подобрать похожую замену.
//! Если признаков мало, категория `Unknown` и низкая уверенность.

use super::block_detector::DetectedTextBlock;
use super::{FontCategory, FontWeight, InkMask, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct FontAnalysis {
    pub category: FontCategory,
    pub weight: FontWeight,
    pub italic: bool,
    /// Толщина вертикального штриха, px кадра.
    pub stroke_px: f32,
    /// Высота прописных/выносных (высокие глифы строки), px кадра.
    pub cap_height_px: f32,
    /// Ширина глифа к его высоте (медиана).
    pub width_ratio: f32,
    /// Смещение верха глифа вправо относительно низа, в долях высоты.
    pub slant: f32,
    pub monospace: bool,
    pub condensed: bool,
    /// 0.0–1.0.
    pub confidence: f32,
}

#[derive(Default)]
pub struct FontClassifier;

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

fn percentile(mut v: Vec<f32>, p: f32) -> Option<f32> {
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[((v.len() - 1) as f32 * p).round() as usize])
}

fn cv(v: &[f32]) -> f32 {
    if v.len() < 2 { return 0.0; }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    let var = v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32;
    if m > 0.0 { var.sqrt() / m } else { 0.0 }
}

/// Части одной буквы (две ножки «П», точка «i») объединяются, если зазор между ними мал.
fn merge_glyphs(mut parts: Vec<Rect>, line_h: f32) -> Vec<Rect> {
    parts.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut out: Vec<Rect> = Vec::new();
    for p in parts {
        match out.last_mut() {
            Some(last) if p.x - last.right() <= 0.12 * line_h || p.right() <= last.right() => *last = last.union(&p),
            _ => out.push(p),
        }
    }
    out
}

/// Ширина чернил в строке `y` внутри `r`.
fn row_ink(mask: &InkMask, r: &Rect, y: usize) -> usize {
    let (x0, x1) = (r.x as usize, (r.right() as usize).min(mask.w));
    (x0..x1).filter(|&x| mask.at(x, y)).count()
}

fn weight_from_ratio(r: f32) -> FontWeight {
    match r {
        r if r < 0.05 => FontWeight::Thin,
        r if r < 0.07 => FontWeight::ExtraLight,
        r if r < 0.09 => FontWeight::Light,
        r if r < 0.13 => FontWeight::Normal,
        r if r < 0.15 => FontWeight::Medium,
        r if r < 0.17 => FontWeight::DemiBold,
        r if r < 0.21 => FontWeight::Bold,
        r if r < 0.26 => FontWeight::ExtraBold,
        _ => FontWeight::Black,
    }
}

impl FontClassifier {
    pub fn classify(&self, mask: &InkMask, block: &DetectedTextBlock) -> FontAnalysis {
        let mut runs = Vec::new();
        let (mut heights, mut widths, mut slants, mut serif_votes, mut slab_votes, mut tall) = (vec![], vec![], vec![], 0usize, 0usize, 0usize);
        let mut advances_cv = Vec::new();
        let mut glyph_count = 0usize;
        for line in &block.lines {
            let lh = line.rect.h;
            // Горизонтальные серии чернил внутри строки: их типичная длина — толщина вертикального штриха.
            for y in line.rect.y as usize..(line.rect.bottom() as usize).min(mask.h) {
                let mut run = 0usize;
                for x in line.rect.x as usize..(line.rect.right() as usize).min(mask.w) {
                    if mask.at(x, y) { run += 1; } else if run > 0 { runs.push(run as f32); run = 0; }
                }
                if run > 0 { runs.push(run as f32); }
            }
            let glyphs = merge_glyphs(line.glyphs.clone(), lh);
            glyph_count += glyphs.len();
            for g in &glyphs {
                heights.push(g.h);
                if g.h >= 0.6 * lh {
                    widths.push(g.w / g.h);
                }
            }
            let centers: Vec<f32> = glyphs.iter().map(|g| g.center().0).collect();
            let adv: Vec<f32> = centers.windows(2).map(|p| p[1] - p[0]).filter(|d| *d < 1.5 * lh).collect();
            if adv.len() >= 4 { advances_cv.push(cv(&adv)); }
            // Засечки и наклон по высоким компонентам (вертикальные штрихи).
            for c in line.glyphs.iter().filter(|c| c.h >= 0.6 * lh && c.h >= 6.0) {
                tall += 1;
                let (top, bottom) = (c.y as usize, (c.bottom() as usize).saturating_sub(1).min(mask.h - 1));
                let mid = (top + bottom) / 2;
                let wm = row_ink(mask, c, mid).max(1);
                let (wt, wb) = (row_ink(mask, c, top), row_ink(mask, c, bottom));
                if wt * 2 >= wm * 3 && wb * 2 >= wm * 3 {
                    serif_votes += 1;
                    // Толщина засечки: сколько строк сверху шире середины.
                    let thick = (top..mid).take_while(|&y| row_ink(mask, c, y) * 2 >= wm * 3).count();
                    if thick as f32 >= 0.12 * c.h { slab_votes += 1; }
                }
                let centroid = |y0: usize, y1: usize| {
                    let (mut sx, mut n) = (0.0f32, 0.0f32);
                    for y in y0..y1 { for x in c.x as usize..(c.right() as usize).min(mask.w) { if mask.at(x, y) { sx += x as f32; n += 1.0; } } }
                    (n > 0.0).then(|| sx / n)
                };
                let third = ((bottom - top) / 3).max(1);
                if let (Some(a), Some(b)) = (centroid(top, top + third), centroid(bottom + 1 - third, bottom + 1)) {
                    slants.push((a - b) / c.h);
                }
            }
        }
        let stroke = median(runs.clone()).unwrap_or(1.0);
        let contrast = cv(&runs);
        let cap = percentile(heights, 0.75).unwrap_or(block.line_height());
        let width_ratio = median(widths.clone()).unwrap_or(0.6);
        let slant = median(slants).unwrap_or(0.0);
        let monospace = !advances_cv.is_empty() && median(advances_cv).unwrap_or(1.0) < 0.06;
        let condensed = width_ratio < 0.42;
        let square = widths.iter().filter(|w| (0.8..=1.25).contains(*w)).count();
        let cjk = widths.len() >= 3 && square * 10 >= widths.len() * 6 && contrast < 2.5 && glyph_count >= 3;
        let serif_share = if tall > 0 { serif_votes as f32 / tall as f32 } else { 0.0 };
        let slab = serif_votes > 0 && slab_votes * 2 >= serif_votes;
        let ratio = stroke / cap.max(1.0);

        let enough = glyph_count >= 4 && tall >= 3;
        let category = if !enough {
            FontCategory::Unknown
        } else if cjk {
            if contrast > 0.6 { FontCategory::CjkSerif } else { FontCategory::CjkSans }
        } else if ratio > 0.26 {
            FontCategory::Display
        } else if monospace {
            FontCategory::Monospace
        } else if serif_share >= 0.4 {
            if slab { FontCategory::SlabSerif } else if contrast > 0.5 { FontCategory::ModernSerif } else { FontCategory::Serif }
        } else if condensed {
            FontCategory::Condensed
        } else if width_ratio >= 0.75 {
            FontCategory::GeometricSans
        } else {
            FontCategory::SansSerif
        };
        let confidence = if enough { (glyph_count as f32 / 12.0).min(1.0) * 0.8 + 0.2 } else { 0.1 };
        FontAnalysis {
            category,
            weight: weight_from_ratio(ratio),
            italic: slant > 0.12,
            stroke_px: stroke,
            cap_height_px: cap,
            width_ratio,
            slant,
            monospace,
            condensed,
            confidence,
        }
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
        FontClassifier.classify(&mask, &blocks[0])
    }

    #[test]
    fn plain_and_serif_and_bold() {
        let mut sans = canvas(900, 120, [20, 20, 30]);
        draw_line(&mut sans, 20, 40, 30, 14, 5, 24, 3, [250, 250, 250], false);
        let a = analyse(&sans);
        assert!(a.category.is_sans() || a.category == FontCategory::SansSerif, "{a:?}");
        assert_eq!(a.weight, FontWeight::Normal, "{a:?}");
        assert!(!a.italic);

        let mut serif = canvas(900, 120, [20, 20, 30]);
        draw_line(&mut serif, 20, 40, 30, 14, 5, 24, 2, [250, 250, 250], true);
        let s = analyse(&serif);
        assert!(matches!(s.category, FontCategory::Serif | FontCategory::ModernSerif | FontCategory::SlabSerif), "{s:?}");

        let mut bold = canvas(900, 120, [20, 20, 30]);
        draw_line(&mut bold, 20, 40, 30, 16, 5, 24, 5, [250, 250, 250], false);
        assert!(analyse(&bold).weight >= FontWeight::Bold);
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
        let a = FontClassifier.classify(&mask, &blocks[0]);
        assert_eq!(a.category, FontCategory::Unknown);
        assert!(a.confidence < 0.3);
    }
}
