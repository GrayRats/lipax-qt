//! OCR geometry and duplicate ownership. Image coordinates remain typed frame pixels.
//! A nested detector box alone is not sufficient evidence to suppress an independent label.
use super::{
    Rect,
    typography::{normalized_lines, reconstructed_ocr_lines},
};

#[derive(Debug, Clone, PartialEq)]
pub struct OcrSpan {
    rect: Rect,
    text: String,
    confidence: f32,
}

impl OcrSpan {
    /// Reject invalid geometry, empty/noise readings and confidence outside the OCR 0..100 scale.
    #[must_use]
    pub fn new(rect: Rect, text: &str, confidence: f32) -> Option<Self> {
        let text = crate::text::normalize(text);
        ([rect.x, rect.y, rect.w, rect.h, rect.right(), rect.bottom()]
            .iter()
            .all(|x| x.is_finite())
            && rect.w > 0.0
            && rect.h > 0.0
            && confidence.is_finite()
            && (0.0..=100.0).contains(&confidence)
            && crate::text::is_meaningful(&text))
        .then_some(Self {
            rect,
            text,
            confidence,
        })
    }
    pub fn rect(&self) -> Rect {
        self.rect
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn confidence(&self) -> f32 {
        self.confidence
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrField {
    id: u64,
    spans: Vec<OcrSpan>,
    text: String,
    bounds: Rect,
    cap_height: f32,
    crop_bounds: Option<Rect>,
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn word_similarity(word: &str, other: &str) -> f32 {
    if word == other {
        return 1.0;
    }
    let n = word.chars().count();
    let chars: Vec<char> = other.chars().collect();
    let m = chars.len();
    let full = if n.min(m) * 2 >= n.max(m) {
        crate::text::similarity(word, other)
    } else {
        0.0
    };
    if n < 3 || m < n || m > 3 * n {
        return full;
    }
    chars
        .windows(n)
        .map(|part| crate::text::similarity(word, &part.iter().collect::<String>()))
        .fold(full, f32::max)
}

/// Share of a row's pixels another reading must cover, and of its words it must repeat, for the two to be one text.
/// The words must match exactly (case aside): the fuzzy `text_support` also credits `but` for `ut`, which would
/// join a garbled fragment to the line it was clipped from.
const SHARED_ROW: f32 = 0.5;
const SHARED_ROW_TEXT: f32 = 0.9;
/// Share of the whole readings that may be repeated by the other one before it counts as a duplicate or a fragment.
const SHARED_WHOLE_TEXT: f32 = 0.8;

/// Share of the letters of `fragment` that are in words `owner` has too, word for word.
fn exact_support(fragment: &str, owner: &str) -> f32 {
    let target: Vec<String> = words(owner).iter().map(|w| w.to_lowercase()).collect();
    let source = words(fragment);
    let total: usize = source.iter().map(|w| w.chars().count()).sum();
    if total == 0 { return 0.0; }
    let matched: usize = source.iter().filter(|w| target.contains(&w.to_lowercase())).map(|w| w.chars().count()).sum();
    matched as f32 / total as f32
}

/// Partial crops can read a suffix/prefix of a word and occasional OCR substitutions.
/// Score whole word evidence, not a character subsequence of an arbitrarily long paragraph.
fn text_support(fragment: &str, owner: &str) -> f32 {
    let source = words(fragment);
    let target = words(owner);
    let total: usize = source.iter().map(|s| s.chars().count()).sum();
    if total == 0 {
        return 0.0;
    }
    let matched: f32 = source
        .iter()
        .map(|word| {
            let n = word.chars().count();
            let best = target
                .iter()
                .map(|other| word_similarity(word, other))
                .fold(0.0_f32, f32::max);
            best * n as f32
        })
        .sum();
    matched / total as f32
}

/// Exact union coverage: overlapping parent spans must not count their pixels twice.
fn covered_area(rect: Rect, spans: &[OcrSpan]) -> f32 {
    covered_source_area(rect, spans, false)
}

fn covered_source_area(rect: Rect, spans: &[OcrSpan], clipped: bool) -> f32 {
    let overlaps: Vec<Rect> = spans
        .iter()
        .filter(|s| {
            let a = rect.h.min(s.rect.h);
            let b = rect.h.max(s.rect.h);
            clipped || (a >= b * 0.25 && (rect.center().1 - s.rect.center().1).abs() <= b * 0.3)
        })
        .map(|s| s.rect.clipped_to(&rect))
        .filter(|r| r.area() > 0.0)
        .collect();
    let mut xs: Vec<f32> = overlaps.iter().flat_map(|r| [r.x, r.right()]).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    let mut area = 0.0;
    let mut ys = Vec::with_capacity(overlaps.len());
    for x in xs.windows(2) {
        ys.clear();
        ys.extend(
            overlaps
                .iter()
                .filter(|r| r.x < x[1] && r.right() > x[0])
                .map(|r| (r.y, r.bottom())),
        );
        ys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = f32::NEG_INFINITY;
        let mut length = 0.0;
        for &(lo, hi) in &ys {
            length += (hi - lo.max(end)).max(0.0);
            end = end.max(hi);
        }
        area += (x[1] - x[0]) * length;
    }
    area
}

impl OcrField {
    #[must_use]
    pub fn new(id: u64, spans: Vec<OcrSpan>, cap_height: f32) -> Option<Self> {
        if !cap_height.is_finite() || cap_height < 0.0 {
            return None;
        }
        let first = spans.first()?;
        let bounds = spans
            .iter()
            .skip(1)
            .fold(first.rect, |r, s| r.union(&s.rect));
        if ![
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            bounds.right(),
            bounds.bottom(),
        ]
        .iter()
        .all(|x| x.is_finite())
        {
            return None;
        }
        let text = spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut field = Self {
            id,
            spans,
            bounds,
            text,
            cap_height,
            crop_bounds: None,
        };
        if matches!(super::Script::of_text(&field.text), super::Script::Latin | super::Script::Cyrillic | super::Script::Greek) {
            let order = super::typography::ocr_reading_order(&field.spans.iter().map(|s| s.rect).collect::<Vec<_>>(), &field.rows());
            field.spans = order.into_iter().map(|i| field.spans[i].clone()).collect();
            field.text = field.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        }
        Some(field)
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Preserve the actual OCR input crop in frame pixels. This distinguishes
    /// glyph fragments cut by capture from independent, genuinely small labels.
    #[must_use]
    pub fn with_crop_bounds(mut self, crop: Rect) -> Self {
        if [crop.x, crop.y, crop.w, crop.h, crop.right(), crop.bottom()].iter().all(|v| v.is_finite())
            && crop.w > 0.0 && crop.h > 0.0 && crop.contains(&self.bounds) {
            self.crop_bounds = Some(crop);
        }
        self
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn spans(&self) -> &[OcrSpan] {
        &self.spans
    }
    pub fn mean_confidence(&self) -> f32 {
        self.spans.iter().map(|s| s.confidence).sum::<f32>() / self.spans.len() as f32
    }
    pub fn rows(&self) -> Vec<Rect> {
        // The spans already belong to one source field. Its label/value columns can have a
        // wide horizontal gap while sharing a baseline; counting them as successive lines
        // produces a false 1px line pitch. Independent fields are never combined here.
        let fragments =
            reconstructed_ocr_lines(&self.spans.iter().map(|s| s.rect).collect::<Vec<_>>());
        let mut rows: Vec<Rect> = Vec::new();
        for fragment in fragments {
            if let Some(row) = rows.iter_mut().find(|row| {
                let low = row.h.min(fragment.h);
                let high = row.h.max(fragment.h);
                let overlap = row.bottom().min(fragment.bottom()) - row.y.max(fragment.y);
                low >= high * 0.6 && overlap >= low * 0.6
            }) {
                *row = row.union(&fragment);
            } else {
                rows.push(fragment);
            }
        }
        normalized_lines(&rows)
    }
    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for span in &mut self.spans {
            span.rect.x += dx;
            span.rect.y += dy;
        }
        self.bounds.x += dx;
        self.bounds.y += dy;
        if let Some(crop) = &mut self.crop_bounds { crop.x += dx; crop.y += dy; }
    }

    /// Share of the words of this reading that `other` repeats (0..=1), by fuzzy word match.
    #[must_use]
    pub fn text_supported_by(&self, other: &Self) -> f32 {
        text_support(self.text(), other.text())
    }

    /// Related overlapping crops which must be read again together: neither reading owns all
    /// the source pixels. The detector must independently confirm overlapping source support.
    #[must_use]
    pub fn needs_join(&self, other: &Self) -> bool {
        if self.id == other.id || self.duplicate_of(other) || other.duplicate_of(self) {
            return false;
        }
        let (small, big) = if self.bounds.area() <= other.bounds.area() {
            (self, other)
        } else {
            (other, self)
        };
        let area: f32 = small.spans.iter().map(|s| s.rect.area()).sum();
        let shared: f32 = small
            .spans
            .iter()
            .map(|s| covered_area(s.rect, &big.spans))
            .sum();
        let similar_text = text_support(small.text(), big.text()) >= 0.65 && shared >= 0.6 * area;
        let single_word = self.spans.len() == 1
            && other.spans.len() == 1
            && self.text.chars().all(char::is_alphabetic)
            && other.text.chars().all(char::is_alphabetic)
            && self.text.chars().count() <= 16
            && other.text.chars().count() <= 16
            && self.cap_height.min(other.cap_height) > 0.0
            && self.cap_height.max(other.cap_height) <= 1.5 * self.cap_height.min(other.cap_height)
            && (self.bounds.bottom() - other.bounds.bottom()).abs()
                <= 0.3 * self.bounds.h.max(other.bounds.h)
            && self.bounds.intersection(&other.bounds)
                >= 0.4 * self.bounds.area().min(other.bounds.area());
        // One row of the smaller reading lies on the pixels of the bigger one and says the same words there: the
        // detector cut a line of the dialogue in two and one half also took the neighbouring row. The union has to
        // be read again; the parts alone overlap and are both refused for lack of room.
        let shares_row = |a: &Self, b: &Self| a.spans.iter().any(|row| {
            row.rect.area() > 0.0
                && covered_area(row.rect, &b.spans) >= SHARED_ROW * row.rect.area()
                && exact_support(&row.text, b.text()) >= SHARED_ROW_TEXT
        });
        // Each reading has words the other lacks; one that the other repeats almost entirely is a duplicate or a
        // fragment (`duplicate_of`, the cuts), not half of a line.
        let shared_row = (shares_row(small, big) || shares_row(big, small))
            && small.text_supported_by(big) < SHARED_WHOLE_TEXT && big.text_supported_by(small) < SHARED_WHOLE_TEXT;
        similar_text || single_word || shared_row
    }

    /// Both text evidence and shared source pixels are required. Short, severely clipped glyph
    /// fragments can be owned by the complete reading, but a genuine short label is retained.
    #[must_use]
    pub fn duplicate_of(&self, owner: &Self) -> bool {
        if self.id == owner.id
            || owner.mean_confidence() < 70.0
            || owner.mean_confidence() + 10.0 < self.mean_confidence()
        {
            return false;
        }
        let n = self.text.chars().count();
        let m = owner.text.chars().count();
        if m < n || (m == n && owner.id >= self.id) {
            return false;
        }
        if self.bounds.intersection(&owner.bounds) < 0.7 * self.bounds.area() {
            return false;
        }
        let cut_at_input = self.crop_bounds.is_some_and(|crop| {
            self.bounds.x - crop.x <= 1.0 || self.bounds.y - crop.y <= 1.0
                || crop.right() - self.bounds.right() <= 1.0 || crop.bottom() - self.bounds.bottom() <= 1.0
        });
        let clipped_glyph = n <= 2
            && self.spans.len() == 1
            && (self.cap_height > 2.0 * self.spans[0].rect.h || cut_at_input)
            && owner.bounds.area() > 20.0 * self.bounds.area();
        if !clipped_glyph && text_support(&self.text, &owner.text) < 0.8 {
            return false;
        }
        // OCR rectangles are integer pixel bounds. A one-pixel border distance
        // is their rounding allowance, not a relaxation of source ownership.
        self.spans
            .iter()
            .all(|s| covered_source_area(s.rect, &owner.spans, cut_at_input) >= 0.8 * s.rect.area())
    }
}
