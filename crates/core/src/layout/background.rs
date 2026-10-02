//! Фон под переводом — отдельно от шрифта и типографики.
//!
//! `BackgroundAnalyzer` смотрит на кольцо пикселей вокруг поля без глифов: медиана и среднее
//! цвета, яркость, текстура, плотность краёв, перепад сверху вниз. `BackgroundInpainter`
//! строит замену по выбранному режиму:
//! - `InpaintBlur`: глифы (с расширением маски) закрашиваются диффузией от соседних пикселей
//!   фона, фрагмент уменьшается и размывается по Гауссу; растянутый в QML со сглаживанием,
//!   он закрывает оригинал;
//! - `SolidFill`: цвет фона по кольцу;
//! - `AdaptivePaddingFill`: то же, с расширенными полями;
//! - `Transparent`: оригинал не стирается, перевод читается за счёт обводки и контраста.

use super::{InkMask, Rect, luma};
use crate::settings::InplaceBackgroundMode;
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
    pub mode: InplaceBackgroundMode,
    pub color: [u8; 3],
    /// Уменьшенная размытая заливка для `InpaintBlur`; область — `rect`.
    pub image: Option<RgbaImage>,
    /// Область заливки, px кадра (поле с полями).
    pub rect: Rect,
    pub analysis: BackgroundAnalysis,
}

#[derive(Default)]
pub struct BackgroundAnalyzer;

#[derive(Default)]
pub struct BackgroundInpainter;

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
    /// Режим `Auto`: однородный фон — заливка, сложный — восстановление с размытием.
    pub fn resolve_mode(mode: InplaceBackgroundMode, a: &BackgroundAnalysis) -> InplaceBackgroundMode {
        match mode {
            InplaceBackgroundMode::Auto if !a.reliable => InplaceBackgroundMode::SolidFill,
            InplaceBackgroundMode::Auto if a.texture < 12.0 && a.edge_density < 0.08 && a.gradient.abs() < 20.0 => InplaceBackgroundMode::SolidFill,
            InplaceBackgroundMode::Auto => InplaceBackgroundMode::InpaintBlur,
            m => m,
        }
    }

    pub fn render(&self, img: &RgbaImage, mask: &InkMask, rect: &Rect, line_height: f32, analysis: BackgroundAnalysis, mode: InplaceBackgroundMode) -> BackgroundResult {
        let mode = Self::resolve_mode(mode, &analysis);
        let (fw, fh) = (img.width() as f32, img.height() as f32);
        let base_pad = (0.3 * line_height).clamp(2.0, 16.0);
        let pad = if mode == InplaceBackgroundMode::AdaptivePaddingFill { (0.6 * line_height).clamp(4.0, 28.0) } else { base_pad };
        let area = rect.expand(pad, fw, fh);
        let image = (mode == InplaceBackgroundMode::InpaintBlur).then(|| inpaint_blur(img, mask, &area, analysis.median));
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
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, InplaceBackgroundMode::Auto);
        assert_eq!(r.mode, InplaceBackgroundMode::SolidFill);
        assert!(r.image.is_none() && r.rect.w > rect.w);
    }

    #[test]
    fn textured_scene_is_inpainted_without_the_old_letters() {
        let mut img = canvas(800, 200, [0, 0, 0]);
        for y in 0..200 { for x in 0..800 { let v = ((x / 6 + y / 6) % 2 * 90 + 40) as u8; img.put_pixel(x, y, Rgba([v, v / 2, 30, 255])); } }
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, InplaceBackgroundMode::Auto);
        assert_eq!(r.mode, InplaceBackgroundMode::InpaintBlur);
        let small = r.image.unwrap();
        assert!(small.width() <= 64);
        // Белые буквы не просвечивают: самый яркий пиксель заливки далёк от белого.
        let brightest = small.pixels().map(|p| luma(&p.0)).max().unwrap();
        assert!(brightest < 200, "brightest {brightest}");
    }

    #[test]
    fn explicit_modes_are_kept_and_padding_adapts() {
        let mut img = canvas(800, 200, [200, 200, 200]);
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [10, 10, 10], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, 5.0);
        let solid = BackgroundInpainter.render(&img, &mask, &rect, lh, a.clone(), InplaceBackgroundMode::SolidFill);
        let adaptive = BackgroundInpainter.render(&img, &mask, &rect, lh, a.clone(), InplaceBackgroundMode::AdaptivePaddingFill);
        let transparent = BackgroundInpainter.render(&img, &mask, &rect, lh, a, InplaceBackgroundMode::Transparent);
        assert!(adaptive.rect.w > solid.rect.w);
        assert_eq!(transparent.mode, InplaceBackgroundMode::Transparent);
        assert!(transparent.image.is_none());
    }
}
