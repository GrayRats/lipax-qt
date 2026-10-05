//! Размещение полей «поверх оригинала» на экране: подгонка перевода под поле, затем защита
//! соседних полей от перекрытия, затем запись, которую QML только рисует.
//!
//! Раньше это жило в мосте Qt и на каждый вызов заново подгоняло каждое поле. Здесь результат
//! типизирован (нет правок JSON по строковым ключам и `unwrap`), измерение текста — через
//! трейт [`TextMeasure`] (Qt в приложении, модель в тестах), а подгонка кэшируется:
//!
//! - ключ кэша — поле, его ревизия (меняется при смене текста, шрифта, фона), масштаб экрана и
//!   отпечаток настроек, поэтому перемещение окна игры и повторные публикации ничего не пересчитывают;
//! - положение на рабочем столе и коллизии считаются каждый раз: это арифметика над прямоугольниками.

use super::background::BackgroundRenderMode;
use super::collision::{self, Candidate};
use super::engine::{InplaceBlock, InplaceFrame};
use super::fit::{FitInput, FitResult, FontSpec, TextMeasure, fit_translation_to_box};
use super::{DesktopRect, FontCategory, FrameToDesktop, LogicalScale, Rect, TextAlignment, TextBlockType, WrapMode, contrast_ratio, hex, parse_hex_color};
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
    /// How the field was degraded to fit, if it was: `plain-plate` (a plain plate instead of the
    /// restored background) or `beside-original` (a compact label next to the text it translates).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<&'static str>,
}

/// A translation that could not be shown over the game at all: it goes to the translation window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FallbackText {
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
    pub fallback: Vec<FallbackText>,
}

/// Область с её последним результатом движка.
pub struct RegionInput<'a> {
    pub id: &'a str,
    pub rect: NormRect,
    pub frame: &'a InplaceFrame,
}

/// Результат подгонки, не зависящий от положения окна игры.
#[derive(Debug, Clone)]
struct Fitted {
    fit: FitResult,
    inner: [f32; 4],
    /// Область заливки, px кадра.
    area: Rect,
}

struct CachedFit {
    revision: u64,
    /// `LogicalScale::key`: a refit is needed when the screen scale of the frame changes.
    scale: u32,
    settings: u64,
    fitted: Fitted,
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

/// Подгонка одного поля: области заливки и текста, затем перенос, интервалы и кегль.
fn fit_block(b: &InplaceBlock, frame: (u32, u32), scale: LogicalScale, settings: &InplaceSettings, measure: &dyn TextMeasure) -> Fitted {
    let st = &b.style;
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let transparent = b.background.mode == BackgroundRenderMode::Transparent;
    // Заливка закрывает поле с полями; в прозрачном режиме — только сам текст и его обводка.
    let area = if transparent { b.text_rect.expand(scale.frame_px(effect_px(settings)), fw, fh) } else { b.background.rect };
    // Отступы текста внутри заливки, px экрана: ручные — как заданы, иначе — где был оригинал.
    let inner = if st.padding_manual {
        [st.padding.left, st.padding.top, st.padding.right, st.padding.bottom]
    } else {
        [scale.px(b.text_rect.x - area.x), scale.px(b.text_rect.y - area.y),
         scale.px(area.right() - b.text_rect.right()), scale.px(area.bottom() - b.text_rect.bottom())]
    };
    fit_in(b, area, inner, scale, measure)
}

/// Подгонка перевода в заливку `area` с отступами `inner` (px экрана).
fn fit_in(b: &InplaceBlock, area: Rect, inner: [f32; 4], scale: LogicalScale, measure: &dyn TextMeasure) -> Fitted {
    let st = &b.style;
    let width = (scale.px(area.w) - inner[0] - inner[2]).max(1.0);
    let height = (scale.px(area.h) - inner[1] - inner[3]).max(1.0);
    let font = FontSpec { family: st.font_family.clone(), weight: st.font_weight, italic: st.italic };
    let fit = fit_translation_to_box(&FitInput {
        text: &b.translation, width, height, font,
        manual_px: st.font_size, cap_height_px: scale.px(st.cap_height_px),
        min_px: st.min_font_size, max_px: st.max_font_size, source_lines: st.source_lines,
        manual_line_height: st.line_height, source_line_px: st.line_height_px.map(|v| scale.px(v)),
        letter_spacing: if st.letter_spacing_manual { st.letter_spacing } else { scale.px(st.letter_spacing) },
        letter_spacing_manual: st.letter_spacing_manual, manual_wrap: st.wrap_mode, script: b.script,
        condensed_family: if st.allow_condensed && st.font_size.is_none() { b.font.condensed_family.clone() } else { None },
    }, measure);
    Fitted { fit, inner, area }
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

fn placed_entry(region: &RegionInput, b: &InplaceBlock, f: &Fitted, frame: (u32, u32), settings: &InplaceSettings, image: Option<&str>) -> Placed {
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
        box_rect: f.area.fraction_of(frame),
        inner: f.inner,
        text: b.translation.clone(),
        original: b.original.clone(),
        font_family: f.fit.family.clone(),
        font_px: f.fit.font_px.floor().max(1.0),
        font_weight: st.font_weight.value(),
        italic: st.italic,
        line_height: f.fit.line_height,
        letter_spacing: f.fit.letter_spacing,
        alignment: alignment_name(st.alignment),
        wrap: wrap_name(f.fit.wrap_mode),
        max_lines: f.fit.max_lines,
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
fn chip_area(b: &InplaceBlock, side: Beside) -> Rect {
    let line = (b.text_rect.h / b.style.source_lines.max(1) as f32).max(8.0);
    let (w, h, gap) = (b.text_rect.w.max(10.0 * line), (2.2 * line).max(b.text_rect.h), 0.25 * line);
    match side {
        Beside::Below => Rect::new(b.text_rect.x, b.text_rect.bottom() + gap, w, h),
        Beside::Above => Rect::new(b.text_rect.x, b.text_rect.y - gap - h, w, h),
    }
}

/// Logical padding of the label's plate.
const CHIP_PAD: f32 = 4.0;

/// Keeps what is already placed: a variant is taken only if the field it is meant for fits now and
/// every field that fitted before still does.
fn accepted(trial: &[Candidate], before: &[Option<collision::Placement>], index: usize, bounds: DesktopRect) -> Option<Vec<Option<collision::Placement>>> {
    let after = collision::resolve_quiet(trial, bounds);
    (after[index].is_some() && before.iter().zip(&after).all(|(old, new)| old.is_none() || new.is_some())).then_some(after)
}

/// Размещает все поля всех областей. `window` — клиентская область окна игры на рабочем столе
/// (логические пиксели). `images` — готовые URL подложек по (область, поле).
///
/// Поле, которое не помещается без перекрытия, не пропадает: оно деградирует по ступеням —
/// простая плашка вместо восстановленного фона, затем компактная плашка рядом с оригиналом,
/// и только если и она не помещается, перевод уходит в окно перевода (`fallback`). Уже
/// размещённые поля при этом не сдвигаются.
pub fn place_regions(regions: &[RegionInput], window: &WindowGeometry, settings: &InplaceSettings,
                     images: &HashMap<(String, u64), String>, measure: &dyn TextMeasure, cache: &mut PlacementCache) -> PlacementOutcome {
    struct Owner<'a> { region: &'a RegionInput<'a>, block: &'a InplaceBlock, map: FrameToDesktop, fitted: Fitted, image: Option<&'a str> }
    let fingerprint = settings_fingerprint(settings);
    let mut live = HashSet::new();
    let mut owners: Vec<Owner> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut fallback: Vec<FallbackText> = Vec::new();
    for region in regions {
        let frame = region.frame.frame;
        // Where the region frame lies on the desktop: the only conversion between frame and desktop pixels.
        let map = FrameToDesktop::new(window, region.rect, frame);
        let scale = map.scale();
        for u in &region.frame.undrawable {
            fallback.push(FallbackText { region_id: region.id.to_owned(), block_id: u.id, text: u.translation.clone(), original: u.original.clone(), reason: u.reason });
        }
        for b in &region.frame.blocks {
            let key = (region.id.to_owned(), b.id);
            live.insert(key.clone());
            let fitted = match cache.entries.get(&key) {
                Some(c) if c.revision == b.revision && c.scale == scale.key() && c.settings == fingerprint => {
                    cache.hits += 1;
                    c.fitted.clone()
                }
                _ => {
                    cache.fits += 1;
                    let fitted = fit_block(b, frame, scale, settings, measure);
                    cache.entries.insert(key.clone(), CachedFit { revision: b.revision, scale: scale.key(), settings: fingerprint, fitted: fitted.clone() });
                    fitted
                }
            };
            let image = images.get(&key).map(String::as_str).filter(|u| !u.is_empty());
            let box_rect = map.rect(&fitted.area);
            let text_rect = DesktopRect::in_space(box_rect.x + fitted.inner[0], box_rect.y + fitted.inner[1],
                (box_rect.w - fitted.inner[0] - fitted.inner[2]).max(1.0), (box_rect.h - fitted.inner[1] - fitted.inner[3]).max(1.0));
            candidates.push(Candidate {
                id: format!("{}:{}", region.id, b.id),
                source_rect: map.rect(&b.text_rect),
                text_rect,
                background_rect: box_rect,
                effect_margin: if b.background.mode == BackgroundRenderMode::Transparent { effect_px(settings) } else { 0.0 },
                image_background: b.background.image.is_some(),
                allow_text_shift: b.background.mode != BackgroundRenderMode::Transparent,
            });
            owners.push(Owner { region, block: b, map, fitted, image });
        }
    }
    cache.retain_live(&live);
    let bounds = DesktopRect::of_window(window);
    let original = candidates.clone();
    let mut results = collision::resolve(&candidates, bounds);
    // How each field ended up, and the fit of a label if it became one.
    let mut tiers: Vec<Option<&'static str>> = vec![None; owners.len()];
    let mut chip_fit: Vec<Option<Fitted>> = vec![None; owners.len()];
    for i in 0..owners.len() {
        if results[i].is_some() { continue; }
        // 1. The same place with a plain plate.
        let mut trial = candidates.clone();
        trial[i] = plain_plate(&candidates[i]);
        if let Some(after) = accepted(&trial, &results, i, bounds) {
            candidates = trial;
            results = after;
            tiers[i] = Some("plain-plate");
            continue;
        }
        // 2. A compact label next to the original, below it first.
        let owner = &owners[i];
        for side in [Beside::Below, Beside::Above] {
            let area = chip_area(owner.block, side);
            let inner = [CHIP_PAD; 4];
            let fitted = fit_in(owner.block, area, inner, owner.map.scale(), measure);
            let plate = owner.map.rect(&area);
            let mut trial = candidates.clone();
            trial[i] = Candidate {
                text_rect: DesktopRect::in_space(plate.x + inner[0], plate.y + inner[1], (plate.w - 2.0 * CHIP_PAD).max(1.0), (plate.h - 2.0 * CHIP_PAD).max(1.0)),
                background_rect: plate,
                effect_margin: 0.0,
                image_background: false,
                allow_text_shift: false,
                ..candidates[i].clone()
            };
            if let Some(after) = accepted(&trial, &results, i, bounds) {
                candidates = trial;
                results = after;
                tiers[i] = Some("beside-original");
                chip_fit[i] = Some(fitted);
                break;
            }
        }
    }
    let mut placed = Vec::with_capacity(owners.len());
    for (i, owner) in owners.iter().enumerate() {
        let Some(place) = results[i].as_ref() else {
            // 3. Nothing fits: the translation is not lost, the translation window shows it.
            tracing::warn!(target: "inplace.fallback", block = %candidates[i].id, "no room over the game: the translation goes to the translation window");
            fallback.push(FallbackText { region_id: owner.region.id.to_owned(), block_id: owner.block.id, text: owner.block.translation.clone(),
                original: owner.block.original.clone(), reason: "для перевода нет места поверх игры" });
            continue;
        };
        let mut entry = placed_entry(owner.region, owner.block, chip_fit[i].as_ref().unwrap_or(&owner.fitted), owner.region.frame.frame, settings, owner.image);
        if tiers[i].is_some() || place.background_rect != original[i].background_rect || place.text_rect != original[i].text_rect {
            let bg = place.background_rect;
            entry.box_rect = owner.map.fraction(&bg);
            entry.inner = [place.text_rect.x - bg.x, place.text_rect.y - bg.y, bg.right() - place.text_rect.right(), bg.bottom() - place.text_rect.bottom()];
        }
        if let Some(tier) = tiers[i] {
            tracing::info!(target: "inplace.fallback", block = %candidates[i].id, tier, "field shown in a simpler form");
            // A plain plate carries the text itself: no restored picture, no outline halo.
            entry.degraded = Some(tier);
            entry.background.mode = BackgroundRenderMode::SolidFill;
            entry.background.image = String::new();
            entry.outline = false;
        }
        placed.push(entry);
    }
    PlacementOutcome { placed, fallback }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::engine::InplaceEngine;
    use crate::layout::fit::{ApproxMeasure, Measured};
    use crate::layout::font_database::InstalledFontDatabase;
    use crate::layout::testing::*;
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
        InplaceFrame { frame: (1200, 400), blocks, undrawable: Vec::new(), new_translations: Vec::new() }
    }

    fn outcome(frame: &InplaceFrame) -> PlacementOutcome {
        let region = RegionInput { id: "r", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame };
        place_regions(&[region], &WindowGeometry::from([0.0, 0.0, 1200.0, 400.0]), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default())
    }

    #[test]
    fn fields_that_fit_are_not_degraded() {
        let frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                   (Rect::new(500.0, 100.0, 200.0, 20.0), Rect::new(492.0, 92.0, 216.0, 36.0))]);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 2);
        assert!(out.fallback.is_empty() && out.placed.iter().all(|p| p.degraded.is_none()));
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
        assert!(out.fallback.is_empty());
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
    fn a_field_that_cannot_be_placed_anywhere_goes_to_the_translation_window() {
        // Two fields on the same text (an OCR duplicate): the second cannot be drawn without covering the first.
        let frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0)),
                                   (Rect::new(110.0, 104.0, 200.0, 20.0), Rect::new(102.0, 96.0, 216.0, 36.0))]);
        let out = outcome(&frame);
        assert_eq!(out.placed.len(), 1, "the first stays");
        assert_eq!(out.placed[0].block_id, 1);
        assert_eq!(out.fallback.len(), 1);
        assert_eq!((out.fallback[0].block_id, out.fallback[0].text.as_str()), (2, "перевод 1"), "the translation is not lost");
        assert!(out.fallback[0].reason.contains("нет места"));
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
    fn undrawable_fields_are_handed_to_the_translation_window() {
        let mut frame = custom_frame(&[(Rect::new(100.0, 100.0, 200.0, 20.0), Rect::new(92.0, 92.0, 216.0, 36.0))]);
        frame.undrawable.push(crate::layout::engine::UndrawableField { id: 9, original: "src".into(), translation: "текст".into(), reason: "нет встроенного шрифта с нужными глифами" });
        let out = outcome(&frame);
        assert_eq!((out.placed.len(), out.fallback.len()), (1, 1));
        assert_eq!((out.fallback[0].block_id, out.fallback[0].text.as_str()), (9, "текст"));
    }

    #[test]
    fn places_every_field_with_a_typed_complete_record() {
        let frame = frame_with_blocks();
        let mut cache = PlacementCache::default();
        let out = place_regions(&[region(&frame)], &WindowGeometry::from([0.0, 0.0, 1200.0, 800.0]), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache).placed;
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
        let run = |window: [f64; 4], settings: &InplaceSettings, cache: &mut PlacementCache| place_regions(&[region(&frame)], &WindowGeometry::from(window), settings, &HashMap::new(), &count, cache).placed;
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
        place_regions(&[region(&frame)], &WindowGeometry::from([0.0, 0.0, 1200.0, 800.0]), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
        assert_eq!(cache.entries.len(), frame.blocks.len());
        place_regions(&[], &WindowGeometry::from([0.0, 0.0, 1200.0, 800.0]), &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn backdrop_url_and_manual_colours_reach_the_record() {
        let frame = frame_with_blocks();
        let id = frame.blocks[0].id;
        let mut images = HashMap::new();
        images.insert(("subtitles".to_string(), id), "file:///x.png?r=1".to_string());
        let mut s = InplaceSettings::default();
        s.fill_color = crate::settings::PropertyMode::Manual("#112233".into());
        s.outline_color = crate::settings::PropertyMode::Manual("#445566".into());
        let out = place_regions(&[region(&frame)], &WindowGeometry::from([0.0, 0.0, 1200.0, 800.0]), &s, &images, &ApproxMeasure, &mut PlacementCache::default()).placed;
        let p = out.iter().find(|p| p.block_id == id).unwrap();
        assert_eq!((p.background.color.as_str(), p.outline_color.as_str(), p.background.image.as_str()), ("#112233", "#445566", "file:///x.png?r=1"));
    }
}
