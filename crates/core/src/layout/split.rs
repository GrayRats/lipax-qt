//! Splitting a detected block where its lines are too far apart to belong to one field.
//!
//! The detector joins a line to a block when it is directly below the block's last line, of the same
//! height, colour and alignment. That also joins two *fields* that merely look alike: a character name
//! printed in the dialogue's colour right above it, or menu entries set in one style. The rule here
//! tells them apart by the rhythm of the lines.
//!
//! # The rule
//!
//! Let the text direction be the angle θ (0 for upright text) and `n = (−sin θ, cos θ)` the unit
//! normal to the baselines, pointing down the page. For every line take its centre `c` (the mean of its
//! glyph centres) and its height `h`: the extent of its glyphs along `n`, from the top of the tallest
//! letter to the bottom of the lowest, floored at `1.35 ×` the median glyph height. The extent is
//! what a line occupies, about one em for text with ascenders and descenders; the floor is for text
//! set in capitals, which has neither, so that its extent (≈ 0.7 em) is not mistaken for the whole line.
//! The position of a line across the text is `p = c · n`.
//!
//! Lines sorted by `p` have the pitch `d_i = p_{i+1} − p_i`. With `h_avg` the mean height of the
//! candidate block, a cut is made between lines `i` and `i+1` whenever
//!
//! ```text
//! d_i > k · h_avg
//! ```
//!
//! and each part is checked again against its *own* `h_avg` (a part of mixed sizes is judged by its
//! own average). The default `k = 1.8`: ordinary line spacing is 1.2–1.5 em, so `d > 1.8 h` means a
//! clear gap of more than about `0.8 h`, which a paragraph never has. The detector joins lines up
//! to `1.0 h` apart, so for ordinary text the rule is decisive exactly in the band
//! `1.8 h < d ≤ 2.0 h`: fields set one blank line apart that the detector took for one.
//!
//! The pitch (centre to centre) is used, not the clear gap between boxes: with the clear gap the rule
//! `gap > 1.8 h` could never fire, because the detector does not join lines more than `1.0 h` apart.
//!
//! The angle comes from the lines with enough glyphs: for each, the median of the slopes between pairs of
//! glyph bottoms (robust against letters of different heights and against descenders); the median over
//! lines is taken. Under 0.5° the text counts as upright, and anything steeper than 30° is treated as
//! no rotation at all (vertical text is not split by this rule).

use super::Rect;
use super::block_detector::TextLine;

/// Default of `k`.
pub const DEFAULT_LINE_GAP_FACTOR: f32 = 1.8;
/// A line needs this many glyphs for its slope to mean anything.
const MIN_GLYPHS_FOR_SLOPE: usize = 4;
const MAX_GLYPHS_FOR_SLOPE: usize = 60;
/// Below this the text is upright: an angle that small is noise, and a tilted normal would only inflate the extents.
const DEAD_ZONE: f32 = 0.5 * std::f32::consts::PI / 180.0;
/// Steeper text is left alone.
const MAX_ANGLE: f32 = 30.0 * std::f32::consts::PI / 180.0;

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() { return 0.0; }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// Height of a line across the text (`normal` is the unit normal to the baselines): the extent of its
/// glyph boxes along the normal, at least `CAPITALS_FLOOR ×` the median glyph height. The line box if it has no glyphs.
pub fn line_height(line: &TextLine, normal: (f32, f32)) -> f32 {
    if line.glyphs.is_empty() { return line.rect.h; }
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for g in &line.glyphs {
        for (x, y) in [(g.x, g.y), (g.right(), g.y), (g.x, g.bottom()), (g.right(), g.bottom())] {
            let v = x * normal.0 + y * normal.1;
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    (hi - lo).max(CAPITALS_FLOOR * median(line.glyphs.iter().map(|g| g.h).collect()))
}

/// Text in capitals has no ascenders or descenders: its extent is the cap height, about 0.7 em, while
/// its lines are spaced like any other (1.2–1.5 em). A line is taken to be at least this many times the
/// height of a typical glyph, which puts the limit for such text back at about 1.7 em.
const CAPITALS_FLOOR: f32 = 1.35;

/// Centre of a line: the mean of its glyph centres, the middle of the box if it has none.
fn centre(line: &TextLine) -> (f32, f32) {
    if line.glyphs.is_empty() { return line.rect.center(); }
    let n = line.glyphs.len() as f32;
    let (sx, sy) = line.glyphs.iter().map(Rect::center).fold((0.0, 0.0), |(x, y), (cx, cy)| (x + cx, y + cy));
    (sx / n, sy / n)
}

/// Slope angle (radians) of the baseline of one line: the median of the slopes between pairs of glyph
/// bottoms (Theil–Sen). Letters of different heights move the glyph *centres* about, and descenders move
/// some bottoms; the median of pairwise slopes ignores both as long as most letters sit on the baseline.
fn slope(line: &TextLine) -> Option<f32> {
    if line.glyphs.len() < MIN_GLYPHS_FOR_SLOPE { return None; }
    // Long lines are sampled: the pairs grow with the square of the glyphs.
    let step = line.glyphs.len().div_ceil(MAX_GLYPHS_FOR_SLOPE);
    let points: Vec<(f32, f32)> = line.glyphs.iter().step_by(step).map(|g| (g.center().0, g.bottom())).collect();
    let apart = 0.8 * median(line.glyphs.iter().map(|g| g.w).collect());
    let mut slopes = Vec::new();
    for (i, a) in points.iter().enumerate() {
        for b in &points[i + 1..] {
            // Glyphs stacked in one column say nothing about the direction.
            if (b.0 - a.0).abs() >= apart.max(1.0) { slopes.push((b.1 - a.1) / (b.0 - a.0)); }
        }
    }
    if slopes.len() < MIN_GLYPHS_FOR_SLOPE { return None; }
    Some(median(slopes).atan())
}

/// Direction of the text (radians, 0 = upright): the median slope over the lines that have one,
/// 0 when none has or the text is steeper than 30°.
pub fn text_angle(lines: &[TextLine]) -> f32 {
    let angle = median(lines.iter().filter_map(slope).collect());
    if angle.abs() > MAX_ANGLE || angle.abs() < DEAD_ZONE { 0.0 } else { angle }
}

/// Splits `lines` (the lines of one detected block) into the fields the rule above finds. A block of
/// one line, or one that is evenly spaced, comes back whole; lines keep their order across the text.
pub fn split_by_line_gaps(lines: Vec<TextLine>, k: f32) -> Vec<Vec<TextLine>> {
    if lines.len() < 2 || !k.is_finite() || k <= 0.0 { return vec![lines]; }
    let theta = text_angle(&lines);
    let normal = (-theta.sin(), theta.cos());
    let mut placed: Vec<(f32, f32, TextLine)> = lines.into_iter().map(|l| {
        let (cx, cy) = centre(&l);
        (cx * normal.0 + cy * normal.1, line_height(&l, normal), l)
    }).collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    cut(&placed, k, &mut out);
    out
}

/// Checks one candidate against its own average height; recurses on the two sides of the first cut.
/// Generic over what the lines carry: detector lines here, indices of OCR lines in [`split_rects`].
fn cut<T: Clone>(candidate: &[(f32, f32, T)], k: f32, out: &mut Vec<Vec<T>>) {
    let average = candidate.iter().map(|l| l.1).sum::<f32>() / candidate.len().max(1) as f32;
    match candidate.windows(2).position(|w| w[1].0 - w[0].0 > k * average) {
        Some(i) => { cut(&candidate[..=i], k, out); cut(&candidate[i + 1..], k, out); }
        None => out.push(candidate.iter().map(|l| l.2.clone()).collect()),
    }
}

/// The same rule for the lines an OCR engine found (their boxes only: the engine gives no glyphs).
/// `theta` is the direction of the text (from the detector's lines, see [`text_angle`]) and `glyph` the
/// typical glyph height there, which sets the floor for text in capitals just as in [`line_height`].
/// Returns the indices of `rects` in groups, in order across the text; one group if nothing is cut.
pub fn split_rects(rects: &[Rect], theta: f32, glyph: f32, k: f32) -> Vec<Vec<usize>> {
    let whole = vec![(0..rects.len()).collect::<Vec<_>>()];
    if rects.len() < 2 || !k.is_finite() || k <= 0.0 { return whole; }
    let normal = (-theta.sin(), theta.cos());
    let mut placed: Vec<(f32, f32, usize)> = rects.iter().enumerate().map(|(i, r)| {
        let (cx, cy) = r.center();
        (cx * normal.0 + cy * normal.1, r.h.max(CAPITALS_FLOOR * glyph), i)
    }).collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    cut(&placed, k, &mut out);
    out
}

/// Distributes the detector's lines among partitions that were found elsewhere (by the OCR engine): each
/// line goes to the partition whose vertical extent is nearest to its centre. Groups come in the order of
/// the partitions; empty ones are dropped.
pub fn split_by_partitions(lines: Vec<TextLine>, partitions: &[Rect]) -> Vec<Vec<TextLine>> {
    let mut groups: Vec<Vec<TextLine>> = vec![Vec::new(); partitions.len()];
    for line in lines {
        let cy = centre(&line).1;
        let distance = |p: &Rect| if cy < p.y { p.y - cy } else if cy > p.bottom() { cy - p.bottom() } else { 0.0 };
        let nearest = partitions.iter().enumerate().min_by(|a, b| distance(a.1).total_cmp(&distance(b.1))).map(|(i, _)| i);
        if let Some(i) = nearest { groups[i].push(line); }
    }
    groups.retain(|g| !g.is_empty());
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line of `glyphs` letters `h` tall and `h/2` wide, starting at (x, y), turned by `angle`
    /// about its first glyph (the glyph boxes stay axis-aligned, as the detector produces them).
    fn line(x: f32, y: f32, glyphs: usize, h: f32, angle: f32) -> TextLine {
        let boxes: Vec<Rect> = (0..glyphs).map(|i| {
            let along = i as f32 * h * 0.7;
            Rect::new(x + along * angle.cos(), y + along * angle.sin(), h * 0.5, h)
        }).collect();
        let rect = boxes.iter().skip(1).fold(boxes[0], |u, g| u.union(g));
        TextLine { rect, glyphs: boxes, ink_color: [240; 3] }
    }

    /// Like `line`, but set in lower case: every third letter is tall (ascender, capital), the others are
    /// 0.6 of that and sit on the baseline, so a line occupies `h` while its typical glyph is 0.6 `h`.
    fn prose(x: f32, y: f32, glyphs: usize, h: f32) -> TextLine {
        let boxes: Vec<Rect> = (0..glyphs).map(|i| {
            let tall = i % 3 == 0;
            let gh = if tall { h } else { 0.6 * h };
            Rect::new(x + i as f32 * h * 0.55, y + h - gh, h * 0.45, gh)
        }).collect();
        let rect = boxes.iter().skip(1).fold(boxes[0], |u, g| u.union(g));
        TextLine { rect, glyphs: boxes, ink_color: [240; 3] }
    }

    fn y_of(parts: &[Vec<TextLine>]) -> Vec<Vec<u32>> {
        parts.iter().map(|p| p.iter().map(|l| l.rect.y as u32).collect()).collect()
    }

    #[test]
    fn evenly_spaced_lines_are_one_field() {
        // Ordinary prose: a line occupies 32 px, lines are 46 px apart (1.44 h).
        let lines = vec![prose(100.0, 100.0, 12, 32.0), prose(100.0, 146.0, 9, 32.0), prose(100.0, 192.0, 14, 32.0)];
        assert_eq!(split_by_line_gaps(lines, DEFAULT_LINE_GAP_FACTOR).len(), 1);
    }

    #[test]
    fn a_paragraph_in_capitals_is_not_cut_by_its_short_lines() {
        // Capitals: a line is 22 px tall (the cap height, ≈ 0.7 em of a 31 px font) but spaced like prose, 41 px
        // (1.3 em): a pitch of 1.86 line extents. The floor on the height keeps it one field.
        let lines = vec![line(100.0, 100.0, 12, 22.0, 0.0), line(100.0, 141.0, 9, 22.0, 0.0), line(100.0, 182.0, 14, 22.0, 0.0)];
        assert_eq!(split_by_line_gaps(lines, DEFAULT_LINE_GAP_FACTOR).len(), 1);
    }

    #[test]
    fn a_line_set_apart_is_cut_off() {
        // A name in the dialogue's colour one blank line above it (pitch 62 = 1.94 h, the band the detector
        // still joins); the dialogue itself is spaced 46 px.
        let lines = vec![prose(100.0, 100.0, 8, 32.0), prose(100.0, 162.0, 30, 32.0), prose(100.0, 208.0, 24, 32.0)];
        let parts = split_by_line_gaps(lines, DEFAULT_LINE_GAP_FACTOR);
        assert_eq!(y_of(&parts), vec![vec![100], vec![162, 208]]);
    }

    #[test]
    fn the_threshold_decides_exactly() {
        let near = |pitch: f32| vec![prose(0.0, 0.0, 9, 20.0), prose(0.0, pitch, 9, 20.0)];
        // Lines of one extent, 20 px: with k = 1.8 the cut is made above a pitch of 36.
        assert_eq!(split_by_line_gaps(near(35.5), 1.8).len(), 1);
        assert_eq!(split_by_line_gaps(near(36.5), 1.8).len(), 2);
        // A smaller k is stricter: 28 px is a pitch of 1.4 h.
        assert_eq!(split_by_line_gaps(near(28.0), 1.8).len(), 1);
        assert_eq!(split_by_line_gaps(near(28.0), 1.3).len(), 2);
    }

    #[test]
    fn a_single_line_and_a_broken_factor_come_back_whole() {
        assert_eq!(split_by_line_gaps(vec![prose(0.0, 0.0, 5, 20.0)], 1.8).len(), 1);
        assert!(split_by_line_gaps(Vec::new(), 1.8)[0].is_empty());
        let two = vec![prose(0.0, 0.0, 5, 20.0), prose(0.0, 200.0, 5, 20.0)];
        for k in [0.0, -1.0, f32::NAN, f32::INFINITY] { assert_eq!(split_by_line_gaps(two.clone(), k).len(), 1, "k = {k}"); }
    }

    #[test]
    fn every_gap_in_a_stack_is_found_and_each_part_is_judged_by_its_own_average() {
        // Three fields: a one-line title, a two-line paragraph and a footer, each set apart.
        let lines = vec![prose(0.0, 0.0, 9, 20.0), prose(0.0, 50.0, 9, 20.0), prose(0.0, 78.0, 9, 20.0), prose(0.0, 140.0, 9, 20.0)];
        assert_eq!(y_of(&split_by_line_gaps(lines, 1.8)), vec![vec![0], vec![50, 78], vec![140]]);
    }

    #[test]
    fn mixed_sizes_are_judged_by_the_average_height_of_the_candidate() {
        // A heading (extent 36) over small print (extent 14) set close: the average height of the three lines
        // is 21.3, the limit 38.4, the pitch (centre to centre) 37 — one field.
        let together = vec![prose(0.0, 0.0, 9, 36.0), prose(0.0, 40.0, 18, 14.0), prose(0.0, 58.0, 18, 14.0)];
        assert_eq!(split_by_line_gaps(together, 1.8).len(), 1);
        // The same heading with the small print 30 px further down: the pitch is far over the limit.
        let apart = vec![prose(0.0, 0.0, 9, 36.0), prose(0.0, 70.0, 18, 14.0), prose(0.0, 88.0, 18, 14.0)];
        assert_eq!(y_of(&split_by_line_gaps(apart, 1.8)), vec![vec![0], vec![70, 88]]);
    }

    #[test]
    fn slanted_text_is_measured_across_the_lines_not_down_the_page() {
        // Text turned by 12°: the second line starts far to the right of where the first one does. Measured
        // straight down the page, the pitch of lines that belong together would look larger than it is.
        let angle = 12.0_f32.to_radians();
        let across = |gap: f32| { // a second line `gap` px further along the normal, shifted along the text by 40 px
            let (dx, dy) = (-gap * angle.sin() + 40.0 * angle.cos(), gap * angle.cos() + 40.0 * angle.sin());
            vec![line(100.0, 100.0, 12, 22.0, angle), line(100.0 + dx, 100.0 + dy, 12, 22.0, angle)]
        };
        let detected = text_angle(&across(30.0));
        assert!((detected - angle).abs() < 0.03, "the direction is found: {detected} vs {angle}");
        assert_eq!(split_by_line_gaps(across(31.0), 1.8).len(), 1, "an ordinary pitch across the lines");
        assert_eq!(split_by_line_gaps(across(62.0), 1.8).len(), 2, "a pitch far over the limit across the lines");
        // Down the page the first pair is further apart than the limit, yet it is one field.
        let straight = |lines: &[TextLine]| lines[1].rect.center().1 - lines[0].rect.center().1;
        assert!(straight(&across(31.0)) > 1.8 * 22.0 * 0.9, "{}", straight(&across(31.0)));
    }

    #[test]
    fn the_rule_works_on_the_boxes_an_ocr_engine_gives() {
        let boxes = |ys: &[f32]| ys.iter().map(|y| Rect::new(10.0, *y, 200.0, 30.0)).collect::<Vec<_>>();
        // Lines 30 px tall, 46 px apart: a paragraph.
        assert_eq!(split_rects(&boxes(&[0.0, 46.0, 92.0]), 0.0, 20.0, 1.8), vec![vec![0, 1, 2]]);
        // A title, then two lines set well apart from it (pitch 90 = 3 h).
        assert_eq!(split_rects(&boxes(&[0.0, 90.0, 136.0]), 0.0, 20.0, 1.8), vec![vec![0], vec![1, 2]]);
        // Capitals: boxes 22 px tall, 41 px apart; the floor from the glyph height (22 * 1.35) keeps them together.
        let caps: Vec<Rect> = [0.0, 41.0, 82.0].iter().map(|y| Rect::new(10.0, *y, 200.0, 22.0)).collect();
        assert_eq!(split_rects(&caps, 0.0, 22.0, 1.8), vec![vec![0, 1, 2]]);
        // One box, no boxes, a broken factor.
        assert_eq!(split_rects(&boxes(&[0.0]), 0.0, 20.0, 1.8), vec![vec![0]]);
        assert_eq!(split_rects(&[], 0.0, 20.0, 1.8), vec![Vec::<usize>::new()]);
        assert_eq!(split_rects(&boxes(&[0.0, 300.0]), 0.0, 20.0, f32::NAN), vec![vec![0, 1]]);
    }

    #[test]
    fn detector_lines_are_distributed_among_remembered_partitions() {
        let lines = vec![prose(0.0, 0.0, 9, 20.0), prose(0.0, 30.0, 9, 20.0), prose(0.0, 100.0, 9, 20.0)];
        let partitions = [Rect::new(0.0, 0.0, 100.0, 50.0), Rect::new(0.0, 95.0, 100.0, 30.0)];
        assert_eq!(y_of(&split_by_partitions(lines.clone(), &partitions)), vec![vec![0, 30], vec![100]]);
        // A line between the partitions goes to the nearer one; an unused partition leaves no empty group.
        let between = vec![prose(0.0, 60.0, 9, 20.0)];
        assert_eq!(y_of(&split_by_partitions(between, &partitions)), vec![vec![60]]);
        assert!(split_by_partitions(lines, &[]).is_empty());
    }

    #[test]
    fn steep_or_unreadable_direction_means_no_rotation() {
        let vertical = vec![line(0.0, 0.0, 8, 20.0, 80.0_f32.to_radians()), line(30.0, 0.0, 8, 20.0, 80.0_f32.to_radians())];
        assert_eq!(text_angle(&vertical), 0.0);
        // Too few glyphs: no slope to speak of.
        assert_eq!(text_angle(&[line(0.0, 0.0, 2, 20.0, 0.3)]), 0.0);
        assert_eq!(text_angle(&[]), 0.0);
    }
}
