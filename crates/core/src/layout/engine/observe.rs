//! What the OCR engine's readings mean for the fields: the observed rows and lines, and the reconciliation of
//! repeated readings of the same pixels.

use super::*;

impl InplaceEngine {
    /// Keep per-span text/confidence and source coordinates before translation. OCR outside the
    /// detected glyph support is excluded from the source reading (including text in crop margins).
    pub fn observe_ocr_result(&mut self, id: u64, result: &crate::ocr::OcrResult,
                              origin: (u32, u32), s: &Settings) -> OcrObservation {
        if result.lines.is_empty() {
            if let Some(t) = self.tracker.get_mut(id)
                && t.ocr_field.take().is_some()
            {
                t.ocr_detector_rect = None;
                t.ocr_owner = None;
                t.ocr_bounds = None;
                t.refined_rect = None;
                t.ocr_lines = None;
                // A backend without geometry uses the detector again, including its plate.
                if let Some((snapshot, d)) = self.snapshot.as_ref().and_then(|snap|
                    snap.blocks.get(&id).map(|d| (snap, d))) {
                    let lh = d.line_height();
                    let margin = BackgroundAnalyzer::margin(lh);
                    let analysis = BackgroundAnalyzer.analyze(&snapshot.image, &snapshot.mask, &d.rect, margin);
                    t.background = Some(BackgroundInpainter.render(&snapshot.image, &snapshot.mask, &d.rect, lh,
                        analysis, &s.appearance.inplace, d.block_type));
                    t.background_for = d.rect;
                    t.background_signature = Some(BackgroundAnalyzer::signature(&snapshot.image, &d.rect, margin));
                    if let Some(font) = &t.font_analysis {
                        t.typography = Some(TypographyEstimator.estimate(d, font, t.background.as_ref().unwrap().color,
                            snapshot.image.width() as f32));
                    }
                    t.lines_note = "геометрия по детектору: OCR без координат".into();
                }
                t.revision += 1;
                self.ocr_dirty = true;
            }
            return OcrObservation { text: crate::text::normalize(&result.text), verdict: LinesVerdict::Normal };
        }
        let Some(snapshot) = &self.snapshot else {
            return OcrObservation { text: String::new(), verdict: LinesVerdict::Normal };
        };
        let Some(d) = snapshot.blocks.get(&id) else {
            return OcrObservation { text: String::new(), verdict: LinesVerdict::Normal };
        };
        let detector_rect = d.rect;
        let cap = self.tracker.get(id).and_then(|t| t.font_analysis.as_ref()).map_or_else(|| typical_glyph_height(d), |a| a.cap_height_px);
        let support = d.rect.expand(0.5 * cap.max(1.0), snapshot.image.width() as f32, snapshot.image.height() as f32);
        let spans: Vec<_> = result.lines.iter().filter_map(|line| {
            let rect = line.rect.in_frame(origin);
            let rect = self.pending_crops.get(&id).map_or(rect, |crop| rect.clipped_to(crop));
            let center = rect.center();
            (line.confidence >= s.effective_minimum_confidence() as f32
                && center.0 >= support.x && center.0 <= support.right()
                && center.1 >= support.y && center.1 <= support.bottom())
                .then(|| crate::layout::ocr_geometry::OcrSpan::new(rect.clipped_to(&Rect::new(0.0, 0.0,
                    snapshot.image.width() as f32, snapshot.image.height() as f32)), &line.text, line.confidence)).flatten()
        }).collect();
        let field = crate::layout::ocr_geometry::OcrField::new(id, spans, cap).map(|field| {
            match self.pending_crops.get(&id) {
                Some(&crop) => field.with_crop_bounds(crop),
                None => field,
            }
        });
        let field = field.map(|f| self.better_readings(f));
        let text = field.as_ref().map(|field| field.text().to_owned()).unwrap_or_default();
        let lines = field.as_ref().map(|f| f.rows()).unwrap_or_default();
        let verdict = self.observe_supported_ocr_lines(id, &lines, s, true);
        if let Some(t) = self.tracker.get_mut(id) {
            t.ocr_field = if verdict == LinesVerdict::Normal { field } else { None };
            t.ocr_detector_rect = t.ocr_field.as_ref().map(|_| detector_rect);
            t.ocr_owner = None;
            t.revision += 1;
            self.ocr_dirty = true;
        }
        let verdict = if verdict==LinesVerdict::Normal && self.request_join(id) { LinesVerdict::Split } else { verdict };
        OcrObservation { text, verdict }
    }

    /// Resolve repeated reads of the same source pixels before placement. Canonical OCR geometry
    /// replaces noisy component bounds; independent labels remain collision obstacles.
    pub(super) fn reconcile_ocr(&mut self, s: &Settings) {
        if !self.ocr_dirty { return; }
        self.ocr_dirty = false;
        let Some(snapshot) = &self.snapshot else { return };
        let fresh: Vec<Rect> = self.tracker.blocks().iter().filter(|t| t.misses == 0).map(|t| t.text_rect()).collect();
        let fields: Vec<_> = self.tracker.blocks().iter().filter(|t| t.misses <= 1 && !t.translated_text.is_empty() && !gives_way(t, &fresh))
            .filter_map(|t| t.ocr_field.clone()).collect();
        // A crop can contain an independent speaker plus truncated dialogue rows.
        // A second, complete reading of the same pixels establishes the boundary;
        // re-read both parts instead of allowing the mixed crop to block the dialogue.
        // A field each of whose rows another, more complete reading covers (the right halves of two dialogue
        // lines the detector grouped across them) is a repeated reading of those pixels: it is not drawn.
        let mut covered: HashMap<u64, u64> = HashMap::new();
        for field in &fields {
            let rows = field.rows();
            if rows.is_empty() { continue; }
            let supporters: Vec<Option<&crate::layout::ocr_geometry::OcrField>> = rows.iter().map(|row| fields.iter().filter(|other| {
                other.id() != field.id() && other.mean_confidence() + 5.0 >= field.mean_confidence()
                    && other.rows().iter().any(|complete| complete.w > row.w * 1.1
                        && row.intersection(complete) >= row.area() * 0.8)
            }).max_by_key(|other| (other.text().chars().count(), std::cmp::Reverse(other.id())))).collect();
            if supporters.iter().all(Option::is_some) {
                // Same words, not only the same place: the previous dialogue still lingers there for a scan.
                if let Some(owner) = supporters.iter().flatten().filter(|o| field.text_supported_by(o) >= 0.8)
                    .max_by_key(|o| (o.text().chars().count(), std::cmp::Reverse(o.id()))) {
                    covered.insert(field.id(), owner.id());
                    continue;
                }
            }
            if rows.len() < 2 { continue; }
            let supported: Vec<bool> = supporters.iter().map(Option::is_some).collect();
            if !supported.iter().any(|&x| x) { continue; }
            let Some(d) = snapshot.blocks.get(&field.id()) else { continue };
            if self.cuts.iter().any(|cut| cut.parent.iou(&d.rect) >= LINES_AGREE_IOU) { continue; }
            let mut groups: Vec<Vec<Rect>> = Vec::new();
            for (i, &row) in rows.iter().enumerate() {
                if i == 0 || supported[i] != supported[i-1] { groups.push(Vec::new()); }
                groups.last_mut().unwrap().push(row);
            }
            let partitions = groups.iter().map(|rows| rows.iter().skip(1).fold(rows[0], |u, r| u.union(r))).collect();
            self.cuts.push(CutMemory { parent: d.rect, partitions, rows: groups,
                seen: self.last_scan.unwrap_or_else(Instant::now) });
            self.pending_cuts.insert(field.id());
        }
        let owners: Vec<Option<u64>> = fields.iter().map(|f| fields.iter().filter(|other| f.duplicate_of(other))
            .max_by_key(|other| (other.text().chars().count(), std::cmp::Reverse(other.id()))).map(|other| other.id())
            .or_else(|| self.joined_aliases.get(&f.id()).copied().filter(|id| fields.iter().any(|other| other.id()==*id)))
            .or_else(|| covered.get(&f.id()).copied())
            // Two readings of the same pixels that say the same words: the fuller one stays. Without this both would
            // overlap and both be refused for lack of room, and the dialogue would stay untranslated.
            .or_else(|| fields.iter().filter(|o| o.id() != f.id()
                && o.mean_confidence() + 5.0 >= f.mean_confidence()
                && o.text().chars().count() > f.text().chars().count()
                && f.bounds().intersection(&o.bounds()) >= 0.5 * f.bounds().area().min(o.bounds().area())
                && f.text_supported_by(o) >= 0.8)
                .max_by_key(|o| (o.text().chars().count(), std::cmp::Reverse(o.id()))).map(|o| o.id()))).collect();
        for (field, owner) in fields.iter().zip(&owners) {
            tracing::debug!(target: "inplace.ownership", block_id = field.id(), owner = ?owner,
                spans = field.spans().len(), bounds = ?field.bounds(), characters = field.text().chars().count(),
                confidence = field.mean_confidence(), "validated OCR field ownership");
        }
        let mut rects: Vec<Rect> = fields.iter().map(|f| {
            let r = f.bounds(); Rect::new(r.x-1.0,r.y-1.0,r.w+2.0,r.h+2.0)
        }).collect();
        // Adjacent rows can have a few pixels of OCR padding in common. Split that narrow strip;
        // substantial overlaps between independent fields are still handled by collision refusal.
        for i in 0..fields.len() { for j in i+1..fields.len() {
            if owners[i].is_some() || owners[j].is_some() { continue; }
            let (a,b) = (rects[i],rects[j]);
            let overlap_y = a.bottom().min(b.bottom()) - a.y.max(b.y);
            if a.x < b.right() && a.right() > b.x && overlap_y > 0.0 && overlap_y <= 0.15*a.h.min(b.h) {
                if a.center().1 < b.center().1 {
                    let mid = (a.bottom()+b.y)*0.5; rects[i].h = mid-a.y;
                    rects[j].h = b.bottom()-mid; rects[j].y = mid;
                } else {
                    let mid = (b.bottom()+a.y)*0.5; rects[j].h = mid-b.y;
                    rects[i].h = a.bottom()-mid; rects[i].y = mid;
                }
            }
        } }
        for (i,field) in fields.iter().enumerate() {
            let Some(t) = self.tracker.get_mut(field.id()) else { continue };
            if t.ocr_owner != owners[i] { t.ocr_owner = owners[i]; t.revision += 1; }
            if owners[i].is_some() { continue; }
            let Some(d) = snapshot.blocks.get(&field.id()) else { continue };
            let rect = rects[i];
            let rows = field.rows();
            if t.refined_rect != Some(rect) || t.ocr_lines.as_ref() != Some(&rows) {
                let lh = rows.iter().map(|r| r.h).sum::<f32>() / rows.len().max(1) as f32;
                let analysis = BackgroundAnalyzer.analyze(&snapshot.image, &snapshot.mask, &rect, BackgroundAnalyzer::margin(lh));
                t.background = Some(BackgroundInpainter.render(&snapshot.image, &snapshot.mask, &rect, lh, analysis, &s.appearance.inplace, d.block_type));
                t.background_for = rect;
                t.background_signature = Some(BackgroundAnalyzer::signature(&snapshot.image, &rect, BackgroundAnalyzer::margin(lh)));
                t.ocr_bounds = Some(rect);
                t.refined_rect = Some(rect);
                t.ocr_lines = Some(rows);
                if let Some(font) = &mut t.font_analysis {
                    // A panel/portrait component cannot determine the cap height
                    // of OCR-confirmed small text inside it.
                    let max_row_height = t.ocr_lines.as_ref().unwrap().iter().map(|r| r.h).fold(0.0, f32::max);
                    font.cap_height_px = font.cap_height_px.min(max_row_height);
                    t.typography = Some(TypographyEstimator.estimate_from(d, font, t.background.as_ref().unwrap().color,
                        snapshot.image.width() as f32, t.ocr_lines.as_deref()));
                }
                t.lines_note = "строки и границы по валидированным OCR-фрагментам".into();
                t.revision += 1;
            }
        }
    }

    /// What the OCR engine says about the lines of a field it has just read (`lines` in frame pixels,
    /// top to bottom). If they lie on the detected block, they decide the line structure — how many
    /// lines, the step between them, the alignment — and the typography is estimated again; if they
    /// do not, the detector's lines stay and the inspector says why. No lines (an engine without
    /// geometry) changes nothing.
    pub fn observe_ocr_lines(&mut self, id: u64, lines: &[Rect], s: &Settings) -> LinesVerdict {
        self.observe_supported_ocr_lines(id, lines, s, false)
    }

    pub(super) fn observe_supported_ocr_lines(&mut self, id: u64, lines: &[Rect], s: &Settings, supported: bool) -> LinesVerdict {
        let normalized = crate::layout::typography::reconstructed_ocr_lines(lines);
        let lines = normalized.as_slice();
        if lines.is_empty() { return LinesVerdict::Normal; }
        let Some(snapshot) = &self.snapshot else { return LinesVerdict::Normal };
        let Some(d) = snapshot.blocks.get(&id) else { return LinesVerdict::Normal };
        let frame_w = snapshot.image.width() as f32;
        let agree = lines_agree(lines, &d.rect);
        // The same pitch rule as the detector's, on the engine's own lines: a gap the detector did not see.
        // Once per scene: a memory that matches this block means the parts did not appear (nothing to retry).
        if agree || supported {
            // A detector component may be the connected panel, not a glyph.
            // Its cap height cannot enlarge the pitch threshold beyond the
            // actual OCR row height and merge unrelated paragraphs.
            let mut heights: Vec<_> = lines.iter().map(|r| r.h).collect();
            heights.sort_by(f32::total_cmp);
            let glyph = typical_glyph_height(d).min(heights[heights.len() / 2]);
            let groups = crate::layout::split::split_rects(lines, crate::layout::split::text_angle(&d.lines), glyph, s.appearance.inplace.line_gap_factor);
            if groups.len() > 1 && !self.cuts.iter().any(|m| m.parent.iou(&d.rect) >= LINES_AGREE_IOU) {
                let partitions: Vec<Rect> = groups.iter().map(|g| g.iter().skip(1).fold(lines[g[0]], |u, &i| u.union(&lines[i]))).collect();
                tracing::info!(target: "inplace.lines", block_id = id, parts = partitions.len(), "OCR lines show several fields in one block: split on the next scan");
                let rows = groups.iter().map(|g| g.iter().map(|&i| lines[i]).collect()).collect();
                self.cuts.push(CutMemory { parent: d.rect, partitions, rows, seen: self.last_scan.unwrap_or_else(Instant::now) });
                self.pending_cuts.insert(id);
                return LinesVerdict::Split;
            }
        }
        let Some(t) = self.tracker.get_mut(id) else { return LinesVerdict::Normal };
        if !agree {
            t.ocr_lines = None;
            if t.ocr_bounds.take().is_some() || t.refined_rect.take().is_some() { t.background_signature = None; }
            let on = lines.iter().skip(1).fold(lines[0], |u, l| u.union(l));
            t.lines_note = format!("строки по детектору: геометрия OCR ({} стр.) не совпала с блоком (IoU {:.2})", lines.len(), on.iou(&d.rect));
            tracing::debug!(target: "inplace.lines", block_id = id, ocr_lines = lines.len(), detected = d.lines.len(), "OCR geometry does not match the block; detector lines kept");
            return LinesVerdict::Normal;
        }
        t.ocr_lines = Some(lines.to_vec());
        t.lines_note = if lines.len() == d.lines.len() { format!("строк: {} (OCR и детектор согласны)", lines.len()) }
            else { format!("строк: {} по OCR, детектор насчитал {}", lines.len(), d.lines.len()) };
        if lines.len() != d.lines.len() {
            tracing::debug!(target: "inplace.lines", block_id = id, ocr_lines = lines.len(), detected = d.lines.len(), "OCR and the detector disagree on the lines; OCR decides");
        }
        // The boundary of the glyphs: the lines' union, if it stays close to the detected block.
        let refined = refine_bounds(lines, &d.rect, d.line_height())
            .map(|r| bounded_refinement(r, &d.rect, snapshot.blocks.values().map(|b| b.rect)));
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
            t.background_for = rect;
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
        LinesVerdict::Normal
    }
}
