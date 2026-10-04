//! Защита соседних OCR-полей от перекрытия переводом и его подложкой.
//! Входные прямоугольники — логические пиксели рабочего стола (`DesktopRect`) после Qt-подгонки
//! текста; прямоугольник кадра сюда не подойдёт, это проверяет компилятор.

use super::DesktopRect as Rect;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub source_rect: Rect,
    pub text_rect: Rect,
    pub background_rect: Rect,
    pub effect_margin: f32,
    /// Восстановленную картинку нельзя растягивать после обрезки.
    pub image_background: bool,
    pub allow_text_shift: bool,
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub safe_rect: Rect,
    pub text_rect: Rect,
    pub background_rect: Rect,
    pub visual_rect: Rect,
}

fn clip(r: Rect, bounds: Rect) -> Rect {
    let x0 = r.x.max(bounds.x);
    let y0 = r.y.max(bounds.y);
    Rect::in_space(x0, y0, (r.right().min(bounds.right()) - x0).max(0.0), (r.bottom().min(bounds.bottom()) - y0).max(0.0))
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x - 0.5 && inner.y >= outer.y - 0.5
        && inner.right() <= outer.right() + 0.5 && inner.bottom() <= outer.bottom() + 0.5
}

pub fn resolve(candidates: &[Candidate], bounds: Rect) -> Vec<Option<Placement>> {
    resolve_with(candidates, bounds, true)
}

/// `resolve` without the log: used to try a simpler variant of a field that did not fit, where
/// every attempt would repeat the same warning.
pub fn resolve_quiet(candidates: &[Candidate], bounds: Rect) -> Vec<Option<Placement>> {
    resolve_with(candidates, bounds, false)
}

fn resolve_with(candidates: &[Candidate], bounds: Rect, log: bool) -> Vec<Option<Placement>> {
    let mut accepted: Vec<Rect> = Vec::new();
    let mut results = Vec::with_capacity(candidates.len());
    for (index, item) in candidates.iter().enumerate() {
        let mut safe = bounds;
        for (other_index, other) in candidates.iter().enumerate() {
            if index == other_index { continue; }
            let a = item.source_rect;
            let b = other.source_rect;
            let horizontal_overlap = a.x < b.right() && a.right() > b.x;
            let vertical_overlap = a.y < b.bottom() && a.bottom() > b.y;
            if vertical_overlap && !horizontal_overlap {
                if a.center().0 < b.center().0 { safe.w = safe.w.min(((a.right() + b.x) * 0.5 - safe.x).max(0.0)); }
                else { let left = (b.right() + a.x) * 0.5; safe.w = safe.right() - left.max(safe.x); safe.x = left.max(safe.x); }
            } else if horizontal_overlap && !vertical_overlap {
                if a.center().1 < b.center().1 { safe.h = safe.h.min(((a.bottom() + b.y) * 0.5 - safe.y).max(0.0)); }
                else { let top = (b.bottom() + a.y) * 0.5; safe.h = safe.bottom() - top.max(safe.y); safe.y = top.max(safe.y); }
            } else if horizontal_overlap && vertical_overlap {
                // OCR duplicates/nested fields cannot both be drawn safely.
                if other_index < index { safe = Rect::default(); break; }
            }
        }
        let m = item.effect_margin.max(0.0);
        let mut text = item.text_rect;
        let expanded = |r: Rect| clip(Rect::in_space(r.x - m, r.y - m, r.w + 2.0 * m, r.h + 2.0 * m), bounds);
        let mut text_visual = expanded(text);
        if item.allow_text_shift && !contains(safe, text_visual) {
            let dx = if text_visual.x < safe.x { safe.x - text_visual.x }
                else if text_visual.right() > safe.right() { safe.right() - text_visual.right() } else { 0.0 };
            let dy = if text_visual.y < safe.y { safe.y - text_visual.y }
                else if text_visual.bottom() > safe.bottom() { safe.bottom() - text_visual.bottom() } else { 0.0 };
            if dx.abs() <= item.source_rect.h.min(16.0) * 0.25 && dy.abs() <= item.source_rect.h.min(16.0) * 0.25 {
                text.x += dx;
                text.y += dy;
                text_visual = expanded(text);
            }
        }
        // Fit (wrap, line spacing, tracking, size, condensed) has already run using Qt metrics.
        // Then remove excessive padding. No movement is attempted if it would uncover the
        // original glyphs; an impossible field is skipped instead of overlapping the game HUD.
        let background = clip(item.background_rect, safe);
        let visual = background.union(&text_visual);
        let impossible = !contains(safe, text_visual)
            || (item.image_background && background != item.background_rect)
            || accepted.iter().any(|r| r.intersection(&visual) > 0.5)
            || candidates.iter().enumerate().any(|(j, other)| j != index && other.source_rect.intersection(&visual) > 0.5);
        if impossible {
            if log {
                tracing::debug!(target: "inplace.collision", block_id = %item.id, safe = ?safe, text = ?item.text_rect,
                    background = ?item.background_rect, "unable to place block without overlap");
            }
            results.push(None);
        } else {
            accepted.push(visual);
            results.push(Some(Placement { safe_rect: safe, text_rect: text, background_rect: background, visual_rect: visual }));
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(id: &str, x: f32, y: f32, w: f32, h: f32, pad: f32) -> Candidate {
        let text = Rect::in_space(x, y, w, h);
        Candidate { id: id.into(), source_rect: text, text_rect: text,
            background_rect: Rect::in_space(x - pad, y - pad, w + 2.0 * pad, h + 2.0 * pad), effect_margin: 0.0, image_background: false, allow_text_shift: true }
    }
    #[test]
    fn separate_name_and_dialogue_keep_separate_visuals() {
        let entries = [c("name", 40.0, 30.0, 100.0, 20.0, 8.0), c("dialogue", 40.0, 64.0, 340.0, 52.0, 8.0)];
        let out = resolve(&entries, Rect::in_space(0.0, 0.0, 800.0, 200.0));
        assert!(out.iter().all(Option::is_some));
        assert_eq!(out[0].as_ref().unwrap().visual_rect.intersection(&out[1].as_ref().unwrap().visual_rect), 0.0);
    }
    #[test]
    fn oversized_background_is_clipped_before_neighbor() {
        let entries = [c("button1", 20.0, 30.0, 70.0, 20.0, 30.0), c("button2", 120.0, 30.0, 70.0, 20.0, 5.0)];
        let out = resolve(&entries, Rect::in_space(0.0, 0.0, 300.0, 100.0));
        assert!(out.iter().all(Option::is_some));
        assert!(out[0].as_ref().unwrap().background_rect.right() <= out[1].as_ref().unwrap().safe_rect.x);
    }
    #[test]
    fn overlapping_text_is_never_painted() {
        let entries = [c("subtitle", 20.0, 30.0, 140.0, 20.0, 2.0), c("hud", 150.0, 30.0, 80.0, 20.0, 2.0)];
        let out = resolve(&entries, Rect::in_space(0.0, 0.0, 300.0, 100.0));
        assert!(out.iter().any(Option::is_none));
    }

    #[test]
    fn negative_monitor_coordinates_are_preserved() {
        let entries = [c("subtitle", -800.0, 200.0, 250.0, 40.0, 10.0),
            c("hud", -400.0, 200.0, 120.0, 40.0, 10.0)];
        let out = resolve(&entries, Rect::in_space(-900.0, 0.0, 900.0, 600.0));
        assert!(out.iter().all(Option::is_some));
        assert!(out[0].as_ref().unwrap().visual_rect.x < 0.0);
    }
    #[test]
    fn small_text_shift_can_save_an_outline() {
        let mut first = c("left", 20.0, 20.0, 20.0, 20.0, 0.0);
        first.effect_margin = 4.0;
        let second = c("right", 45.0, 20.0, 20.0, 20.0, 0.0);
        let out = resolve(&[first, second], Rect::in_space(0.0, 0.0, 100.0, 100.0));
        assert!(out[0].is_some());
        assert!(out[0].as_ref().unwrap().text_rect.x < 20.0);
    }
}
