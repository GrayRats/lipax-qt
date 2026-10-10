//! Размещение полей «поверх оригинала» на экране: подгонка перевода под поле, затем защита
//! соседних полей от перекрытия, затем запись, которую QML только рисует.
//!
//! Раньше это жило в мосте Qt и на каждый вызов заново подгоняло каждое поле. Здесь результат
//! типизирован (нет правок JSON по строковым ключам и `unwrap`), измерение текста — через
//! трейт [`TextMeasure`] (Qt в приложении, модель в тестах), а подгонка кэшируется:
//!
//! - ключ кэша — поле, его ревизия (меняется при смене текста, шрифта, фона), размер его текстовой
//!   области на экране, масштаб кадра по обеим осям и отпечаток настроек, поэтому перемещение окна
//!   игры и повторные публикации ничего не пересчитывают;
//! - положение на рабочем столе и коллизии считаются каждый раз: это арифметика над прямоугольниками.
//!
//! Всё, что рисуется, остаётся в видимой части окна игры ([`Viewport`]): клиентская область,
//! обрезанная экраном, на котором виден оригинал. Перевод, который не помещается в поле оригинала
//! даже минимальным кеглем, получает больше места рядом с ним (свободная часть окна, но не чужой
//! текст); если и этого мало — обрезается многоточием в пределах найденного места.

use super::background::BackgroundRenderMode;
use super::collision::{self, Candidate};
use super::engine::{InplaceBlock, InplaceFrame};
use super::fit::{FitInput, FitResult, FontSpec, TextMeasure, fit_translation_to_box};
use super::{DesktopRect, FontCategory, FrameToDesktop, TextAlignment, TextBlockType, WrapMode, contrast_ratio, hex, parse_hex_color};
use crate::capture::kwin::WindowGeometry;
use crate::settings::{InplaceSettings, NormRect};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedBackground {
    pub mode: BackgroundRenderMode,
    pub color: String,
    pub opacity: f32,
    pub radius: f32,
    /// URL подложки `InpaintBlur`; пусто, если картинка не нужна.
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedFont {
    pub category: FontCategory,
    pub generic: bool,
    pub confidence: f32,
}

/// Поле, готовое к отрисовке: всё решено, QML ничего не выбирает.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    pub key: String,
    pub region_id: String,
    pub block_id: u64,
    pub block_type: TextBlockType,
    /// Область внутри окна игры (доли окна).
    pub rect: NormRect,
    /// Заливка поля внутри области (доли кадра области).
    #[serde(rename = "box")]
    pub box_rect: [f64; 4],
    /// Отступы текста внутри заливки: слева, сверху, справа, снизу; px экрана.
    pub inner: [f32; 4],
    pub text: String,
    pub original: String,
    pub font_family: String,
    pub font_px: f32,
    pub font_weight: i32,
    pub italic: bool,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub alignment: &'static str,
    pub wrap: &'static str,
    pub max_lines: u32,
    pub text_color: String,
    /// Без заливки текст читается за счёт обводки.
    pub outline: bool,
    pub outline_color: String,
    pub outline_width: f32,
    pub shadow: bool,
    pub text_opacity: f32,
    pub background: PlacedBackground,
    pub font_selection: PlacedFont,
    /// The translation did not fit the box of the original even at the minimum size and was given more
    /// room next to it: free space of the game window, never over other text.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub expanded: bool,
    /// How the field was degraded to fit, if it was: `plain-plate` (a plain plate instead of the restored
    /// background), `beside-original` (a compact label next to the text it translates), `inside-original`
    /// (the plate is the box of the original text and the translation is cut to it) or `elided` (no free
    /// space is large enough: the translation is cut with an ellipsis in the largest box there is).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<&'static str>,
}

/// A translation that could not be drawn over the game at all. It is not sent anywhere else: the in-place mode never
/// opens the translation window. It is only counted, so the main window can say that something was left out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DroppedText {
    pub region_id: String,
    pub block_id: u64,
    pub text: String,
    pub original: String,
    pub reason: &'static str,
}

/// Everything `place_regions` decided.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlacementOutcome {
    pub placed: Vec<Placed>,
    pub dropped: Vec<DroppedText>,
}

/// Область с её последним результатом движка.
pub struct RegionInput<'a> {
    pub id: &'a str,
    pub rect: NormRect,
    pub frame: &'a InplaceFrame,
}

/// Where fields may be drawn: the client area of the game window (no title bar or frame) and the
/// screens of the desktop, both in logical desktop pixels as Qt and KWin report them.
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    pub window: WindowGeometry,
    pub screens: Vec<DesktopRect>,
}

impl Viewport {
    pub fn new(window: WindowGeometry, screens: Vec<DesktopRect>) -> Self {
        Self { window, screens }
    }

    /// Only the window, taken as fully visible: before Qt has reported its screens, and in tests.
    pub fn window(window: WindowGeometry) -> Self {
        Self::new(window, Vec::new())
    }

    /// Where a field whose original lies at `source` may be drawn: the part of the client area on the
    /// screen that shows most of the original. A window across two monitors gives each field its own
    /// monitor (a layer-shell surface lives on one output), and a window partly off the desktop gives
    /// only its visible part. `None`: the original is on no screen, there is nothing to cover.
    pub fn bounds_for(&self, source: &DesktopRect) -> Option<DesktopRect> {
        let window = DesktopRect::of_window(&self.window);
        let visible = if self.screens.is_empty() { window } else {
            let screen = self.screens.iter().max_by(|a, b| a.intersection(source).total_cmp(&b.intersection(source)))?;
            if screen.intersection(source) <= 0.0 { return None; }
            window.clipped_to(screen)
        };
        (visible.w >= 1.0 && visible.h >= 1.0).then_some(visible)
    }
}

/// What the fit of a field depends on besides its text and style: the size of its text box on the
/// screen and the scale of the frame. A moved window keeps both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FitKey {
    scale: (u32, u32),
    size: (i64, i64),
}

impl FitKey {
    fn new(map: &FrameToDesktop, text: &DesktopRect) -> Self {
        Self { scale: (map.scale().key(), map.scale_y().key()), size: (quantize(text.w), quantize(text.h)) }
    }
}

/// Eighths of a pixel: sums of fractions differ in the last bits from one publication to the next.
fn quantize(v: f32) -> i64 {
    (v * 8.0).round() as i64
}

/// More room for a field, found by [`expand`]: how far each side of its plate moved out (left, top,
/// right, bottom, logical px) and the fit in the larger box.
#[derive(Debug, Clone)]
struct Expansion {
    grow: [f32; 4],
    fit: FitResult,
}

struct CachedFit {
    revision: u64,
    settings: u64,
    key: FitKey,
    fit: FitResult,
    /// The last expansion worked out for the field, keyed by the free space around it (relative to the
    /// field, so a moved window finds it again). `Some((_, None))`: there was no room to grow.
    expansion: Option<(u64, Option<Expansion>)>,
}

/// Кэш подгонки; живёт столько же, сколько область.
#[derive(Default)]
pub struct PlacementCache {
    entries: HashMap<(String, u64), CachedFit>,
    /// Счётчики для тестов и журнала: сколько полей подогнано заново и сколько взято из кэша.
    pub fits: u64,
    pub hits: u64,
}

impl PlacementCache {
    fn retain_live(&mut self, live: &HashSet<(String, u64)>) {
        self.entries.retain(|k, _| live.contains(k));
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Отпечаток настроек, влияющих на подгонку и вид поля.
pub fn settings_fingerprint(s: &InplaceSettings) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(s).unwrap_or_default().hash(&mut h);
    h.finish()
}

fn effect_px(s: &InplaceSettings) -> f32 {
    s.outline_width.max(1.0) + if s.shadow { 2.0 } else { 0.0 }
}

/// `plate` less the padding `inner` (left, top, right, bottom); at least a pixel, never outside the plate.
fn inset(plate: &DesktopRect, inner: [f32; 4]) -> DesktopRect {
    let w = (plate.w - inner[0] - inner[2]).max(1.0).min(plate.w.max(1.0));
    let h = (plate.h - inner[1] - inner[3]).max(1.0).min(plate.h.max(1.0));
    let x = (plate.x + inner[0]).min(plate.right() - w).max(plate.x);
    let y = (plate.y + inner[1]).min(plate.bottom() - h).max(plate.y);
    DesktopRect::in_space(x, y, w, h)
}

/// The padding of `text` inside `plate`: left, top, right, bottom; never negative.
fn inner_of(plate: &DesktopRect, text: &DesktopRect) -> [f32; 4] {
    [text.x - plate.x, text.y - plate.y, plate.right() - text.right(), plate.bottom() - text.bottom()].map(|v| v.max(0.0))
}

fn grow(r: &DesktopRect, by: [f32; 4]) -> DesktopRect {
    DesktopRect::in_space(r.x - by[0], r.y - by[1], r.w + by[0] + by[2], r.h + by[1] + by[3])
}

/// The plate and the text box of a field on the desktop, inside `bounds`.
struct Boxes {
    plate: DesktopRect,
    text: DesktopRect,
    /// The plate was cut by the bounds (the original is at the edge of the visible window).
    clipped: bool,
}

/// Where a field is drawn before any adaptation: the plate restored around the original (without a fill,
/// the original and its outline) and the text box where the original was, or at the manual padding.
/// Both are kept to `bounds`: a field at the edge of the screen is fitted to the part that can be seen,
/// instead of being fitted to the whole box and cut afterwards.
fn base_boxes(b: &InplaceBlock, map: &FrameToDesktop, bounds: &DesktopRect, frame: (u32, u32), settings: &InplaceSettings) -> Boxes {
    let st = &b.style;
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let transparent = b.background.mode == BackgroundRenderMode::Transparent;
    // Заливка закрывает поле с полями; в прозрачном режиме — только сам текст и его обводка.
    let area = map.rect(&if transparent { b.text_rect.expand(map.scale().frame_px(effect_px(settings)), fw, fh) } else { b.background.rect });
    let plate = area.clipped_to(bounds);
    // Отступы текста внутри заливки, px экрана: ручные — как заданы, иначе — где был оригинал.
    let inner = if st.padding_manual {
        [st.padding.left, st.padding.top, st.padding.right, st.padding.bottom]
    } else {
        inner_of(&plate, &map.rect(&b.text_rect).clipped_to(&plate))
    };
    Boxes { plate, text: inset(&plate, inner), clipped: plate != area }
}

/// Fits the translation into a text box of the desktop: wrap, line spacing, tracking, size, condensed face.
/// Lengths measured in the frame reach the screen through the scale of their own axis, so a frame that is
/// not shaped like the window (a resolution change in progress) gives text of the right height. A pixel is
/// kept in reserve on each axis: QML places the window on whole pixels, which may take up to one from the box.
fn fit_text(b: &InplaceBlock, text: &DesktopRect, map: &FrameToDesktop, measure: &dyn TextMeasure) -> FitResult {
    let st = &b.style;
    let (sx, sy) = (map.scale(), map.scale_y());
    let font = FontSpec { family: st.font_family.clone(), weight: st.font_weight, italic: st.italic };
    fit_translation_to_box(&FitInput {
        text: &b.translation, width: (text.w - 1.0).max(1.0), height: (text.h - 1.0).max(1.0), font,
        manual_px: st.font_size, cap_height_px: sy.px(st.cap_height_px),
        min_px: st.min_font_size, max_px: st.max_font_size, source_lines: st.source_lines,
        manual_line_height: st.line_height, source_line_px: st.line_height_px.map(|v| sy.px(v)),
        letter_spacing: if st.letter_spacing_manual { st.letter_spacing } else { sx.px(st.letter_spacing) },
        letter_spacing_manual: st.letter_spacing_manual, manual_wrap: st.wrap_mode, script: b.script,
        condensed_family: if st.allow_condensed && st.font_size.is_none() { b.font.condensed_family.clone() } else { None },
    }, measure)
}

/// Space kept between a grown plate and the original text of another field.
const EXPAND_GAP: f32 = 2.0;

/// How far each side of `r` (left, top, right, bottom) may move out before leaving `safe` or reaching the
/// original text of another field. A field beside `r` limits the side it faces, one above or below the
/// top or the bottom; a field diagonal to `r` limits nothing until `r` grows into its row or column.
fn room(r: &DesktopRect, safe: &DesktopRect, others: &[DesktopRect]) -> [f32; 4] {
    let mut room = [r.x - safe.x, r.y - safe.y, safe.right() - r.right(), safe.bottom() - r.bottom()];
    for o in others {
        let column = o.x < r.right() && o.right() > r.x;
        let row = o.y < r.bottom() && o.bottom() > r.y;
        if column && o.y >= r.bottom() { room[3] = room[3].min(o.y - r.bottom() - EXPAND_GAP); }
        if column && o.bottom() <= r.y { room[1] = room[1].min(r.y - o.bottom() - EXPAND_GAP); }
        if row && o.x >= r.right() { room[2] = room[2].min(o.x - r.right() - EXPAND_GAP); }
        if row && o.right() <= r.x { room[0] = room[0].min(r.x - o.right() - EXPAND_GAP); }
    }
    room.map(|v| v.max(0.0))
}

/// `extra` shared between two opposite sides with room `a` and `b`: side `a` takes `share` of it first,
/// and what one side has no room for goes to the other.
fn split(extra: f32, a: f32, b: f32, share: f32) -> (f32, f32) {
    let first = (extra * share).min(a);
    let second = (extra - first).min(b);
    ((extra - second).min(a), second)
}

/// The smallest `k` in `1..=max` for which `fits(k)`, if `fits(max)`; `fits` grows with `k`.
fn smallest(max: u32, fits: impl Fn(u32) -> bool) -> Option<u32> {
    if max == 0 || !fits(max) { return None; }
    let (mut lo, mut hi) = (1, max);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(mid) { hi = mid } else { lo = mid + 1 }
    }
    Some(lo)
}

/// More room for a translation that does not fit its box even at the minimum size. The plate grows into the
/// free part of `safe` by steps of one line: first taller (down, then up), keeping the column of the
/// original; if no height is enough, also wider (away from the side the text is aligned to, both ways for
/// centred text). The smallest box in which the whole translation fits wins, so the text stays as close to
/// its original as it can. If even all the room is not enough, the largest box is returned and its fit
/// still overflows: the text is cut with an ellipsis there. `None`: there is no free space at all.
fn expand(b: &InplaceBlock, plate: &DesktopRect, text: &DesktopRect, safe: &DesktopRect, others: &[DesktopRect],
          map: &FrameToDesktop, measure: &dyn TextMeasure) -> Option<Expansion> {
    let st = &b.style;
    let inner = inner_of(plate, text);
    let fit_at = |by: [f32; 4]| fit_text(b, &inset(&grow(plate, by), inner), map, measure);
    let fits = |by: [f32; 4]| !fit_at(by).overflow;
    // One line of the smallest allowed size: a finer step would not show another line.
    let font = FontSpec { family: st.font_family.clone(), weight: st.font_weight, italic: st.italic };
    let step = measure.measure("Hg", &font, st.min_font_size, 0.0, f32::MAX, WrapMode::NoWrap).line_spacing.ceil().max(4.0);
    // The plate widened by `side` (left, right), then `k` steps taller; step `max` takes all the room there is.
    let vertical = |side: (f32, f32)| room(&grow(plate, [side.0, 0.0, side.1, 0.0]), safe, others);
    let steps = |side: (f32, f32)| { let r = vertical(side); ((r[1] + r[3]) / step).ceil() as u32 };
    let tall = |side: (f32, f32), k: u32, max: u32| {
        let r = vertical(side);
        let extra = if k >= max { r[1] + r[3] } else { k as f32 * step };
        let (up, down) = split(extra, r[1], r[3], 0.0);
        [side.0, up, side.1, down]
    };
    let done = |by: [f32; 4]| Some(Expansion { grow: by, fit: fit_at(by) });

    // 1. Taller, the same width.
    let max = steps((0.0, 0.0));
    if let Some(k) = smallest(max, |k| fits(tall((0.0, 0.0), k, max))) {
        return done(tall((0.0, 0.0), k, max));
    }
    // 2. Wider by steps, each with all the height there is; then only as tall as the text needs.
    let r = room(plate, safe, others);
    let share = match st.alignment { TextAlignment::Left => 0.0, TextAlignment::Center => 0.5, TextAlignment::Right => 1.0 };
    let max_w = ((r[0] + r[2]) / step).ceil() as u32;
    let side = |k: u32| if k >= max_w { (r[0], r[2]) } else { split(k as f32 * step, r[0], r[2], share) };
    if let Some(kw) = smallest(max_w, |kw| { let s = side(kw); let m = steps(s); fits(tall(s, m, m)) }) {
        let s = side(kw);
        let m = steps(s);
        return done(tall(s, smallest(m, |k| fits(tall(s, k, m))).unwrap_or(m), m));
    }
    // 3. Nothing is enough: all the room, the translation cut at its end.
    let s = (r[0], r[2]);
    let m = steps(s);
    let by = tall(s, m, m);
    if by.iter().all(|v| *v < 0.5) { return None; }
    done(by)
}

/// Identifies the free space around a field relative to the field itself, so an expansion found once is
/// found again after the window moved, and worked out anew when a neighbour or an edge came closer.
fn expansion_key(plate: &DesktopRect, text: &DesktopRect, safe: &DesktopRect, others: &[DesktopRect]) -> u64 {
    let rel = |r: &DesktopRect| [quantize(r.x - plate.x), quantize(r.y - plate.y), quantize(r.w), quantize(r.h)];
    let near = grow(safe, [EXPAND_GAP; 4]);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (rel(text), rel(safe)).hash(&mut h);
    for o in others.iter().filter(|o| o.intersection(&near) > 0.0) { rel(o).hash(&mut h); }
    h.finish()
}

fn alignment_name(a: TextAlignment) -> &'static str {
    match a { TextAlignment::Left => "left", TextAlignment::Center => "center", TextAlignment::Right => "right" }
}

fn wrap_name(w: WrapMode) -> &'static str {
    match w { WrapMode::WordWrap => "word", WrapMode::WrapAnywhere => "anywhere", WrapMode::NoWrap => "none", WrapMode::Elide => "elide" }
}

/// Обводка по контрасту с цветом фона вокруг поля (а не просто чёрный/белый): тёмная или светлая
/// версия самого фона, выбранная по большему контрасту с цветом текста.
fn outline_for(text: [u8; 3], background: [u8; 3]) -> [u8; 3] {
    let darker = background.map(|v| v.saturating_sub(96));
    let lighter = background.map(|v| v.saturating_add(96));
    if contrast_ratio(text, darker) >= contrast_ratio(text, lighter) { darker } else { lighter }
}

#[allow(clippy::too_many_arguments)]
fn placed_entry(region: &RegionInput, b: &InplaceBlock, fit: &FitResult, box_rect: [f64; 4], inner: [f32; 4],
                settings: &InplaceSettings, image: Option<&str>) -> Placed {
    let st = &b.style;
    let transparent = b.background.mode == BackgroundRenderMode::Transparent;
    let bg = b.background.color;
    let outline = settings.outline_color.manual().and_then(|v| parse_hex_color(v)).unwrap_or_else(|| outline_for(st.text_color, bg));
    let fill = settings.fill_color.manual().and_then(|v| parse_hex_color(v)).unwrap_or(bg);
    Placed {
        key: format!("{}:{}", region.id, b.id),
        region_id: region.id.to_owned(),
        block_id: b.id,
        block_type: b.block_type,
        rect: region.rect,
        box_rect,
        inner,
        text: b.translation.clone(),
        original: b.original.clone(),
        font_family: fit.family.clone(),
        font_px: fit.font_px.floor().max(1.0),
        font_weight: st.font_weight.value(),
        italic: st.italic,
        line_height: fit.line_height,
        letter_spacing: fit.letter_spacing,
        alignment: alignment_name(st.alignment),
        wrap: wrap_name(fit.wrap_mode),
        max_lines: fit.max_lines,
        text_color: hex(st.text_color),
        outline: transparent,
        outline_color: hex(outline),
        outline_width: settings.outline_width,
        shadow: settings.shadow,
        text_opacity: settings.text_opacity,
        background: PlacedBackground {
            mode: b.background.mode,
            color: hex(fill),
            opacity: settings.fill_opacity,
            radius: settings.corner_radius,
            image: image.unwrap_or("").to_owned(),
        },
        font_selection: PlacedFont { category: b.font.category, generic: b.font.generic, confidence: b.font.confidence },
        expanded: false,
        degraded: None,
    }
}

/// A plate hugging the translation, with no restored picture and no outline halo: smaller than the
/// original plate, so it fits where that one overlapped a neighbour.
fn plain_plate(c: &Candidate) -> Candidate {
    let pad = (c.source_rect.h * 0.15).clamp(2.0, 6.0);
    let t = c.text_rect;
    Candidate {
        background_rect: DesktopRect::in_space(t.x - pad, t.y - pad, t.w + 2.0 * pad, t.h + 2.0 * pad),
        effect_margin: 0.0,
        image_background: false,
        allow_text_shift: true,
        ..c.clone()
    }
}

/// Which side of the original a compact label is tried on.
#[derive(Clone, Copy)]
enum Beside {
    Below,
    Above,
}

/// A compact label next to the text it translates, in frame pixels: as wide as the original (at
/// least ten line heights), two lines tall. Its fit, and with it the font size, is made for this box.
fn chip_area(b: &InplaceBlock, side: Beside) -> super::Rect {
    let line = (b.text_rect.h / b.style.source_lines.max(1) as f32).max(8.0);
    let (w, h, gap) = (b.text_rect.w.max(10.0 * line), (2.2 * line).max(b.text_rect.h), 0.25 * line);
    match side {
        Beside::Below => super::Rect::new(b.text_rect.x, b.text_rect.bottom() + gap, w, h),
        Beside::Above => super::Rect::new(b.text_rect.x, b.text_rect.y - gap - h, w, h),
    }
}

/// `r` moved sideways into `bounds` (narrowed to them if wider): a label for an original at the right
/// edge of the window opens to the left instead of hanging over the edge.
fn slide_into(r: DesktopRect, bounds: &DesktopRect) -> DesktopRect {
    let w = r.w.min(bounds.w);
    DesktopRect::in_space(r.x.min(bounds.right() - w).max(bounds.x), r.y, w, r.h)
}

/// Logical padding of the label's plate.
const CHIP_PAD: f32 = 4.0;
/// Logical padding of the plate that covers exactly the original text.
const INSIDE_PAD: f32 = 2.0;

/// Keeps what is already placed: a variant is taken only if the field it is meant for fits now and
/// every field that fitted before still does.
fn accepted(trial: &[Candidate], before: &[Option<collision::Placement>], index: usize) -> Option<Vec<Option<collision::Placement>>> {
    let after = collision::resolve_quiet(trial);
    (after[index].is_some() && before.iter().zip(&after).all(|(old, new)| old.is_none() || new.is_some())).then_some(after)
}

/// Размещает все поля всех областей в видимой части окна игры `view` (логические пиксели рабочего
/// стола). `images` — готовые URL подложек по (область, поле).
///
/// Перевод, который не помещается в поле оригинала даже минимальным кеглем, сначала получает больше
/// места рядом с оригиналом (см. [`expand`]). Поле, которое не помещается без перекрытия, деградирует
/// по ступеням: простая плашка вместо восстановленного фона, компактная плашка рядом с оригиналом,
/// наконец плашка в границах самого оригинала, где перевод обрезается с многоточием. Если не помещается
/// и она (дубль OCR на том же тексте), поле не показывается (`dropped`): в окно перевода «поверх
/// оригинала» ничего не уходит. Уже размещённые поля при этом не сдвигаются.
pub fn place_regions(regions: &[RegionInput], view: &Viewport, settings: &InplaceSettings,
                     images: &HashMap<(String, u64), String>, measure: &dyn TextMeasure, cache: &mut PlacementCache) -> PlacementOutcome {
    struct Owner<'a> {
        region: &'a RegionInput<'a>,
        block: &'a InplaceBlock,
        key: (String, u64),
        map: FrameToDesktop,
        fit: FitResult,
        image: Option<&'a str>,
        /// The restored picture was made for a plate the edge of the screen cut: shown as a plain plate.
        plain: bool,
        expanded: bool,
    }
    let fingerprint = settings_fingerprint(settings);
    let mut live = HashSet::new();
    let mut owners: Vec<Owner> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut dropped: Vec<DroppedText> = Vec::new();
    for region in regions {
        let frame = region.frame.frame;
        // Where the region frame lies on the desktop: the only conversion between frame and desktop pixels.
        let map = FrameToDesktop::new(&view.window, region.rect, frame);
        for u in &region.frame.undrawable {
            tracing::warn!(target: "inplace.fallback", block = u.id, reason = u.reason, "field cannot be drawn: left out (the translation window is not used in the in-place mode)");
            dropped.push(DroppedText { region_id: region.id.to_owned(), block_id: u.id, text: u.translation.clone(), original: u.original.clone(), reason: u.reason });
        }
        for b in &region.frame.blocks {
            let key = (region.id.to_owned(), b.id);
            live.insert(key.clone());
            let source = map.rect(&b.text_rect);
            let placed = view.bounds_for(&source).map(|bounds| (bounds, base_boxes(b, &map, &bounds, frame, settings)));
            let Some((bounds, boxes)) = placed.filter(|(_, boxes)| boxes.plate.w >= 1.0 && boxes.plate.h >= 1.0) else {
                tracing::debug!(target: "inplace.fallback", block = b.id, source = ?source, "the original is off the screen: the field is not shown");
                dropped.push(DroppedText { region_id: region.id.to_owned(), block_id: b.id, text: b.translation.clone(),
                    original: b.original.clone(), reason: "оригинал за пределами экрана" });
                continue;
            };
            let fit_key = FitKey::new(&map, &boxes.text);
            let fit = match cache.entries.get(&key) {
                Some(c) if c.revision == b.revision && c.key == fit_key && c.settings == fingerprint => {
                    cache.hits += 1;
                    c.fit.clone()
                }
                _ => {
                    cache.fits += 1;
                    let fit = fit_text(b, &boxes.text, &map, measure);
                    cache.entries.insert(key.clone(), CachedFit { revision: b.revision, settings: fingerprint, key: fit_key, fit: fit.clone(), expansion: None });
                    fit
                }
            };
            let image = images.get(&key).map(String::as_str).filter(|u| !u.is_empty());
            let plain = boxes.clipped && b.background.image.is_some();
            candidates.push(Candidate {
                id: format!("{}:{}", region.id, b.id),
                source_rect: source,
                text_rect: boxes.text,
                background_rect: boxes.plate,
                bounds,
                effect_margin: if b.background.mode == BackgroundRenderMode::Transparent { effect_px(settings) } else { 0.0 },
                image_background: b.background.image.is_some() && !plain,
                allow_text_shift: b.background.mode != BackgroundRenderMode::Transparent,
            });
            owners.push(Owner { region, block: b, key, map, fit, image, plain, expanded: false });
        }
    }
    cache.retain_live(&live);
    // A translation that does not fit its box even at the minimum size gets more room next to the original.
    // Only the originals of the other fields limit it here (their final boxes are not known yet); the
    // collision check below still has the last word.
    for i in 0..candidates.len() {
        if !owners[i].fit.overflow { continue; }
        let safe = collision::safe_rect(&candidates, i);
        let others: Vec<DesktopRect> = candidates.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| c.source_rect).collect();
        let (plate, text) = (candidates[i].background_rect, candidates[i].text_rect);
        let ekey = expansion_key(&plate, &text, &safe, &others);
        let owner = &owners[i];
        let cached = cache.entries.get(&owner.key).and_then(|c| c.expansion.as_ref()).filter(|(k, _)| *k == ekey).map(|(_, e)| e.clone());
        let expansion = match cached {
            Some(e) => e,
            None => {
                let e = expand(owner.block, &plate, &text, &safe, &others, &owner.map, measure);
                if let Some(c) = cache.entries.get_mut(&owner.key) { c.expansion = Some((ekey, e.clone())); }
                e
            }
        };
        let Some(e) = expansion else { continue };
        let grown = grow(&plate, e.grow);
        candidates[i].text_rect = inset(&grown, inner_of(&plate, &text));
        candidates[i].background_rect = grown;
        candidates[i].image_background = false;
        owners[i].fit = e.fit;
        owners[i].expanded = true;
        tracing::debug!(target: "inplace.fit", block = %candidates[i].id, grow = ?e.grow, complete = !owners[i].fit.overflow, "translation given more room next to the original");
    }
    let mut results = collision::resolve(&candidates);
    // How each field ended up, and the fit of a label if it became one.
    let mut tiers: Vec<Option<&'static str>> = vec![None; owners.len()];
    let mut chip_fit: Vec<Option<FitResult>> = vec![None; owners.len()];
    for i in 0..owners.len() {
        if results[i].is_some() { continue; }
        // 1. The same place with a plain plate.
        let mut trial = candidates.clone();
        trial[i] = plain_plate(&candidates[i]);
        if let Some(after) = accepted(&trial, &results, i) {
            candidates = trial;
            results = after;
            tiers[i] = Some("plain-plate");
            continue;
        }
        // 2. A compact label next to the original, below it first.
        let owner = &owners[i];
        let bounds = candidates[i].bounds;
        for side in [Beside::Below, Beside::Above] {
            let plate = slide_into(owner.map.rect(&chip_area(owner.block, side)), &bounds);
            let text = inset(&plate, [CHIP_PAD; 4]);
            let mut trial = candidates.clone();
            trial[i] = Candidate { text_rect: text, background_rect: plate, effect_margin: 0.0, image_background: false, allow_text_shift: false, ..candidates[i].clone() };
            if let Some(after) = accepted(&trial, &results, i) {
                chip_fit[i] = Some(fit_text(owner.block, &text, &owner.map, measure));
                candidates = trial;
                results = after;
                tiers[i] = Some("beside-original");
                break;
            }
        }
        if tiers[i].is_some() { continue; }
        // 3. The box of the original itself: the translation is cut to it (elided), whatever its length.
        let plate = candidates[i].source_rect.clipped_to(&bounds);
        let text = inset(&plate, [INSIDE_PAD; 4]);
        let mut trial = candidates.clone();
        trial[i] = Candidate { text_rect: text, background_rect: plate, effect_margin: 0.0, image_background: false, allow_text_shift: false, ..candidates[i].clone() };
        if let Some(after) = accepted(&trial, &results, i) {
            chip_fit[i] = Some(fit_text(owner.block, &text, &owner.map, measure));
            candidates = trial;
            results = after;
            tiers[i] = Some("inside-original");
        }
    }
    let mut placed = Vec::with_capacity(owners.len());
    for (i, owner) in owners.iter().enumerate() {
        let Some(place) = results[i].as_ref() else {
            // 4. Not even the box of the original is free (the same text read twice): the field is left out.
            tracing::warn!(target: "inplace.fallback", block = %candidates[i].id, "no room over the game: the field is not shown");
            dropped.push(DroppedText { region_id: owner.region.id.to_owned(), block_id: owner.block.id, text: owner.block.translation.clone(),
                original: owner.block.original.clone(), reason: "для перевода нет места поверх игры" });
            continue;
        };
        // The window QML opens is the plate; a text moved a little to stay clear of a neighbour is still inside it.
        let plate = place.background_rect.union(&place.text_rect);
        let fit = chip_fit[i].as_ref().unwrap_or(&owner.fit);
        let mut entry = placed_entry(owner.region, owner.block, fit, owner.map.fraction(&plate), inner_of(&plate, &place.text_rect), settings, owner.image);
        if let Some(tier) = tiers[i].or(owner.plain.then_some("plain-plate")) {
            tracing::info!(target: "inplace.fallback", block = %candidates[i].id, tier, "field shown in a simpler form");
            // A plain plate carries the text itself: no restored picture, no outline halo.
            entry.degraded = Some(tier);
            entry.background.mode = BackgroundRenderMode::SolidFill;
            entry.background.image = String::new();
            entry.outline = false;
        } else if owner.expanded {
            entry.expanded = true;
            // The restored picture covers the original box only; stretched over the larger plate it would not
            // match the game. The fill of its colour does.
            if entry.background.mode == BackgroundRenderMode::InpaintBlur {
                entry.background.mode = BackgroundRenderMode::SolidFill;
                entry.background.image = String::new();
            }
        }
        if entry.degraded.is_none() && fit.overflow {
            tracing::info!(target: "inplace.fallback", block = %candidates[i].id, "no free space is large enough: the translation is cut with an ellipsis");
            entry.degraded = Some("elided");
        }
        placed.push(entry);
    }
    PlacementOutcome { placed, dropped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::engine::InplaceEngine;
    use crate::layout::fit::{ApproxMeasure, Measured};
    use crate::layout::font_database::InstalledFontDatabase;
    use crate::layout::testing::*;
    use crate::layout::Rect;
    use crate::settings::Settings;
    use std::cell::Cell;
    use std::time::Instant;

    /// Считает обращения к метрикам: кэш подгонки должен их экономить.
    struct Counting(Cell<u64>);
    impl TextMeasure for Counting {
        fn measure(&self, t: &str, f: &FontSpec, px: f32, ls: f32, w: f32, wrap: WrapMode) -> Measured {
            self.0.set(self.0.get() + 1);
            ApproxMeasure.measure(t, f, px, ls, w, wrap)
        }
        fn cap_ratio(&self, f: &FontSpec) -> f32 { ApproxMeasure.cap_ratio(f) }
    }

    fn frame_with_blocks() -> InplaceFrame {
        let mut img = canvas(1200, 400, [25, 30, 40]);
        draw_line(&mut img, 100, 100, 8, 14, 4, 22, 3, [255, 200, 60], false);
        draw_line(&mut img, 100, 140, 40, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 100, 172, 30, 14, 4, 22, 3, [240, 240, 240], false);
        let s = { let mut value = Settings::default(); value.translation.target_language = "ru".into(); value.display_mode = crate::settings::TranslationDisplayMode::Inplace; value };
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(dynamic(img), &s, Instant::now(), false);
        for j in jobs { e.complete(j.id, Some(("src".into(), "Это перевод, который помещается".into())), &s); }
        e.finish(&s).expect("fields")
    }

    fn region(frame: &InplaceFrame) -> RegionInput<'_> {
        RegionInput { id: "subtitles", rect: NormRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 }, frame }
    }

    // ── Degradation: a field that does not fit is shown simpler instead of being lost ──

    /// A frame of three independent fields at known places, one frame pixel = one desktop pixel.
    fn custom_frame(fields: &[(Rect, Rect)]) -> InplaceFrame {
        sized_frame((1200, 400), fields)
    }

    fn sized_frame(size: (u32, u32), fields: &[(Rect, Rect)]) -> InplaceFrame {
        let template = frame_with_blocks().blocks.remove(0);
        let blocks = fields.iter().enumerate().map(|(i, (text, plate))| {
            let mut b = template.clone();
            b.id = i as u64 + 1;
            b.text_rect = *text;
            b.background.rect = *plate;
            b.background.image = None;
            b.original = format!("original {i}");
            b.translation = format!("перевод {i}");
            b.revision = 1;
            b
        }).collect();
        InplaceFrame { frame: size, blocks, undrawable: Vec::new(), new_translations: Vec::new() }
    }

    fn outcome(frame: &InplaceFrame) -> PlacementOutcome {
        place(frame, &Viewport::window(WindowGeometry::from(WINDOW)))
    }

    /// The whole frame is the region, shown in `view`.
    fn place(frame: &InplaceFrame, view: &Viewport) -> PlacementOutcome {
        let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame };
        place_regions(&[region], view, &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default())
    }

    /// The window of `custom_frame`: one frame pixel is one desktop pixel.
    const WINDOW: [f64; 4] = [0.0, 0.0, 1200.0, 400.0];

    #[test]
    fn fields_that_fit_are_not_degraded() {
        let frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                   (Rect::new(500.0, 100.0, 200.0, 20.0), Rect::new(492.0, 92.0, 216.0, 36.0))]);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 2);
        assert!(out.dropped.is_empty() && out.placed.iter().all(|p| p.degraded.is_none()));
    }

    #[test]
    fn a_restored_background_that_cannot_be_clipped_becomes_a_plain_plate() {
        // The first field's picture plate reaches over its neighbour; a picture cannot be cut, a plain plate can be smaller.
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(80.0, 80.0, 400.0, 60.0)),
                                       (Rect::new(320.0, 100.0, 200.0, 20.0), Rect::new(312.0, 92.0, 216.0, 36.0))]);
        frame.blocks[0].background.image = Some(std::sync::Arc::new(image::RgbaImage::new(2, 2)));
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 2, "both fields are still shown");
        let first = &out.placed[0];
        assert_eq!(first.degraded, Some("plain-plate"));
        assert_eq!(first.background.mode, BackgroundRenderMode::SolidFill);
        assert!(first.background.image.is_empty() && !first.outline);
        assert_eq!(out.placed[1].degraded, None, "the neighbour is untouched");
        assert!(out.dropped.is_empty());
        let json = serde_json::to_value(first).unwrap();
        assert_eq!(json["degraded"], "plain-plate");
        assert!(serde_json::to_value(&out.placed[1]).unwrap().get("degraded").is_none(), "absent when not degraded");
    }

    #[test]
    fn a_field_whose_text_area_covers_a_neighbour_gets_a_label_beside_the_original() {
        // Zero padding and a wide plate make the translation's own area cover the neighbour's text: a plain
        // plate around it would still do. A compact label under the original fits.
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(100.0, 100.0, 500.0, 20.0)),
                                       (Rect::new(420.0, 100.0, 100.0, 20.0), Rect::new(412.0, 92.0, 116.0, 36.0))]);
        frame.blocks[0].style.padding_manual = true;
        frame.blocks[0].style.padding = crate::layout::Padding::uniform(0.0);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 2);
        let label = &out.placed[0];
        assert_eq!(label.degraded, Some("beside-original"));
        // Below the original (frame y 120..), inside the window, and clear of the neighbour on the right.
        let [x, y, w, h] = label.box_rect;
        let (px, py, pw, ph) = (x * 1200.0, y * 400.0, w * 1200.0, h * 400.0);
        assert!(py >= 120.0 && py + ph <= 400.0, "below the original: y {py} h {ph}");
        assert!(px + pw <= 420.0 + 0.5, "the label stays left of the neighbour's text: x {px} w {pw}");
        assert!(label.font_px >= 1.0 && !label.text.is_empty());
        assert_eq!(out.placed[1].degraded, None);
    }

    #[test]
    fn a_field_that_cannot_be_placed_anywhere_is_left_out_not_sent_to_a_window() {
        // Two fields on the same text (an OCR duplicate): the second cannot be drawn without covering the first.
        let frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                   (Rect::new(110.0, 104.0, 200.0, 20.0), Rect::new(102.0, 96.0, 216.0, 36.0))]);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 1, "the first stays");
        assert_eq!(out.placed[0].block_id, 1);
        assert_eq!(out.dropped.len(), 1, "the duplicate is counted, not shown anywhere");
        assert_eq!(out.dropped[0].block_id, 2);
        assert!(out.dropped[0].reason.contains("нет места"));
    }

    #[test]
    fn a_translation_with_no_room_for_a_label_is_cut_to_the_original_box() {
        // As above, but the places under and over the original are taken too: the only free place is the original's own box.
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(100.0, 100.0, 500.0, 20.0)),
                                       (Rect::new(420.0, 100.0, 100.0, 20.0), Rect::new(412.0, 92.0, 116.0, 36.0)),
                                       (Rect::new(100.0, 124.0, 150.0, 20.0), Rect::new(96.0, 124.0, 158.0, 24.0)),
                                       (Rect::new(100.0, 70.0, 150.0, 20.0), Rect::new(96.0, 66.0, 158.0, 28.0))]);
        frame.blocks[0].style.padding_manual = true;
        frame.blocks[0].style.padding = crate::layout::Padding::uniform(0.0);
        let out = outcome(&frame);
        assert_eq!((out.placed.len(), out.dropped.len()), (4, 0), "everything is drawn over the game, nothing goes to a window");
        let cut = &out.placed[0];
        assert_eq!(cut.degraded, Some("inside-original"));
        let [x, y, w, h] = cut.box_rect;
        let (px, py, pw, ph) = (x * 1200.0, y * 400.0, w * 1200.0, h * 400.0);
        assert!((px - 100.0).abs() < 0.5 && (py - 100.0).abs() < 0.5 && (pw - 200.0).abs() < 0.5 && (ph - 20.0).abs() < 0.5, "exactly the box of the original text: {px} {py} {pw} {ph}");
    }

    #[test]
    fn degrading_one_field_never_moves_the_ones_already_placed() {
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                       (Rect::new(500.0, 100.0, 200.0, 20.0), Rect::new(492.0, 92.0, 216.0, 36.0)),
                                       (Rect::new(320.0, 100.0, 150.0, 20.0), Rect::new(250.0, 80.0, 400.0, 60.0))]);
        frame.blocks[2].background.image = Some(std::sync::Arc::new(image::RgbaImage::new(2, 2)));
        let alone = outcome(&custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                            (Rect::new(500.0, 100.0, 200.0, 20.0), Rect::new(492.0, 92.0, 216.0, 36.0))]));
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 3);
        assert_eq!(&out.placed[..2], &alone.placed[..], "the placed fields are exactly as they were without the troublemaker");
    }

    #[test]
    fn undrawable_fields_are_counted_not_handed_to_a_window() {
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0))]);
        frame.undrawable.push(crate::layout::engine::UndrawableField { id: 9, original: "src".into(), translation: "текст".into(), reason: "нет встроенного шрифта с нужными глифами" });
        let out = outcome(&frame);
        assert_eq!((out.placed.len(), out.dropped.len()), (1, 1));
        assert_eq!((out.dropped[0].block_id, out.dropped[0].text.as_str()), (9, "текст"));
    }

    #[test]
    fn places_every_field_with_a_typed_complete_record() {
        let frame = frame_with_blocks();
        let mut cache = PlacementCache::default();
        let out = place_regions(&[region(&frame)], &Viewport::window(WindowGeometry::from([0.0, 0.0, 1200.0, 800.0])), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache).placed;
        assert_eq!(out.len(), frame.blocks.len());
        let p = &out[0];
        assert!(p.key.starts_with("subtitles:") && !p.font_family.is_empty() && p.font_px >= 1.0);
        assert!(p.text_color.starts_with('#') && p.outline_color.starts_with('#'));
        // The QML contract: these keys exist in the serialized form.
        let json = serde_json::to_value(p).unwrap();
        for key in ["key", "box", "inner", "text", "font_family", "font_px", "font_weight", "wrap", "alignment", "background", "font_selection"] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert!(json["background"]["mode"].as_str().is_some());
    }

    #[test]
    fn fit_is_cached_by_revision_scale_and_settings() {
        let frame = frame_with_blocks();
        let count = Counting(Cell::new(0));
        let mut cache = PlacementCache::default();
        let settings = InplaceSettings::default();
        let run = |window: [f64; 4], settings: &InplaceSettings, cache: &mut PlacementCache| place_regions(&[region(&frame)], &Viewport::window(WindowGeometry::from(window)), settings, &HashMap::new(), &count, cache).placed;
        let first = run([0.0, 0.0, 1200.0, 800.0], &settings, &mut cache);
        let (fits, calls) = (cache.fits, count.0.get());
        assert_eq!(fits as usize, frame.blocks.len());
        // Same inputs: nothing is measured again.
        assert_eq!(run([0.0, 0.0, 1200.0, 800.0], &settings, &mut cache), first);
        assert_eq!((cache.fits, count.0.get()), (fits, calls));
        // The game window moved (same size): still no refit, same relative record (geometry is
        // recomputed through desktop coordinates, so compare it with a tolerance).
        let moved = run([300.0, 200.0, 1200.0, 800.0], &settings, &mut cache);
        assert_eq!(moved.len(), first.len());
        for (a, b) in moved.iter().zip(&first) {
            assert_eq!((&a.key, &a.text, a.font_px, &a.font_family, &a.wrap), (&b.key, &b.text, b.font_px, &b.font_family, &b.wrap));
            assert!(a.box_rect.iter().zip(&b.box_rect).all(|(x, y)| (x - y).abs() < 1e-3), "{:?} vs {:?}", a.box_rect, b.box_rect);
            assert!(a.inner.iter().zip(&b.inner).all(|(x, y)| (x - y).abs() < 1e-3));
        }
        assert_eq!((cache.fits, count.0.get()), (fits, calls), "moving the window does not refit");
        // A different window size changes the scale: refit.
        run([0.0, 0.0, 900.0, 600.0], &settings, &mut cache);
        assert!(cache.fits > fits);
        // A style setting changes the fingerprint: refit.
        let before = cache.fits;
        let mut changed = settings.clone();
        changed.minimum_font_size = 12.0;
        run([0.0, 0.0, 900.0, 600.0], &changed, &mut cache);
        assert!(cache.fits > before);
    }

    #[test]
    fn vanished_fields_leave_the_cache() {
        let frame = frame_with_blocks();
        let mut cache = PlacementCache::default();
        place_regions(&[region(&frame)], &Viewport::window(WindowGeometry::from([0.0, 0.0, 1200.0, 800.0])), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
        assert_eq!(cache.entries.len(), frame.blocks.len());
        place_regions(&[], &Viewport::window(WindowGeometry::from([0.0, 0.0, 1200.0, 800.0])), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn backdrop_url_and_manual_colours_reach_the_record() {
        let frame = frame_with_blocks();
        let id = frame.blocks[0].id;
        let mut images = HashMap::new();
        images.insert(("subtitles".to_string(), id), "file:///x.png?r=1".to_string());
        let s = InplaceSettings {
            fill_color: crate::settings::PropertyMode::Manual("#112233".into()),
            outline_color: crate::settings::PropertyMode::Manual("#445566".into()),
            ..Default::default()
        };
        let out = place_regions(&[region(&frame)], &Viewport::window(WindowGeometry::from([0.0, 0.0, 1200.0, 800.0])), &s, &images, &ApproxMeasure, &mut PlacementCache::default()).placed;
        let p = out.iter().find(|p| p.block_id == id).unwrap();
        assert_eq!((p.background.color.as_str(), p.outline_color.as_str(), p.background.image.as_str()), ("#112233", "#445566", "file:///x.png?r=1"));
    }

    // ── Room: the translation adapts to the space of the game instead of leaving it ──

    const LONG: &str = "Это очень длинный перевод короткой надписи, который не помещается в её рамку даже минимальным кеглем";

    /// Field `index` gets the translation `text` and the minimum size `min_px`.
    fn with_text(mut frame: InplaceFrame, index: usize, text: &str, min_px: f32) -> InplaceFrame {
        frame.blocks[index].translation = text.into();
        frame.blocks[index].style.min_font_size = min_px;
        frame
    }

    /// The plate and the text box of a placed field on the desktop, worked out from the record as QML does.
    fn on_desktop(p: &Placed, window: [f64; 4]) -> (DesktopRect, DesktopRect) {
        let r = p.rect;
        let (rx, ry, rw, rh) = (window[0] + r.x * window[2], window[1] + r.y * window[3], r.w * window[2], r.h * window[3]);
        let [bx, by, bw, bh] = p.box_rect;
        let plate = DesktopRect::in_space((rx + bx * rw) as f32, (ry + by * rh) as f32, (bw * rw) as f32, (bh * rh) as f32);
        let [l, t, right, bottom] = p.inner;
        (plate, DesktopRect::in_space(plate.x + l, plate.y + t, plate.w - l - right, plate.h - t - bottom))
    }

    /// The field is drawn whole inside `area`, and its text, laid out with the record's font, fits its box
    /// unless the record says it is cut.
    fn assert_inside(p: &Placed, window: [f64; 4], area: DesktopRect) {
        let (plate, text) = on_desktop(p, window);
        assert!(area.contains(&plate), "{}: plate {plate:?} leaves {area:?}", p.key);
        assert!(plate.contains(&text) && text.w >= 1.0 && text.h >= 1.0, "{}: text {text:?} leaves its plate {plate:?}", p.key);
        if !matches!(p.degraded, Some("elided" | "inside-original")) {
            let font = FontSpec { family: p.font_family.clone(), weight: crate::layout::FontWeight::from_value(p.font_weight), italic: p.italic };
            let wrap = match p.wrap { "none" => WrapMode::NoWrap, "anywhere" => WrapMode::WrapAnywhere, _ => WrapMode::WordWrap };
            let m = ApproxMeasure.measure(&p.text, &font, p.font_px, p.letter_spacing, text.w, wrap);
            let height = crate::layout::fit::text_height(&m, p.line_height);
            assert!(m.width <= text.w + 0.5 && height <= text.h + 0.5, "{}: {}×{height} px of text in a {text:?} box", p.key, m.width);
        }
    }

    #[test]
    fn a_translation_too_long_for_its_box_grows_into_the_free_space_below() {
        let frame = with_text(custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(96.0, 96.0, 208.0, 28.0))]), 0, LONG, 14.0);
        let out = outcome(&frame);
        let p = &out.placed[0];
        assert!(p.expanded && p.degraded.is_none(), "grown, nothing cut: {:?}", p.degraded);
        assert!(p.font_px >= 14.0, "never below the minimum size: {}", p.font_px);
        let (plate, _) = on_desktop(p, WINDOW);
        assert!((plate.x - 96.0).abs() < 0.5 && (plate.w - 208.0).abs() < 0.5 && (plate.y - 96.0).abs() < 0.5,
            "the column of the original, grown downward: {plate:?}");
        assert!(plate.h > 28.0 && plate.h < 200.0, "only as tall as the text needs: {plate:?}");
        assert_inside(p, WINDOW, DesktopRect::of_window(&WindowGeometry::from(WINDOW)));
        assert!(serde_json::to_value(p).unwrap()["expanded"].as_bool().unwrap());
        assert!(serde_json::to_value(&out.placed[0]).is_ok());
    }

    #[test]
    fn growing_stops_short_of_a_neighbour_and_goes_the_other_way() {
        // A field just below: the long one cannot grow down, so it grows up, and never over the neighbour's text.
        let frame = with_text(custom_frame(&[(Rect::new(100.0, 200.0, 200.0, 20.0), Rect::new(96.0, 196.0, 208.0, 28.0)),
                                             (Rect::new(100.0, 240.0, 200.0, 20.0), Rect::new(96.0, 236.0, 208.0, 28.0))]), 0, LONG, 14.0);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 2, "{:?}", out.dropped);
        let (long, neighbour) = (&out.placed[0], &out.placed[1]);
        assert!(long.expanded && long.degraded.is_none() && neighbour.degraded.is_none());
        let (plate, _) = on_desktop(long, WINDOW);
        assert!(plate.y < 196.0, "grew upward: {plate:?}");
        assert!(plate.bottom() <= 230.0 + 0.5, "stopped halfway to the neighbour: {plate:?}");
        assert_eq!(plate.intersection(&on_desktop(neighbour, WINDOW).0), 0.0);
        for p in &out.placed { assert_inside(p, WINDOW, DesktopRect::of_window(&WindowGeometry::from(WINDOW))); }
    }

    #[test]
    fn a_boxed_in_translation_grows_sideways() {
        // Neighbours right above and below: no height to gain, so the plate widens (centred text: both ways).
        let mut frame = with_text(custom_frame(&[(Rect::new(500.0, 200.0, 120.0, 20.0), Rect::new(496.0, 196.0, 128.0, 28.0)),
                                                 (Rect::new(500.0, 170.0, 120.0, 20.0), Rect::new(496.0, 170.0, 128.0, 20.0)),
                                                 (Rect::new(500.0, 230.0, 120.0, 20.0), Rect::new(496.0, 230.0, 128.0, 20.0))]), 0, "Довольно длинный перевод надписи", 14.0);
        frame.blocks[0].style.alignment = TextAlignment::Center;
        let out = outcome(&frame);
        let p = out.placed.iter().find(|p| p.block_id == 1).unwrap();
        assert!(p.expanded && p.degraded.is_none(), "{:?}", p.degraded);
        let (plate, _) = on_desktop(p, WINDOW);
        assert!(plate.x < 496.0 && plate.right() > 624.0, "widened on both sides: {plate:?}");
        assert_eq!(out.placed.len(), 3);
        for p in &out.placed { assert_inside(p, WINDOW, DesktopRect::of_window(&WindowGeometry::from(WINDOW))); }
    }

    #[test]
    fn fields_at_all_four_edges_stay_inside_the_window() {
        // Plates restored around originals at the very edges reach out of the frame; translations are long.
        let fields = [(Rect::new(0.0, 180.0, 160.0, 20.0), Rect::new(-6.0, 174.0, 172.0, 32.0)),
                      (Rect::new(500.0, 0.0, 200.0, 20.0), Rect::new(494.0, -6.0, 212.0, 32.0)),
                      (Rect::new(1040.0, 180.0, 160.0, 20.0), Rect::new(1034.0, 174.0, 172.0, 32.0)),
                      (Rect::new(500.0, 380.0, 200.0, 20.0), Rect::new(494.0, 374.0, 212.0, 32.0))];
        let mut frame = custom_frame(&fields);
        for i in 0..4 { frame = with_text(frame, i, LONG, 14.0); }
        for (window, screen) in [(WINDOW, None), ([40.0, 30.0, 1200.0, 400.0], Some(DesktopRect::in_space(0.0, 0.0, 1920.0, 1080.0)))] {
            let view = Viewport::new(WindowGeometry::from(window), screen.into_iter().collect());
            let out = place(&frame, &view);
            assert_eq!((out.placed.len(), out.dropped.len()), (4, 0));
            for p in &out.placed {
                assert_inside(p, window, DesktopRect::of_window(&WindowGeometry::from(window)));
                assert!(p.inner.iter().all(|v| *v >= 0.0), "{}: {:?}", p.key, p.inner);
            }
        }
    }

    #[test]
    fn a_window_partly_off_the_screen_keeps_its_fields_on_the_screen() {
        // The window starts 300 px left of the only screen: one original is half off it, one entirely.
        let window = [-300.0, 0.0, 1200.0, 400.0];
        let screen = DesktopRect::in_space(0.0, 0.0, 1920.0, 1080.0);
        let frame = custom_frame(&[(Rect::new(250.0, 100.0, 200.0, 20.0), Rect::new(240.0, 92.0, 220.0, 36.0)),
                                   (Rect::new(20.0, 200.0, 100.0, 20.0), Rect::new(12.0, 192.0, 116.0, 36.0))]);
        let out = place(&frame, &Viewport::new(WindowGeometry::from(window), vec![screen]));
        assert_eq!(out.placed.len(), 1);
        assert_inside(&out.placed[0], window, DesktopRect::in_space(0.0, 0.0, 900.0, 400.0));
        assert_eq!(out.dropped.len(), 1, "nothing to cover off the screen");
        assert_eq!((out.dropped[0].block_id, out.dropped[0].reason), (2, "оригинал за пределами экрана"));
    }

    #[test]
    fn the_viewport_is_the_window_on_the_screen_of_the_original() {
        // A 100 % monitor and a 125 % one on its right (1920×1080 physical = 1536×864 logical); the window spans both.
        let screens = vec![DesktopRect::in_space(0.0, 0.0, 1920.0, 1080.0), DesktopRect::in_space(1920.0, 0.0, 1536.0, 864.0)];
        let view = Viewport::new(WindowGeometry::from([1500.0, 100.0, 1200.0, 600.0]), screens);
        let at = |x: f32| view.bounds_for(&DesktopRect::in_space(x, 200.0, 100.0, 20.0));
        assert_eq!(at(1600.0), Some(DesktopRect::in_space(1500.0, 100.0, 420.0, 600.0)));
        assert_eq!(at(2000.0), Some(DesktopRect::in_space(1920.0, 100.0, 780.0, 600.0)));
        assert_eq!(at(1880.0).unwrap().x, 1920.0, "across the edge: the monitor with most of the original");
        assert_eq!(at(5000.0), None);
        let only = Viewport::window(WindowGeometry::from([-300.0, 0.0, 1200.0, 400.0]));
        assert_eq!(only.bounds_for(&DesktopRect::in_space(-250.0, 10.0, 10.0, 10.0)), Some(DesktopRect::in_space(-300.0, 0.0, 1200.0, 400.0)),
            "no screens reported yet: the whole window");
    }

    #[test]
    fn a_field_across_two_monitors_is_drawn_on_one() {
        let screens = vec![DesktopRect::in_space(0.0, 0.0, 600.0, 400.0), DesktopRect::in_space(600.0, 0.0, 600.0, 400.0)];
        // The original spans x 540..700 (60 px left of the edge, 100 px right of it).
        let frame = custom_frame(&[(Rect::new(540.0, 100.0, 160.0, 20.0), Rect::new(532.0, 92.0, 176.0, 36.0))]);
        let out = place(&frame, &Viewport::new(WindowGeometry::from(WINDOW), screens));
        assert_eq!(out.placed.len(), 1);
        assert_inside(&out.placed[0], WINDOW, DesktopRect::in_space(600.0, 0.0, 600.0, 400.0));
    }

    #[test]
    fn fractional_scaling_and_wide_frames_keep_every_field_in_the_window() {
        // Physical frames on scaled screens: the window is in logical pixels, the frame in physical ones.
        for (size, scale) in [((3440u32, 1440u32), 1.75f64), ((2560, 1080), 1.25), ((2560, 1080), 1.5), ((1920, 1080), 1.0)] {
            let (fw, fh) = (size.0 as f32, size.1 as f32);
            let fields = [(Rect::new(0.25 * fw, fh - 110.0, 0.5 * fw, 48.0), Rect::new(0.25 * fw - 12.0, fh - 122.0, 0.5 * fw + 24.0, 72.0)),
                          (Rect::new(fw - 300.0, 30.0, 300.0, 40.0), Rect::new(fw - 312.0, 18.0, 324.0, 64.0)),
                          (Rect::new(0.0, fh - 40.0, 260.0, 40.0), Rect::new(-12.0, fh - 52.0, 284.0, 64.0))];
            let mut frame = sized_frame(size, &fields);
            for i in 0..3 { frame = with_text(frame, i, LONG, 14.0); }
            frame.blocks[0].translation = LONG.repeat(3);
            let window = [37.0, 21.0, size.0 as f64 / scale, size.1 as f64 / scale];
            let screen = DesktopRect::in_space(0.0, 0.0, 3840.0, 2160.0);
            let out = place(&frame, &Viewport::new(WindowGeometry::from(window), vec![screen]));
            assert_eq!(out.placed.len(), 3, "{size:?} at {scale}: {:?}", out.dropped);
            for p in &out.placed {
                assert_inside(p, window, DesktopRect::of_window(&WindowGeometry::from(window)));
                assert!(p.font_px >= 14.0, "{size:?} at {scale}: {} px", p.font_px);
            }
        }
    }

    #[test]
    fn heights_reach_the_screen_through_the_vertical_scale() {
        // The same frame in a window of the same width but twice the height (the game is changing its
        // resolution): the glyphs are shown twice as tall, so is the translation.
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 600.0, 40.0), Rect::new(96.0, 96.0, 608.0, 48.0))]);
        frame.blocks[0].style.cap_height_px = 21.0;
        let size = |h: f64| place(&frame, &Viewport::window(WindowGeometry::from([0.0, 0.0, 1200.0, h]))).placed[0].font_px;
        let (flat, tall) = (size(400.0), size(800.0));
        assert!(tall >= 1.9 * flat, "{flat} px → {tall} px");
    }

    #[test]
    fn an_expanded_field_is_worked_out_once_and_follows_the_window_without_drift() {
        let frame = with_text(custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(96.0, 96.0, 208.0, 28.0))]), 0, LONG, 14.0);
        let count = Counting(Cell::new(0));
        let mut cache = PlacementCache::default();
        let mut run = |x: f64| {
            let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
            place_regions(&[region], &Viewport::window(WindowGeometry::from([x, 50.0, 1200.0, 400.0])), &InplaceSettings::default(), &HashMap::new(), &count, &mut cache).placed
        };
        let first = run(0.0);
        assert!(first[0].expanded);
        let calls = count.0.get();
        for x in [300.0, 17.25, -40.5, 0.0] {
            let again = run(x);
            assert_eq!(count.0.get(), calls, "moving the window measures nothing again");
            assert_eq!((again[0].font_px, again[0].inner, again[0].expanded), (first[0].font_px, first[0].inner, true));
            assert!(again[0].box_rect.iter().zip(&first[0].box_rect).all(|(a, b)| (a - b).abs() < 1e-6), "no drift: {:?} vs {:?}", again[0].box_rect, first[0].box_rect);
        }
    }

    #[test]
    fn with_no_room_anywhere_the_translation_is_cut_inside_the_window() {
        // The window is no larger than the field: nothing to grow into, the end of the text is cut.
        let window = [0.0, 0.0, 300.0, 40.0];
        let frame = with_text(sized_frame((300, 40), &[(Rect::new(10.0, 10.0, 280.0, 20.0), Rect::new(4.0, 4.0, 292.0, 32.0))]), 0, &"слово ".repeat(100), 14.0);
        let out = place(&frame, &Viewport::window(WindowGeometry::from(window)));
        let p = &out.placed[0];
        assert_eq!((p.degraded, p.wrap), (Some("elided"), "elide"));
        assert!(p.max_lines >= 1 && p.font_px >= 14.0);
        assert_inside(p, window, DesktopRect::of_window(&WindowGeometry::from(window)));
    }

    #[test]
    fn a_short_translation_is_neither_grown_nor_cut() {
        let out = outcome(&custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(96.0, 96.0, 208.0, 28.0))]));
        let p = &out.placed[0];
        assert!(!p.expanded && p.degraded.is_none());
        let json = serde_json::to_value(p).unwrap();
        assert!(json.get("expanded").is_none() && json.get("degraded").is_none(), "absent unless set");
    }
}


/// Preview-only sample: the same typography resolver, background renderer, fitting and DTO as
/// real fields. Coordinates describe the sample canvas, never a captured game or its geometry.
pub fn preview(settings: &InplaceSettings, width: f32, height: f32, measure: &dyn TextMeasure) -> Placed {
    use super::background::{BackgroundAnalysis, BackgroundInpainter};
    use super::font_matcher::FontSelection;
    use super::typography::{LineHeightEstimate, TypographyEstimate, TypographyEstimator};
    use super::{FontWeight, InkMask, Padding, Rect, Script};
    let (w, h) = (width.clamp(80.0, 1600.0) as u32, height.clamp(60.0, 900.0) as u32);
    let font = FontSelection { family: "Inter".into(), category: FontCategory::SansSerif,
        italic_available: true, available_weights: FontWeight::ALL.to_vec(), generic: false,
        condensed_family: None, confidence: 1.0 };
    let estimate = TypographyEstimate { cap_height_px: 16.0, lines: 2,
        line_height: LineHeightEstimate { absolute_px: 28.0, proportional: 1.2, confidence: 1.0 },
        weight: FontWeight::Normal, italic: false, alignment: TextAlignment::Center,
        letter_spacing_px: 0.0, text_color: [245; 3], text_color_reliable: true, padding: Padding::uniform(8.0) };
    let rect = Rect::new(20.0, 20.0, w as f32 - 40.0, h as f32 - 40.0);
    let canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([38, 49, 61, 255]));
    let mask = InkMask { w: w as usize, h: h as usize, ink: vec![false; (w * h) as usize], ink_is_dark: false };
    let analysis = BackgroundAnalysis { median: [38, 49, 61], mean: [38, 49, 61], luminance: 49.0,
        texture: 0.0, edge_density: 0.0, gradient: 0.0, reliable: true };
    let background = BackgroundInpainter.render(&canvas, &mask, &rect, 24.0, analysis, settings, TextBlockType::Dialogue);
    let block = InplaceBlock { id: 0, block_type: TextBlockType::Dialogue, text_rect: rect,
        original: "The path is clear. Let’s continue our journey.".into(),
        translation: "Путь свободен. Продолжим наше путешествие.".into(), script: Script::Cyrillic,
        style: TypographyEstimator.resolve(&estimate, &font, settings), font, background,
        lines_note: "Демонстрационный пример, не OCR".into(), revision: 0 };
    let full = NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    let map = FrameToDesktop::new(&WindowGeometry::from([0.0, 0.0, w as f64, h as f64]), full, (w, h));
    let boxes = base_boxes(&block, &map, &DesktopRect::in_space(0.0, 0.0, w as f32, h as f32), (w, h), settings);
    let fit = fit_text(&block, &boxes.text, &map, measure);
    let frame = InplaceFrame { frame: (w, h), blocks: vec![], undrawable: vec![], new_translations: vec![] };
    let region = RegionInput { id: "preview", rect: full, frame: &frame };
    placed_entry(&region, &block, &fit, map.fraction(&boxes.plate), inner_of(&boxes.plate, &boxes.text), settings, None)
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use crate::settings::PropertyMode;
    #[test]
    fn sample_uses_production_typography_and_preserves_configuration() {
        let mut s = InplaceSettings::default();
        s.font_size = PropertyMode::Manual(28.0);
        s.text_color = PropertyMode::Manual("#12abef".into());
        s.italic = PropertyMode::Manual(true);
        s.shadow = true;
        s.fill_opacity = 0.25;
        let before = s.clone();
        let p = preview(&s, 420.0, 200.0, &super::super::fit::ApproxMeasure);
        assert!(!p.text.is_empty());
        assert_eq!(p.text_color, "#12abef");
        assert!(p.italic && p.shadow);
        assert_eq!(p.background.opacity, 0.25);
        assert_eq!(s, before);
        assert_eq!(p.region_id, "preview");
    }
}
