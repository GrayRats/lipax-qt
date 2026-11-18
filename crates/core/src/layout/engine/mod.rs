//! Оркестратор движка «перевод поверх оригинала» для одной области.
//!
//! Анализ отделён от ввода-вывода: [`InplaceEngine::begin`] обрабатывает кадр и возвращает
//! задания OCR только для полей, содержимое которых изменилось; pipeline распознаёт и
//! переводит их и передаёт результат в [`InplaceEngine::complete`]; [`InplaceEngine::finish`]
//! собирает поля для отрисовки и возвращает `None`, если видимое не изменилось.
//!
//! Что пересчитывается:
//! - то же поле, тот же текст — почти ничего (только проверка подписи фона);
//! - то же поле, другой текст — OCR, перевод; шрифт стабилен до подтверждённой смены оформления;
//! - то же поле, подвижный фон — только фон;
//! - новое поле — всё; оформление повторно проверяется с гистерезисом каждые 500 мс.

use super::background::{BackgroundAnalyzer, BackgroundInpainter, BackgroundResult};
use super::block_detector::{BlockDetector, DetectedTextBlock, DetectionReport};
use super::font_classifier::{FontAnalysis, FontClassifier};
use super::font_database::InstalledFontDatabase;
use super::font_matcher::{FontMatcher, FontPreferences, FontSelection};
use super::tracker::{TextBlockTracker, TrackedTextBlock};
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

/// OCR box padding must not make separately detected fields collide. Bound refinement at
/// the midpoint of the original free strip, while keeping overlapping detections ambiguous.
fn bounded_refinement(mut refined: Rect, detected: &Rect, neighbors: impl Iterator<Item = Rect>) -> Rect {
    for other in neighbors {
        let overlap_x = detected.x < other.right() && detected.right() > other.x;
        let overlap_y = detected.y < other.bottom() && detected.bottom() > other.y;
        if overlap_x && !overlap_y {
            if detected.bottom() <= other.y {
                refined.h = (refined.bottom().min((detected.bottom() + other.y) * 0.5) - refined.y).max(0.0);
            } else {
                let top = refined.y.max((other.bottom() + detected.y) * 0.5);
                refined.h = (refined.bottom() - top).max(0.0);
                refined.y = top;
            }
        } else if overlap_y && !overlap_x {
            if detected.right() <= other.x {
                refined.w = (refined.right().min((detected.right() + other.x) * 0.5) - refined.x).max(0.0);
            } else {
                let left = refined.x.max((other.right() + detected.x) * 0.5);
                refined.w = (refined.right() - left).max(0.0);
                refined.x = left;
            }
        }
    }
    refined
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

/// A block whose edge lies within this many px of the edge of the frame is taken as cut by it.
pub const EDGE_REACH: f32 = 1.5;

#[derive(Debug, Clone, PartialEq)]
pub struct InplaceFrame {
    pub frame: (u32, u32),
    /// The part of the window this frame was captured from, when it differs from the region the user drew (extended
    /// upwards for text the top edge cut). The overlay is placed by it.
    pub capture: Option<crate::settings::NormRect>,
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
    /// The pipeline gave up waiting for the frame to settle (it kept changing past `MAX_UNSTABLE`): a field whose text is
    /// still growing is then not read yet. Set before `begin` for that tick only.
    unsettled: bool,
    /// The top rows of the frame (px) that lie above the region the user drew: the capture was extended to read a line
    /// its top edge cut. Blocks wholly inside this strip are not fields. Set before `begin` (see `set_reach_above`).
    reach_above: f32,
    /// The line height (px) of the block that the top edge of the last frame cut, if any: the pipeline extends the
    /// capture upwards by it.
    cut_at_top: Option<f32>,
    /// Full backgrounds being computed on their own threads for fields that got the quick one (by field id: the glyph
    /// rectangle they are for, and where the result arrives).
    refinements: HashMap<u64, (Rect, std::sync::mpsc::Receiver<BackgroundResult>)>,
    /// Threads computing a full background right now, also those whose field has moved on and whose result will be
    /// thrown away: they are what `MAX_REFINEMENTS` limits.
    refining_threads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Where the time of the last `begin` went, stage by stage (summed over the fields for the per-field stages).
    timings: Vec<(&'static str, std::time::Duration)>,
    /// Content signatures of the fields handed out by the last `begin`, applied by `complete`.
    pending_signatures: HashMap<u64, Vec<u8>>,
    pending_ink: HashMap<u64, Vec<bool>>,
    pending_crops: HashMap<u64, Rect>,
    /// Where the OCR engine found fields glued together that the detector did not tell apart.
    cuts: Vec<CutMemory>,
    pending_cuts: HashSet<u64>,
    /// When the last `begin` ran: the age of `cuts`.
    last_scan: Option<Instant>,
    /// What the detector found and left out in the latest frame.
    detection: DetectionReport,
    ocr_dirty: bool,
    joins: Vec<JoinMemory>,
    pending_joins: HashSet<u64>,
    joined_aliases: HashMap<u64,u64>,
    recognition_policy: Option<crate::settings::TextRecognitionSettings>,
}

/// A field the latest scan did not find, whose place a field found in it takes (a cut parent, the previous line of
/// dialogue): it gives way at once instead of lingering for the grace scan. It is not drawn (`finish`), so it owns no
/// reading either (`reconcile_ocr`): a repeat of a field that is not drawn would leave its text with no plate at all.
fn gives_way(t: &TrackedTextBlock, fresh: &[Rect]) -> bool {
    if t.misses == 0 { return false; }
    let old = t.text_rect();
    fresh.iter().any(|f| f.intersection(&old) >= 0.3 * old.area().min(f.area()))
}

const REJECTED_OCR_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// What the OCR engine's lines mean for the field they were read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinesVerdict {
    /// The field stands: its structure is settled by the lines (or by the detector).
    Normal,
    /// OCR requires regrouping (split fields or join overlapping crops). The new regions appear
    /// on the next scan; the current reading must not be translated or published as settled.
    Split,
}

/// Text associated with the source field after validating crop-to-frame OCR evidence.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub struct OcrObservation {
    pub text: String,
    pub verdict: LinesVerdict,
}

/// Typical glyph height of a detected block: the floor of the line-pitch rule for text in capitals.
fn typical_glyph_height(block: &DetectedTextBlock) -> f32 {
    let mut heights: Vec<f32> = block.lines.iter().flat_map(|l| l.glyphs.iter().map(|g| g.h)).collect();
    heights.sort_by(f32::total_cmp);
    heights.get(heights.len() / 2).copied().unwrap_or_else(|| block.line_height())
}

impl Default for InplaceEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Full backgrounds computed beside the frames at most at once (each takes a thread and a copy of the frame).
const MAX_REFINEMENTS: usize = 4;

/// Counts a running background thread: it leaves the count when the thread ends or could not start.
struct RefiningThread(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Drop for RefiningThread {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// The stage times of one `begin`: each lap is the time since the previous one.
struct Laps {
    stage: Instant,
    timings: Vec<(&'static str, std::time::Duration)>,
}
impl Laps {
    fn lap(&mut self, name: &'static str) {
        self.timings.push((name, self.stage.elapsed()));
        self.stage = Instant::now();
    }
}

impl InplaceEngine {
    /// Встроенный реестр создаётся при первом выборе шрифта и кешируется на сеанс.
    pub fn new() -> Self {
        Self { tracker: TextBlockTracker::default(), detector: BlockDetector::default(), fonts: None, snapshot: None,
            applied: Fingerprints::default(), published: Vec::new(), new_translations: Vec::new(), missing_font_warned: HashSet::new(), unsettled: false, reach_above: 0.0, cut_at_top: None, refinements: HashMap::new(), refining_threads: Default::default(), timings: Vec::new(), pending_signatures: HashMap::new(), pending_ink: HashMap::new(), pending_crops: HashMap::new(),
            cuts: Vec::new(), pending_cuts: HashSet::new(), last_scan: None, detection: DetectionReport::default(), ocr_dirty: false, joins: Vec::new(),
            pending_joins: HashSet::new(), joined_aliases: HashMap::new(), recognition_policy: None }
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
        self.pending_crops.clear();
        self.published.clear();
        self.cuts.clear();
        self.pending_cuts.clear();
        self.missing_font_warned.clear();
        self.ocr_dirty = false;
        self.joins.clear(); self.pending_joins.clear(); self.joined_aliases.clear();
        self.recognition_policy = None;
    }

    /// Пользователь попросил определить шрифты заново: анализ повторится на следующем кадре.
    pub fn reanalyze_fonts(&mut self) {
        self.tracker.reset_fonts();
        self.missing_font_warned.clear();
    }

    /// The detector's report for the latest frame: components, rejections, blocks dropped by the limit.
    pub fn detection(&self) -> &DetectionReport {
        &self.detection
    }

    pub fn tracker(&self) -> &TextBlockTracker {
        &self.tracker
    }

    /// Takes in the full backgrounds that were being computed beside the frames (`BackgroundInpainter::render_quick`):
    /// the quick fill of a field is replaced when its full one is ready and the field is still what it was computed for.
    /// `true` if some field changed (the layout is to be published again).
    pub fn poll_refinements(&mut self) -> bool {
        use std::sync::mpsc::TryRecvError;
        let mut changed = false;
        let ids: Vec<u64> = self.refinements.keys().copied().collect();
        for id in ids {
            let Some((rect, receiver)) = self.refinements.get(&id) else { continue };
            let rect = *rect;
            match receiver.try_recv() {
                Err(TryRecvError::Empty) => continue,
                Ok(result) => {
                    self.refinements.remove(&id);
                    if let Some(t) = self.tracker.get_mut(id)
                        && t.background_for == rect && t.background.as_ref().is_some_and(|b| b.refinable) {
                        t.background = Some(result);
                        t.revision += 1;
                        changed = true;
                    }
                }
                Err(TryRecvError::Disconnected) => { self.refinements.remove(&id); }
            }
        }
        changed
    }

    /// Whether any full background is still being computed.
    pub fn refining(&self) -> bool {
        !self.refinements.is_empty()
    }

    /// The stages of the last `begin` and what each took (for the log and for profiling).
    pub fn timings(&self) -> &[(&'static str, std::time::Duration)] {
        &self.timings
    }

    /// The next `begin` is of a frame the pipeline stopped waiting for: fields whose text is still growing are held back.
    pub fn set_unsettled(&mut self, unsettled: bool) {
        self.unsettled = unsettled;
    }

    /// How many rows at the top of the next frames lie above the drawn region (the capture is extended upwards).
    pub fn set_reach_above(&mut self, px: f32) {
        self.reach_above = px.max(0.0);
    }

    /// The line height of a block whose top the last frame's top edge cut: the capture should start higher.
    pub fn cut_at_top(&self) -> Option<f32> {
        self.cut_at_top
    }

    /// Кадр: поиск и сопоставление полей, анализ шрифта новых полей, фон и типографика там,
    /// где они изменились. Возвращает задания OCR для полей с изменившимся содержимым.
    pub fn begin(&mut self, frame: DynamicImage, s: &Settings, now: Instant, force: bool) -> Vec<OcrJob> {
        // For this frame only: a later `begin` that was not told otherwise is of a settled frame.
        let unsettled = std::mem::take(&mut self.unsettled);
        self.poll_refinements();
        // The frame is moved in: an RGBA8 capture is kept as is, with no copy.
        let image = frame.into_rgba8();
        let frame_changed = self.snapshot.as_ref().is_some_and(|old| old.image.dimensions() != image.dimensions());
        let recognition_changed = self.recognition_policy.as_ref() != Some(&s.recognition);
        if recognition_changed { self.recognition_policy = Some(s.recognition.clone()); }
        self.pending_signatures.clear();
        self.pending_ink.clear();
        self.pending_crops.clear();
        let begun = Instant::now();
        let mut laps = Laps { stage: Instant::now(), timings: Vec::new() };
        let (mut mask, detected) = self.detect_frame(&image, s, now, &mut laps);
        let mut detected = self.regroup(detected, &image, &mut mask, s, now, frame_changed);
        self.cut_reach_above(&mut detected);
        let assignments = self.track(&detected, now, &mut laps);
        let mut per_field = [std::time::Duration::ZERO; 5];
        let (fw, fh) = (image.width() as f32, image.height() as f32);
        let bg_changed = self.applied.background != Fingerprints::of(s).background;
        let mut jobs = Vec::new();
        let mut blocks = HashMap::new();
        let frame = FrameContext { image: &image, mask: &mask, detected: &detected, now, s, fw, fh,
            frame_changed, recognition_changed, unsettled, force, bg_changed };
        for a in assignments {
            let d = &detected[a.detection];
            let Some(t) = self.tracker.get_mut(a.id) else { continue };
            let lh = d.line_height();
            let (signature, ink, took) = field_signatures(t, d, &frame, &mut self.ocr_dirty);
            per_field[0] += took;
            self.pending_ink.insert(a.id, ink);
            let (typing, took) = field_typing(t, d, &frame);
            per_field[0] += took;
            let (rect, due) = field_ocr_state(t, d, lh, &signature, &frame, &self.joins, &mut self.ocr_dirty);
            if due && typing {
                tracing::debug!(target: "inplace.typing", block_id = a.id, "the text under the field is still being typed: it is read when it stops growing");
            } else if due {
                let started = Instant::now();
                let (job, crop) = ocr_job(a.id, d, lh, &frame);
                jobs.push(job);
                self.pending_crops.insert(a.id, crop);
                // Remembered only when the field is completed (see `complete`): a failed or cancelled
                // recognition must leave the field due for the next scan.
                self.pending_signatures.insert(a.id, signature);
                per_field[1] += started.elapsed();
            }
            per_field[2] += probe_font(t, d, &frame);
            per_field[3] += field_background(t, d, rect, lh, &frame, &self.refining_threads, &mut self.refinements);
            per_field[4] += field_typography(t, d, &frame);
            blocks.insert(a.id, d.clone());
        }
        for (name, took) in ["signatures and ink", "OCR crops", "font probe", "background", "typography"].into_iter().zip(per_field) { laps.timings.push((name, took)); }
        laps.lap("fields (all)");
        if bg_changed { self.applied.background = Fingerprints::of(s).background; }
        // Поле уже распознано, но шрифт сброшен (явный запрос): выбрать сразу, без нового OCR.
        let pending: Vec<u64> = self.tracker.blocks().iter()
            .filter(|t| !t.font_selection_locked() && !t.original_text.is_empty() && t.font_analysis.is_some())
            .map(|t| t.id).collect();
        for id in pending { self.lock_font(id, s); }
        self.snapshot = Some(Snapshot { image, mask, blocks });
        laps.lap("fonts and snapshot");
        laps.timings.push(("begin (whole)", begun.elapsed()));
        self.timings = laps.timings;
        jobs
    }

    /// Stage 1 of `begin`: the ink mask of the frame and the blocks the detector finds in it (the report is kept for
    /// `detection`).
    fn detect_frame(&mut self, image: &RgbaImage, s: &Settings, now: Instant, laps: &mut Laps) -> (InkMask, Vec<DetectedTextBlock>) {
        let mut mask = BlockDetector::ink_mask(image);
        laps.lap("ink mask");
        self.detector.line_gap_factor = s.appearance.inplace.line_gap_factor;
        self.detector.max_blocks = s.appearance.inplace.max_blocks;
        let (detected, report) = self.detector.detect_with_local(image, &mut mask);
        laps.lap("detect blocks");
        if !report.dropped.is_empty() {
            tracing::warn!(target: "inplace.detect", candidates = report.candidates, dropped = report.dropped.len(), limit = self.detector.max_blocks,
                "more text blocks than appearance.inplace.max_blocks: the smallest are not read");
        }
        self.detection = report;
        self.last_scan = Some(now);
        (mask, detected)
    }

    /// Stage 2 of `begin`: the cuts and joins the OCR engine showed earlier are applied to the detected blocks, and the
    /// interface the user asked to leave alone is dropped.
    fn regroup(&mut self, detected: Vec<DetectedTextBlock>, image: &RgbaImage, mask: &mut InkMask, s: &Settings, now: Instant,
        frame_changed: bool) -> Vec<DetectedTextBlock> {
        if frame_changed { self.joins.clear(); }
        self.pending_joins.clear();
        self.pending_cuts.clear();
        self.joined_aliases.clear();
        let detected = self.apply_cuts(detected, now, image, mask);
        let mut detected = self.apply_joins(detected, now);
        // The interface of the game, if the user asked to leave it alone: such blocks never become fields.
        let inplace = &s.appearance.inplace;
        if inplace.skip_controls || inplace.skip_names {
            detected.retain(|d| match d.block_type {
                super::TextBlockType::Button | super::TextBlockType::MenuItem => !inplace.skip_controls,
                super::TextBlockType::CharacterName => !inplace.skip_names,
                _ => true,
            });
        }
        detected
    }

    /// Stage 3 of `begin`: blocks wholly in the strip above the drawn region are dropped, and the line height of text
    /// cut by the top edge is kept for `cut_at_top`.
    fn cut_reach_above(&mut self, detected: &mut Vec<DetectedTextBlock>) {
        // The strip added above the drawn region is read only to complete text its edge cut: what lies wholly in it is
        // not a field (and does not ask for more).
        if self.reach_above > 0.0 {
            let strip = self.reach_above;
            detected.retain(|d| d.rect.bottom() > strip + EDGE_REACH);
        }
        // Text cut by the top edge of the frame: its upper part is missing ("Mika" reads as "Muka").
        self.cut_at_top = detected.iter().filter(|d| d.rect.y <= EDGE_REACH).map(|d| d.line_height()).reduce(f32::max);
    }

    /// Stage 4 of `begin`: the tracker matches the blocks to its fields; fields that lingered under a join now answer to
    /// the joined one.
    fn track(&mut self, detected: &[DetectedTextBlock], now: Instant, laps: &mut Laps) -> Vec<super::tracker::Assignment> {
        let assignments = self.tracker.update(detected, now);
        laps.lap("cuts, joins, tracking");
        for memory in &self.joins {
            let union = memory.members.iter().skip(1).fold(memory.members[0], |r,m| r.union(m));
            if let Some(owner) = assignments.iter().find(|a| detected[a.detection].rect.iou(&union) >= 0.8) {
                for old in self.tracker.blocks().iter().filter(|t| t.misses > 0 && memory.members.iter().any(|r| r.iou(&t.current_rect)>=0.5)) {
                    self.joined_aliases.insert(old.id,owner.id);
                }
            }
        }
        self.missing_font_warned.retain(|id| self.tracker.get(*id).is_some());
        assignments
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

    /// Распознанный текст поля, как он принят в прошлый раз (пусто, пока поле не прочитано).
    pub fn field_text(&self, id: u64) -> Option<&str> {
        self.tracker.get(id).map(|t| t.original_text.as_str()).filter(|t| !t.is_empty())
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
        let ink = self.pending_ink.get(&id).cloned();
        let Some((original, translation)) = result else {
            // Nothing readable is under the field any more, and what it showed belongs to other letters.
            if t.stale { t.translated_text.clear(); t.original_text.clear(); t.stale = false; t.revision += 1; self.ocr_dirty = true; }
            return;
        };
        if let Some(ink) = ink { t.ink_signature = ink; }
        if t.stale { t.stale = false; t.revision += 1; }
        t.rejected_ocr_at = None;
        if original != t.original_text || translation != t.translated_text {
            if translation != t.translated_text { self.new_translations.push((original.clone(), translation.clone())); }
            t.original_text = original;
            t.translated_text = translation;
            self.ocr_dirty = true;
            t.detected_language = Script::of_text(&t.original_text);
            t.revision += 1;
        }
        // Первое удачное распознавание поля: выбор шрифта и его блокировка.
        self.lock_font(id, s);
    }

    /// Cache a successfully observed empty/noise/low-confidence crop, retaining the prior overlay.
    /// Content, dimensions, recognition settings, forced scans and the 5s timeout invalidate it.
    /// I/O errors and cancelled OCR must use neither this method nor a negative cache entry.
    pub fn reject_ocr(&mut self, id: u64, s: &Settings) {
        self.complete(id, None, s);
        if let Some(t) = self.tracker.get_mut(id) { t.rejected_ocr_at = self.last_scan; }
    }

    /// Поля для отрисовки. `None`, если с прошлой отправки ничего видимого не изменилось.
    pub fn finish(&mut self, s: &Settings) -> Option<InplaceFrame> {
        self.apply_settings(s);
        self.reconcile_ocr(s);
        let font_db = self.font_db();
        let snap = self.snapshot.as_ref()?;
        let mut blocks = Vec::new();
        let mut undrawable = Vec::new();
        // Fields found in this scan, translated or not yet: a field that was not found (a cut parent, the previous line
        // of dialogue) gives way to them at once instead of lingering over the new text for the grace scan.
        let fresh: Vec<Rect> = self.tracker.blocks().iter().filter(|t| t.misses == 0).map(|t| t.text_rect()).collect();
        for t in self.tracker.blocks() {
            // Пропуск одного кадра не гасит поле: короткие сбои распознавания не мигают.
            if t.misses > 1 || t.translated_text.is_empty() || t.stale { continue; }
            if gives_way(t, &fresh) { continue; }
            if self.pending_joins.contains(&t.id) {
                undrawable.push(UndrawableField { id:t.id, original:t.original_text.clone(), translation:t.translated_text.clone(),
                    reason:"объединение OCR-полей; требуется повторное распознавание" });
                continue;
            }
            if t.ocr_owner.is_some() {
                undrawable.push(UndrawableField { id: t.id, original: t.original_text.clone(), translation: t.translated_text.clone(),
                    reason: "повторный OCR того же текста" });
                continue;
            }
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
        Some(InplaceFrame { frame: (snap.image.width(), snap.image.height()), capture: None, blocks, undrawable, new_translations: std::mem::take(&mut self.new_translations) })
    }

    /// Настройки поменялись без нового кадра: перестроить то, что от них зависит.
    pub fn restyle(&mut self, s: &Settings) -> Option<InplaceFrame> {
        let refined = self.poll_refinements();
        if self.snapshot.is_none() || (!refined && self.applied == Fingerprints::of(s)) { return None; }
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
mod fields;
mod observe;
mod regroup;
use fields::*;
use regroup::*;
#[cfg(test)]
mod tests;
