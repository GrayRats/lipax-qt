//! Фон под переводом — отдельно от шрифта и типографики.
//!
//! `BackgroundAnalyzer` смотрит на кольцо пикселей вокруг поля без глифов: медиана и среднее
//! цвета, яркость, текстура, плотность краёв, перепад сверху вниз. `BackgroundInpainter`
//! строит замену по выбранному режиму:
//! - `InpaintBlur`: глифы (с расширением маски) закрашиваются диффузией от соседних пикселей
//!   фона, фрагмент уменьшается и размывается по Гауссу; растянутый в QML со сглаживанием,
//!   он закрывает оригинал;
//! - `Reconstruct` (только «Замена текста»): полноразмерный фрагмент кадра, непрозрачный лишь под
//!   глифами (маска расширена по высоте строки), а вокруг них прозрачный, чтобы фон игры остался
//!   настоящим и без прямоугольной границы;
//! - `SolidFill`: цвет фона по кольцу;
//! - `AdaptivePaddingFill`: то же, с расширенными полями;
//! - `Transparent`: оригинал не стирается, перевод читается за счёт обводки и контраста.

use super::{InkMask, Rect, TextBlockType, luma};
use crate::settings::{InplaceBackgroundMode, InplaceSettings};
use image::{DynamicImage, RgbaImage, imageops::FilterType};

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundAnalysis {
    pub median: [u8; 3],
    pub mean: [u8; 3],
    pub luminance: f32,
    /// Стандартное отклонение яркости кольца (0–255): мера текстуры.
    pub texture: f32,
    /// Доля пикселей кольца с резким перепадом яркости.
    pub edge_density: f32,
    /// Разница средней яркости нижней и верхней половины кольца (градиент панели).
    pub gradient: f32,
    /// Пикселей фона хватило для оценки.
    pub reliable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundResult {
    /// Режим, которым фон построен (`Auto` уже разрешён).
    pub mode: BackgroundRenderMode,
    pub color: [u8; 3],
    /// Уменьшенная размытая заливка для `InpaintBlur`; область — `rect`.
    /// Общая картинка: блок клонируется без копирования пикселей.
    pub image: Option<std::sync::Arc<RgbaImage>>,
    /// Область заливки, px кадра (поле с полями).
    pub rect: Rect,
    pub analysis: BackgroundAnalysis,
}

#[derive(Default)]
pub struct BackgroundAnalyzer;

#[derive(Default)]
pub struct BackgroundInpainter;

/// Алгоритм рендеринга — не значение пользовательской настройки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundRenderMode { InpaintBlur, Reconstruct, SolidFill, AdaptivePaddingFill, Transparent }

fn median_u8(mut v: Vec<u8>) -> u8 {
    if v.is_empty() { return 0; }
    v.sort_unstable();
    v[v.len() / 2]
}

impl BackgroundAnalyzer {
    /// Ширина кольца вокруг поля, px кадра.
    pub fn margin(line_height: f32) -> f32 {
        (0.4 * line_height).clamp(3.0, 14.0)
    }

    pub fn analyze(&self, img: &RgbaImage, mask: &InkMask, rect: &Rect, margin: f32) -> BackgroundAnalysis {
        let outer = rect.expand(margin, img.width() as f32, img.height() as f32);
        let (x0, y0, w, h) = outer.pixels(img.width(), img.height());
        let (mut rs, mut gs, mut bs, mut ls) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut top, mut bottom) = ((0.0f64, 0u32), (0.0f64, 0u32));
        let mut edges = 0u32;
        let cy = rect.center().1;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let inside = (x as f32) >= rect.x && (x as f32) < rect.right() && (y as f32) >= rect.y && (y as f32) < rect.bottom();
                // Фон оценивается по кольцу и по просветам между глифами, но не по самим глифам.
                if mask.at(x as usize, y as usize) || (inside && margin > 0.0 && near_ink(mask, x as usize, y as usize)) { continue; }
                let p = img.get_pixel(x, y).0;
                let l = luma(&p);
                rs.push(p[0]); gs.push(p[1]); bs.push(p[2]); ls.push(l);
                let half = if (y as f32) < cy { &mut top } else { &mut bottom };
                half.0 += l as f64; half.1 += 1;
                if x + 1 < x0 + w {
                    let q = luma(&img.get_pixel(x + 1, y).0);
                    if l.abs_diff(q) > 40 { edges += 1; }
                }
            }
        }
        let n = ls.len().max(1) as f32;
        let mean = |v: &[u8]| (v.iter().map(|&x| x as u32).sum::<u32>() as f32 / n) as u8;
        let mean_c = [mean(&rs), mean(&gs), mean(&bs)];
        let lm = ls.iter().map(|&x| x as f32).sum::<f32>() / n;
        let texture = (ls.iter().map(|&x| (x as f32 - lm).powi(2)).sum::<f32>() / n).sqrt();
        let half_mean = |h: (f64, u32)| if h.1 > 0 { (h.0 / h.1 as f64) as f32 } else { lm };
        BackgroundAnalysis {
            median: [median_u8(rs), median_u8(gs), median_u8(bs)],
            mean: mean_c,
            luminance: lm / 255.0,
            texture,
            edge_density: edges as f32 / n,
            gradient: half_mean(bottom) - half_mean(top),
            reliable: ls.len() >= 16,
        }
    }

    /// Дешёвая подпись фона для проверки «изменился ли фон» без полного анализа.
    pub fn signature(img: &RgbaImage, rect: &Rect, margin: f32) -> [u8; 3] {
        let outer = rect.expand(margin, img.width() as f32, img.height() as f32);
        let (x0, y0, w, h) = outer.pixels(img.width(), img.height());
        let (mut sum, mut n) = ([0u64; 3], 0u64);
        // Только верхняя и нижняя полосы кольца: там нет текста поля.
        for y in (y0..y0 + h).filter(|&y| (y as f32) < rect.y || (y as f32) >= rect.bottom()) {
            for x in (x0..x0 + w).step_by(2) {
                let p = img.get_pixel(x, y).0;
                for c in 0..3 { sum[c] += p[c] as u64; }
                n += 1;
            }
        }
        if n == 0 { return [0; 3]; }
        sum.map(|v| (v / n) as u8)
    }
}

fn near_ink(mask: &InkMask, x: usize, y: usize) -> bool {
    let (x0, x1) = (x.saturating_sub(1), (x + 1).min(mask.w - 1));
    let (y0, y1) = (y.saturating_sub(1), (y + 1).min(mask.h - 1));
    (y0..=y1).any(|yy| (x0..=x1).any(|xx| mask.at(xx, yy)))
}

impl BackgroundInpainter {
    /// Баллы считаются отдельно для поля: тип и фактура фона участвуют в выборе.
    pub fn resolve_mode(mode: InplaceBackgroundMode, block: TextBlockType, a: &BackgroundAnalysis) -> BackgroundRenderMode {
        use BackgroundRenderMode as R;
        match mode {
            InplaceBackgroundMode::TransparentOutline => R::Transparent,
            InplaceBackgroundMode::PaddedFill => R::AdaptivePaddingFill,
            // Never a flat rectangle: it would be visible on gradients, textures and translucent panels.
            InplaceBackgroundMode::TextReplacement => R::Reconstruct,
            InplaceBackgroundMode::Auto => {
                let flat = (1.0 - a.texture / 40.0).clamp(0.0, 1.0);
                let complex = (a.texture / 40.0).clamp(0.0, 1.0) + a.edge_density.min(1.0);
                let control = if matches!(block, TextBlockType::Button | TextBlockType::MenuItem) { 1.5 } else { 0.0 };
                // Automatic mode selects an erasing strategy, including subtitles.
                // Drawing both strings at the same coordinates is only an explicit choice.
                let scores = [(R::SolidFill, flat + if a.reliable { 0.5 } else { 0.0 }),
                              (R::InpaintBlur, complex + if a.reliable { 0.3 } else { 0.0 }),
                              (R::AdaptivePaddingFill, control + flat * 0.2)];
                scores.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)).unwrap().0
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // One field's immutable frame/analysis/configuration inputs.
    pub fn render(&self, img: &RgbaImage, mask: &InkMask, rect: &Rect, line_height: f32, analysis: BackgroundAnalysis, settings: &InplaceSettings, block: TextBlockType) -> BackgroundResult {
        let mode = Self::resolve_mode(settings.background_mode, block, &analysis);
        let (fw, fh) = (img.width() as f32, img.height() as f32);
        let base_pad = (0.3 * line_height).clamp(2.0, 16.0);
        let pad = if mode == BackgroundRenderMode::AdaptivePaddingFill { (0.6 * line_height).clamp(4.0, 28.0) + settings.extra_margin } else { base_pad };
        let px = if mode == BackgroundRenderMode::AdaptivePaddingFill { pad + settings.padding_x } else { pad };
        let py = if mode == BackgroundRenderMode::AdaptivePaddingFill { pad + settings.padding_y } else { pad };
        let x0 = (rect.x - px).max(0.0);
        let y0 = (rect.y - py).max(0.0);
        let area = Rect::new(x0, y0, (rect.right() + px).min(fw) - x0, (rect.bottom() + py).min(fh) - y0);
        let image = match mode {
            BackgroundRenderMode::InpaintBlur => Some(std::sync::Arc::new(inpaint_blur(img, mask, &area, analysis.median))),
            BackgroundRenderMode::Reconstruct => Some(std::sync::Arc::new(reconstruct_patch(img, mask, rect, &area, line_height, &analysis))),
            _ => None,
        };
        BackgroundResult { mode, color: analysis.median, image, rect: area, analysis }
    }
}

/// Глифы закрашиваются от краёв к центру средним соседних пикселей фона, затем фрагмент
/// уменьшается и размывается. Результат маленький: растягивает его QML.
fn inpaint_blur(img: &RgbaImage, mask: &InkMask, area: &Rect, fallback: [u8; 3]) -> RgbaImage {
    let (x0, y0, w, h) = area.pixels(img.width(), img.height());
    let (wu, hu) = (w as usize, h as usize);
    // Маска глифов, расширенная на 2 px: обводка и сглаживание букв тоже стираются.
    let mut hole = vec![false; wu * hu];
    for y in 0..hu {
        for x in 0..wu {
            let (gx, gy) = (x0 as usize + x, y0 as usize + y);
            let r = 2usize;
            hole[y * wu + x] = (gy.saturating_sub(r)..=(gy + r).min(mask.h - 1))
                .any(|yy| (gx.saturating_sub(r)..=(gx + r).min(mask.w - 1)).any(|xx| mask.at(xx, yy)));
        }
    }
    let mut px: Vec<[f32; 3]> = (0..hu * wu).map(|i| {
        let p = img.get_pixel(x0 + (i % wu) as u32, y0 + (i / wu) as u32).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    }).collect();
    for _ in 0..48 {
        let mut filled = Vec::new();
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                if !hole[i] { continue; }
                let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= wu as i32 || ny >= hu as i32 { continue; }
                    let j = ny as usize * wu + nx as usize;
                    if !hole[j] { for c in 0..3 { sum[c] += px[j][c]; } n += 1.0; }
                }
                if n > 0.0 { filled.push((i, sum.map(|v| v / n))); }
            }
        }
        if filled.is_empty() { break; }
        for (i, c) in filled { px[i] = c; hole[i] = false; }
    }
    let mut out = RgbaImage::new(w, h);
    for (i, p) in px.iter().enumerate() {
        let c = if hole[i] { fallback.map(|v| v as f32) } else { *p };
        out.put_pixel((i % wu) as u32, (i / wu) as u32, image::Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255]));
    }
    let small_w = (w / 8).clamp(4, 64);
    let small_h = ((h as f32 * small_w as f32 / w as f32).round() as u32).max(2);
    let small = DynamicImage::ImageRgba8(out).resize_exact(small_w, small_h, FilterType::Triangle);
    image::imageops::blur(&small.to_rgba8(), 1.2)
}

/// Exemplar fill: the hole is filled from its border inwards. Each border pixel takes the colour of the
/// clean pixel (outside the original hole) whose surroundings best match the known surroundings of the
/// target, searched within `reach` pixels in steps of two. Whatever finds no match is left to diffusion.
fn copy_similar(px: &mut [[f32; 3]], hole: &mut [bool], original_hole: &[bool], w: usize, h: usize, reach: i32) {
    const R: i32 = 2;
    /// Largest mean squared colour difference (summed over the channels) per compared pixel.
    const MAX_MATCH: f32 = 900.0;
    let at = |x: i32, y: i32| y as usize * w + x as usize;
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < w as i32 && y < h as i32;
    // A candidate window must be clean (never a pixel of the original text) and lie in the area.
    let usable = |x: i32, y: i32| (-R..=R).all(|dy| (-R..=R).all(|dx| inside(x + dx, y + dy) && !original_hole[at(x + dx, y + dy)]));
    let mut rounds = 0;
    loop {
        rounds += 1;
        let front: Vec<(i32, i32)> = (0..h as i32).flat_map(|y| (0..w as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| hole[at(x, y)] && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| inside(x + dx, y + dy) && !hole[at(x + dx, y + dy)]))
            .collect();
        if front.is_empty() || rounds > 64 { break; }
        let mut filled = Vec::with_capacity(front.len());
        for &(x, y) in &front {
            let known: Vec<(i32, i32)> = (-R..=R).flat_map(|dy| (-R..=R).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| inside(x + dx, y + dy) && !hole[at(x + dx, y + dy)]).collect();
            let mut best: Option<(f32, [f32; 3])> = None;
            let mut qy = (y - reach).max(R);
            while qy <= (y + reach).min(h as i32 - 1 - R) {
                let mut qx = (x - reach).max(R);
                while qx <= (x + reach).min(w as i32 - 1 - R) {
                    if usable(qx, qy) {
                        let mut cost = 0.0f32;
                        for &(dx, dy) in &known {
                            let (a, b) = (px[at(x + dx, y + dy)], px[at(qx + dx, qy + dy)]);
                            cost += (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
                            if best.is_some_and(|(c, _)| cost >= c) { break; }
                        }
                        if best.is_none_or(|(c, _)| cost < c) { best = Some((cost, px[at(qx, qy)])); }
                    }
                    qx += 2;
                }
                qy += 2;
            }
            // A poor match (complex artwork, no repeating structure) would smear a foreign fragment into the
            // hole; such pixels are left to the diffusion, which at least continues the surrounding colour.
            if let Some((_, c)) = best.filter(|(cost, _)| *cost <= MAX_MATCH * known.len() as f32) { filled.push((at(x, y), c)); }
        }
        if filled.is_empty() { break; }
        for (i, c) in filled { px[i] = c; hole[i] = false; }
    }
}

/// Which pixels belong to the glyphs of `text`. The global ink mask also takes light or dark scenery for ink,
/// and hiding that would wipe parts of the picture. Here only pixels inside the field, whose colour is that
/// of the text itself (the most common colour among mask pixels of the field), count; the outline and
/// anti-aliasing around them are covered by the dilation of the caller.
fn glyph_pixels<'a>(img: &'a RgbaImage, mask: &'a InkMask, text: &Rect, dilation: usize) -> impl Fn(usize, usize) -> bool + 'a {
    let (x0, y0, w, h) = text.expand(dilation as f32, img.width() as f32, img.height() as f32).pixels(img.width(), img.height());
    let inside = move |x: usize, y: usize| x >= x0 as usize && x < (x0 + w) as usize && y >= y0 as usize && y < (y0 + h) as usize;
    let mut bins: std::collections::HashMap<(u8, u8, u8), (u32, [u32; 3])> = std::collections::HashMap::new();
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            if !mask.at(x as usize, y as usize) { continue; }
            let p = img.get_pixel(x, y).0;
            let e = bins.entry((p[0] >> 4, p[1] >> 4, p[2] >> 4)).or_default();
            e.0 += 1;
            for c in 0..3 { e.1[c] += p[c] as u32; }
        }
    }
    // The text colour: the heaviest bin together with its neighbours (anti-aliasing spreads a colour over several).
    let heaviest = bins.iter().max_by_key(|(_, v)| v.0).map(|(k, _)| *k);
    let ink = heaviest.map(|k| {
        let (mut n, mut sum) = (0u32, [0u32; 3]);
        for (b, v) in &bins {
            if (b.0 as i32 - k.0 as i32).abs() <= 1 && (b.1 as i32 - k.1 as i32).abs() <= 1 && (b.2 as i32 - k.2 as i32).abs() <= 1 {
                n += v.0;
                for c in 0..3 { sum[c] += v.1[c]; }
            }
        }
        sum.map(|v| (v / n.max(1)) as i32)
    });
    move |x, y| {
        if !inside(x, y) || !mask.at(x, y) { return false; }
        let Some(ink) = ink else { return true };
        let p = img.get_pixel(x as u32, y as u32).0;
        (0..3).all(|c| (p[c] as i32 - ink[c]).abs() <= 56)
    }
}

/// A full-resolution patch of `area` for "text replacement". The glyphs (grown by a fraction of the
/// line height, so outlines and anti-aliasing go too) are repainted from the surrounding background;
/// only those pixels are opaque, so the game's own background stays visible everywhere else and the
/// patch has no rectangular edge. A pixel next to the hole is half-transparent to hide the seam.
fn reconstruct_patch(img: &RgbaImage, mask: &InkMask, text: &Rect, area: &Rect, line_height: f32, analysis: &BackgroundAnalysis) -> RgbaImage {
    let fallback = analysis.median;
    let (x0, y0, w, h) = area.pixels(img.width(), img.height());
    let (wu, hu) = (w as usize, h as usize);
    let r = (line_height * 0.12).round().clamp(2.0, 6.0) as usize;
    let (pw, ph) = (wu + 2 * r, hu + 2 * r);
    // The mask over the area plus a margin of `r`, so that glyphs just outside still reach the edge.
    let glyphs = glyph_pixels(img, mask, text, r);
    let mut grid = vec![false; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let (gx, gy) = ((x0 as usize + x).wrapping_sub(r), (y0 as usize + y).wrapping_sub(r));
            grid[y * pw + x] = gx < mask.w && gy < mask.h && glyphs(gx, gy);
        }
    }
    let mut across = vec![false; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            across[y * pw + x] = (x.saturating_sub(r)..=(x + r).min(pw - 1)).any(|xx| grid[y * pw + xx]);
        }
    }
    let mut hole = vec![false; wu * hu];
    for y in 0..hu {
        for x in 0..wu {
            // Padded row of this pixel is y + r; its neighbourhood is rows y..=y + 2r.
            hole[y * wu + x] = (y..=y + 2 * r).any(|yy| across[yy * pw + x + r]);
        }
    }
    let original: Vec<[f32; 3]> = (0..hu * wu).map(|i| {
        let p = img.get_pixel(x0 + (i % wu) as u32, y0 + (i / wu) as u32).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    }).collect();
    let opaque = hole.clone();
    let mut px = original.clone();
    // A patterned background (checks, halftone, clouds) is continued by copying the best-matching
    // clean pixel nearby; a smooth one or a gradient is better served by the diffusion below.
    let textured = analysis.reliable && analysis.texture >= 12.0;
    if textured { copy_similar(&mut px, &mut hole, &opaque, wu, hu, (line_height * 2.0).round().clamp(16.0, 40.0) as i32); }
    // Peel the hole from its border inwards: each pixel takes the mean of its known neighbours.
    for _ in 0..(4 * r + 24) {
        let mut filled = Vec::new();
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                if !hole[i] { continue; }
                let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= wu as i32 || ny >= hu as i32 { continue; }
                    let j = ny as usize * wu + nx as usize;
                    if !hole[j] { for c in 0..3 { sum[c] += px[j][c]; } n += 1.0; }
                }
                if n > 0.0 { filled.push((i, sum.map(|v| v / n))); }
            }
        }
        if filled.is_empty() { break; }
        for (i, c) in filled { px[i] = c; hole[i] = false; }
    }
    let mut out = RgbaImage::new(w, h);
    for i in 0..wu * hu {
        let (x, y) = (i % wu, i / wu);
        // A pixel nothing could be copied or diffused into is left to the game rather than painted black.
        let c = if hole[i] { fallback.map(|v| v as f32) } else { px[i] };
        let near = !opaque[i] && [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            nx >= 0 && ny >= 0 && nx < wu as i32 && ny < hu as i32 && opaque[ny as usize * wu + nx as usize]
        });
        let alpha = if hole[i] { 0 } else if opaque[i] { 255 } else if near { 128 } else { 0 };
        out.put_pixel(x as u32, y as u32, image::Rgba([c[0] as u8, c[1] as u8, c[2] as u8, alpha]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::block_detector::BlockDetector;
    use crate::layout::testing::*;
    use image::Rgba;

    fn setup(img: &RgbaImage) -> (InkMask, Rect, f32) {
        let mask = BlockDetector::ink_mask(img);
        let blocks = BlockDetector::default().detect_text_blocks(img, &mask);
        (mask, blocks[0].rect, blocks[0].line_height())
    }

    #[test]
    fn flat_panel_gets_a_solid_fill_of_its_colour() {
        let mut img = canvas(800, 200, [40, 60, 120]);
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        assert!(a.reliable && a.texture < 5.0, "{a:?}");
        assert_eq!(a.median, [40, 60, 120], "glyph pixels are excluded");
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &InplaceSettings::default(), TextBlockType::Unknown);
        assert_eq!(r.mode, BackgroundRenderMode::SolidFill);
        assert!(r.image.is_none() && r.rect.w > rect.w);
    }

    #[test]
    fn automatic_strategy_depends_on_the_field_type() {
        let a = BackgroundAnalysis { median: [32, 40, 50], mean: [32, 40, 50], luminance: 0.16,
            texture: 3.0, edge_density: 0.01, gradient: 0.0, reliable: true };
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Dialogue, &a), BackgroundRenderMode::SolidFill);
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Subtitle, &a), BackgroundRenderMode::SolidFill);
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Button, &a), BackgroundRenderMode::AdaptivePaddingFill);
    }

    #[test]
    fn textured_scene_is_inpainted_without_the_old_letters() {
        let mut img = canvas(800, 200, [0, 0, 0]);
        for y in 0..200 { for x in 0..800 { let v = ((x / 6 + y / 6) % 2 * 90 + 40) as u8; img.put_pixel(x, y, Rgba([v, v / 2, 30, 255])); } }
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        for block in [TextBlockType::Unknown, TextBlockType::Subtitle] {
            let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a.clone(), &InplaceSettings::default(), block);
            assert_eq!(r.mode, BackgroundRenderMode::InpaintBlur, "{block:?} must erase the original text");
            let small = r.image.unwrap();
            assert!(small.width() <= 64);
            // Белые буквы не просвечивают: самый яркий пиксель заливки далёк от белого.
            let brightest = small.pixels().map(|p| luma(&p.0)).max().unwrap();
            assert!(brightest < 200, "{block:?}: brightest {brightest}");
        }
    }

    #[test]
    fn explicit_modes_are_kept_and_padding_adapts() {
        let mut img = canvas(800, 200, [200, 200, 200]);
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [10, 10, 10], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, 5.0);
        let solid = BackgroundInpainter.render(&img, &mask, &rect, lh, a.clone(), &InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() }, TextBlockType::Unknown);
        let adaptive = BackgroundInpainter.render(&img, &mask, &rect, lh, a.clone(), &InplaceSettings { background_mode: InplaceBackgroundMode::PaddedFill, ..Default::default() }, TextBlockType::Unknown);
        let transparent = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &InplaceSettings { background_mode: InplaceBackgroundMode::TransparentOutline, ..Default::default() }, TextBlockType::Unknown);
        assert!(adaptive.rect.w > solid.rect.w);
        assert_eq!(transparent.mode, BackgroundRenderMode::Transparent);
        assert!(transparent.image.is_none());
    }

    #[test]
    fn text_replacement_patch_is_opaque_only_under_the_glyphs() {
        let mut img = canvas(800, 200, [0, 0, 0]);
        // A vertical gradient: a flat rectangle of one colour would be visible on it.
        for y in 0..200 { for x in 0..800 { img.put_pixel(x, y, Rgba([(40 + y / 2) as u8, 60, 90, 255])); } }
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let s = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &s, TextBlockType::Dialogue);
        assert_eq!(r.mode, BackgroundRenderMode::Reconstruct);
        let patch = r.image.expect("a patch");
        let (ax, ay, aw, ah) = r.rect.pixels(img.width(), img.height());
        assert_eq!((patch.width(), patch.height()), (aw, ah), "full resolution");
        let (mut opaque, mut transparent) = (0, 0);
        for (x, y, p) in patch.enumerate_pixels() {
            let truth = img.get_pixel(ax + x, ay + y).0;
            if p.0[3] == 255 {
                opaque += 1;
                // Repainted pixels follow the gradient instead of keeping the white glyphs.
                assert!(luma(&p.0) < 160, "glyph left at {x},{y}: {p:?}");
                assert!((p.0[0] as i32 - (40 + (ay + y) as i32 / 2)).abs() < 14, "background not reconstructed at {x},{y}: {p:?} vs {truth:?}");
            }
            if p.0[3] == 0 { transparent += 1; }
        }
        // The margin of the area stays clear, so the patch has no rectangular edge of its own.
        assert!(opaque > 0 && transparent > 0, "{opaque} opaque, {transparent} clear");
        for (x, y, p) in patch.enumerate_pixels().filter(|(x, y, _)| *x == 0 || *y == 0 || *x == aw - 1 || *y == ah - 1) {
            assert_eq!(p.0[3], 0, "the edge of the patch must be clear at {x},{y}");
        }
    }
}
