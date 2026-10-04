//! Отслеживание текстовых полей между кадрами.
//!
//! Поле сохраняет свой id при сдвиге на несколько пикселей и небольшом изменении размера
//! (IoU, расстояние между центрами, близкие размеры и высота строк). Пропавшее поле живёт ещё
//! `ttl` (гистерезис: один неудачный кадр не уничтожает состояние). Всё, что решено для поля
//! один раз, — прежде всего шрифт — хранится здесь и переживает смену текста и перевода.

use super::background::BackgroundResult;
use super::block_detector::DetectedTextBlock;
use super::font_classifier::FontAnalysis;
use super::font_matcher::FontSelection;
use super::typography::TypographyEstimate;
use super::{Rect, Script, TextBlockType};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TrackedTextBlock {
    pub id: u64,
    pub current_rect: Rect,
    pub previous_rect: Rect,
    pub block_type: TextBlockType,
    pub line_height: f32,
    /// Цвет чернил оригинала: поля одного оформления (кнопки, пункты меню) похожи цветом.
    pub ink_color: [u8; 3],
    pub original_text: String,
    pub translated_text: String,
    pub detected_language: Script,
    /// Признаки шрифта с первого удачного анализа поля.
    pub font_analysis: Option<FontAnalysis>,
    /// Выбранный шрифт. `Some` — выбор заблокирован до сброса идентичности поля.
    pub font: Option<FontSelection>,
    pub typography: Option<TypographyEstimate>,
    /// Lines as the OCR engine found them (frame pixels, top to bottom): the primary source of the
    /// line structure while they still agree with the detected block.
    pub ocr_lines: Option<Vec<Rect>>,
    /// For the inspector: how the line structure was decided.
    pub lines_note: String,
    /// Where the glyphs really are according to the OCR lines (frame pixels), when that differs from
    /// the detected block within the allowed tolerance.
    pub ocr_bounds: Option<Rect>,
    /// `ocr_bounds` while it still lies on the detected block, else `None`: set on every scan.
    pub refined_rect: Option<Rect>,
    pub background: Option<BackgroundResult>,
    /// Уменьшенная яркость содержимого: по ней видно, менялся ли текст поля.
    pub content_signature: Vec<u8>,
    /// Средний цвет кольца вокруг поля: по нему видно, менялся ли фон.
    pub background_signature: Option<[u8; 3]>,
    /// Прямоугольник, по которому оценена типографика: пересчёт при заметной смене размера.
    pub typography_rect: Rect,
    /// Сколько обработанных кадров подряд поле не найдено.
    pub misses: u32,
    pub first_seen: Instant,
    pub last_seen: Instant,
    /// 0.0–1.0: насколько уверенно поле сопоставлено и распознано.
    pub confidence: f32,
    /// Растёт при каждом изменении того, что видно пользователю: по ней мост не пересылает
    /// неизменные поля.
    pub revision: u64,
}

impl TrackedTextBlock {
    fn new(id: u64, d: &DetectedTextBlock, now: Instant) -> Self {
        Self {
            id,
            current_rect: d.rect,
            previous_rect: d.rect,
            block_type: d.block_type,
            line_height: d.line_height(),
            ink_color: d.ink_color,
            original_text: String::new(),
            translated_text: String::new(),
            detected_language: Script::Other,
            font_analysis: None,
            font: None,
            typography: None,
            ocr_lines: None,
            lines_note: String::new(),
            ocr_bounds: None,
            refined_rect: None,
            background: None,
            content_signature: Vec::new(),
            background_signature: None,
            typography_rect: Rect::default(),
            misses: 0,
            first_seen: now,
            last_seen: now,
            confidence: 0.5,
            revision: 0,
        }
    }

    /// Glyph rectangle of the field: refined by the OCR lines when they agree, else as detected.
    pub fn text_rect(&self) -> Rect {
        self.refined_rect.unwrap_or(self.current_rect)
    }

    pub fn font_selection_locked(&self) -> bool {
        self.font.is_some()
    }

    /// Поле видно в последнем обработанном кадре.
    pub fn visible_at(&self, now: Instant) -> bool {
        self.last_seen == now
    }
}

/// Сопоставление обнаруженного блока с отслеживаемым полем.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Assignment {
    pub id: u64,
    pub detection: usize,
    pub is_new: bool,
}

pub struct TextBlockTracker {
    blocks: Vec<TrackedTextBlock>,
    next_id: u64,
    /// Сколько поле живёт без подтверждения.
    pub ttl: Duration,
}

impl Default for TextBlockTracker {
    fn default() -> Self {
        Self { blocks: Vec::new(), next_id: 1, ttl: Duration::from_millis(1500) }
    }
}

/// Насколько обнаруженный блок похож на отслеживаемое поле; `None` — это другое поле.
fn match_score(t: &TrackedTextBlock, d: &DetectedTextBlock) -> Option<f32> {
    let r = d.rect;
    // Совсем другая высота строки на том же месте — новое поле (например, сменилось окно игры).
    let lh = d.line_height();
    if t.line_height.max(lh) / t.line_height.min(lh).max(1.0) > 1.4 { return None; }
    let iou = t.current_rect.iou(&r);
    if iou >= 0.3 { return Some(iou); }
    let (tx, ty) = t.current_rect.center();
    let (dx, dy) = r.center();
    let dist = ((tx - dx).powi(2) + (ty - dy).powi(2)).sqrt();
    let near = 0.6 * t.current_rect.h.min(r.h).max(lh);
    let ratio = |a: f32, b: f32| a.max(b) / a.min(b).max(1.0);
    (dist <= near && ratio(t.current_rect.w, r.w) <= 1.6 && ratio(t.current_rect.h, r.h) <= 1.6)
        .then(|| 0.3 * (1.0 - dist / near.max(1.0)))
}

impl TextBlockTracker {
    pub fn update(&mut self, detected: &[DetectedTextBlock], now: Instant) -> Vec<Assignment> {
        let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
        for (ti, t) in self.blocks.iter().enumerate() {
            for (di, d) in detected.iter().enumerate() {
                if let Some(s) = match_score(t, d) { candidates.push((s, ti, di)); }
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mut used_t, mut used_d) = (vec![false; self.blocks.len()], vec![false; detected.len()]);
        let mut out = Vec::new();
        for (score, ti, di) in candidates {
            if used_t[ti] || used_d[di] { continue; }
            used_t[ti] = true;
            used_d[di] = true;
            let (t, d) = (&mut self.blocks[ti], &detected[di]);
            t.previous_rect = t.current_rect;
            t.current_rect = d.rect;
            t.line_height = d.line_height();
            t.ink_color = d.ink_color;
            if d.block_type != TextBlockType::Unknown { t.block_type = d.block_type; }
            t.last_seen = now;
            t.misses = 0;
            t.confidence = (t.confidence * 0.7 + score.min(1.0) * 0.3).clamp(0.0, 1.0);
            out.push(Assignment { id: t.id, detection: di, is_new: false });
        }
        for (di, d) in detected.iter().enumerate() {
            if used_d[di] { continue; }
            let id = self.next_id;
            self.next_id += 1;
            self.blocks.push(TrackedTextBlock::new(id, d, now));
            out.push(Assignment { id, detection: di, is_new: true });
        }
        for (ti, t) in self.blocks.iter_mut().enumerate() {
            if ti < used_t.len() && !used_t[ti] { t.misses += 1; }
        }
        let ttl = self.ttl;
        self.blocks.retain(|t| now.saturating_duration_since(t.last_seen) <= ttl);
        out.sort_by_key(|a| a.detection);
        out
    }

    pub fn get(&self, id: u64) -> Option<&TrackedTextBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut TrackedTextBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    pub fn blocks(&self) -> &[TrackedTextBlock] {
        &self.blocks
    }

    /// Признаки шрифта поля для выбора. Короткий текст («OK», «Quit») по пикселям классифицируется
    /// ненадёжно, а кнопки и пункты меню одного оформления почти всегда набраны одним шрифтом.
    /// Малонадёжное поле поэтому перенимает признаки у заметно более надёжного поля с близкой
    /// высотой строки. Обычно цвет тоже совпадает; выделенный пункт вертикального меню может
    /// иметь другой цвет, поэтому для соседних строк одной колонки цвет не обязателен.
    /// Результат не зависит от порядка обработки полей: берутся
    /// признаки, измеренные по пикселям, а не уже выбранные шрифты.
    pub fn analysis_with_peers(&self, id: u64) -> Option<FontAnalysis> {
        const WEAK: f32 = 0.7;
        const MARGIN: f32 = 0.10;
        let own = self.get(id)?;
        let mut best = own.font_analysis.clone()?;
        if best.confidence >= WEAK { return Some(best); }
        let color_distance = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).map(|(x, y)| (*x as f32 - y as f32).powi(2)).sum::<f32>().sqrt();
        let peer = self.blocks.iter()
            .filter(|p| p.id != id && p.misses == 0)
            .filter(|p| {
                let one_line = |b: &TrackedTextBlock| b.current_rect.h <= b.line_height * 1.15;
                let single_lines = one_line(own) && one_line(p);
                let same_color = color_distance(p.ink_color, own.ink_color) <= 70.0;
                let row = p.line_height.max(own.line_height);
                let vertical_gap = (p.current_rect.center().1 - own.current_rect.center().1).abs();
                let same_menu_column = single_lines
                    && (p.current_rect.x - own.current_rect.x).abs() <= 1.5 * row
                    && (1.4 * row..=6.0 * row).contains(&vertical_gap);
                let height_ratio = p.line_height.max(own.line_height) / p.line_height.min(own.line_height).max(1.0);
                if height_ratio > if same_menu_column { 1.35 } else { 1.1 } { return false; }
                same_color || same_menu_column
            })
            .filter_map(|p| p.font_analysis.as_ref())
            .filter(|a| a.confidence >= best.confidence + MARGIN)
            .max_by(|a, b| a.confidence.total_cmp(&b.confidence));
        if let Some(peer) = peer {
            // Начертание берётся у соседа; геометрия (высота прописных, наклон к своему тексту) — своя.
            best = FontAnalysis { cap_height_px: best.cap_height_px, stroke_px: best.stroke_px, features: best.features.clone(),
                confidence: (best.confidence + peer.confidence) / 2.0, ..peer.clone() };
        }
        Some(best)
    }

    /// Новый сеанс (другое окно, сброс): все поля и их шрифты забываются.
    pub fn reset(&mut self) {
        self.blocks.clear();
    }

    /// Пользователь попросил определить шрифты заново: идентичность полей сохраняется.
    pub fn reset_fonts(&mut self) {
        for b in &mut self.blocks {
            b.font = None;
            b.font_analysis = None;
            b.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::block_detector::TextLine;

    fn block(x: f32, y: f32, w: f32, h: f32, lines: usize) -> DetectedTextBlock {
        let lh = h / lines as f32;
        let lines = (0..lines).map(|i| TextLine { rect: Rect::new(x, y + i as f32 * lh, w, lh * 0.8), glyphs: vec![], ink_color: [255; 3] }).collect();
        DetectedTextBlock { rect: Rect::new(x, y, w, h), lines, block_type: TextBlockType::Unknown, ink_color: [255; 3] }
    }

    #[test]
    fn identity_survives_geometry_noise() {
        let mut t = TextBlockTracker::default();
        let t0 = Instant::now();
        let a = t.update(&[block(500.0, 620.0, 420.0, 95.0, 3)], t0);
        assert!(a[0].is_new);
        let b = t.update(&[block(502.0, 619.0, 421.0, 96.0, 3)], t0 + Duration::from_millis(100));
        assert_eq!((b[0].id, b[0].is_new), (a[0].id, false));
        let r = t.get(a[0].id).unwrap();
        assert_eq!((r.previous_rect.x, r.current_rect.x), (500.0, 502.0));
    }

    #[test]
    fn several_fields_keep_their_own_ids() {
        let mut t = TextBlockTracker::default();
        let t0 = Instant::now();
        let first = t.update(&[block(100.0, 100.0, 120.0, 22.0, 1), block(100.0, 140.0, 700.0, 60.0, 2)], t0);
        // Порядок обнаружения поменялся, поля — нет.
        let second = t.update(&[block(101.0, 141.0, 698.0, 60.0, 2), block(100.0, 101.0, 118.0, 22.0, 1)], t0 + Duration::from_millis(100));
        assert_eq!(second[0].id, first[1].id);
        assert_eq!(second[1].id, first[0].id);
    }

    #[test]
    fn missing_field_survives_ttl_then_new_identity() {
        let mut t = TextBlockTracker::default();
        let t0 = Instant::now();
        let id = t.update(&[block(0.0, 0.0, 300.0, 30.0, 1)], t0)[0].id;
        t.get_mut(id).unwrap().font = Some(crate::layout::font_matcher::FontSelection::generic(crate::layout::FontCategory::Serif));
        t.update(&[], t0 + Duration::from_millis(500));
        let back = t.update(&[block(0.0, 0.0, 300.0, 30.0, 1)], t0 + Duration::from_millis(900));
        assert_eq!(back[0].id, id, "one bad frame does not destroy the field");
        assert!(t.get(id).unwrap().font_selection_locked());
        t.update(&[], t0 + Duration::from_millis(3000));
        let fresh = t.update(&[block(0.0, 0.0, 300.0, 30.0, 1)], t0 + Duration::from_millis(3100));
        assert!(fresh[0].is_new && fresh[0].id != id);
        assert!(!t.get(fresh[0].id).unwrap().font_selection_locked(), "a new field selects its font again");
    }

    #[test]
    fn different_text_size_in_same_place_is_a_new_field() {
        let mut t = TextBlockTracker::default();
        let t0 = Instant::now();
        let a = t.update(&[block(100.0, 100.0, 400.0, 20.0, 1)], t0);
        let b = t.update(&[block(100.0, 100.0, 400.0, 40.0, 1)], t0 + Duration::from_millis(100));
        assert_ne!(a[0].id, b[0].id);
    }
}
