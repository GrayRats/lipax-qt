//! A measure of how close a translated frame is to a reference (the same frame with the text replaced by hand).
//!
//! Three images of one frame take part: the `original` of the game, `ours` (the original with our translation) and
//! the `reference`. What each of them changed is the set of pixels that differ from the original. Comparing those sets
//! says what pixel colours cannot: whether the text was replaced in the same places (`recall`), whether something was
//! replaced that the reference leaves alone (`extra_blocks`, e.g. a scene's own lettering taken for text), how much the
//! plates and fills reach beyond the reference's text (`excess`), and where the blocks sit and what colour they are.
#![allow(dead_code)]

use image::RgbImage;

/// A pixel counts as changed when any channel differs from the original by more than this (levels).
pub const CHANGED: u8 = 28;
/// Letters are joined into blocks by growing the changed pixels by this many pixels.
pub const JOIN: usize = 10;
/// How far from the reference's text our changes may reach and still be the same replacement.
pub const NEAR: usize = 14;
/// Blocks smaller than this many pixels (after joining) are specks, not text.
pub const MIN_BLOCK: usize = 120;

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub bbox: (usize, usize, usize, usize),
    /// The changed pixels (not the grown ones) of the block, as indices into the frame.
    pub pixels: Vec<u32>,
}

impl Block {
    fn centre(&self) -> (f32, f32) {
        ((self.bbox.0 + self.bbox.2) as f32 / 2.0, (self.bbox.1 + self.bbox.3) as f32 / 2.0)
    }
    fn height(&self) -> f32 {
        (self.bbox.3 - self.bbox.1) as f32
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub reference_blocks: usize,
    pub our_blocks: usize,
    /// Share of the reference's changed pixels that we changed too (within `JOIN` pixels).
    pub recall: f32,
    /// Our blocks that lie where the reference changed nothing, and the pixels in them.
    pub extra_blocks: usize,
    pub extra_pixels: usize,
    /// Our changed pixels far from the reference's, per changed pixel of the reference: 0 is no plates or fills beyond
    /// the text, 1 is as much again.
    pub excess: f32,
    /// Mean distance between the centres of a reference block and our best matching block, px; `None` without a match.
    pub offset: Option<f32>,
    /// Mean of |our height / reference height − 1| over the matched blocks.
    pub height_error: Option<f32>,
    /// Mean distance of the average colour of the changed pixels of a matched block, ours against the reference (0–441).
    pub colour: Option<f32>,
}

pub fn changed_mask(original: &RgbImage, other: &RgbImage) -> Vec<bool> {
    assert_eq!(original.dimensions(), other.dimensions(), "the frames must be the same size");
    original.pixels().zip(other.pixels()).map(|(a, b)| (0..3).any(|c| a.0[c].abs_diff(b.0[c]) > CHANGED)).collect()
}

/// `mask` grown by `r` pixels in every direction (a square window, two passes).
pub fn dilate(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let mut rows = vec![false; w * h];
    for y in 0..h {
        let row = &mask[y * w..(y + 1) * w];
        let mut last: Option<usize> = None;
        // The nearest set pixel to the left (inclusive) and to the right decides.
        let mut right = vec![usize::MAX; w];
        let mut next = usize::MAX;
        for x in (0..w).rev() { if row[x] { next = x; } right[x] = next; }
        for x in 0..w {
            if row[x] { last = Some(x); }
            let near_left = last.is_some_and(|l| x - l <= r);
            let near_right = right[x] != usize::MAX && right[x] - x <= r;
            rows[y * w + x] = near_left || near_right;
        }
    }
    let mut out = vec![false; w * h];
    for x in 0..w {
        let mut last: Option<usize> = None;
        let mut below = vec![usize::MAX; h];
        let mut next = usize::MAX;
        for y in (0..h).rev() { if rows[y * w + x] { next = y; } below[y] = next; }
        for y in 0..h {
            if rows[y * w + x] { last = Some(y); }
            let near_up = last.is_some_and(|l| y - l <= r);
            let near_down = below[y] != usize::MAX && below[y] - y <= r;
            out[y * w + x] = near_up || near_down;
        }
    }
    out
}

/// The blocks of changed pixels: pixels `JOIN` apart belong together, specks are dropped.
pub fn blocks(changed: &[bool], w: usize, h: usize) -> Vec<Block> {
    let grown = dilate(changed, w, h, JOIN);
    let mut label = vec![u32::MAX; w * h];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !grown[start] || label[start] != u32::MAX { continue; }
        let id = found.len() as u32;
        let mut block = Block { bbox: (w, h, 0, 0), pixels: Vec::new() };
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            if changed[i] {
                block.pixels.push(i as u32);
                block.bbox = (block.bbox.0.min(x), block.bbox.1.min(y), block.bbox.2.max(x + 1), block.bbox.3.max(y + 1));
            }
            let mut visit = |j: usize| if grown[j] && label[j] == u32::MAX { label[j] = id; stack.push(j); };
            if x > 0 { visit(i - 1); }
            if x + 1 < w { visit(i + 1); }
            if y > 0 { visit(i - w); }
            if y + 1 < h { visit(i + w); }
        }
        found.push(block);
    }
    found.retain(|b| b.pixels.len() >= MIN_BLOCK);
    found
}

fn iou(a: (usize, usize, usize, usize), b: (usize, usize, usize, usize)) -> f32 {
    let (ix, iy) = ((a.2.min(b.2) as f32 - a.0.max(b.0) as f32).max(0.0), (a.3.min(b.3) as f32 - a.1.max(b.1) as f32).max(0.0));
    let inter = ix * iy;
    let union = (a.2 - a.0) as f32 * (a.3 - a.1) as f32 + (b.2 - b.0) as f32 * (b.3 - b.1) as f32 - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

fn mean_colour(image: &RgbImage, block: &Block) -> [f32; 3] {
    let w = image.width() as usize;
    let mut sum = [0.0f32; 3];
    for &i in &block.pixels {
        let p = image.get_pixel((i as usize % w) as u32, (i as usize / w) as u32).0;
        for c in 0..3 { sum[c] += p[c] as f32; }
    }
    sum.map(|v| v / block.pixels.len().max(1) as f32)
}

pub fn compare(original: &RgbImage, ours: &RgbImage, reference: &RgbImage) -> Report {
    let (w, h) = (original.width() as usize, original.height() as usize);
    let (changed_ref, changed_ours) = (changed_mask(original, reference), changed_mask(original, ours));
    let (ref_blocks, our_blocks) = (blocks(&changed_ref, w, h), blocks(&changed_ours, w, h));
    let ref_total: usize = ref_blocks.iter().map(|b| b.pixels.len()).sum();
    let mut report = Report { reference_blocks: ref_blocks.len(), our_blocks: our_blocks.len(), ..Default::default() };
    // Only the pixels that belong to a block count (specks are noise of the encoding, not text).
    let mut ref_px = vec![false; w * h];
    for b in &ref_blocks { for &i in &b.pixels { ref_px[i as usize] = true; } }
    let mut our_px = vec![false; w * h];
    for b in &our_blocks { for &i in &b.pixels { our_px[i as usize] = true; } }
    let (near_ours, near_ref) = (dilate(&our_px, w, h, JOIN), dilate(&ref_px, w, h, NEAR));
    report.recall = if ref_total == 0 { 1.0 } else { ref_blocks.iter().flat_map(|b| &b.pixels).filter(|&&i| near_ours[i as usize]).count() as f32 / ref_total as f32 };
    let outside = our_blocks.iter().flat_map(|b| &b.pixels).filter(|&&i| !near_ref[i as usize]).count();
    report.excess = outside as f32 / ref_total.max(1) as f32;
    for b in &our_blocks {
        let near = b.pixels.iter().filter(|&&i| near_ref[i as usize]).count();
        if (near as f32) < 0.25 * b.pixels.len() as f32 {
            report.extra_blocks += 1;
            report.extra_pixels += b.pixels.len();
        }
    }
    let (mut offset, mut height, mut colour, mut weight, mut coloured) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for r in &ref_blocks {
        let Some(o) = our_blocks.iter().filter(|o| iou(r.bbox, o.bbox) > 0.05).max_by(|a, b| iou(r.bbox, a.bbox).total_cmp(&iou(r.bbox, b.bbox))) else { continue };
        let wt = r.pixels.len() as f32;
        let ((rx, ry), (ox, oy)) = (r.centre(), o.centre());
        offset += wt * ((rx - ox).powi(2) + (ry - oy).powi(2)).sqrt();
        height += wt * (o.height() / r.height().max(1.0) - 1.0).abs();
        weight += wt;
        let (a, b) = (mean_colour(reference, r), mean_colour(ours, o));
        colour += wt * (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt();
        coloured += wt;
    }
    if weight > 0.0 {
        report.offset = Some(offset / weight);
        report.height_error = Some(height / weight);
        report.colour = Some(colour / coloured);
    }
    report
}
