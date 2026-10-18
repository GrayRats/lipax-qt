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
//!
//! Замена только глифов не должна оставлять «призрак» оригинала: после `Reconstruct` считается доля пикселей
//! поля, которые всё ещё ближе к цвету букв, чем к фону (`ghost_residual`). Если она выше `GHOST_LIMIT`, маска
//! учитывает все «чернила» поля, а не только цвет основного текста (выделенное слово другого цвета), и
//! расширяется. Только в явном режиме «Подмена текста»: `Auto` выбирает режим как раньше (заливка или размытая
//! плашка), потому что на живых кадрах (диалог поверх картинки) перерисовка под буквами в `Auto` не всегда лучше.

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
    /// Средний перепад яркости между соседними пикселями кольца вокруг поля (0–255). Облако или градиент неба — большая
    /// `texture`, но малый `detail` рядом с ней; клетка и точки — заметная доля `texture`. Только такое стоит копировать.
    pub detail: f32,
}

/// Доля пикселей поля, которым разрешено остаться похожими на старый текст после замены. Предел взят из
/// Kizurium: его карточка остаётся не менее чем на 98,5 % непрозрачной, потому что при 10 % просвета 170
/// уровней серого оригинала дают ~17 уровней призрака, а при 1,5 % — меньше 3. Здесь тот же порядок: 1,5 %.
const GHOST_LIMIT: f32 = 0.015;
/// Насколько пиксель должен сдвинуться от цвета фона к цвету букв (доля пути), чтобы считаться призраком.
const GHOST_REACH: f32 = 0.35;
/// Если буквы и фон различаются меньше, призрак всё равно не виден: метрика ничего не говорит.
const GHOST_MIN_CONTRAST: f32 = 40.0;
/// На сколько px маска букв расширяется на последней повторной попытке и предел расширения.
const GHOST_GROW: usize = 3;
const MAX_DILATION: usize = 9;
/// Пиксель маски чернил, отличающийся от фона хотя бы на столько по какому-либо каналу, считается буквой
/// при повторной попытке (любой цвет, а не только цвет основного текста).
const ANY_INK_DISTANCE: i32 = 40;
/// The most a retry may enlarge the repainted (opaque) area of the patch and still be believed.
const MAX_GROWTH: f32 = 1.6;
/// Largest area (px) of a field that still gets the exemplar search.
const EXEMPLAR_MAX_AREA: usize = 150_000;
/// Up to this area the candidates are searched at every second pixel; above it at every fourth (4× less work).
const EXEMPLAR_FINE_AREA: usize = 12_000;
/// Sweeps of the smoothing after the peeling diffusion (see `relax`).
const RELAX_SWEEPS: usize = 80;
/// The exemplar search needs a pattern to copy (checks, dots): much of the brightness variation of the ring is in the
/// steps between neighbouring pixels, `detail / texture`. Measured on game frames: checks 0.066, a cloud 0.025, a sky
/// gradient 0.015. Copying from clouds and gradients pastes light pieces of cloud into the sky, because a context of
/// a few pixels cannot tell where on a smooth picture a fragment came from.
const EXEMPLAR_MIN_DETAIL_RATIO: f32 = 0.04;

/// Whether the hole of a field of `area` pixels is filled by copying from its surroundings (a repeating pattern) and
/// not by diffusion alone.
fn exemplar_worthy(analysis: &BackgroundAnalysis, area: usize) -> bool {
    analysis.reliable && analysis.texture >= 12.0 && analysis.detail >= EXEMPLAR_MIN_DETAIL_RATIO * analysis.texture && area <= EXEMPLAR_MAX_AREA
}

/// How `reconstruct_patch` decides what is a letter: the first attempt takes the colour of the main text only.
#[derive(Debug, Clone, Copy)]
struct Retry { any_colour: bool, grow: usize }
impl Retry {
    const FIRST: Self = Self { any_colour: false, grow: 0 };
    const ANY_COLOUR: Self = Self { any_colour: true, grow: 0 };
    const WIDEST: Self = Self { any_colour: true, grow: GHOST_GROW };
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
    /// Доля пикселей поля, оставшихся похожими на буквы оригинала после `Reconstruct` (0 для других режимов
    /// и когда буквы не отличить от фона).
    pub ghost: f32,
    /// Это быстрый патч (`render_quick`): фон узорный, и поиск образца, который его улучшил бы, пропущен. Полный
    /// результат считается отдельно (`render`) и подменяет этот.
    pub refinable: bool,
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
        let (mut steps, mut step_n) = (0.0f64, 0u32);
        let cy = rect.center().1;
        let in_rect = |x: u32, y: u32| (x as f32) >= rect.x && (x as f32) < rect.right() && (y as f32) >= rect.y && (y as f32) < rect.bottom();
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let inside = in_rect(x, y);
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
                    // Only pairs in the ring around the field count: it holds no text, and the ink mask also takes the
                    // squares of a pattern for ink, which would hide exactly the patterns that are worth copying.
                    if !inside && !in_rect(x + 1, y) { steps += l.abs_diff(q) as f64; step_n += 1; }
                }
                if y + 1 < y0 + h && !inside && !in_rect(x, y + 1) {
                    steps += l.abs_diff(luma(&img.get_pixel(x, y + 1).0)) as f64;
                    step_n += 1;
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
            detail: (steps / step_n.max(1) as f64) as f32,
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
        self.render_with(img, mask, rect, line_height, analysis, settings, block, false)
    }

    /// The same, without the exemplar search (see `exemplar_worthy`): a smooth fill in a few milliseconds that stands
    /// in until the full one is ready. `refinable` says whether a full one is any different.
    #[allow(clippy::too_many_arguments)]
    pub fn render_quick(&self, img: &RgbaImage, mask: &InkMask, rect: &Rect, line_height: f32, analysis: BackgroundAnalysis, settings: &InplaceSettings, block: TextBlockType) -> BackgroundResult {
        self.render_with(img, mask, rect, line_height, analysis, settings, block, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_with(&self, img: &RgbaImage, mask: &InkMask, rect: &Rect, line_height: f32, analysis: BackgroundAnalysis, settings: &InplaceSettings, block: TextBlockType, quick: bool) -> BackgroundResult {
        let mode = Self::resolve_mode(settings.background_mode, block, &analysis);
        let (fw, fh) = (img.width() as f32, img.height() as f32);
        let base_pad = (0.3 * line_height).clamp(2.0, 16.0);
        let pad = if mode == BackgroundRenderMode::AdaptivePaddingFill { (0.6 * line_height).clamp(4.0, 28.0) + settings.extra_margin } else { base_pad };
        let px = if mode == BackgroundRenderMode::AdaptivePaddingFill { pad + settings.padding_x } else { pad };
        let py = if mode == BackgroundRenderMode::AdaptivePaddingFill { pad + settings.padding_y } else { pad };
        let x0 = (rect.x - px).max(0.0);
        let y0 = (rect.y - py).max(0.0);
        let area = Rect::new(x0, y0, (rect.right() + px).min(fw) - x0, (rect.bottom() + py).min(fh) - y0);
        let mut ghost = 0.0;
        let mut refinable = false;
        let image = match mode {
            BackgroundRenderMode::InpaintBlur => Some(std::sync::Arc::new(inpaint_blur(img, mask, &area, analysis.median))),
            BackgroundRenderMode::Reconstruct => {
                refinable = quick && exemplar_worthy(&analysis, (area.w * area.h) as usize);
                let (patch, left) = reconstruct_without_ghost(img, mask, rect, &area, line_height, &analysis, !quick);
                ghost = left;
                if left > GHOST_LIMIT { tracing::warn!(target: "inplace.background", ghost = left, "text replacement leaves traces of the old text"); }
                Some(std::sync::Arc::new(patch))
            }
            _ => None,
        };
        BackgroundResult { mode, color: analysis.median, image, rect: area, analysis, ghost, refinable }
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

/// Threads for `items` independent jobs: half of the cores (the game keeps the rest) and at least 64 jobs each, so a
/// small field does not pay for starting threads.
fn workers_for(items: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 2).clamp(1, 8).min(items / 64).max(1)
}

/// Exemplar fill: the hole is filled from its border inwards. Each border pixel takes the colour of the
/// clean pixel (outside the original hole) whose surroundings best match the known surroundings of the
/// target, searched within `reach` pixels in steps of two. Whatever finds no match is left to diffusion.
fn copy_similar(px: &mut [[f32; 3]], hole: &mut [bool], original_hole: &[bool], w: usize, h: usize, reach: i32, stride: i32) {
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
        // Every pixel of the front is solved against the picture as it was before the round, so the pixels are
        // independent: the front is cut into pieces for several threads (the order of the results is that of the front).
        let current: &[[f32; 3]] = px;
        let solve = |&(x, y): &(i32, i32)| -> Option<(usize, [f32; 3])> {
            let px = current;
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
                    qx += stride;
                }
                qy += stride;
            }
            // A poor match (complex artwork, no repeating structure) would smear a foreign fragment into the
            // hole; such pixels are left to the diffusion, which at least continues the surrounding colour.
            best.filter(|(cost, _)| *cost <= MAX_MATCH * known.len() as f32).map(|(_, c)| (at(x, y), c))
        };
        let workers = workers_for(front.len());
        let filled: Vec<(usize, [f32; 3])> = if workers <= 1 {
            front.iter().filter_map(solve).collect()
        } else {
            let piece = front.len().div_ceil(workers);
            std::thread::scope(|scope| {
                let solve = &solve;
                let handles: Vec<_> = front.chunks(piece).map(|part| scope.spawn(move || part.iter().filter_map(solve).collect::<Vec<_>>())).collect();
                handles.into_iter().flat_map(|handle| handle.join().expect("exemplar search panicked")).collect()
            })
        };
        if filled.is_empty() { break; }
        for (i, c) in filled { px[i] = c; hole[i] = false; }
    }
}

/// Smooths what the peeling diffusion filled: every pixel of `pending` becomes the mean of its four neighbours, the
/// pixels around the hole stay as they are (Gauss-Seidel, sweeping forwards and backwards). The peeling alone leaves
/// the streaks of its fill order (rows and columns copied inwards); the relaxed fill is the smooth surface that joins
/// the border of the hole.
fn relax(px: &mut [[f32; 3]], pending: &[bool], w: usize, h: usize, sweeps: usize) {
    let cells: Vec<usize> = (0..w * h).filter(|&i| pending[i]).collect();
    for sweep in 0..sweeps {
        let order: Box<dyn Iterator<Item = &usize>> = if sweep % 2 == 0 { Box::new(cells.iter()) } else { Box::new(cells.iter().rev()) };
        for &i in order {
            let (x, y) = (i % w, i / w);
            let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
            if x > 0 { for c in 0..3 { sum[c] += px[i - 1][c]; } n += 1.0; }
            if x + 1 < w { for c in 0..3 { sum[c] += px[i + 1][c]; } n += 1.0; }
            if y > 0 { for c in 0..3 { sum[c] += px[i - w][c]; } n += 1.0; }
            if y + 1 < h { for c in 0..3 { sum[c] += px[i + w][c]; } n += 1.0; }
            if n > 0.0 { px[i] = sum.map(|v| v / n); }
        }
    }
}

/// Which pixels belong to the glyphs of `text`. The global ink mask also takes light or dark scenery for ink,
/// and hiding that would wipe parts of the picture. Here only pixels inside the field, whose colour is that
/// of the text itself (the most common colour among mask pixels of the field), count; the outline and
/// anti-aliasing around them are covered by the dilation of the caller.
///
/// With `any_colour` (a retry after a ghost was found) every mask pixel of the field that differs from the
/// background counts: a keyword set in another colour is text as well.
fn glyph_pixels<'a>(img: &'a RgbaImage, mask: &'a InkMask, text: &Rect, dilation: usize, any_colour: Option<[u8; 3]>) -> impl Fn(usize, usize) -> bool + 'a {
    let (x0, y0, w, h) = text.expand(dilation as f32, img.width() as f32, img.height() as f32).pixels(img.width(), img.height());
    let inside = move |x: usize, y: usize| x >= x0 as usize && x < (x0 + w) as usize && y >= y0 as usize && y < (y0 + h) as usize;
    let ink = text_ink(img, mask, text, dilation);
    move |x, y| {
        if !inside(x, y) || !mask.at(x, y) { return false; }
        let p = img.get_pixel(x as u32, y as u32).0;
        let Some(ink) = ink else { return true };
        // The retry only adds to the first pass: whatever the first pass repainted stays repainted.
        (0..3).all(|c| (p[c] as i32 - ink[c]).abs() <= 56)
            || any_colour.is_some_and(|bg| (0..3).any(|c| (p[c] as i32 - bg[c] as i32).abs() >= ANY_INK_DISTANCE))
    }
}

/// The colour of the text itself: the heaviest colour bin among the mask pixels of the field, together with its
/// neighbouring bins. `None` if the field has no mask pixels.
fn text_ink(img: &RgbaImage, mask: &InkMask, text: &Rect, dilation: usize) -> Option<[i32; 3]> {
    let (x0, y0, w, h) = text.expand(dilation as f32, img.width() as f32, img.height() as f32).pixels(img.width(), img.height());
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
    heaviest.map(|k| {
        let (mut n, mut sum) = (0u32, [0u32; 3]);
        for (b, v) in &bins {
            if (b.0 as i32 - k.0 as i32).abs() <= 1 && (b.1 as i32 - k.1 as i32).abs() <= 1 && (b.2 as i32 - k.2 as i32).abs() <= 1 {
                n += v.0;
                for c in 0..3 { sum[c] += v.1[c]; }
            }
        }
        sum.map(|v| (v / n.max(1)) as i32)
    })
}

/// A full-resolution patch of `area` for "text replacement". The glyphs (grown by a fraction of the
/// line height, so outlines and anti-aliasing go too) are repainted from the surrounding background;
/// only those pixels are opaque, so the game's own background stays visible everywhere else and the
/// patch has no rectangular edge. A pixel next to the hole is half-transparent to hide the seam.
#[allow(clippy::too_many_arguments)] // One field's frame, mask, rectangles and the two switches of the attempt.
fn reconstruct_patch(img: &RgbaImage, mask: &InkMask, text: &Rect, area: &Rect, line_height: f32, analysis: &BackgroundAnalysis, retry: Retry, exemplar: bool) -> RgbaImage {
    let fallback = analysis.median;
    let (x0, y0, w, h) = area.pixels(img.width(), img.height());
    let (wu, hu) = (w as usize, h as usize);
    let r = ((line_height * 0.12).round().clamp(2.0, 6.0) as usize + retry.grow).min(MAX_DILATION);
    let (pw, ph) = (wu + 2 * r, hu + 2 * r);
    // The mask over the area plus a margin of `r`, so that glyphs just outside still reach the edge.
    let glyphs = glyph_pixels(img, mask, text, r, retry.any_colour.then_some(fallback));
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
    // The outline and the soft edge of a letter have another colour than its core and the colour test above leaves
    // them out; a bright dot of them left beside the hole would be spread into it. Ink of the mask that touches the hole
    // joins it (two pixels deep: the width of an outline).
    for _ in 0..2 {
        let before = hole.clone();
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                if before[i] || !mask.at(x0 as usize + x, y0 as usize + y) { continue; }
                let touches = (x > 0 && before[i - 1]) || (x + 1 < wu && before[i + 1]) || (y > 0 && before[i - wu]) || (y + 1 < hu && before[i + wu]);
                if touches { hole[i] = true; }
            }
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
    // The exemplar search costs ~20 µs per pixel of the area at stride 2 (2 s for a dialogue block) and runs in the OCR
    // tick, before the new translation can appear: a big field is searched at a coarser stride, a huge one not at all.
    if exemplar && exemplar_worthy(analysis, wu * hu) {
        let stride = if wu * hu <= EXEMPLAR_FINE_AREA { 2 } else { 4 };
        copy_similar(&mut px, &mut hole, &opaque, wu, hu, (line_height * 2.0).round().clamp(16.0, 40.0) as i32, stride);
    }
    // What the exemplar search could not fill: the diffusion below fills it and `relax` smooths it.
    let to_diffuse = hole.clone();
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
    relax(&mut px, &to_diffuse, wu, hu, RELAX_SWEEPS);
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

/// How much of the old text still shows through `patch`. The patch is laid over the frame as QML does (alpha
/// blending); a pixel of the text rectangle counts as a ghost when it lies at least `GHOST_REACH` of the colour
/// distance between the letters and the background away from the background (whatever the direction: a letter
/// of another colour than the main text ghosts as well). The same share among the pixels around the text
/// (background that merely looks like the letters, e.g. white clouds behind white text) is not charged.
fn ghost_residual(img: &RgbaImage, patch: &RgbaImage, area: &Rect, text: &Rect, ink: [i32; 3], background: [u8; 3]) -> f32 {
    let contrast2: f32 = (0..3).map(|c| ((ink[c] - background[c] as i32) as f32).powi(2)).sum();
    if contrast2 < GHOST_MIN_CONTRAST * GHOST_MIN_CONTRAST { return 0.0; }
    let reach2 = GHOST_REACH * GHOST_REACH * contrast2;
    let (ax, ay, aw, ah) = area.pixels(img.width(), img.height());
    let (tx, ty, tw, th) = text.pixels(img.width(), img.height());
    let share = |inside_text: bool| -> f32 {
        let (mut ghost, mut total) = (0u32, 0u32);
        for y in ay..ay + ah {
            for x in ax..ax + aw {
                let in_text = x >= tx && x < tx + tw && y >= ty && y < ty + th;
                if in_text != inside_text { continue; }
                let o = img.get_pixel(x, y).0;
                let q = patch.get_pixel(x - ax, y - ay).0;
                let a = q[3] as f32 / 255.0;
                let away2: f32 = (0..3).map(|c| (o[c] as f32 * (1.0 - a) + q[c] as f32 * a - background[c] as f32).powi(2)).sum();
                total += 1;
                if away2 >= reach2 { ghost += 1; }
            }
        }
        if total == 0 { 0.0 } else { ghost as f32 / total as f32 }
    };
    (share(true) - share(false)).max(0.0)
}

/// Pixels of the patch that replace the picture completely.
fn repainted(patch: &RgbaImage) -> usize { patch.pixels().filter(|p| p.0[3] == 255).count() }

/// `reconstruct_patch` and its ghost. While the ghost is above the limit the patch is built again, at most twice (once on a patterned background):
/// first counting every ink pixel of the field as a letter (a keyword in another colour), then also with a
/// wider mask (the outline or the soft edge of the letters was wider than the first dilation).
fn reconstruct_without_ghost(img: &RgbaImage, mask: &InkMask, text: &Rect, area: &Rect, line_height: f32, analysis: &BackgroundAnalysis, exemplar: bool) -> (RgbaImage, f32) {
    let ink = text_ink(img, mask, text, 0);
    let measure = |patch: &RgbaImage| ink.map_or(0.0, |ink| ghost_residual(img, patch, area, text, ink, analysis.median));
    let mut patch = reconstruct_patch(img, mask, text, area, line_height, analysis, Retry::FIRST, exemplar);
    let mut ghost = measure(&patch);
    // On a patterned background every attempt runs the costly exemplar search (`copy_similar`): one retry, the
    // widest, instead of two.
    let textured = exemplar && exemplar_worthy(analysis, (area.w * area.h) as usize);
    let retries: &[Retry] = if textured { &[Retry::WIDEST] } else { &[Retry::ANY_COLOUR, Retry::WIDEST] };
    for &retry in retries {
        if ghost <= GHOST_LIMIT { break; }
        let retry = reconstruct_patch(img, mask, text, area, line_height, analysis, retry, exemplar);
        let left = measure(&retry);
        // A retry that repaints more can also repaint scenery (and pull the colours of its neighbours into the
        // hole): it is kept only if it clearly removes the ghost and does not grow the repainted area much.
        // A keyword in another colour adds a part of the text; scenery taken for letters adds far more.
        if left < ghost * 0.5 && repainted(&retry) as f32 <= MAX_GROWTH * repainted(&patch).max(1) as f32 { (patch, ghost) = (retry, left); }
    }
    (patch, ghost)
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
            texture: 3.0, edge_density: 0.01, gradient: 0.0, reliable: true, detail: 1.0 };
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Dialogue, &a), BackgroundRenderMode::SolidFill);
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Subtitle, &a), BackgroundRenderMode::SolidFill);
        assert_eq!(BackgroundInpainter::resolve_mode(InplaceBackgroundMode::Auto, TextBlockType::Button, &a), BackgroundRenderMode::AdaptivePaddingFill);
    }

    /// The frame the engine sees, with `patch` laid over it the way QML does.
    fn composite(img: &RgbaImage, r: &BackgroundResult) -> RgbaImage {
        let patch = r.image.as_ref().expect("a patch");
        let (ax, ay, _, _) = r.rect.pixels(img.width(), img.height());
        let mut out = img.clone();
        for (x, y, q) in patch.enumerate_pixels() {
            let o = out.get_pixel_mut(ax + x, ay + y);
            let a = q.0[3] as f32 / 255.0;
            for c in 0..3 { o.0[c] = (o.0[c] as f32 * (1.0 - a) + q.0[c] as f32 * a).round() as u8; }
        }
        out
    }

    /// A small checkerboard: the exemplar search of the repaint is slow in a debug build.
    fn checks() -> RgbaImage {
        let mut img = canvas(260, 90, [0, 0, 0]);
        for y in 0..90 { for x in 0..260 { let v = ((x / 6 + y / 6) % 2 * 90 + 40) as u8; img.put_pixel(x, y, Rgba([v, v / 2, 30, 255])); } }
        img
    }

    #[test]
    fn the_ghost_is_measured_where_the_old_text_stays_visible() {
        let mut img = canvas(800, 200, [30, 40, 60]);
        draw_line(&mut img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let area = rect.expand(6.0, 800.0, 200.0);
        let ink = text_ink(&img, &mask, &rect, 0).expect("the letters have a colour");
        assert!(ink.iter().all(|&v| v > 240), "{ink:?}");
        let (aw, ah) = { let (_, _, w, h) = area.pixels(800, 200); (w, h) };
        // A patch that repaints nothing leaves every glyph pixel: a clearly visible ghost.
        let nothing = RgbaImage::new(aw, ah);
        assert!(ghost_residual(&img, &nothing, &area, &rect, ink, a.median) > 0.1);
        // The true background (what the frame looked like before the text) leaves none.
        let (ax, ay, _, _) = area.pixels(800, 200);
        let clean = RgbaImage::from_fn(aw, ah, |_, _| Rgba([30, 40, 60, 255]));
        assert_eq!(ghost_residual(&img, &clean, &area, &rect, ink, a.median), 0.0);
        let _ = (ax, ay);
        // Letters that barely differ from the background cannot ghost.
        assert_eq!(ghost_residual(&img, &nothing, &area, &rect, [40, 50, 70], a.median), 0.0);
    }

    /// Two words of one line: white and yellow, the second the smaller part of the ink.
    fn white_and_yellow(img: &mut RgbaImage) {
        let end = draw_line(img, 100, 80, 30, 14, 4, 22, 3, [255, 255, 255], false);
        draw_line(img, end + 12, 80, 10, 14, 4, 22, 3, [255, 220, 0], false);
    }

    #[test]
    fn a_keyword_of_another_colour_is_repainted_too() {
        let mut img = canvas(900, 200, [20, 24, 40]);
        white_and_yellow(&mut img);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let ink = text_ink(&img, &mask, &rect, 0).unwrap();
        assert!(ink.iter().all(|&v| v > 200), "the white text is the main ink: {ink:?}");
        let area = rect.expand(BackgroundAnalyzer::margin(lh), 900.0, 200.0);
        let first = reconstruct_patch(&img, &mask, &rect, &area, lh, &a, Retry::FIRST, true);
        let before = ghost_residual(&img, &first, &area, &rect, ink, a.median);
        let (_, after) = reconstruct_without_ghost(&img, &mask, &rect, &area, lh, &a, true);
        eprintln!("yellow word ghost: first {before:.4}, after the retry {after:.4}");
        assert!(before > GHOST_LIMIT, "the scene must defeat the first pass: {before}");
        assert!(after <= GHOST_LIMIT, "{after} vs {before}");
        // End to end, through the explicit mode.
        let s = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &s, TextBlockType::Dialogue);
        assert_eq!(r.mode, BackgroundRenderMode::Reconstruct);
        assert!(r.ghost <= GHOST_LIMIT, "{}", r.ghost);
        let shown = composite(&img, &r);
        let (tx, ty, tw, th) = rect.pixels(img.width(), img.height());
        let yellow_left = (ty..ty + th).flat_map(|y| (tx..tx + tw).map(move |x| (x, y))).filter(|&(x, y)| { let p = shown.get_pixel(x, y).0; p[0] > 200 && p[1] > 160 && p[2] < 80 }).count();
        assert_eq!(yellow_left, 0, "no yellow pixel of the old text remains");
    }

    /// A smooth sky: blue to light blue, brighter towards one corner (what a photographic background looks like).
    fn sky() -> RgbaImage {
        RgbaImage::from_fn(640, 180, |x, y| {
            let (t, u) = (x as f32 / 640.0, y as f32 / 180.0);
            Rgba([(40.0 + 120.0 * t * u) as u8, (110.0 + 70.0 * t) as u8, (210.0 - 40.0 * u) as u8, 255])
        })
    }

    /// The field as OCR gives it: the bounds of the ink in the rows the text was drawn in.
    fn drawn_field(mask: &InkMask, rows: std::ops::Range<usize>) -> Rect {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in rows { for x in 0..mask.w { if mask.at(x, y) { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } }
        Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
    }

    fn analysis_of(img: &RgbaImage) -> BackgroundAnalysis {
        let mask = BlockDetector::ink_mask(img);
        BackgroundAnalyzer.analyze(img, &mask, &drawn_field(&mask, 60..110), BackgroundAnalyzer::margin(30.0))
    }

    #[test]
    fn a_pattern_is_copied_but_a_smooth_picture_is_not() {
        // Checks every 8 px: a pattern to copy. Wide soft bands (a cloud): bright and dark, yet no detail.
        let mut checks = canvas(600, 160, [0, 0, 0]);
        for y in 0..160 { for x in 0..600 { let v = ((x / 8 + y / 8) % 2 * 70 + 60) as u8; checks.put_pixel(x, y, Rgba([v, v, 200, 255])); } }
        draw_line(&mut checks, 80, 70, 20, 14, 4, 22, 3, [255, 255, 255], false);
        let mut cloud = canvas(600, 160, [0, 0, 0]);
        for y in 0..160 { for x in 0..600 { let v = (130.0 + 110.0 * (x as f32 / 70.0).sin() * (y as f32 / 55.0).cos()) as u8; cloud.put_pixel(x, y, Rgba([v, v, 230, 255])); } }
        draw_line(&mut cloud, 80, 70, 20, 14, 4, 22, 3, [20, 20, 20], false);
        let (checks, cloud) = (analysis_of(&checks), analysis_of(&cloud));
        assert!(checks.detail >= EXEMPLAR_MIN_DETAIL_RATIO * checks.texture, "{checks:?}");
        assert!(cloud.texture >= 12.0 && cloud.detail < EXEMPLAR_MIN_DETAIL_RATIO * cloud.texture, "{cloud:?}");
        assert!(exemplar_worthy(&checks, 10_000) && !exemplar_worthy(&cloud, 10_000));
        assert!(!exemplar_worthy(&checks, EXEMPLAR_MAX_AREA + 1), "a huge field is never searched");
    }

    #[test]
    fn text_on_a_smooth_sky_is_replaced_by_the_sky_without_light_patches_or_streaks() {
        let clean = sky();
        let mut img = clean.clone();
        // White letters with a dark outline (copies of the dark text a little to every side).
        for (dx, dy) in [(-2i32, 0i32), (2, 0), (0, -2), (0, 2)] { draw_line(&mut img, (120 + dx) as u32, (80 + dy) as u32, 24, 14, 4, 26, 4, [20, 20, 20], false); }
        draw_line(&mut img, 120, 80, 24, 14, 4, 26, 4, [250, 250, 250], false);
        let mask = BlockDetector::ink_mask(&img);
        let (rect, lh) = (drawn_field(&mask, 70..120), 40.0);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let s = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &s, TextBlockType::Unknown);
        let out = composite(&img, &r);
        let (x0, y0, w, h) = r.rect.pixels(640, 180);
        let (mut sum, mut lightest) = (0.0f32, f32::MIN);
        for y in y0..y0 + h { for x in x0..x0 + w {
            let d = luma(&out.get_pixel(x, y).0) as f32 - luma(&clean.get_pixel(x, y).0) as f32;
            sum += d.abs();
            lightest = lightest.max(d);
        } }
        let mean = sum / (w * h) as f32;
        assert!(mean < 3.0, "the sky comes back: mean error {mean:.2}");
        assert!(lightest < 14.0, "no light patch or dot is left: {lightest:.1} levels lighter than the clean sky");
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
    fn explicit_text_replacement_repaints_a_textured_scene_under_the_glyphs_without_a_plate() {
        let mut img = checks();
        draw_line(&mut img, 30, 30, 8, 14, 4, 22, 3, [255, 255, 255], false);
        let (mask, rect, lh) = setup(&img);
        let a = BackgroundAnalyzer.analyze(&img, &mask, &rect, BackgroundAnalyzer::margin(lh));
        let s = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
        let r = BackgroundInpainter.render(&img, &mask, &rect, lh, a, &s, TextBlockType::Unknown);
        assert_eq!(r.mode, BackgroundRenderMode::Reconstruct);
        assert!(r.ghost <= GHOST_LIMIT, "ghost {}", r.ghost);
        let shown = composite(&img, &r);
        let (tx, ty, tw, th) = rect.pixels(img.width(), img.height());
        let brightest = (ty..ty + th).flat_map(|y| (tx..tx + tw).map(move |x| (x, y))).map(|(x, y)| luma(&shown.get_pixel(x, y).0)).max().unwrap();
        // The checkerboard has no white square: whatever is that bright is the old text.
        assert!(brightest < 160, "brightest pixel of the field {brightest}");
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
