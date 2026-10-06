//! Оркестратор движка «перевод поверх оригинала» для одной области.
//!
//! Анализ отделён от ввода-вывода: [`InplaceEngine::begin`] обрабатывает кадр и возвращает
//! задания OCR только для полей, содержимое которых изменилось; pipeline распознаёт и
//! переводит их и передаёт результат в [`InplaceEngine::complete`]; [`InplaceEngine::finish`]
//! собирает поля для отрисовки и возвращает `None`, если видимое не изменилось.
//!
//! Что пересчитывается:
//! - то же поле, тот же текст — почти ничего (только проверка подписи фона);
//! - то же поле, другой текст — OCR, перевод; шрифт и категория остаются;
//! - то же поле, подвижный фон — только фон;
//! - новое поле — всё, включая однократный выбор шрифта.

use super::background::{BackgroundAnalyzer, BackgroundInpainter};
use super::block_detector::{BlockDetector, DetectedTextBlock};
use super::font_classifier::{FontAnalysis, FontClassifier};
use super::font_database::InstalledFontDatabase;
use super::font_matcher::{FontMatcher, FontPreferences, FontSelection};
use super::tracker::TextBlockTracker;
use super::typography::{TranslationTextStyle, TypographyEstimator};
use super::{FontCategory, InkMask, Rect, Script, TextBlockType, luma};
use crate::settings::Settings;
use image::{DynamicImage, RgbaImage, imageops::FilterType};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

/// OCR lines belong to a block if their union covers about the same area: a different place means
/// the engine read something else than the detector found, and neither should override the other blindly.
fn lines_agree(lines: &[Rect], block: &Rect) -> bool {
    let Some(first) = lines.first() else { return false };
    lines.iter().skip(1).fold(*first, |union, l| union.union(l)).iou(block) >= LINES_AGREE_IOU
}
const LINES_AGREE_IOU: f32 = 0.5;

/// The glyph rectangle according to the OCR lines, if it may replace the detected one: each side
/// moves by at most half a line height, so a field whose second line the engine missed keeps the
/// detector's boundary (the original line would otherwise stay uncovered). `None` keeps the detector.
fn refine_bounds(lines: &[Rect], detected: &Rect, line_height: f32) -> Option<Rect> {
    let first = lines.first()?;
    let u = lines.iter().skip(1).fold(*first, |union, l| union.union(l));
    let tolerance = (0.5 * line_height).max(2.0);
    let within = (u.x - detected.x).abs() <= tolerance && (u.y - detected.y).abs() <= tolerance
        && (u.right() - detected.right()).abs() <= tolerance && (u.bottom() - detected.bottom()).abs() <= tolerance;
    // One pixel of margin: the engine's boxes are tight to the glyphs.
    within.then(|| Rect::new(u.x - 1.0, u.y - 1.0, u.w + 2.0, u.h + 2.0))
}

/// Поле, текст которого нужно распознать.
pub struct OcrJob {
    pub id: u64,
    /// Glyph rectangle of the field, px of the frame.
    pub rect: Rect,
    /// Where `image` was cut out of the frame: OCR geometry is relative to it.
    pub origin: (u32, u32),
    pub image: DynamicImage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InplaceBlock {
    pub id: u64,
    pub block_type: TextBlockType,
    /// Глифы оригинала, px кадра.
    pub text_rect: Rect,
    pub original: String,
    pub translation: String,
    pub script: Script,
    pub font: FontSelection,
    pub style: TranslationTextStyle,
    pub background: super::background::BackgroundResult,
    /// Откуда взята структура строк (OCR или детектор) и почему; для инспектора.
    pub lines_note: String,
    /// Меняется вместе с видимым состоянием поля.
    pub revision: u64,
}

/// A field that has a translation but cannot be drawn over the original at all (for example no
/// bundled font covers its script): its text goes to the translation window instead of being lost.
#[derive(Debug, Clone, PartialEq)]
pub struct UndrawableField {
    pub id: u64,
    pub original: String,
    pub translation: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InplaceFrame {
    pub frame: (u32, u32),
    pub blocks: Vec<InplaceBlock>,
    pub undrawable: Vec<UndrawableField>,
    /// Новые пары «оригинал → перевод» с последней отправки: для истории.
    pub new_translations: Vec<(String, String)>,
}

struct Snapshot {
    image: RgbaImage,
    mask: InkMask,
    blocks: HashMap<u64, DetectedTextBlock>,
}

/// Отпечатки настроек: что именно пользователь поменял.
#[derive(Default, PartialEq, Clone)]
struct Fingerprints {
    style: String,
    background: String,
    fonts: String,
}

impl Fingerprints {
    fn of(s: &Settings) -> Self {
        let i = &s.appearance.inplace;
        let style = serde_json::json!([i.font_family, i.font_size, i.font_weight, i.italic, i.line_height, i.letter_spacing,
            i.alignment, i.wrap_mode, i.text_color, i.padding, i.minimum_font_size, i.maximum_font_size, i.allow_condensed_fallback]).to_string();
        Self {
            style,
            background: serde_json::json!([i.background_mode, i.fill_color, i.fill_opacity, i.padding_x, i.padding_y, i.extra_margin, i.corner_radius, i.outline_color, i.outline_width, i.shadow, i.text_opacity]).to_string(),
            fonts: serde_json::json!([i.font_overrides, i.preferred_fonts, s.translation.target_language]).to_string(),
        }
    }
}

pub struct InplaceEngine {
    tracker: TextBlockTracker,
    detector: BlockDetector,
    fonts: Option<Arc<InstalledFontDatabase>>,
    snapshot: Option<Snapshot>,
    applied: Fingerprints,
    published: Vec<(u64, u64)>,
    new_translations: Vec<(String, String)>,
    missing_font_warned: HashSet<u64>,
    /// Content signatures of the fields handed out by the last `begin`, applied by `complete`.
    pending_signatures: HashMap<u64, Vec<u8>>,
}

impl Default for InplaceEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Уменьшенная яркость поля 32×8: сравнение показывает, изменился ли его текст.
fn content_signature(img: &RgbaImage, r: &Rect) -> Vec<u8> {
    let (x, y, w, h) = r.pixels(img.width(), img.height());
    let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let small = image::imageops::resize(&crop, 32, 8, FilterType::Triangle);
    small.pixels().map(|p| luma(&p.0)).collect()
}

fn signature_changed(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || a.is_empty() { return true; }
    let mad = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u32).sum::<u32>() as f32 / a.len() as f32;
    mad > 10.0
}

fn color_distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (*x as f32 - y as f32).powi(2)).sum::<f32>().sqrt()
}

/// Признаки поля для выбора шрифта. Пиксели CJK-текста не отличить от моноширинного (ровный шаг)
/// и не классифицировать по засечкам, поэтому, если OCR прочитал CJK, категория берётся
/// по языку оригинала: с засечками (минтё) при высоком контрасте штрихов, иначе — готика.
fn analysis_for(tracker: &TextBlockTracker, id: u64) -> Option<FontAnalysis> {
    let t = tracker.get(id)?;
    let mut a = tracker.analysis_with_peers(id)?;
    if t.detected_language.is_cjk() {
        a.category = if a.features.contrast >= 1.8 { FontCategory::CjkSerif } else { FontCategory::CjkSans };
        a.monospace = false;
    }
    Some(a)
}

fn preferences(s: &Settings) -> FontPreferences {
    let category_overrides = s.appearance.inplace.font_overrides.iter()
        .filter_map(|(k, v)| serde_json::from_value::<FontCategory>(serde_json::Value::String(k.clone())).ok().map(|c| (c, v.clone())))
        .collect();
    FontPreferences { category_overrides, preferred: s.appearance.inplace.preferred_fonts.clone() }
}

impl InplaceEngine {
    /// Встроенный реестр создаётся при первом выборе шрифта и кешируется на сеанс.
    pub fn new() -> Self {
        Self { tracker: TextBlockTracker::default(), detector: BlockDetector::default(), fonts: None, snapshot: None,
            applied: Fingerprints::default(), published: Vec::new(), new_translations: Vec::new(), missing_font_warned: HashSet::new(), pending_signatures: HashMap::new() }
    }

    pub fn with_fonts(db: impl Into<Arc<InstalledFontDatabase>>) -> Self {
        Self { fonts: Some(db.into()), ..Self::new() }
    }

    fn font_db(&mut self) -> Arc<InstalledFontDatabase> {
        self.fonts.get_or_insert_with(InstalledFontDatabase::bundled).clone()
    }

    /// Новый сеанс: поля и их шрифты забываются.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.snapshot = None;
        self.published.clear();
        self.missing_font_warned.clear();
    }

    /// Пользователь попросил определить шрифты заново: анализ повторится на следующем кадре.
    pub fn reanalyze_fonts(&mut self) {
        self.tracker.reset_fonts();
        self.missing_font_warned.clear();
    }

    pub fn tracker(&self) -> &TextBlockTracker {
        &self.tracker
    }

    /// Кадр: поиск и сопоставление полей, анализ шрифта новых полей, фон и типографика там,
    /// где они изменились. Возвращает задания OCR для полей с изменившимся содержимым.
    pub fn begin(&mut self, frame: DynamicImage, s: &Settings, now: Instant, force: bool) -> Vec<OcrJob> {
        // The frame is moved in: an RGBA8 capture is kept as is, with no copy.
        let image = frame.into_rgba8();
        self.pending_signatures.clear();
        let mask = BlockDetector::ink_mask(&image);
        self.detector.line_gap_factor = s.appearance.inplace.line_gap_factor;
        let detected = self.detector.detect_text_blocks(&image, &mask);
        let assignments = self.tracker.update(&detected, now);
        self.missing_font_warned.retain(|id| self.tracker.get(*id).is_some());
        let (fw, fh) = (image.width() as f32, image.height() as f32);
        let bg_changed = self.applied.background != Fingerprints::of(s).background;
        let mut jobs = Vec::new();
        let mut blocks = HashMap::new();
        for a in assignments {
            let d = &detected[a.detection];
            let Some(t) = self.tracker.get_mut(a.id) else { continue };
            let lh = d.line_height();
            // Содержимое поля изменилось — распознать заново.
            // Where the glyphs are: the OCR lines' boundary while it agrees with the detected block.
            let rect = t.ocr_bounds.filter(|b| b.iou(&d.rect) >= LINES_AGREE_IOU).and_then(|b| refine_bounds(&[b], &d.rect, lh)).unwrap_or(d.rect);
            t.refined_rect = (rect != d.rect).then_some(rect);
            let signature = content_signature(&image, &d.rect);
            if force || t.original_text.is_empty() || signature_changed(&signature, &t.content_signature) {
                let (x, y, w, h) = d.rect.expand((0.25 * lh).max(3.0), fw, fh).pixels(image.width(), image.height());
                jobs.push(OcrJob { id: a.id, rect: d.rect, origin: (x, y), image: DynamicImage::ImageRgba8(image::imageops::crop_imm(&image, x, y, w, h).to_image()) });
                // Remembered only when the field is completed (see `complete`): a failed or cancelled
                // recognition must leave the field due for the next scan.
                self.pending_signatures.insert(a.id, signature);
            }
            // Признаки шрифта — один раз за жизнь поля (до сброса идентичности или явного запроса).
            if t.font_analysis.is_none() {
                t.font_analysis = Some(FontClassifier.classify(&image, &mask, d));
            }
            // Фон — только если он заметно изменился (подвижная сцена) или сменили режим.
            let margin = BackgroundAnalyzer::margin(lh);
            let bg_sig = BackgroundAnalyzer::signature(&image, &rect, margin);
            let moved = (t.previous_rect.x - t.current_rect.x).abs() > 3.0 || (t.previous_rect.y - t.current_rect.y).abs() > 3.0;
            if t.background.is_none() || bg_changed || moved || t.background_signature.is_none_or(|old| color_distance(old, bg_sig) > 18.0) {
                let analysis = BackgroundAnalyzer.analyze(&image, &mask, &rect, margin);
                t.background = Some(BackgroundInpainter.render(&image, &mask, &rect, lh, analysis, &s.appearance.inplace, d.block_type));
                t.background_signature = Some(bg_sig);
                t.revision += 1;
            }
            // Типографика — для нового поля и при заметной смене размера или числа строк.
            let resized = (t.typography_rect.w - d.rect.w).abs() > 0.1 * d.rect.w || (t.typography_rect.h - d.rect.h).abs() > 0.1 * d.rect.h;
            // The lines the OCR engine found count while they still lie on the detected block.
            if t.ocr_lines.as_ref().is_some_and(|lines| !lines_agree(lines, &d.rect)) {
                t.ocr_lines = None;
                t.ocr_bounds = None;
                t.refined_rect = None;
                t.lines_note = "строки по детектору: геометрия OCR больше не совпадает с блоком".into();
            }
            let expected_lines = t.ocr_lines.as_ref().map_or(d.lines.len(), Vec::len);
            let relined = t.typography.as_ref().is_some_and(|e| e.lines as usize != expected_lines);
            if t.typography.is_none() || resized || relined {
                let bg = t.background.as_ref().map(|b| b.color).unwrap_or([0; 3]);
                t.typography = Some(TypographyEstimator.estimate_from(d, t.font_analysis.as_ref().unwrap(), bg, fw, t.ocr_lines.as_deref()));
                t.typography_rect = d.rect;
                t.revision += 1;
            }
            blocks.insert(a.id, d.clone());
        }
        if bg_changed { self.applied.background = Fingerprints::of(s).background; }
        // Поле уже распознано, но шрифт сброшен (явный запрос): выбрать сразу, без нового OCR.
        let pending: Vec<u64> = self.tracker.blocks().iter()
            .filter(|t| !t.font_selection_locked() && !t.original_text.is_empty() && t.font_analysis.is_some())
            .map(|t| t.id).collect();
        for id in pending { self.lock_font(id, s); }
        self.snapshot = Some(Snapshot { image, mask, blocks });
        jobs
    }

    /// What the OCR engine says about the lines of a field it has just read (`lines` in frame pixels,
    /// top to bottom). If they lie on the detected block, they decide the line structure — how many
    /// lines, the step between them, the alignment — and the typography is estimated again; if they
    /// do not, the detector's lines stay and the inspector says why. No lines (an engine without
    /// geometry) changes nothing.
    pub fn observe_ocr_lines(&mut self, id: u64, lines: &[Rect], s: &Settings) {
        if lines.is_empty() { return; }
        let Some(snapshot) = &self.snapshot else { return };
        let Some(d) = snapshot.blocks.get(&id) else { return };
        let frame_w = snapshot.image.width() as f32;
        let agree = lines_agree(lines, &d.rect);
        let Some(t) = self.tracker.get_mut(id) else { return };
        if !agree {
            t.ocr_lines = None;
            if t.ocr_bounds.take().is_some() || t.refined_rect.take().is_some() { t.background_signature = None; }
            let on = lines.iter().skip(1).fold(lines[0], |u, l| u.union(l));
            t.lines_note = format!("строки по детектору: геометрия OCR ({} стр.) не совпала с блоком (IoU {:.2})", lines.len(), on.iou(&d.rect));
            tracing::debug!(target: "inplace.lines", block_id = id, ocr_lines = lines.len(), detected = d.lines.len(), "OCR geometry does not match the block; detector lines kept");
            return;
        }
        t.ocr_lines = Some(lines.to_vec());
        t.lines_note = if lines.len() == d.lines.len() { format!("строк: {} (OCR и детектор согласны)", lines.len()) }
            else { format!("строк: {} по OCR, детектор насчитал {}", lines.len(), d.lines.len()) };
        if lines.len() != d.lines.len() {
            tracing::debug!(target: "inplace.lines", block_id = id, ocr_lines = lines.len(), detected = d.lines.len(), "OCR and the detector disagree on the lines; OCR decides");
        }
        // The boundary of the glyphs: the lines' union, if it stays close to the detected block.
        let refined = refine_bounds(lines, &d.rect, d.line_height());
        t.ocr_bounds = refined;
        let rect = refined.unwrap_or(d.rect);
        if refined.is_none() { t.lines_note.push_str("; граница блока по детектору (OCR расходится больше допуска)"); }
        if t.refined_rect != refined.filter(|r| *r != d.rect) {
            t.refined_rect = refined.filter(|r| *r != d.rect);
            // The plate follows the glyphs: it is restored again for the new boundary.
            let margin = BackgroundAnalyzer::margin(d.line_height());
            let analysis = BackgroundAnalyzer.analyze(&snapshot.image, &snapshot.mask, &rect, margin);
            t.background = Some(BackgroundInpainter.render(&snapshot.image, &snapshot.mask, &rect, d.line_height(), analysis, &s.appearance.inplace, d.block_type));
            t.background_signature = Some(BackgroundAnalyzer::signature(&snapshot.image, &rect, margin));
            t.revision += 1;
            if refined.is_some() { t.lines_note.push_str("; граница блока по OCR"); }
        }
        if let (Some(analysis), Some(old)) = (t.font_analysis.as_ref(), t.typography.as_ref()) {
            let bg = t.background.as_ref().map(|b| b.color).unwrap_or([0; 3]);
            let estimate = TypographyEstimator.estimate_from(d, analysis, bg, frame_w, t.ocr_lines.as_deref());
            if estimate != *old {
                t.typography = Some(estimate);
                t.typography_rect = d.rect;
                t.revision += 1;
            }
        }
    }

    /// Однократный выбор шрифта поля по признакам первого анализа.
    fn lock_font(&mut self, id: u64, s: &Settings) {
        if self.tracker.get(id).is_none_or(|t| t.font_selection_locked()) { return; }
        let db = self.font_db();
        let Some(analysis) = analysis_for(&self.tracker, id) else { return };
        let Some(t) = self.tracker.get_mut(id) else { return };
        let selected = FontMatcher.select_font(&analysis, Script::from_lang(&s.translation.target_language), t.block_type, &db, &preferences(s));
        if analysis.category == FontCategory::Unknown {
            tracing::warn!(target: "inplace.font", block_id = id, fallback = %selected.family, "font category could not be determined");
        }
        tracing::debug!(target: "inplace.font", block_id = id, block_type = ?t.block_type, script = ?t.detected_language,
            category = ?analysis.category, confidence = analysis.confidence, selected_fallback = %selected.family, "font analysis");
        if selected.generic {
            if self.missing_font_warned.insert(id) {
                tracing::warn!(target: "inplace.font", block_id = id, script = ?Script::from_lang(&s.translation.target_language), "no bundled font has full glyph coverage");
            }
            return;
        }
        tracing::debug!(target: "inplace.font", block_id = id, family = %selected.family, category = ?selected.category, confidence = selected.confidence, "font selected");
        t.font = Some(selected);
        t.revision += 1;
    }

    /// Перевод, уже известный для поля: если распознан тот же текст, повторный перевод не нужен.
    pub fn cached_translation(&self, id: u64, original: &str) -> Option<String> {
        let t = self.tracker.get(id)?;
        (!t.translated_text.is_empty() && crate::text::similarity(original, &t.original_text) >= 0.92).then(|| t.translated_text.clone())
    }

    /// Результат OCR и перевода поля. `None` — распознать не удалось: прежнее состояние поля
    /// сохраняется (гистерезис), новое поле остаётся скрытым.
    pub fn complete(&mut self, id: u64, result: Option<(String, String)>, s: &Settings) {
        let Some(t) = self.tracker.get_mut(id) else { return };
        // The field was handled (recognised, or recognised as empty): remember what it looked like.
        if let Some(signature) = self.pending_signatures.remove(&id) { t.content_signature = signature; }
        let Some((original, translation)) = result else { return };
        if original != t.original_text || translation != t.translated_text {
            if translation != t.translated_text { self.new_translations.push((original.clone(), translation.clone())); }
            t.original_text = original;
            t.translated_text = translation;
            t.detected_language = Script::of_text(&t.original_text);
            t.revision += 1;
        }
        // Первое удачное распознавание поля: выбор шрифта и его блокировка.
        self.lock_font(id, s);
    }

    /// Поля для отрисовки. `None`, если с прошлой отправки ничего видимого не изменилось.
    pub fn finish(&mut self, s: &Settings) -> Option<InplaceFrame> {
        self.apply_settings(s);
        let font_db = self.font_db();
        let snap = self.snapshot.as_ref()?;
        let mut blocks = Vec::new();
        let mut undrawable = Vec::new();
        for t in self.tracker.blocks() {
            // Пропуск одного кадра не гасит поле: короткие сбои распознавания не мигают.
            if t.misses > 1 || t.translated_text.is_empty() { continue; }
            let (Some(font), Some(est), Some(bg)) = (t.font.as_ref(), t.typography.as_ref(), t.background.as_ref()) else {
                // Not lost: a field with a translation that cannot be drawn in place is handed on.
                if t.font.is_none() && t.typography.is_some() {
                    undrawable.push(UndrawableField { id: t.id, original: t.original_text.clone(), translation: t.translated_text.clone(),
                        reason: "нет встроенного шрифта с нужными глифами" });
                }
                continue;
            };
            let mut style = TypographyEstimator.resolve(est, font, &s.appearance.inplace);
            // Старый ручной шрифт из системного списка не должен попасть в inplace.
            if style.font_family != font.family && !font_db.find(&style.font_family).is_some_and(|f| f.covers(Script::from_lang(&s.translation.target_language).fontconfig_lang())) {
                tracing::warn!(target: "inplace.font", block_id = t.id, family = %style.font_family, "manual font lacks bundled glyph coverage; using selected font");
                style.font_family = font.family.clone();
            }
            blocks.push(InplaceBlock {
                id: t.id, block_type: t.block_type, text_rect: t.text_rect(),
                original: t.original_text.clone(), translation: t.translated_text.clone(),
                script: Script::from_lang(&s.translation.target_language), font: font.clone(), style, background: bg.clone(), lines_note: t.lines_note.clone(), revision: t.revision,
            });
        }
        // Undrawable fields are part of what was published (marked by the top bit of the id).
        let revision_of = |id: u64| self.tracker.get(id).map_or(0, |t| t.revision);
        let state: Vec<(u64, u64)> = blocks.iter().map(|b| (b.id, b.revision)).chain(undrawable.iter().map(|u| (u.id | 1 << 63, revision_of(u.id)))).collect();
        if state == self.published { return None; }
        self.published = state;
        Some(InplaceFrame { frame: (snap.image.width(), snap.image.height()), blocks, undrawable, new_translations: std::mem::take(&mut self.new_translations) })
    }

    /// Настройки поменялись без нового кадра: перестроить то, что от них зависит.
    pub fn restyle(&mut self, s: &Settings) -> Option<InplaceFrame> {
        if self.applied == Fingerprints::of(s) || self.snapshot.is_none() { return None; }
        self.finish(s)
    }

    fn apply_settings(&mut self, s: &Settings) {
        let now = Fingerprints::of(s);
        if now == self.applied { return; }
        let ids: Vec<u64> = self.tracker.blocks().iter().map(|t| t.id).collect();
        // Стиль: ручные значения применяются ко всем полям, шрифт не перевыбирается.
        if now.style != self.applied.style {
            for id in &ids { if let Some(t) = self.tracker.get_mut(*id) { t.revision += 1; } }
        }
        // Фон: другой алгоритм по последнему кадру; шрифт и типографика не трогаются.
        if now.background != self.applied.background
            && let Some(snap) = &self.snapshot {
                for (id, d) in &snap.blocks {
                    if let Some(t) = self.tracker.get_mut(*id) {
                        let margin = BackgroundAnalyzer::margin(d.line_height());
                        let analysis = BackgroundAnalyzer.analyze(&snap.image, &snap.mask, &d.rect, margin);
                        t.background = Some(BackgroundInpainter.render(&snap.image, &snap.mask, &d.rect, d.line_height(), analysis, &s.appearance.inplace, d.block_type));
                        t.revision += 1;
                    }
                }
            }
        // Замены и предпочтения шрифтов или язык перевода — явный запрос нового выбора.
        if now.fonts != self.applied.fonts && !self.applied.fonts.is_empty() {
            self.missing_font_warned.clear();
            let db = self.font_db();
            let target = Script::from_lang(&s.translation.target_language);
            for id in &ids {
                let analysis = analysis_for(&self.tracker, *id);
                if let Some(t) = self.tracker.get_mut(*id)
                    && let (Some(a), true) = (analysis, t.font.is_some()) {
                        let selected = FontMatcher.select_font(&a, target, t.block_type, &db, &preferences(s));
                        t.font = if selected.generic { None } else { Some(selected) };
                        t.revision += 1;
                    }
            }
        }
        self.applied = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::font_database::tests::SAMPLE;
    use crate::layout::font_database::InstalledFontDatabase;
    use crate::layout::testing::*;
    use crate::settings::{InplaceBackgroundMode, PropertyMode};
    use std::time::Duration;

    fn scene(shift: u32, name_color: [u8; 3]) -> DynamicImage {
        let mut img = canvas(1200, 400, [25, 30, 40]);
        draw_line(&mut img, 100 + shift, 100, 8, 14, 4, 22, 3, name_color, false);
        draw_line(&mut img, 100 + shift, 140, 40, 14, 4, 22, 3, [240, 240, 240], true);
        draw_line(&mut img, 100 + shift, 172, 30, 14, 4, 22, 3, [240, 240, 240], true);
        dynamic(img)
    }

    fn settings() -> Settings {
        { let mut value = Settings::default(); value.translation.target_language = "ru".into(); value.display_mode = crate::settings::TranslationDisplayMode::Inplace; value }
    }

    /// A character name one blank line above its dialogue, all one colour: the grouping alone makes one block.
    /// `lines` dialogue lines, the name optional, everything moved right by `shift` px.
    fn glued(lines: usize, name: bool, shift: u32) -> DynamicImage {
        let mut img = canvas(1200, 500, [25, 30, 40]);
        if name { draw_prose_line(&mut img, 100 + shift, 100, 8, 14, 4, 32, 3, [240, 240, 240]); }
        for i in 0..lines as u32 { draw_prose_line(&mut img, 100 + shift, 162 + 46 * i, 34 - 6 * i as usize, 14, 4, 32, 3, [240, 240, 240]); }
        dynamic(img)
    }

    /// Reads every job as the text of its field and translates it; returns the published frame, the jobs' ids and the new pairs.
    fn read_all(e: &mut InplaceEngine, frame: DynamicImage, s: &Settings, now: Instant) -> (Vec<u64>, Option<InplaceFrame>) {
        let jobs = e.begin(frame, s, now, false);
        let ids: Vec<u64> = jobs.iter().map(|j| j.id).collect();
        for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), s); }
        (ids, e.finish(s))
    }

    #[test]
    fn a_split_gives_two_fields_with_their_own_plates_and_stable_ids() {
        let s = settings();
        let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
        let (jobs, frame) = read_all(&mut e, glued(2, true, 0), &s, t0);
        assert_eq!(jobs.len(), 2, "the name and the dialogue are read separately");
        let blocks = frame.expect("both fields").blocks;
        let by_type = |t: TextBlockType| blocks.iter().find(|b| b.block_type == t).unwrap_or_else(|| panic!("{t:?} in {:?}", blocks.iter().map(|b| b.block_type).collect::<Vec<_>>())).clone();
        let (name, dialogue) = (by_type(TextBlockType::CharacterName), by_type(TextBlockType::Dialogue));
        // Each field has the plate of its own boundary, not the merged parent's: the name's plate stops above the dialogue.
        assert!(name.background.rect.bottom() <= dialogue.text_rect.y, "{:?} vs {:?}", name.background.rect, dialogue.text_rect);
        assert!(dialogue.background.rect.y >= name.text_rect.bottom(), "{:?} vs {:?}", dialogue.background.rect, name.text_rect);
        // The same picture again: the same fields, nothing re-read, nothing sent again.
        let (again, frame) = read_all(&mut e, glued(2, true, 0), &s, t0 + Duration::from_millis(200));
        assert!(again.is_empty() && frame.is_none(), "stable ids: no flicker, no repeated work");
    }

    #[test]
    fn changing_one_field_keeps_the_font_lock_of_its_neighbour() {
        let s = settings();
        let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
        let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
        let before = first.expect("fields").blocks;
        let name_before = before.iter().find(|b| b.block_type == TextBlockType::CharacterName).unwrap().clone();
        // The dialogue gets a third line; the name is untouched.
        let (jobs, frame) = read_all(&mut e, glued(3, true, 0), &s, t0 + Duration::from_millis(300));
        assert_eq!(jobs.len(), 1, "only the changed dialogue is read again");
        assert!(jobs[0] != name_before.id);
        let name_after = frame.expect("the dialogue changed").blocks.into_iter().find(|b| b.id == name_before.id).expect("the name keeps its id");
        assert_eq!(name_after.font, name_before.font, "the font chosen for the name stays");
        assert_eq!(name_after.translation, name_before.translation);
    }

    #[test]
    fn a_field_that_blinks_out_keeps_its_id_through_the_grace_period() {
        let s = settings();
        let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
        let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
        let name_id = first.expect("fields").blocks.iter().find(|b| b.block_type == TextBlockType::CharacterName).unwrap().id;
        // One scan without the name: it is still shown.
        let (_, frame) = read_all(&mut e, glued(2, false, 0), &s, t0 + Duration::from_millis(200));
        assert!(frame.is_none_or(|f| f.blocks.iter().any(|b| b.id == name_id)), "a short miss does not drop the field");
        // It comes back: the same id, read from memory (no new read).
        let (again, frame) = read_all(&mut e, glued(2, true, 0), &s, t0 + Duration::from_millis(400));
        assert!(again.is_empty(), "{again:?}");
        assert!(frame.is_none_or(|f| f.blocks.iter().any(|b| b.id == name_id)));
    }

    #[test]
    fn a_move_alone_changes_the_geometry_not_the_translations() {
        let s = settings();
        let (mut e, t0) = (InplaceEngine::with_fonts(InstalledFontDatabase::bundled()), Instant::now());
        let (_, first) = read_all(&mut e, glued(2, true, 0), &s, t0);
        assert_eq!(first.expect("fields").new_translations.len(), 2, "the first reading is new text");
        // The scene moves 6 px right: the fields follow, and since the text is the same no translation is new,
        // so neither the cache nor the history has anything to write.
        let (jobs, frame) = read_all(&mut e, glued(2, true, 6), &s, t0 + Duration::from_millis(200));
        assert!(jobs.is_empty(), "the text did not change");
        if let Some(frame) = frame {
            assert!(frame.new_translations.is_empty(), "{:?}", frame.new_translations);
            assert!(frame.blocks.iter().all(|b| b.text_rect.x >= 100.0 + 5.0), "the fields moved with the scene");
        }
    }

    /// One block of three lines, as dialogue text.
    fn three_lines() -> DynamicImage {
        let mut img = canvas(1200, 400, [25, 30, 40]);
        for y in [100, 134, 168] { draw_line(&mut img, 100, y, 30, 14, 4, 22, 3, [240, 240, 240], false); }
        dynamic(img)
    }

    fn halves(r: &Rect) -> [Rect; 2] {
        [Rect::new(r.x, r.y, r.w, r.h / 2.0 - 2.0), Rect::new(r.x, r.y + r.h / 2.0, r.w, r.h / 2.0)]
    }

    #[test]
    fn ocr_lines_decide_the_line_structure_when_they_lie_on_the_block() {
        let s = settings();
        let (frame, t0) = (three_lines(), Instant::now());
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(frame.clone(), &s, t0, false);
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        // The detector sees three lines; the engine that read the text found two.
        e.observe_ocr_lines(job.id, &halves(&job.rect), &s);
        e.complete(job.id, Some(("a b".into(), "в г".into())), &s);
        let block = e.finish(&s).expect("the field").blocks.remove(0);
        assert_eq!(block.style.source_lines, 2, "the OCR line count wins");
        assert!(block.lines_note.contains("2 по OCR") && block.lines_note.contains("детектор насчитал 3"), "{}", block.lines_note);
        // The decision is stable: the same picture again neither re-reads the field nor changes it.
        let again = e.begin(frame, &s, t0 + Duration::from_millis(200), false);
        assert!(again.is_empty());
        assert!(e.finish(&s).is_none(), "an OCR-decided structure must not make the field flap");
    }

    #[test]
    fn ocr_lines_that_agree_with_the_detector_change_nothing_but_the_note() {
        let s = settings();
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(three_lines(), &s, Instant::now(), false);
        let job = &jobs[0];
        let third = job.rect.h / 3.0;
        let lines: Vec<Rect> = (0..3).map(|i| Rect::new(job.rect.x, job.rect.y + third * i as f32, job.rect.w, third - 2.0)).collect();
        e.observe_ocr_lines(job.id, &lines, &s);
        e.complete(job.id, Some(("a b c".into(), "в г д".into())), &s);
        let block = e.finish(&s).expect("the field").blocks.remove(0);
        assert_eq!(block.style.source_lines, 3);
        assert!(block.lines_note.contains("согласны"), "{}", block.lines_note);
    }

    /// The OCR lines of `job.rect` with every side moved by `by` pixels (negative: inwards).
    fn lines_moved_by(rect: &Rect, by: f32) -> [Rect; 1] {
        [Rect::new(rect.x - by, rect.y - by, rect.w + 2.0 * by, rect.h + 2.0 * by)]
    }

    fn block_with(frame: DynamicImage, lines: impl Fn(&Rect) -> Vec<Rect>) -> (InplaceBlock, Rect) {
        let s = settings();
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(frame, &s, Instant::now(), false);
        let job = &jobs[0];
        e.observe_ocr_lines(job.id, &lines(&job.rect), &s);
        e.complete(job.id, Some(("a".into(), "б".into())), &s);
        (e.finish(&s).expect("the field").blocks.remove(0), job.rect)
    }

    #[test]
    fn the_boundary_follows_the_ocr_lines_when_they_are_close_to_the_block() {
        let (plain, detected) = block_with(three_lines(), |_| Vec::new());
        assert_eq!(plain.text_rect, detected, "no OCR geometry: the detector's boundary");
        // The engine's lines are 4 px tighter on every side than the detected block.
        let (refined, _) = block_with(three_lines(), |r| lines_moved_by(r, -4.0).to_vec());
        assert!(refined.text_rect.w < detected.w - 5.0 && refined.text_rect.h < detected.h - 5.0, "{:?} vs {:?}", refined.text_rect, detected);
        assert!((refined.text_rect.x - (detected.x + 3.0)).abs() < 0.01, "tight box plus one pixel of margin: {:?}", refined.text_rect);
        assert!(refined.lines_note.contains("граница блока по OCR"), "{}", refined.lines_note);
        // The plate follows: it was restored again for the new boundary, not left at the old size.
        assert!(refined.background.rect.w < plain.background.rect.w && refined.background.rect.h < plain.background.rect.h,
            "{:?} vs {:?}", refined.background.rect, plain.background.rect);
    }

    #[test]
    fn a_boundary_that_cuts_off_lines_is_not_taken() {
        // The engine read only the top two of three lines: its boundary would leave the third one
        // uncovered under the translation, so the detector's boundary stays.
        let (block, detected) = block_with(three_lines(), |r| vec![Rect::new(r.x, r.y, r.w, r.h * 0.66)]);
        assert_eq!(block.text_rect, detected, "{}", block.lines_note);
        assert!(block.lines_note.contains("граница блока по детектору"), "{}", block.lines_note);
    }

    #[test]
    fn a_refined_boundary_is_stable_between_scans() {
        let s = settings();
        let (frame, t0) = (three_lines(), Instant::now());
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let job = e.begin(frame.clone(), &s, t0, false).remove(0);
        e.observe_ocr_lines(job.id, &lines_moved_by(&job.rect, -4.0), &s);
        e.complete(job.id, Some(("a".into(), "б".into())), &s);
        let first = e.finish(&s).expect("the field").blocks.remove(0);
        // The same picture is detected again (with the old, wider boundary): the refinement stays.
        assert!(e.begin(frame.clone(), &s, t0 + Duration::from_millis(200), false).is_empty());
        assert!(e.finish(&s).is_none(), "nothing changed, nothing is sent again");
        // A forced re-read gives back the same boundary and plate.
        let again = e.begin(frame, &s, t0 + Duration::from_millis(400), true).remove(0);
        e.observe_ocr_lines(again.id, &lines_moved_by(&again.rect, -4.0), &s);
        e.complete(again.id, Some(("a".into(), "б".into())), &s);
        let second = e.finish(&s).map_or(first.clone(), |f| f.blocks[0].clone());
        assert_eq!((second.text_rect, second.background.rect), (first.text_rect, first.background.rect));
    }

    #[test]
    fn ocr_lines_somewhere_else_do_not_override_the_detector() {
        let s = settings();
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let jobs = e.begin(three_lines(), &s, Instant::now(), false);
        let job = &jobs[0];
        let elsewhere = [Rect::new(job.rect.x, job.rect.y + 250.0, job.rect.w, 20.0)];
        e.observe_ocr_lines(job.id, &elsewhere, &s);
        e.complete(job.id, Some(("a".into(), "б".into())), &s);
        let block = e.finish(&s).expect("the field").blocks.remove(0);
        assert_eq!(block.style.source_lines, 3, "the detector's lines stay");
        assert!(block.lines_note.contains("не совпала"), "{}", block.lines_note);
        // An engine without geometry changes nothing at all.
        e.observe_ocr_lines(job.id, &[], &s);
    }

    fn run(e: &mut InplaceEngine, frame: &DynamicImage, s: &Settings, now: Instant) -> (usize, Option<InplaceFrame>) {
        let jobs = e.begin(frame.clone(), s, now, false);
        let n = jobs.len();
        for j in jobs { e.complete(j.id, Some((format!("text {}", j.id), format!("перевод {}", j.id))), s); }
        (n, e.finish(s))
    }

    /// Where the time goes on a 2560×1080 frame with six lines of text:
    /// cargo test --release -p lipa-core layout::engine::tests::stages -- --ignored --nocapture
    #[test]
    #[ignore]
    fn stages() {
        let mut img = canvas(2560, 1080, [25, 30, 40]);
        for row in 0..6 { draw_line(&mut img, 200, 120 + row * 150, 60, 14, 4, 22, 3, [240, 240, 240], row % 2 == 0); }
        let dynamic_frame = dynamic(img.clone());
        let time = |name: &str, runs: u32, mut f: Box<dyn FnMut() + '_>| {
            let t = Instant::now();
            for _ in 0..runs { f(); }
            println!("{name:<28} {:>9.3} ms", t.elapsed().as_secs_f64() * 1000.0 / runs as f64);
        };
        time("to_rgba8 copy", 20, Box::new(|| { std::hint::black_box(dynamic_frame.to_rgba8()); }));
        time("ink_mask", 20, Box::new(|| { std::hint::black_box(BlockDetector::ink_mask(&img)); }));
        let mask = BlockDetector::ink_mask(&img);
        time("detect_text_blocks", 20, Box::new(|| { std::hint::black_box(BlockDetector::default().detect_text_blocks(&img, &mask)); }));
        let s = settings();
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let t0 = Instant::now();
        for j in e.begin(dynamic_frame.clone(), &s, t0, false) { e.complete(j.id, Some(("a".into(), "б".into())), &s); }
        let mut i = 0u64;
        time("begin (stable, whole)", 20, Box::new(|| { i += 1; e.begin(dynamic_frame.clone(), &s, t0 + Duration::from_millis(100 * i), false); }));
    }

    /// cargo test --release -p lipa-core layout::engine::tests::speed -- --ignored --nocapture
    #[test]
    #[ignore]
    fn speed() {
        let mut img = canvas(2560, 1080, [25, 30, 40]);
        for row in 0..6 { draw_line(&mut img, 200, 120 + row * 150, 60, 14, 4, 22, 3, [240, 240, 240], row % 2 == 0); }
        let frame = dynamic(img);
        let s = settings();
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
        let t0 = Instant::now();
        let jobs = e.begin(frame.clone(), &s, t0, false);
        for j in jobs { e.complete(j.id, Some(("a".into(), "б".into())), &s); }
        println!("first frame (detect + classify + background + fonts): {:?}", t0.elapsed());
        let runs = 10u32;
        let t1 = Instant::now();
        for i in 0..runs { e.begin(frame.clone(), &s, t0 + Duration::from_millis(100 * (i as u64 + 1)), false); e.finish(&s); }
        println!("stable frame: {:?}", t1.elapsed() / runs);
    }

    #[test]
    fn font_is_selected_once_per_field_and_stays_locked() {
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
        let s = settings();
        let t0 = Instant::now();
        let (jobs, out) = run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
        assert_eq!(jobs, 2, "name and dialogue are recognised separately");
        let out = out.unwrap();
        assert_eq!(out.blocks.len(), 2);
        let fonts: HashMap<u64, String> = out.blocks.iter().map(|b| (b.id, b.font.family.clone())).collect();

        // Сдвиг на 2 px и новый перевод того же поля: id и шрифт те же, OCR не повторяется для неизменного.
        let jobs = e.begin(scene(2, [255, 200, 60]), &s, t0 + Duration::from_millis(500), false);
        assert!(jobs.is_empty(), "same content → no OCR");
        let dialogue = out.blocks.iter().find(|b| b.style.source_lines == 2).unwrap().id;
        e.complete(dialogue, Some(("text changed".into(), "гораздо более длинный перевод этой реплики".into())), &s);
        let again = e.finish(&s).unwrap();
        for b in &again.blocks { assert_eq!(fonts[&b.id], b.font.family, "font locked for field {}", b.id); }
        assert_eq!(again.new_translations.len(), 1);
        assert!(e.finish(&s).is_none(), "nothing changed → nothing retransmitted");
    }

    #[test]
    fn background_mode_change_keeps_fonts_and_typography() {
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
        let mut s = settings();
        let first = run(&mut e, &scene(0, [255, 200, 60]), &s, Instant::now()).1.unwrap();
        s.appearance.inplace.background_mode = InplaceBackgroundMode::TransparentOutline;
        let restyled = e.restyle(&s).unwrap();
        for (a, b) in first.blocks.iter().zip(&restyled.blocks) {
            assert_eq!((a.id, &a.font, &a.style), (b.id, &b.font, &b.style));
            assert_eq!(b.background.mode, super::super::background::BackgroundRenderMode::Transparent);
        }
        // Ручной трекинг меняет только его.
        s.appearance.inplace.letter_spacing = PropertyMode::Manual(2.0);
        let spaced = e.restyle(&s).unwrap();
        for (a, b) in restyled.blocks.iter().zip(&spaced.blocks) {
            assert_eq!(b.style.letter_spacing, 2.0);
            assert_eq!((&a.font, a.style.alignment, a.style.font_weight), (&b.font, b.style.alignment, b.style.font_weight));
        }
    }

    #[test]
    fn failed_recognition_keeps_the_previous_state() {
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
        let s = settings();
        let t0 = Instant::now();
        let out = run(&mut e, &scene(0, [255, 200, 60]), &s, t0).1.unwrap();
        let jobs = e.begin(scene(0, [255, 200, 60]), &s, t0 + Duration::from_millis(300), true);
        for j in jobs { e.complete(j.id, None, &s); }
        assert!(e.finish(&s).is_none(), "the stable fields stay as they were");
        assert_eq!(e.tracker().blocks().iter().filter(|t| !t.translated_text.is_empty()).count(), out.blocks.len());
    }

    #[test]
    fn a_changed_field_stays_due_until_it_is_completed() {
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
        let s = settings();
        let t0 = Instant::now();
        run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
        // The name changes colour and the dialogue is replaced by a different image; recognition
        // does not complete (it failed, or the tick was cancelled).
        let changed = scene(0, [60, 200, 255]);
        let first = e.begin(changed.clone(), &s, t0 + Duration::from_millis(200), false);
        assert!(!first.is_empty(), "the changed field needs recognising");
        let again = e.begin(changed.clone(), &s, t0 + Duration::from_millis(400), false);
        assert_eq!(again.len(), first.len(), "an unfinished field is handed out again, not forgotten");
        // Completing it (even as 'nothing recognised') settles it.
        for j in again { e.complete(j.id, None, &s); }
        assert!(e.begin(changed, &s, t0 + Duration::from_millis(600), false).is_empty());
    }

    #[test]
    fn explicit_reanalysis_selects_fonts_again() {
        let mut e = InplaceEngine::with_fonts(InstalledFontDatabase::parse(SAMPLE));
        let s = settings();
        let t0 = Instant::now();
        run(&mut e, &scene(0, [255, 200, 60]), &s, t0);
        e.reanalyze_fonts();
        assert!(e.tracker().blocks().iter().all(|t| !t.font_selection_locked()));
        let jobs = e.begin(scene(0, [255, 200, 60]), &s, t0 + Duration::from_millis(200), false);
        assert!(jobs.is_empty(), "text is unchanged: no OCR needed");
        assert!(e.tracker().blocks().iter().all(|t| t.font_selection_locked()), "fonts chosen again from the last analysis");
        assert!(e.finish(&s).is_some());
    }
}
