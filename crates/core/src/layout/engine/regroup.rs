//! Cuts and joins: what the OCR engine showed about the grouping of the detector's blocks is remembered and applied
//! to the blocks of the later scans.

use super::*;

/// A block the detector keeps seeing as one that the OCR engine showed to be several fields: the parts
/// (frame pixels, in order across the text) are applied to the detector's lines on every later scan, so
/// the fields exist from the next frame on with ids the tracker can follow.
pub(super) struct CutMemory {
    pub(super) parent: Rect,
    pub(super) partitions: Vec<Rect>,
    pub(super) rows: Vec<Vec<Rect>>,
    pub(super) seen: Instant,
}

pub(super) struct JoinMemory {
    pub(super) members: Vec<Rect>,
    /// What the members read before the join: a row of the union read again may take a better reading from here.
    pub(super) better: Vec<crate::layout::ocr_geometry::OcrSpan>,
    pub(super) seen: Instant,
}

/// A memory not applied for this long is forgotten: the scene it described is gone.
pub(super) const CUT_TTL: std::time::Duration = std::time::Duration::from_secs(3);

impl InplaceEngine {
    /// Applies what the OCR engine showed earlier: a detected block that matches a remembered one is divided
    /// into the remembered parts, the types of all blocks are decided again, and the rest is as if the
    /// detector itself had found the parts.
    pub(super) fn apply_cuts(&mut self, detected: Vec<DetectedTextBlock>, now: Instant, image: &RgbaImage, mask: &mut InkMask) -> Vec<DetectedTextBlock> {
        self.cuts.retain(|m| now.saturating_duration_since(m.seen) <= CUT_TTL);
        if self.cuts.is_empty() { return detected; }
        let mut changed = false;
        let mut out = Vec::with_capacity(detected.len() + 2);
        for block in detected {
            let Some(memory) = self.cuts.iter_mut().find(|m| m.parent.iou(&block.rect) >= LINES_AGREE_IOU) else { out.push(block); continue };
            memory.seen = now;
            let mut groups = crate::layout::split::split_by_partitions(block.lines.clone(), &memory.partitions);
            // A connected poster/panel can be one detector "line" spanning
            // several real OCR fields. Assigning that line to its nearest part
            // cannot perform the requested cut. Use the validated OCR rows,
            // retaining only actual detector glyphs supported by each row.
            if groups.len() != memory.partitions.len() {
                groups = memory.rows.iter().map(|rows| rows.iter().map(|&rect| {
                    let (x, y, w, h) = rect.pixels(image.width(), image.height());
                    let crop = image::imageops::crop_imm(image, x, y, w, h).to_image();
                    // The full-frame threshold merged the panel instead of its
                    // letters. Reuse the detector on the OCR-confirmed row,
                    // whose local threshold separates foreground and background.
                    let local = BlockDetector::ink_mask(&crop);
                    let recovered = self.detector.detect_text_blocks(&crop, &local);
                    let glyphs = recovered.iter().flat_map(|b| &b.lines).flat_map(|l| &l.glyphs)
                        .map(|g| Rect::new(g.x + x as f32, g.y + y as f32, g.w, g.h)).collect();
                    for ly in 0..h as usize { for lx in 0..w as usize {
                        mask.ink[(y as usize + ly) * mask.w + x as usize + lx] = local.at(lx, ly);
                    }}
                    let ink_color = crate::layout::mean_color(&crop, &Rect::new(0.0, 0.0, w as f32, h as f32),
                        |lx, ly| local.at(lx, ly)).unwrap_or(block.ink_color);
                    crate::layout::block_detector::TextLine { rect, glyphs, ink_color }
                }).collect()).collect();
                tracing::debug!(target: "inplace.lines", parts = groups.len(),
                    "detector components cross OCR partitions; use validated OCR rows");
            }
            if groups.len() < 2 { out.push(block); continue; }
            changed = true;
            for lines in groups {
                let rect = lines.iter().skip(1).fold(lines[0].rect, |u, l| u.union(&l.rect));
                let ink_color = lines[0].ink_color;
                out.push(DetectedTextBlock { rect, lines, block_type: crate::layout::TextBlockType::Unknown, ink_color });
            }
        }
        if changed {
            out.sort_by(|a, b| (a.rect.y, a.rect.x).partial_cmp(&(b.rect.y, b.rect.x)).unwrap_or(std::cmp::Ordering::Equal));
            crate::layout::block_detector::classify_blocks(&mut out, image.width() as f32, image.height() as f32);
        }
        out
    }

    pub(super) fn apply_joins(&mut self, mut detected: Vec<DetectedTextBlock>, now: Instant) -> Vec<DetectedTextBlock> {
        self.joins.retain(|m| now.saturating_duration_since(m.seen)<=CUT_TTL);
        for memory in &mut self.joins {
            let mut indices = Vec::new();
            for member in &memory.members {
                if let Some((index,_)) = detected.iter().enumerate().filter(|(i,_)| !indices.contains(i))
                    .filter(|(_,d)| member.iou(&d.rect)>=0.5).max_by(|(_,a),(_,b)| member.iou(&a.rect).total_cmp(&member.iou(&b.rect))) {
                    indices.push(index);
                }
            }
            if indices.len()!=memory.members.len() { continue; }
            indices.sort_unstable();
            let mut combined = detected[indices[0]].clone();
            for &index in indices.iter().skip(1) {
                combined.rect = combined.rect.union(&detected[index].rect);
                combined.lines.extend(detected[index].lines.clone());
            }
            combined.lines.sort_by(|a,b| a.rect.y.total_cmp(&b.rect.y).then_with(|| a.rect.x.total_cmp(&b.rect.x)));
            combined.lines.dedup();
            for &index in indices.iter().rev() { detected.remove(index); }
            detected.push(combined);
            memory.seen = now;
        }
        detected.sort_by(|a,b| a.rect.y.total_cmp(&b.rect.y).then_with(|| a.rect.x.total_cmp(&b.rect.x)));
        detected
    }

    /// A regrouped crop must be OCR-read again before it may supply a translation.
    pub fn needs_rescan(&self) -> bool { !self.pending_joins.is_empty() || !self.pending_cuts.is_empty() }
    pub(crate) fn is_regrouping(&self, id: u64) -> bool { self.pending_joins.contains(&id) || self.pending_cuts.contains(&id) }

    pub(super) fn request_join(&mut self, id: u64) -> bool {
        let Some(snapshot) = &self.snapshot else { return false };
        let Some(item) = self.tracker.get(id).and_then(|t| t.ocr_field.as_ref()) else { return false };
        let Some(d) = snapshot.blocks.get(&id) else { return false };
        let peer = self.tracker.blocks().iter().filter(|t| t.id!=id && t.misses==0)
            .filter_map(|t| t.ocr_field.as_ref()).find(|other| snapshot.blocks.get(&other.id()).is_some_and(|other_d|
                d.rect.intersection(&other_d.rect)>0.0 && item.needs_join(other)));
        let Some(peer) = peer else { return false };
        let members = vec![d.rect,snapshot.blocks[&peer.id()].rect];
        let union = members[0].union(&members[1]);
        if self.joins.iter().any(|m| m.members.iter().skip(1).fold(m.members[0],|r,b|r.union(b)).iou(&union)>=0.8) {
            return false;
        }
        self.pending_joins.insert(id); self.pending_joins.insert(peer.id());
        let better = item.spans().iter().chain(peer.spans()).cloned().collect();
        self.joins.push(JoinMemory { members, better, seen: self.last_scan.unwrap_or_else(Instant::now) });
        true
    }

    /// A field read again after a join takes, row by row, the better readings the members had before it.
    pub(super) fn better_readings(&self, mut field: crate::layout::ocr_geometry::OcrField) -> crate::layout::ocr_geometry::OcrField {
        for m in &self.joins {
            let union = m.members.iter().skip(1).fold(m.members[0], |r, b| r.union(b));
            if field.bounds().intersection(&union) <= 0.0 { continue; }
            if let Some(corrected) = field.corrected_by(&m.better) { field = corrected; }
        }
        field
    }
}
