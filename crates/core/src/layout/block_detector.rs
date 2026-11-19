//! Поиск независимых текстовых блоков в кадре области.
//!
//! Маска «чернил» (порог Оцу, текст — меньший класс) → связные компоненты → строки (перекрытие
//! по вертикали, разрыв по горизонтали не больше полутора высот) → блоки (близкие по вертикали
//! строки одной высоты, выравнивания и цвета). Имя персонажа, реплика и кнопки под ней остаются
//! разными блоками: их разделяют цвет, зазоры и выравнивание.

use super::{InkMask, Rect, TextBlockType, luma, mean_color};
use image::RgbaImage;

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub rect: Rect,
    /// Связные компоненты (буквы или их части) строки.
    pub glyphs: Vec<Rect>,
    pub ink_color: [u8; 3],
}

impl TextLine {
    /// Median glyph height of the line.
    fn glyph_height(&self) -> f32 {
        median(self.glyphs.iter().map(|g| g.h).filter(|h| *h > 0.0).collect())
    }

    /// The baseline (y of the bottom edge most letters share). Descenders (`g`, `p`, `,`) end below it and
    /// are outvoted: the answer is the largest group of bottoms within a tenth of a glyph height, the upper
    /// one when two groups are equally large. `None` for a line without glyphs.
    pub fn baseline(&self) -> Option<f32> {
        if self.glyphs.is_empty() { return None; }
        let tol = (0.1 * self.glyph_height()).max(1.0);
        let bottoms: Vec<f32> = self.glyphs.iter().map(|g| g.bottom()).collect();
        let (mut best, mut best_n) = (bottoms[0], 0usize);
        for &b in &bottoms {
            let n = bottoms.iter().filter(|&&o| (o - b).abs() <= tol).count();
            if n > best_n || (n == best_n && b < best) { best = b; best_n = n; }
        }
        let group: Vec<f32> = bottoms.into_iter().filter(|o| (o - best).abs() <= tol).collect();
        Some(group.iter().sum::<f32>() / group.len() as f32)
    }

    /// Word boxes: neighbouring glyphs are one word until the gap between them exceeds a quarter of the
    /// typical glyph height (letter gaps are about a tenth, word spaces more than a third). Derived from the
    /// detected components only, so a letter split in pieces by the mask stays in one word.
    pub fn words(&self) -> Vec<Rect> {
        let mut glyphs = self.glyphs.clone();
        glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
        let gap = 0.25 * self.glyph_height();
        let mut words: Vec<Rect> = Vec::new();
        let mut right = f32::MIN;
        for g in glyphs {
            match words.last_mut() {
                Some(w) if g.x - right <= gap => *w = w.union(&g),
                _ => words.push(g),
            }
            right = right.max(g.right());
        }
        words
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedTextBlock {
    /// Граница глифов блока, без полей.
    pub rect: Rect,
    pub lines: Vec<TextLine>,
    pub block_type: TextBlockType,
    pub ink_color: [u8; 3],
}

impl DetectedTextBlock {
    pub fn line_height(&self) -> f32 {
        let mut h: Vec<f32> = self.lines.iter().map(|l| l.rect.h).filter(|h| h.is_finite() && *h > 0.0).collect();
        h.sort_by(|a, b| a.total_cmp(b));
        h.get(h.len() / 2).copied().unwrap_or(0.0)
    }
}

/// Accents and dots join a line when they lie within this fraction of its tallest glyph from it.
const ACCENT_REACH: f32 = 0.35;

/// What the detector saw and what it left out: nothing is dropped without being counted here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetectionReport {
    /// Connected components that passed the noise / scenery filters.
    pub components: usize,
    /// Blocks rejected as repeated filled shapes (halftone dots, studs) rather than letters.
    pub rejected_pattern: usize,
    /// Large components rejected as scenery (not text-like in their context).
    pub rejected_scenery: usize,
    /// Blocks found before the `max_blocks` limit was applied.
    pub candidates: usize,
    /// Blocks found only by the local-contrast pass (text the global threshold missed).
    pub local_blocks: usize,
    /// Blocks removed by the limit (smallest first), in reading order.
    pub dropped: Vec<Rect>,
}

pub struct BlockDetector {
    /// Resource bound, not a quality filter: blocks beyond it (the smallest) are removed and
    /// listed in `DetectionReport::dropped`.
    pub max_blocks: usize,
    /// `k` правила разделения по шагу строк (`layout::split`): склеенные поля, строки которых
    /// стоят реже, чем в абзаце, становятся отдельными блоками.
    pub line_gap_factor: f32,
}

/// Default bound on blocks per frame: every block costs one OCR job and a font probe.
pub const DEFAULT_MAX_BLOCKS: usize = 64;

impl Default for BlockDetector {
    fn default() -> Self {
        Self { max_blocks: DEFAULT_MAX_BLOCKS, line_gap_factor: super::split::DEFAULT_LINE_GAP_FACTOR }
    }
}

/// A local-contrast line is accepted only with at least this many glyph-like components.
const LOCAL_MIN_GLYPHS: usize = 5;
const LOCAL_MIN_LINE_HEIGHT: f32 = 6.0;
/// Glyphs of the local-contrast pass at least this tall are large type (titles, labels): the only lines whose fragments
/// `join_row_fragments` puts back together. Ordinary text sizes keep the plain grouping.
const LARGE_GLYPH: f32 = 4.0 * LOCAL_MIN_LINE_HEIGHT;

#[derive(Clone, Copy)]
struct Component {
    rect: Rect,
    pixels: u32,
}

fn otsu(hist: &[u64; 256], total: u64) -> u8 {
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut sum_b, mut w_b, mut best, mut threshold) = (0.0, 0u64, -1.0, 128u8);
    for (t, &c) in hist.iter().enumerate() {
        w_b += c;
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += t as f64 * c as f64;
        let (m_b, m_f) = (sum_b / w_b as f64, (sum - sum_b) / w_f as f64);
        let between = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
        if between > best { best = between; threshold = t as u8; }
    }
    threshold
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

/// Связные компоненты маски (8-связность) через серии пикселей и union-find.
fn components(mask: &InkMask) -> Vec<Component> {
    // Серии текущей и предыдущей строки: (x0, x1, метка).
    let mut parent: Vec<u32> = Vec::new();
    let mut runs: Vec<(usize, usize, usize, u32)> = Vec::new(); // y, x0, x1, label
    let mut prev: Vec<(usize, usize, u32)> = Vec::new();
    let mut cur: Vec<(usize, usize, u32)> = Vec::new();
    for y in 0..mask.h {
        let row = &mask.ink[y * mask.w..(y + 1) * mask.w];
        cur.clear();
        let mut first_overlap = 0;
        let mut x = 0;
        while x < mask.w {
            if !row[x] { x += 1; continue; }
            let x0 = x;
            while x < mask.w && row[x] { x += 1; }
            let label = parent.len() as u32;
            parent.push(label);
            // Runs are ordered and disjoint. Skip previous runs that can no longer
            // touch this or any later run, and stop at the first run to its right.
            // End coordinates are exclusive: equality still means diagonal contact.
            while first_overlap < prev.len() && prev[first_overlap].1 < x0 { first_overlap += 1; }
            for &(px0, px1, pl) in &prev[first_overlap..] {
                if px0 > x { break; }
                // 8-связность: серии касаются, если перекрываются с допуском в один пиксель.
                if px0 <= x && x0 <= px1 {
                    let (a, b) = (find(&mut parent, label), find(&mut parent, pl));
                    if a != b { parent[a.max(b) as usize] = a.min(b); }
                }
            }
            cur.push((x0, x, label));
            runs.push((y, x0, x, label));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let mut boxes: std::collections::HashMap<u32, (usize, usize, usize, usize, u32)> = Default::default();
    for (y, x0, x1, label) in runs {
        let root = find(&mut parent, label);
        let e = boxes.entry(root).or_insert((x0, y, x1, y + 1, 0));
        e.0 = e.0.min(x0);
        e.1 = e.1.min(y);
        e.2 = e.2.max(x1);
        e.3 = e.3.max(y + 1);
        e.4 += (x1 - x0) as u32;
    }
    let mut components: Vec<_> = boxes.into_values()
        .map(|(x0, y0, x1, y1, n)| Component { rect: Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32), pixels: n })
        .collect();
    // Hash-map iteration must not decide ties in the greedy line-band grouping.
    // Canonical component order makes identical captures produce identical geometry.
    components.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y)
        .then_with(|| a.rect.x.total_cmp(&b.rect.x))
        .then_with(|| a.rect.h.total_cmp(&b.rect.h))
        .then_with(|| a.rect.w.total_cmp(&b.rect.w)));
    components
}

/// Solid discs in a row (a halftone, a stud pattern) are not letters. A disc is round, fills about 0.785
/// of its bounding box and, unlike `o`, `a`, `e` or `0` of any weight, has no counter (enclosed background).
/// A pattern needs several of them: a lone bullet or a ring `0` is not affected.
fn is_blob_pattern(b: &DetectedTextBlock, mask: &InkMask) -> bool {
    let (mut total, mut blobs) = (0usize, 0usize);
    for g in b.lines.iter().flat_map(|l| &l.glyphs) {
        if g.w.min(g.h) < 4.0 { continue; }
        total += 1;
        if g.w.max(g.h) > 1.35 * g.w.min(g.h) { continue; }
        let (x0, y0, w, h) = g.pixels(mask.w as u32, mask.h as u32);
        let (x0, y0, w, h) = (x0 as usize, y0 as usize, w as usize, h as usize);
        if w == 0 || h == 0 { continue; }
        let ink = |x: usize, y: usize| mask.ink[(y0 + y) * mask.w + x0 + x];
        let filled = (0..h).map(|y| (0..w).filter(|&x| ink(x, y)).count()).sum::<usize>() as f32 / (w * h) as f32;
        if !(0.68..=0.88).contains(&filled) { continue; }
        // Background reachable from the box border; whatever background is left is a counter.
        let mut seen = vec![false; w * h];
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for x in 0..w { stack.push((x, 0)); stack.push((x, h - 1)); }
        for y in 0..h { stack.push((0, y)); stack.push((w - 1, y)); }
        while let Some((x, y)) = stack.pop() {
            if seen[y * w + x] || ink(x, y) { continue; }
            seen[y * w + x] = true;
            if x > 0 { stack.push((x - 1, y)); }
            if y > 0 { stack.push((x, y - 1)); }
            if x + 1 < w { stack.push((x + 1, y)); }
            if y + 1 < h { stack.push((x, y + 1)); }
        }
        let enclosed = (0..w * h).filter(|&i| !seen[i] && !ink(i % w, i / w)).count();
        if (enclosed as f32) < 0.01 * (w * h) as f32 { blobs += 1; }
    }
    total >= 4 && blobs as f32 >= 0.5 * total as f32
}

/// Fill and size from which a component is a plate (`is_plate`).
const PLATE_FILL: f32 = 0.8;
/// As wide as a one-component line must be to stay a line (`lines_from`), so a plate on its own is kept.
const PLATE_ASPECT: f32 = 2.0;
const PLATE_HEIGHT: f32 = 2.5;

/// A large solid rectangle: the plate of a label, its letters holes in it (light plate, dark text, while the mask takes
/// light for ink). It is the background of text, not a glyph: as a member of a row it would join its neighbours across
/// the gaps between plates (a row of tiles becomes one line, read in one piece). A letter is never this full and this
/// wide at once while being much taller than the typical component; a dash or a rule is low.
fn is_plate(c: &Component, typical: f32) -> bool {
    c.pixels as f32 >= PLATE_FILL * c.rect.area() && c.rect.w >= PLATE_ASPECT * c.rect.h && c.rect.h >= PLATE_HEIGHT * typical
}

/// A component that can be a glyph of the local-contrast pass: text-sized, compact, neither a hairline nor a blob.
fn local_glyph_like(c: &Component, frame_h: f32) -> bool {
    let (w, h) = (c.rect.w, c.rect.h);
    let fill = c.pixels as f32 / c.rect.area().max(1.0);
    (5.0..=(0.12 * frame_h).clamp(24.0, 64.0)).contains(&h) && w <= 4.0 * h + 10.0 && (0.12..=0.9).contains(&fill)
}

/// A line of glyphs in the local-contrast pass: enough glyphs, similar heights, a common baseline.
/// Scenery edges rarely satisfy all three.
fn text_like_line(l: &TextLine) -> bool {
    text_like_line_min(l, LOCAL_MIN_GLYPHS)
}

fn text_like_line_min(l: &TextLine, minimum_glyphs: usize) -> bool {
    let n = l.glyphs.len();
    // Below 6 px a line is a fragment (half a word cut by the mask), not something worth reading or translating.
    if n < minimum_glyphs || l.rect.w < 2.0 * l.rect.h || l.rect.h < LOCAL_MIN_LINE_HEIGHT { return false; }
    let h = median(l.glyphs.iter().map(|g| g.h).collect());
    if h <= 0.0 { return false; }
    let base = median(l.glyphs.iter().map(|g| g.bottom()).collect());
    let similar = l.glyphs.iter().filter(|g| g.h >= 0.5 * h && g.h <= 2.0 * h).count();
    let on_base = l.glyphs.iter().filter(|g| (g.bottom() - base).abs() <= 0.3 * h).count();
    // Cells of a mesh or rivets: identical in width and height, unlike letters.
    let cv = |v: Vec<f32>| { let m = v.iter().sum::<f32>() / v.len() as f32; (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt() / m.max(1e-3) };
    let uniform = n >= 6 && cv(l.glyphs.iter().map(|g| g.w).collect()) < 0.15 && cv(l.glyphs.iter().map(|g| g.h).collect()) < 0.15;
    similar as f32 >= 0.7 * n as f32 && on_base as f32 >= 0.5 * n as f32 && !uniform
}

/// Rows are gathered across the whole frame (`BlockDetector::lines_from`), and in a local-contrast mask the specks of the
/// scenery seed row bands at every height: the letters of one large word can fall into different bands and come out as
/// fragments side by side, each too short to pass as a line. Neighbouring fragments of large type on the same row, with
/// glyphs of one height, are put back together, but only when neither passes as a line on its own and together they make
/// a text-like line of letters of one cap height (`one_cap_height`): every line that passed before stays as it was.
fn join_row_fragments(img: &RgbaImage, mask: &InkMask, lines: Vec<TextLine>) -> Vec<TextLine> {
    let height = |l: &TextLine| median(l.glyphs.iter().map(|g| g.h).collect());
    let fragment = |l: &TextLine| !text_like_line_min(l, 3) && height(l) >= LARGE_GLYPH;
    let (mut fragments, mut out): (Vec<TextLine>, Vec<TextLine>) = lines.into_iter().partition(|l| fragment(l));
    fragments.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    let mut used = vec![false; fragments.len()];
    for i in 0..fragments.len() {
        if used[i] { continue; }
        let mut line = fragments[i].clone();
        let mut members = vec![i];
        for j in i + 1..fragments.len() {
            if used[j] { continue; }
            let (a, b) = (&line, &fragments[j]);
            let (ha, hb) = (height(a), height(b));
            let same_height = ha.max(hb) <= 1.45 * ha.min(hb);
            let same_row = a.rect.bottom().min(b.rect.bottom()) - a.rect.y.max(b.rect.y) >= 0.7 * a.rect.h.min(b.rect.h);
            let gap = b.rect.x - a.rect.right();
            if !(same_height && same_row && gap >= -0.5 * ha.max(hb) && gap <= 1.5 * ha.max(hb)) { continue; }
            let mut glyphs = line.glyphs.clone();
            glyphs.extend(b.glyphs.iter().copied());
            let rect = a.rect.union(&b.rect);
            line = TextLine { rect, glyphs, ink_color: line.ink_color };
            members.push(j);
        }
        if members.len() > 1 && text_like_line(&line) && one_cap_height(&line) {
            line.ink_color = mean_color(img, &line.rect, |x, y| mask.at(x, y)).unwrap_or(line.ink_color);
            for &m in &members { used[m] = true; }
            out.push(line);
        }
    }
    out.extend(fragments.into_iter().zip(used).filter(|(_, u)| !u).map(|(l, _)| l));
    out
}

/// A value under its caption ("TUTORIAL" over "TR-1"): a block of large type with fewer glyphs than a full row, right
/// below a smaller full one and as wide as it, whose short text is the point of the label.
const VALUE_GAP: f32 = 1.0;
/// Glyphs of such a value are at least this many times as tall as those of the caption.
const VALUE_SCALE: f32 = 1.5;

/// The blocks of one local-contrast window worth keeping: those with a full text-like row, and the short blocks of large
/// type that are the value of such a block's caption. Standalone short blocks stay rejected as noise.
fn keep_with_value_labels(blocks: Vec<DetectedTextBlock>) -> Vec<DetectedTextBlock> {
    let (full, short): (Vec<_>, Vec<_>) = blocks.into_iter().partition(|b| b.lines.iter().any(text_like_line));
    let values: Vec<DetectedTextBlock> = short.into_iter().filter(|v| {
        let height = median(v.lines.iter().map(|l| l.glyph_height()).collect());
        height >= LARGE_GLYPH && full.iter().any(|c| {
            let caption = median(c.lines.iter().map(|l| l.glyph_height()).collect());
            let overlap = (c.rect.right().min(v.rect.right()) - c.rect.x.max(v.rect.x)).max(0.0);
            let gap = v.rect.y - c.rect.bottom();
            caption > 0.0 && height >= VALUE_SCALE * caption && overlap >= 0.6 * v.rect.w.min(c.rect.w)
                && (0.0..=VALUE_GAP * v.rect.h).contains(&gap)
        })
    }).collect();
    full.into_iter().chain(values).collect()
}

/// At least `LOCAL_MIN_GLYPHS` glyphs of the line share one height (within 15 %), one top and one bottom (within a fifth of
/// the height), and none lies inside another: capitals or digits of one size. Outlines of a picture cut into pieces (eyes,
/// folds) differ in height; an icon and its rim are nested.
fn one_cap_height(l: &TextLine) -> bool {
    let h = median(l.glyphs.iter().map(|g| g.h).collect());
    let top = median(l.glyphs.iter().map(|g| g.y).collect());
    let base = median(l.glyphs.iter().map(|g| g.bottom()).collect());
    l.glyphs.iter().filter(|g| (g.h - h).abs() <= 0.15 * h && (g.y - top).abs() <= 0.2 * h && (g.bottom() - base).abs() <= 0.2 * h)
        .count() >= LOCAL_MIN_GLYPHS
        // Letters of a line stand side by side; a shape inside another one (the rim of an icon around it) is a picture.
        && !l.glyphs.iter().enumerate().any(|(i, g)| l.glyphs.iter().enumerate()
            .any(|(j, o)| i != j && o.area() >= g.area() && g.intersection(o) >= 0.8 * g.area()))
}

/// Large connected components are scenery unless they look like oversized type: several of them side by
/// side, similar in height, strokes rather than filled shapes. Judged in context, not by one absolute height.
fn filter_scenery(comps: Vec<Component>, fw: f32, fh: f32, report: &mut DetectionReport) -> Vec<Component> {
    let ordinary = (0.15 * fh).max(64.0);
    let (mut out, mut big) = (Vec::with_capacity(comps.len()), Vec::new());
    for c in comps {
        if c.rect.w > 0.6 * fw && c.rect.h > 0.4 * fh { report.rejected_scenery += 1; }
        else if c.rect.h <= ordinary { out.push(c); }
        else { big.push(c); }
    }
    for (i, c) in big.iter().enumerate() {
        let fill = c.pixels as f32 / c.rect.area().max(1.0);
        let shaped = c.rect.h <= 0.6 * fh && fill <= 0.7 && c.rect.w >= 0.1 * c.rect.h && c.rect.w <= 3.0 * c.rect.h;
        let peers = big.iter().enumerate().filter(|(j, o)| {
            let (a, b) = (c.rect, o.rect);
            *j != i && a.h.max(b.h) <= 2.0 * a.h.min(b.h)
                && a.bottom().min(b.bottom()) - a.y.max(b.y) >= 0.5 * a.h.min(b.h)
                && (a.x.max(b.x) - a.right().min(b.right())) <= 1.5 * a.h.max(b.h)
        }).count();
        if shaped && peers >= 2 { out.push(*c); } else { report.rejected_scenery += 1; }
    }
    out
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() { return 0.0; }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn color_distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (*x as f32 - y as f32).powi(2)).sum::<f32>().sqrt()
}

impl BlockDetector {
    /// Маска «чернил» для всего кадра. Считается один раз и используется всеми стадиями.
    pub fn ink_mask(img: &RgbaImage) -> InkMask {
        let (w, h) = (img.width() as usize, img.height() as usize);
        // Яркость считается один раз в плоскость u8 (цикл без ветвлений векторизуется),
        // гистограмма — четырьмя независимыми счётчиками: один массив с `+= 1` по случайному
        // индексу упирается в зависимость чтения после записи на однотонных кадрах.
        let luma_plane: Vec<u8> = img.as_raw().as_chunks::<4>().0.iter().map(|p| luma(p)).collect();
        let mut lanes = [[0u32; 256]; 4];
        let (quads, rest) = luma_plane.as_chunks::<4>();
        for q in quads {
            for (lane, &v) in lanes.iter_mut().zip(q) { lane[v as usize] += 1; }
        }
        for &v in rest { lanes[0][v as usize] += 1; }
        let mut hist = [0u64; 256];
        for (i, slot) in hist.iter_mut().enumerate() { *slot = lanes.iter().map(|l| l[i] as u64).sum(); }
        let total = (w * h).max(1) as u64;
        let t = otsu(&hist, total);
        let dark: u64 = hist[..=t as usize].iter().sum();
        let ink_is_dark = dark * 2 < total;
        let ink = luma_plane.iter().map(|&l| (l <= t) == ink_is_dark).collect();
        InkMask { w, h, ink, ink_is_dark }
    }

    pub fn detect_text_blocks(&self, img: &RgbaImage, mask: &InkMask) -> Vec<DetectedTextBlock> {
        self.detect(img, mask).0
    }

    /// Blocks of the global (Otsu) mask only, with a report of what the limit removed.
    pub fn detect(&self, img: &RgbaImage, mask: &InkMask) -> (Vec<DetectedTextBlock>, DetectionReport) {
        let mut report = DetectionReport::default();
        let blocks = self.collect(img, mask, &mut report);
        let blocks = self.finish(blocks, mask.w as f32, mask.h as f32, &mut report);
        (blocks, report)
    }

    /// The global pass plus a local-contrast pass for text the global threshold cannot see. Ink of the
    /// blocks found by the second pass is added to `mask`, so that later stages (font analysis, background)
    /// see those glyphs as ink too.
    pub fn detect_with_local(&self, img: &RgbaImage, mask: &mut InkMask) -> (Vec<DetectedTextBlock>, DetectionReport) {
        let mut report = DetectionReport::default();
        let mut blocks = self.collect(img, mask, &mut report);
        let extra = self.local_blocks(img, mask, &blocks);
        report.local_blocks = extra.len();
        blocks.extend(extra);
        let blocks = self.finish(blocks, mask.w as f32, mask.h as f32, &mut report);
        (blocks, report)
    }

    /// The local-contrast candidates of one colour plane (0 brightness, 1 red, 2 blue), both polarities in order: the
    /// mask and the blocks of text-like lines found in it. Pure: nothing here depends on what other planes found, so the
    /// planes are searched on separate threads and the choice between candidates (which does depend on the order) is
    /// made afterwards.
    fn local_candidates(&self, img: &RgbaImage, plane: usize, w: usize, h: usize) -> Vec<(InkMask, Vec<DetectedTextBlock>)> {
        let fh = h as f32;
        let fw = w as f32;
        let pixels = img.as_raw().as_chunks::<4>().0;
        // Brightness first; then the red and blue planes, in which tinted text on a glowing panel of another
        // tint (blue on light blue) stands out far more than it does in brightness.
        let values: Vec<u8> = match plane {
            0 => pixels.iter().map(|p| luma(p)).collect(),
            1 => pixels.iter().map(|p| p[0]).collect(),
            _ => pixels.iter().map(|p| p[2]).collect(),
        };
        let mut found = Vec::new();
        for local in super::local_ink::contrast_masks(&values, w, h) {
            let comps: Vec<Component> = components(&local).into_iter()
                .filter(|c| c.pixels >= 6 && local_glyph_like(c, fh))
                .collect();
            if comps.len() < LOCAL_MIN_GLYPHS { continue; }
            // A paragraph may end with a short word. Keep a geometrically
            // compatible short continuation only when the same block also
            // contains a full text-like row; standalone noise stays rejected.
            let lines: Vec<TextLine> = join_row_fragments(img, &local, self.lines_from(img, &local, comps, fw, 0)).into_iter()
                .filter(|line| text_like_line_min(line, 3)).collect();
            let blocks = keep_with_value_labels(self.blocks_from_lines_by_glyph_height(lines, true));
            found.push((local, blocks));
        }
        found
    }

    fn local_blocks(&self, img: &RgbaImage, mask: &mut InkMask, existing: &[DetectedTextBlock]) -> Vec<DetectedTextBlock> {
        let (w, h) = (mask.w, mask.h);
        let (fw, fh) = (w as f32, h as f32);
        if w < 32 || h < 32 { return Vec::new(); }
        // The three planes are independent: one thread each, so the frame is not walked six times in a row on one core.
        let candidates: Vec<Vec<(InkMask, Vec<DetectedTextBlock>)>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..3).map(|plane| scope.spawn(move || self.local_candidates(img, plane, w, h))).collect();
            workers.into_iter().map(|worker| worker.join().expect("local-contrast search panicked")).collect()
        });
        let mut accepted: Vec<DetectedTextBlock> = Vec::new();
        // In the order the planes and polarities were searched before (the later candidate yields to the earlier).
        for (local, blocks) in candidates.into_iter().flatten() {
            let covered = |candidate: &DetectedTextBlock, blocks: &[DetectedTextBlock]| {
                blocks.iter().any(|b| b.rect.expand(0.5 * b.line_height().max(4.0), fw, fh).contains(&candidate.rect))
                    || candidate.lines.iter().all(|line| blocks.iter().flat_map(|b| &b.lines).any(|existing|
                        existing.rect.expand(0.5 * existing.rect.h.max(4.0), fw, fh).contains(&line.rect)))
            };
            for block in blocks {
                // Deduplicate complete text candidates. Removing covered
                // letters before line reconstruction perpetuated fragmented
                // global detections and made complete OCR crops impossible.
                if covered(&block, existing) || covered(&block, &accepted) { continue; }
                if is_blob_pattern(&block, mask) { continue; }
                let (x0, y0, bw, bh) = block.rect.pixels(w as u32, h as u32);
                for y in y0 as usize..(y0 + bh) as usize {
                    for x in x0 as usize..(x0 + bw) as usize {
                        if local.at(x, y) { mask.ink[y * w + x] = true; }
                    }
                }
                accepted.push(block);
            }
        }
        accepted
    }

    fn collect(&self, img: &RgbaImage, mask: &InkMask, report: &mut DetectionReport) -> Vec<DetectedTextBlock> {
        let (fw, fh) = (mask.w as f32, mask.h as f32);
        if mask.w < 8 || mask.h < 8 { return Vec::new(); }
        let comps: Vec<Component> = components(mask).into_iter().filter(|c| c.pixels >= 3).collect();
        let comps = filter_scenery(comps, fw, fh, report);
        report.components += comps.len();
        if comps.is_empty() { return Vec::new(); }
        let lines = self.lines_from(img, mask, comps, fw, 0);
        let blocks = self.blocks_from_lines(lines);
        let before = blocks.len();
        let blocks: Vec<_> = blocks.into_iter().filter(|b| !is_blob_pattern(b, mask)).collect();
        report.rejected_pattern += before - blocks.len();
        blocks
    }

    /// Glyph components → text lines. Components much smaller than the typical glyph are accents and
    /// dots and join the line they touch; those that touch none (a small label under a large title)
    /// are grouped again on their own scale, one level deep.
    fn lines_from(&self, img: &RgbaImage, mask: &InkMask, comps: Vec<Component>, fw: f32, depth: u8) -> Vec<TextLine> {
        let typical = median(comps.iter().filter(|c| c.rect.h >= 4.0).map(|c| c.rect.h).collect());
        let main_min = (typical * 0.4).max(4.0);
        let (mut main, small): (Vec<_>, Vec<_>) = comps.into_iter().partition(|c| c.rect.h >= main_min);
        // Слишком широкие при малой высоте — линии-разделители интерфейса.
        main.retain(|c| !(c.rect.w > 8.0 * c.rect.h && c.rect.w > 0.3 * fw && c.pixels as f32 > 0.8 * c.rect.area()));
        // A plate is a line of its own: it neither joins its neighbours in a row nor loses the text it carries.
        let (plates, mut main): (Vec<_>, Vec<_>) = main.into_iter().partition(|c| is_plate(c, typical));

        // Строки: сначала полосы по перекрытию по вертикали, затем разрывы по горизонтали.
        main.sort_by(|a, b| a.rect.center().1.total_cmp(&b.rect.center().1));
        let mut bands: Vec<(f32, f32, Vec<Rect>)> = Vec::new(); // top, bottom (средние), члены
        for c in &main {
            let r = c.rect;
            let fit = bands.iter_mut().find(|(t, b, _)| {
                let overlap = r.bottom().min(*b) - r.y.max(*t);
                overlap >= 0.5 * r.h.min(*b - *t)
            });
            match fit {
                Some((t, b, members)) => {
                    let n = members.len() as f32;
                    *t = (*t * n + r.y) / (n + 1.0);
                    *b = (*b * n + r.bottom()) / (n + 1.0);
                    members.push(r);
                }
                None => bands.push((r.y, r.bottom(), vec![r])),
            }
        }
        let mut lines: Vec<TextLine> = Vec::new();
        for (_, _, mut members) in bands {
            members.sort_by(|a, b| a.x.total_cmp(&b.x));
            let h = median(members.iter().map(|r| r.h).collect());
            let mut current: Vec<Rect> = Vec::new();
            let mut right = f32::MIN;
            for r in members {
                if !current.is_empty() && r.x - right > 1.5 * h {
                    lines.push(TextLine { rect: Rect::default(), glyphs: std::mem::take(&mut current), ink_color: [0; 3] });
                }
                right = right.max(r.right());
                current.push(r);
            }
            if !current.is_empty() {
                lines.push(TextLine { rect: Rect::default(), glyphs: current, ink_color: [0; 3] });
            }
        }
        lines.extend(plates.iter().map(|p| TextLine { rect: Rect::default(), glyphs: vec![p.rect], ink_color: [0; 3] }));
        // Точки, акценты, запятые — к ближайшей строке, которая их накрывает по горизонтали.
        let mut orphans: Vec<Component> = Vec::new();
        for c in small {
            let r = c.rect;
            let (cx, cy) = r.center();
            let host = lines.iter_mut()
                .map(|l| { let lr = l.glyphs.iter().fold(l.glyphs[0], |a, g| a.union(g)); (l, lr) })
                .filter(|(_, lr)| cx >= lr.x - 2.0 && cx <= lr.right() + 2.0)
                .map(|(l, lr)| (l, (cy - lr.center().1).abs() - lr.h / 2.0))
                .filter(|(l, d)| *d <= ACCENT_REACH * l.glyphs.iter().map(|g| g.h).fold(0.0, f32::max))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match host {
                Some((l, _)) => l.glyphs.push(r),
                None => orphans.push(c),
            }
        }
        for l in &mut lines {
            l.rect = l.glyphs.iter().fold(l.glyphs[0], |a, g| a.union(g));
            l.ink_color = mean_color(img, &l.rect, |x, y| mask.at(x, y)).unwrap_or([255; 3]);
        }
        // Одиночная крошечная «строка» — шум.
        lines.retain(|l| l.glyphs.len() >= 2 || l.rect.w >= 2.0 * l.rect.h);
        // A line under 5 px high with only a few components is a speck of scenery or half a glyph row.
        lines.retain(|l| l.rect.h >= 5.0 || l.glyphs.len() >= 5);
        if depth == 0 && orphans.len() >= 3 {
            lines.extend(self.lines_from(img, mask, orphans, fw, depth + 1));
        }
        lines
    }

    fn blocks_from_lines(&self, lines: Vec<TextLine>) -> Vec<DetectedTextBlock> {
        self.blocks_from_lines_by_glyph_height(lines, false)
    }

    fn blocks_from_lines_by_glyph_height(&self, mut lines: Vec<TextLine>, glyph_height: bool) -> Vec<DetectedTextBlock> {
        // Блоки: строка присоединяется к блоку, если она прямо под его последней строкой,
        // той же высоты, того же цвета и выровнена с ним.
        lines.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
        let mut blocks: Vec<Vec<TextLine>> = Vec::new();
        for line in lines {
            let r = line.rect;
            let host = blocks.iter_mut().find(|b| {
                let last = b.last().unwrap().rect;
                let h = last.h.max(r.h);
                let gap = r.y - last.bottom();
                // Ascenders and descenders change row bounds, especially on a short
                // final word. Compare the typical glyph size across both rows.
                let previous_glyph_height = median(b.last().unwrap().glyphs.iter().map(|g| g.h).collect());
                let current_glyph_height = median(line.glyphs.iter().map(|g| g.h).collect());
                let same_height = if glyph_height {
                    previous_glyph_height.max(current_glyph_height)
                        / previous_glyph_height.min(current_glyph_height).max(1.0) <= 1.45
                } else { last.h.max(r.h) / last.h.min(r.h).max(1.0) <= 1.45 };
                let aligned = r.intersection(&Rect::new(last.x, r.y, last.w, r.h)) > 0.0
                    || (r.x - last.x).abs() <= 2.0 * h
                    || (r.center().0 - last.center().0).abs() <= 2.0 * h;
                let same_color = color_distance(b.last().unwrap().ink_color, line.ink_color) <= 70.0;
                gap >= -0.3 * h && gap <= 1.0 * h && same_height && aligned && same_color
            });
            match host {
                Some(b) => b.push(line),
                None => blocks.push(vec![line]),
            }
        }
        // Fields the grouping above joined because they look alike (a name in the dialogue's colour right
        // above it, menu entries in one style) are told apart by the pitch of their lines. Each part is a
        // block of its own and is classified on its own below.
        let blocks = blocks.into_iter().flat_map(|lines| super::split::split_by_line_gaps(lines, self.line_gap_factor));
        blocks.map(|lines| {
            let rect = lines.iter().skip(1).fold(lines[0].rect, |a, l| a.union(&l.rect));
            let ink_color = lines[0].ink_color;
            DetectedTextBlock { rect, lines, block_type: TextBlockType::Unknown, ink_color }
        }).collect()
    }

    /// Applies the resource bound deterministically, orders the rest by position and types them.
    fn finish(&self, mut out: Vec<DetectedTextBlock>, fw: f32, fh: f32, report: &mut DetectionReport) -> Vec<DetectedTextBlock> {
        report.candidates = out.len();
        // Area, then position, decide who stays: the same frame always gives the same set.
        out.sort_by(|a, b| b.rect.area().total_cmp(&a.rect.area())
            .then_with(|| a.rect.y.total_cmp(&b.rect.y)).then_with(|| a.rect.x.total_cmp(&b.rect.x)));
        if out.len() > self.max_blocks {
            report.dropped = out.split_off(self.max_blocks).into_iter().map(|b| b.rect).collect();
            report.dropped.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));
        }
        out.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y).then_with(|| a.rect.x.total_cmp(&b.rect.x)));
        classify(&mut out, fw, fh);
        out
    }
}

/// The type of every block again, after the set of blocks changed (a block was split elsewhere).
pub fn classify_blocks(blocks: &mut [DetectedTextBlock], frame_w: f32, frame_h: f32) {
    classify(blocks, frame_w, frame_h);
}

/// Тип блока по геометрии. Только уверенные случаи, иначе `Unknown`.
fn classify(blocks: &mut [DetectedTextBlock], fw: f32, fh: f32) {
    let rects: Vec<(Rect, usize)> = blocks.iter().map(|b| (b.rect, b.lines.len())).collect();
    let short = |r: &Rect, lines: usize| lines == 1 && r.w < 0.3 * fw;
    let mut types = vec![TextBlockType::Unknown; blocks.len()];
    for (i, (r, lines)) in rects.iter().enumerate() {
        let (cx, cy) = r.center();
        if *lines >= 2 && r.w >= 0.35 * fw {
            types[i] = TextBlockType::Dialogue;
        } else if *lines == 1 && r.w >= 0.25 * fw && (cx - fw / 2.0).abs() <= 0.1 * fw && cy >= 0.55 * fh {
            types[i] = TextBlockType::Subtitle;
        }
    }
    for (i, (r, lines)) in rects.iter().enumerate() {
        if types[i] != TextBlockType::Unknown || !short(r, *lines) { continue; }
        // Имя прямо над репликой или субтитром, у её левого края или по центру.
        let names = rects.iter().enumerate().any(|(j, (o, _))| {
            j != i && matches!(types[j], TextBlockType::Dialogue | TextBlockType::Subtitle)
                && o.y >= r.bottom() && o.y - r.bottom() <= 1.5 * r.h
                && ((r.x - o.x).abs() <= 0.1 * fw || (r.center().0 - o.center().0).abs() <= 0.1 * fw)
        });
        let same_row = rects.iter().enumerate().filter(|(j, (o, l))| {
            *j != i && short(o, *l) && o.bottom().min(r.bottom()) - o.y.max(r.y) >= 0.5 * o.h.min(r.h)
        }).count();
        let same_column = rects.iter().enumerate().filter(|(j, (o, l))| {
            *j != i && short(o, *l) && (o.x - r.x).abs() <= 0.5 * r.h && (o.y - r.y).abs() <= 4.0 * r.h
        }).count();
        types[i] = if names { TextBlockType::CharacterName }
            else if same_row >= 1 { TextBlockType::Button }
            else if same_column >= 2 { TextBlockType::MenuItem }
            else { TextBlockType::Unknown };
    }
    for (b, t) in blocks.iter_mut().zip(types) { b.block_type = t; }
}

#[cfg(test)]
mod tests {
    /// A block of one line of glyphs `h` tall, `n` of them, starting at (`x`, `y`).
    fn label(x: f32, y: f32, n: usize, h: f32) -> DetectedTextBlock {
        let glyphs: Vec<Rect> = (0..n).map(|i| Rect::new(x + i as f32 * (h * 0.8), y, h * (0.4 + 0.15 * (i % 3) as f32), h)).collect();
        let rect = glyphs.iter().skip(1).fold(glyphs[0], |r, g| r.union(g));
        DetectedTextBlock { rect, lines: vec![TextLine { rect, glyphs, ink_color: [240; 3] }],
            block_type: TextBlockType::Unknown, ink_color: [240; 3] }
    }

    #[test]
    fn a_short_value_of_large_type_under_its_caption_is_kept_but_standalone_short_blocks_are_not() {
        let caption = label(1240.0, 610.0, 8, 10.0);
        // "TR-1" right under "TUTORIAL", three times as tall.
        let value = label(1238.0, 628.0, 3, 30.0);
        let noise = label(300.0, 628.0, 3, 30.0);
        let small = label(1240.0, 624.0, 3, 10.0);
        let far = label(1238.0, 700.0, 3, 30.0);
        let kept = keep_with_value_labels(vec![caption.clone(), value.clone(), noise, small, far]);
        let rects: Vec<Rect> = kept.iter().map(|b| b.rect).collect();
        assert_eq!(rects, vec![caption.rect, value.rect]);
        // Without a caption nothing short is kept.
        assert!(keep_with_value_labels(vec![value]).is_empty());
    }

    use super::*;
    use crate::layout::testing::*;

    #[test]
    fn dialogue_scene_splits_into_name_dialogue_and_buttons() {
        let mut img = canvas(1200, 500, [25, 30, 40]);
        // Имя персонажа другим цветом, над репликой.
        draw_line(&mut img, 100, 100, 8, 14, 4, 22, 3, [255, 200, 60], false);
        // Реплика в две строки.
        draw_line(&mut img, 100, 140, 40, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 100, 172, 30, 14, 4, 22, 3, [240, 240, 240], false);
        // Две кнопки в одном ряду, далеко друг от друга.
        draw_line(&mut img, 300, 380, 7, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 700, 380, 6, 14, 4, 22, 3, [240, 240, 240], false);

        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let summary: Vec<_> = blocks.iter().map(|b| (b.block_type, b.lines.len(), b.rect.x as u32, b.rect.y as u32)).collect();
        assert_eq!(blocks.len(), 4, "{summary:?}");
        assert_eq!(summary[0], (TextBlockType::CharacterName, 1, 100, 100));
        assert_eq!(summary[1], (TextBlockType::Dialogue, 2, 100, 140));
        assert_eq!((summary[2].0, summary[3].0), (TextBlockType::Button, TextBlockType::Button));
        assert!(blocks[1].ink_color[0] > 200 && blocks[0].ink_color[2] < 120);
    }

    /// A character name and the dialogue below it, all in one colour and one size, the name one blank line
    /// above the dialogue (pitch 62 px = 1.94 line heights): the detector's grouping takes them for one block.
    fn glued_scene() -> RgbaImage {
        let mut img = canvas(1200, 500, [25, 30, 40]);
        draw_prose_line(&mut img, 100, 100, 8, 14, 4, 32, 3, [240, 240, 240]);
        draw_prose_line(&mut img, 100, 162, 34, 14, 4, 32, 3, [240, 240, 240]);
        draw_prose_line(&mut img, 100, 208, 28, 14, 4, 32, 3, [240, 240, 240]);
        img
    }

    #[test]
    fn a_name_glued_to_its_dialogue_is_split_and_each_part_gets_its_own_type() {
        let img = glued_scene();
        let mask = BlockDetector::ink_mask(&img);
        // Without the rule the grouping makes one block of three lines...
        let glued = BlockDetector { line_gap_factor: 100.0, ..BlockDetector::default() }.detect_text_blocks(&img, &mask);
        assert_eq!(glued.iter().map(|b| b.lines.len()).collect::<Vec<_>>(), vec![3]);
        // ...with it the name and the dialogue are two, each classified on its own.
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let summary: Vec<_> = blocks.iter().map(|b| (b.block_type, b.lines.len(), b.rect.y as u32)).collect();
        assert_eq!(summary.len(), 2, "{summary:?}");
        assert_eq!((summary[0].0, summary[0].1), (TextBlockType::CharacterName, 1), "{summary:?}");
        assert_eq!((summary[1].0, summary[1].1), (TextBlockType::Dialogue, 2), "{summary:?}");
        assert!(blocks[0].rect.bottom() < blocks[1].rect.y, "the parts do not overlap");
        // The union of the parts is what the block was.
        let whole = glued[0].rect;
        assert_eq!(blocks[0].rect.union(&blocks[1].rect), whole);
    }

    #[test]
    fn ordinary_paragraphs_and_menus_are_left_whole() {
        let mut img = canvas(1200, 500, [25, 30, 40]);
        // Three lines of a paragraph, 46 px apart (1.44 line heights).
        for y in [100, 146, 192] { draw_prose_line(&mut img, 100, y, 34, 14, 4, 32, 3, [240, 240, 240]); }
        let blocks = BlockDetector::default().detect_text_blocks(&img, &BlockDetector::ink_mask(&img));
        assert_eq!(blocks.iter().map(|b| b.lines.len()).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn centred_bottom_line_is_a_subtitle() {
        let mut img = canvas(1000, 300, [60, 70, 50]);
        draw_line(&mut img, 250, 230, 30, 13, 4, 20, 3, [255, 255, 255], false);
        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, TextBlockType::Subtitle);
        assert!((blocks[0].line_height() - 20.0).abs() < 1.0);
    }

    #[test]
    fn empty_and_tiny_frames() {
        let img = canvas(300, 100, [90, 90, 90]);
        assert!(BlockDetector::default().detect_text_blocks(&img, &BlockDetector::ink_mask(&img)).is_empty());
        let tiny = canvas(4, 4, [0, 0, 0]);
        assert!(BlockDetector::default().detect_text_blocks(&tiny, &BlockDetector::ink_mask(&tiny)).is_empty());
    }

    /// Light capitals on a dark plate in a light frame: the global mask takes the plate for ink and the letters for
    /// background, so only the local pass can find them. Light specks elsewhere on the same rows seed bands of the
    /// frame-wide row grouping first and the word falls apart into two short fragments (two and three letters), neither
    /// of which passes as a line: the fragments are put back together into one word.
    #[test]
    fn a_large_word_of_reversed_polarity_is_not_split_by_specks_on_its_rows() {
        let mut img = canvas(1000, 600, [200, 200, 200]);
        for y in 370..490 { for x in 500..900 { img.put_pixel(x, y, image::Rgba([40, 40, 40, 255])); } }
        // Ring-shaped specks of the scenery, far to the left, higher than the word's centre.
        for i in 0..10u32 {
            let x0 = 20 + 40 * i;
            for y in 392..404 { for x in x0..x0 + 12 {
                if !(x0 + 2..x0 + 10).contains(&x) || !(394..402).contains(&y) { img.put_pixel(x, y, image::Rgba([255, 255, 255, 255])); }
            } }
        }
        // Five letters; the last three sit a few pixels lower (a slanted baseline, a different cut of the font).
        let next = draw_line(&mut img, 540, 396, 2, 26, 10, 60, 6, [220, 220, 220], false);
        draw_line(&mut img, next, 404, 3, 26, 10, 60, 6, [220, 220, 220], false);

        let mut mask = BlockDetector::ink_mask(&img);
        assert!(mask.ink_is_dark, "the plate is the ink of the global mask");
        let (blocks, _) = BlockDetector::default().detect_with_local(&img, &mut mask);
        let word = Rect::new(540.0, 396.0, 170.0, 68.0);
        let summary: Vec<_> = blocks.iter().map(|b| (b.rect.x as u32, b.rect.y as u32, b.rect.w as u32, b.rect.h as u32)).collect();
        assert!(blocks.iter().any(|b| b.rect.expand(2.0, 1000.0, 600.0).contains(&word) && b.rect.w <= word.w + 4.0), "{summary:?}");
    }

    /// A row of light label plates with dark text on them, closer to each other than one and a half plate heights, under a
    /// line of light text: the mask takes light for ink, so each plate is one solid component with its letters as holes.
    /// The plates are not glyphs of one line: each stays a block of its own (its text is read on its own), none reaches
    /// over to the next. Solid letters that are narrow (`I`) or low (a dash) are still glyphs of their line.
    #[test]
    fn label_plates_in_a_row_are_not_joined_into_one_line() {
        let mut img = canvas(1400, 500, [20, 60, 120]);
        draw_line(&mut img, 100, 60, 30, 13, 4, 20, 3, [240, 240, 240], false);
        let plates: Vec<Rect> = (0..6).map(|i| Rect::new(100.0 + 200.0 * i as f32, 150.0, 160.0, 68.0)).collect();
        for p in &plates {
            let (x0, y0) = (p.x as u32, p.y as u32);
            for y in y0..y0 + 68 { for x in x0..x0 + 160 { img.put_pixel(x, y, image::Rgba([235, 240, 245, 255])); } }
            draw_line(&mut img, x0 + 12, y0 + 24, 8, 12, 4, 20, 3, [30, 40, 60], false);
        }
        // Large letters with solid strokes between them: an `I` (narrow) and a dash (low), both filled.
        let fill = |img: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32| {
            for y in y0..y0 + h { for x in x0..x0 + w { img.put_pixel(x, y, image::Rgba([240, 240, 240, 255])); } }
        };
        let next = draw_line(&mut img, 100, 340, 3, 40, 12, 64, 8, [240, 240, 240], false);
        fill(&mut img, next, 340, 14, 64);
        fill(&mut img, next + 26, 368, 52, 10);
        draw_line(&mut img, next + 90, 340, 3, 40, 12, 64, 8, [240, 240, 240], false);

        let mask = BlockDetector::ink_mask(&img);
        assert!(!mask.ink_is_dark, "the plates and the light text are the ink");
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let summary: Vec<_> = blocks.iter().map(|b| (b.rect.x as u32, b.rect.y as u32, b.rect.w as u32, b.rect.h as u32)).collect();
        for b in &blocks {
            assert!(plates.iter().filter(|p| b.rect.intersection(p) > 0.0).count() <= 1, "a block over two plates: {summary:?}");
        }
        for p in &plates {
            assert!(blocks.iter().any(|b| b.rect.intersection(p) >= 0.9 * p.area()), "plate {p:?} lost: {summary:?}");
        }
        let large: Vec<_> = blocks.iter().filter(|b| b.rect.y >= 330.0).collect();
        assert_eq!(large.len(), 1, "{summary:?}");
        assert_eq!(large[0].lines.len(), 1, "{summary:?}");
        assert!(large[0].rect.x <= 100.0 && large[0].rect.right() >= (next + 90 + 140) as f32, "{summary:?}");
    }

    /// Two lines one paragraph pitch apart (46 px = 1.44 line heights), aligned and of one size: `ink` is the colour of
    /// the second line, `word` the colour of the second word of the first line (five letters out of thirty-four).
    fn two_lines(second: [u8; 3], word: [u8; 3]) -> Vec<usize> {
        let mut img = canvas(1200, 400, [25, 30, 40]);
        let white = [240, 240, 240];
        let next = draw_prose_line(&mut img, 100, 100, 5, 14, 4, 32, 3, white);
        let next = draw_prose_line(&mut img, next + 12, 100, 5, 14, 4, 32, 3, word);
        draw_prose_line(&mut img, next + 12, 100, 24, 14, 4, 32, 3, white);
        draw_prose_line(&mut img, 100, 146, 34, 14, 4, 32, 3, second);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &BlockDetector::ink_mask(&img));
        blocks.iter().map(|b| b.lines.len()).collect()
    }

    /// Lines of sharply different ink are separate blocks even at the pitch of one paragraph; the colour is compared
    /// between whole lines, so a highlighted word inside a line of the paragraph does not cut it.
    #[test]
    fn a_line_of_another_ink_is_a_block_of_its_own_but_a_highlighted_word_is_not() {
        let (white, green) = ([240, 240, 240], [110, 170, 110]);
        assert_eq!(two_lines(green, white), vec![1, 1], "a white line over a green one");
        assert_eq!(two_lines(white, white), vec![2], "a paragraph of one colour");
        assert_eq!(two_lines(white, green), vec![2], "a paragraph with a word of another colour");
    }

    /// Fragments of one row are joined only into a row of capitals of one height; pieces of a picture of different heights
    /// and an icon with its rim inside stay apart (and are later rejected as lines).
    #[test]
    fn only_fragments_of_one_cap_height_are_joined_into_a_line() {
        let img = canvas(400, 200, [0, 0, 0]);
        let mask = BlockDetector::ink_mask(&img);
        let line = |glyphs: Vec<(f32, f32, f32, f32)>| {
            let glyphs: Vec<Rect> = glyphs.into_iter().map(|(x, y, w, h)| Rect::new(x, y, w, h)).collect();
            let rect = glyphs.iter().skip(1).fold(glyphs[0], |a, g| a.union(g));
            TextLine { rect, glyphs, ink_color: [255; 3] }
        };
        let joined = |a: TextLine, b: TextLine| join_row_fragments(&img, &mask, vec![a, b]).len() == 1;
        // Two capitals, then three, a little lower: one word.
        assert!(joined(line(vec![(10.0, 40.0, 24.0, 60.0), (44.0, 40.0, 24.0, 60.0)]),
            line(vec![(78.0, 44.0, 24.0, 60.0), (112.0, 44.0, 24.0, 60.0), (146.0, 44.0, 24.0, 60.0)])));
        // The same places, but the heights of a drawing's outlines (30 to 64 px).
        assert!(!joined(line(vec![(10.0, 70.0, 24.0, 30.0), (44.0, 62.0, 24.0, 40.0)]),
            line(vec![(78.0, 40.0, 24.0, 64.0), (112.0, 66.0, 24.0, 34.0), (146.0, 62.0, 24.0, 40.0)])));
        // Capitals of one height, but the last shape holds another one inside it (an icon and its rim).
        assert!(!joined(line(vec![(10.0, 40.0, 24.0, 60.0), (44.0, 40.0, 24.0, 60.0)]),
            line(vec![(78.0, 44.0, 24.0, 60.0), (112.0, 44.0, 50.0, 60.0), (118.0, 46.0, 40.0, 56.0)])));
    }
}
