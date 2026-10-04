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
use super::{FontCategory, Rect, TextAlignment, TextBlockType, WrapMode, contrast_ratio, hex, parse_hex_color};
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
fn fit_block(b: &InplaceBlock, frame: (u32, u32), scale: f32, settings: &InplaceSettings, measure: &dyn TextMeasure) -> Fitted {
    let st = &b.style;
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let transparent = b.background.mode == BackgroundRenderMode::Transparent;
    // Заливка закрывает поле с полями; в прозрачном режиме — только сам текст и его обводка.
    let area = if transparent { b.text_rect.expand(effect_px(settings) / scale.max(0.01), fw, fh) } else { b.background.rect };
    // Отступы текста внутри заливки, px экрана: ручные — как заданы, иначе — где был оригинал.
    let inner = if st.padding_manual {
        [st.padding.left, st.padding.top, st.padding.right, st.padding.bottom]
    } else {
        [(b.text_rect.x - area.x) * scale, (b.text_rect.y - area.y) * scale,
         (area.right() - b.text_rect.right()) * scale, (area.bottom() - b.text_rect.bottom()) * scale]
    };
    let width = (area.w * scale - inner[0] - inner[2]).max(1.0);
    let height = (area.h * scale - inner[1] - inner[3]).max(1.0);
    let font = FontSpec { family: st.font_family.clone(), weight: st.font_weight, italic: st.italic };
    let fit = fit_translation_to_box(&FitInput {
        text: &b.translation, width, height, font,
        manual_px: st.font_size, cap_height_px: st.cap_height_px * scale,
        min_px: st.min_font_size, max_px: st.max_font_size, source_lines: st.source_lines,
        manual_line_height: st.line_height, source_line_px: st.line_height_px.map(|v| v * scale),
        letter_spacing: if st.letter_spacing_manual { st.letter_spacing } else { st.letter_spacing * scale },
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

fn placed(region: &RegionInput, b: &InplaceBlock, f: &Fitted, frame: (u32, u32), settings: &InplaceSettings, image: Option<&str>) -> Placed {
    let st = &b.style;
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
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
        box_rect: [(f.area.x / fw) as f64, (f.area.y / fh) as f64, (f.area.w / fw) as f64, (f.area.h / fh) as f64],
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
    }
}

/// Размещает все поля всех областей. `window` — клиентская область окна игры на рабочем столе
/// (x, y, ширина, высота). `images` — готовые URL подложек по (область, поле).
pub fn place_regions(regions: &[RegionInput], window: [f64; 4], settings: &InplaceSettings,
                     images: &HashMap<(String, u64), String>, measure: &dyn TextMeasure, cache: &mut PlacementCache) -> Vec<Placed> {
    let fingerprint = settings_fingerprint(settings);
    let mut live = HashSet::new();
    let mut entries: Vec<Placed> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for region in regions {
        let frame = region.frame.frame;
        let (rx, ry) = (window[0] + region.rect.x * window[2], window[1] + region.rect.y * window[3]);
        let (sx, sy) = (region.rect.w * window[2] / frame.0.max(1) as f64, region.rect.h * window[3] / frame.1.max(1) as f64);
        let scale = sx as f32;
        // Прямоугольник кадра области -> рабочий стол.
        let to_desktop = |r: &Rect| Rect::new((rx + r.x as f64 * sx) as f32, (ry + r.y as f64 * sy) as f32, (r.w as f64 * sx) as f32, (r.h as f64 * sy) as f32);
        for b in &region.frame.blocks {
            let key = (region.id.to_owned(), b.id);
            live.insert(key.clone());
            let fitted = match cache.entries.get(&key) {
                Some(c) if c.revision == b.revision && c.scale == scale.to_bits() && c.settings == fingerprint => {
                    cache.hits += 1;
                    c.fitted.clone()
                }
                _ => {
                    cache.fits += 1;
                    let fitted = fit_block(b, frame, scale, settings, measure);
                    cache.entries.insert(key.clone(), CachedFit { revision: b.revision, scale: scale.to_bits(), settings: fingerprint, fitted: fitted.clone() });
                    fitted
                }
            };
            let entry = placed(region, b, &fitted, frame, settings, images.get(&key).map(String::as_str).filter(|u| !u.is_empty()));
            let box_rect = to_desktop(&fitted.area);
            let text_rect = Rect::new(box_rect.x + fitted.inner[0], box_rect.y + fitted.inner[1],
                (box_rect.w - fitted.inner[0] - fitted.inner[2]).max(1.0), (box_rect.h - fitted.inner[1] - fitted.inner[3]).max(1.0));
            candidates.push(Candidate {
                id: entry.key.clone(),
                source_rect: to_desktop(&b.text_rect),
                text_rect,
                background_rect: box_rect,
                effect_margin: if entry.outline { effect_px(settings) } else { 0.0 },
                image_background: b.background.image.is_some(),
                allow_text_shift: b.background.mode != BackgroundRenderMode::Transparent,
            });
            entries.push(entry);
        }
    }
    cache.retain_live(&live);
    let bounds = Rect::new(window[0] as f32, window[1] as f32, window[2] as f32, window[3] as f32);
    let resolved = collision::resolve(&candidates, bounds);
    let mut out = Vec::with_capacity(entries.len());
    for ((mut entry, candidate), result) in entries.into_iter().zip(&candidates).zip(resolved) {
        // Не удалось разместить без перекрытия: поле не рисуется (причина уже в журнале).
        let Some(place) = result else { continue };
        if place.background_rect != candidate.background_rect || place.text_rect != candidate.text_rect {
            let Some(region) = regions.iter().find(|r| r.id == entry.region_id) else { continue };
            let (rx, ry) = (window[0] + region.rect.x * window[2], window[1] + region.rect.y * window[3]);
            let (rw, rh) = ((region.rect.w * window[2]).max(1e-6), (region.rect.h * window[3]).max(1e-6));
            let bg = place.background_rect;
            entry.box_rect = [(bg.x as f64 - rx) / rw, (bg.y as f64 - ry) / rh, bg.w as f64 / rw, bg.h as f64 / rh];
            entry.inner = [place.text_rect.x - bg.x, place.text_rect.y - bg.y,
                bg.right() - place.text_rect.right(), bg.bottom() - place.text_rect.bottom()];
        }
        out.push(entry);
    }
    out
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
        let s = Settings { target_lang: "ru".into(), translation_display: crate::settings::TranslationDisplay::Inplace, ..Settings::default() };
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(dynamic(img), &s, Instant::now(), false);
        for j in jobs { e.complete(j.id, Some(("src".into(), "Это перевод, который помещается".into())), &s); }
        e.finish(&s).expect("fields")
    }

    fn region(frame: &InplaceFrame) -> RegionInput<'_> {
        RegionInput { id: "subtitles", rect: NormRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 }, frame }
    }

    #[test]
    fn places_every_field_with_a_typed_complete_record() {
        let frame = frame_with_blocks();
        let mut cache = PlacementCache::default();
        let out = place_regions(&[region(&frame)], [0.0, 0.0, 1200.0, 800.0], &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
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
        let run = |window: [f64; 4], settings: &InplaceSettings, cache: &mut PlacementCache| place_regions(&[region(&frame)], window, settings, &HashMap::new(), &count, cache);
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
        place_regions(&[region(&frame)], [0.0, 0.0, 1200.0, 800.0], &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
        assert_eq!(cache.entries.len(), frame.blocks.len());
        place_regions(&[], [0.0, 0.0, 1200.0, 800.0], &InplaceSettings::default(), &HashMap::new(), &ApproxMeasure, &mut cache);
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
        let out = place_regions(&[region(&frame)], [0.0, 0.0, 1200.0, 800.0], &s, &images, &ApproxMeasure, &mut PlacementCache::default());
        let p = out.iter().find(|p| p.block_id == id).unwrap();
        assert_eq!((p.background.color.as_str(), p.outline_color.as_str(), p.background.image.as_str()), ("#112233", "#445566", "file:///x.png?r=1"));
    }
}
