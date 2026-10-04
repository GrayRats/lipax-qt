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

/// Поле, текст которого нужно распознать.
pub struct OcrJob {
    pub id: u64,
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
    /// Меняется вместе с видимым состоянием поля.
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InplaceFrame {
    pub frame: (u32, u32),
    pub blocks: Vec<InplaceBlock>,
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
        let i = &s.inplace;
        let style = serde_json::json!([i.font_family, i.font_size, i.font_weight, i.italic, i.line_height, i.letter_spacing,
            i.alignment, i.wrap_mode, i.text_color, i.padding, i.minimum_font_size, i.maximum_font_size, i.allow_condensed_fallback]).to_string();
        Self {
            style,
            background: serde_json::json!([i.background_mode, i.fill_color, i.fill_opacity, i.padding_x, i.padding_y, i.extra_margin, i.corner_radius, i.outline_color, i.outline_width, i.shadow, i.text_opacity]).to_string(),
            fonts: serde_json::json!([i.font_overrides, i.preferred_fonts, s.target_lang]).to_string(),
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
fn analysis_for(t: &super::tracker::TrackedTextBlock) -> Option<FontAnalysis> {
    let mut a = t.font_analysis.clone()?;
    if t.detected_language.is_cjk() {
        a.category = if a.features.contrast >= 1.8 { FontCategory::CjkSerif } else { FontCategory::CjkSans };
        a.monospace = false;
    }
    Some(a)
}

fn preferences(s: &Settings) -> FontPreferences {
    let category_overrides = s.inplace.font_overrides.iter()
        .filter_map(|(k, v)| serde_json::from_value::<FontCategory>(serde_json::Value::String(k.clone())).ok().map(|c| (c, v.clone())))
        .collect();
    FontPreferences { category_overrides, preferred: s.inplace.preferred_fonts.clone() }
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
            let signature = content_signature(&image, &d.rect);
            if force || t.original_text.is_empty() || signature_changed(&signature, &t.content_signature) {
                let (x, y, w, h) = d.rect.expand((0.25 * lh).max(3.0), fw, fh).pixels(image.width(), image.height());
                jobs.push(OcrJob { id: a.id, image: DynamicImage::ImageRgba8(image::imageops::crop_imm(&image, x, y, w, h).to_image()) });
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
            let bg_sig = BackgroundAnalyzer::signature(&image, &d.rect, margin);
            let moved = (t.previous_rect.x - t.current_rect.x).abs() > 3.0 || (t.previous_rect.y - t.current_rect.y).abs() > 3.0;
            if t.background.is_none() || bg_changed || moved || t.background_signature.is_none_or(|old| color_distance(old, bg_sig) > 18.0) {
                let analysis = BackgroundAnalyzer.analyze(&image, &mask, &d.rect, margin);
                t.background = Some(BackgroundInpainter.render(&image, &mask, &d.rect, lh, analysis, &s.inplace, d.block_type));
                t.background_signature = Some(bg_sig);
                t.revision += 1;
            }
            // Типографика — для нового поля и при заметной смене размера или числа строк.
            let resized = (t.typography_rect.w - d.rect.w).abs() > 0.1 * d.rect.w || (t.typography_rect.h - d.rect.h).abs() > 0.1 * d.rect.h;
            let relined = t.typography.as_ref().is_some_and(|e| e.lines as usize != d.lines.len());
            if t.typography.is_none() || resized || relined {
                let bg = t.background.as_ref().map(|b| b.color).unwrap_or([0; 3]);
                t.typography = Some(TypographyEstimator.estimate(d, t.font_analysis.as_ref().unwrap(), bg, fw));
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

    /// Однократный выбор шрифта поля по признакам первого анализа.
    fn lock_font(&mut self, id: u64, s: &Settings) {
        if self.tracker.get(id).is_none_or(|t| t.font_selection_locked()) { return; }
        let db = self.font_db();
        let Some(t) = self.tracker.get_mut(id) else { return };
        let Some(analysis) = analysis_for(t) else { return };
        let selected = FontMatcher.select_font(&analysis, Script::from_lang(&s.target_lang), t.block_type, &db, &preferences(s));
        if analysis.category == FontCategory::Unknown {
            tracing::warn!(target: "inplace.font", block_id = id, fallback = %selected.family, "font category could not be determined");
        }
        tracing::debug!(target: "inplace.font", block_id = id, block_type = ?t.block_type, script = ?t.detected_language,
            category = ?analysis.category, confidence = analysis.confidence, selected_fallback = %selected.family, "font analysis");
        if selected.generic {
            if self.missing_font_warned.insert(id) {
                tracing::warn!(target: "inplace.font", block_id = id, script = ?Script::from_lang(&s.target_lang), "no bundled font has full glyph coverage");
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
        for t in self.tracker.blocks() {
            // Пропуск одного кадра не гасит поле: короткие сбои распознавания не мигают.
            if t.misses > 1 || t.translated_text.is_empty() { continue; }
            let (Some(font), Some(est), Some(bg)) = (t.font.as_ref(), t.typography.as_ref(), t.background.as_ref()) else { continue };
            let mut style = TypographyEstimator.resolve(est, font, &s.inplace);
            // Старый ручной шрифт из системного списка не должен попасть в inplace.
            if style.font_family != font.family && !font_db.find(&style.font_family).is_some_and(|f| f.covers(Script::from_lang(&s.target_lang).fontconfig_lang())) {
                tracing::warn!(target: "inplace.font", block_id = t.id, family = %style.font_family, "manual font lacks bundled glyph coverage; using selected font");
                style.font_family = font.family.clone();
            }
            blocks.push(InplaceBlock {
                id: t.id, block_type: t.block_type, text_rect: t.current_rect,
                original: t.original_text.clone(), translation: t.translated_text.clone(),
                script: Script::from_lang(&s.target_lang), font: font.clone(), style, background: bg.clone(), revision: t.revision,
            });
        }
        let state: Vec<(u64, u64)> = blocks.iter().map(|b| (b.id, b.revision)).collect();
        if state == self.published { return None; }
        self.published = state;
        Some(InplaceFrame { frame: (snap.image.width(), snap.image.height()), blocks, new_translations: std::mem::take(&mut self.new_translations) })
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
                        t.background = Some(BackgroundInpainter.render(&snap.image, &snap.mask, &d.rect, d.line_height(), analysis, &s.inplace, d.block_type));
                        t.revision += 1;
                    }
                }
            }
        // Замены и предпочтения шрифтов или язык перевода — явный запрос нового выбора.
        if now.fonts != self.applied.fonts && !self.applied.fonts.is_empty() {
            self.missing_font_warned.clear();
            let db = self.font_db();
            let target = Script::from_lang(&s.target_lang);
            for id in &ids {
                if let Some(t) = self.tracker.get_mut(*id)
                    && let (Some(a), true) = (analysis_for(t), t.font.is_some()) {
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
        Settings { target_lang: "ru".into(), translation_display: crate::settings::TranslationDisplay::Inplace, ..Settings::default() }
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
        s.inplace.background_mode = InplaceBackgroundMode::TransparentOutline;
        let restyled = e.restyle(&s).unwrap();
        for (a, b) in first.blocks.iter().zip(&restyled.blocks) {
            assert_eq!((a.id, &a.font, &a.style), (b.id, &b.font, &b.style));
            assert_eq!(b.background.mode, super::super::background::BackgroundRenderMode::Transparent);
        }
        // Ручной трекинг меняет только его.
        s.inplace.letter_spacing = PropertyMode::Manual(2.0);
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
