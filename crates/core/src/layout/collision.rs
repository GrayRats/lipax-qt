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
    /// Where the field may be drawn at all: the part of the game's client area on the screen that
    /// shows its original. Nothing of the field (text, plate, outline) is accepted outside it.
    pub bounds: Rect,
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

/// The room of field `index`: its bounds, cut halfway towards every neighbour whose original it faces
/// across a free strip (side by side or one above the other). Empty if an earlier field reads the same
/// text (an OCR duplicate or a nested field): both cannot be drawn.
pub fn safe_rect(candidates: &[Candidate], index: usize) -> Rect {
    let item = &candidates[index];
    let mut safe = item.bounds;
    for (other_index, other) in candidates.iter().enumerate() {
        if index == other_index { continue; }
        let a = item.source_rect;
        let b = other.source_rect;
        let horizontal_overlap = a.x < b.right() && a.right() > b.x;
        let vertical_overlap = a.y < b.bottom() && a.bottom() > b.y;
        if vertical_overlap && !horizontal_overlap {
            if a.center().0 < b.center().0 { safe.w = safe.w.min(((a.right() + b.x) * 0.5 - safe.x).max(0.0)); }
            else { let left = (b.right() + a.x) * 0.5; safe.w = (safe.right() - left.max(safe.x)).max(0.0); safe.x = left.max(safe.x); }
        } else if horizontal_overlap && !vertical_overlap {
            if a.center().1 < b.center().1 { safe.h = safe.h.min(((a.bottom() + b.y) * 0.5 - safe.y).max(0.0)); }
            else { let top = (b.bottom() + a.y) * 0.5; safe.h = (safe.bottom() - top.max(safe.y)).max(0.0); safe.y = top.max(safe.y); }
        } else if horizontal_overlap && vertical_overlap && other_index < index {
            return Rect::default();
        }
    }
    safe
}

pub fn resolve(candidates: &[Candidate]) -> Vec<Option<Placement>> {
    resolve_with(candidates, true)
}

/// `resolve` without the log: used to try a simpler variant of a field that did not fit, where
/// every attempt would repeat the same warning.
pub fn resolve_quiet(candidates: &[Candidate]) -> Vec<Option<Placement>> {
    resolve_with(candidates, false)
}

fn resolve_with(candidates: &[Candidate], log: bool) -> Vec<Option<Placement>> {
    let mut accepted: Vec<Rect> = Vec::new();
    let mut results = Vec::with_capacity(candidates.len());
    for (index, item) in candidates.iter().enumerate() {
        let safe = safe_rect(candidates, index);
        let m = item.effect_margin.max(0.0);
        let mut text = item.text_rect;
        // The outline may be cut at the edge of the bounds; the text itself may not.
        let expanded = |r: Rect| Rect::in_space(r.x - m, r.y - m, r.w + 2.0 * m, r.h + 2.0 * m).clipped_to(&item.bounds);
        let mut text_visual = expanded(text);
        if item.allow_text_shift && !(safe.contains(&text_visual) && safe.contains(&text)) {
            let shift = |lo: f32, hi: f32, from: f32, to: f32| if lo < from { from - lo } else if hi > to { to - hi } else { 0.0 };
            let dx = shift(text.x.min(text_visual.x), text.right().max(text_visual.right()), safe.x, safe.right());
            let dy = shift(text.y.min(text_visual.y), text.bottom().max(text_visual.bottom()), safe.y, safe.bottom());
            if dx.abs() <= item.source_rect.h.min(16.0) * 0.25 && dy.abs() <= item.source_rect.h.min(16.0) * 0.25 {
                text.x += dx;
                text.y += dy;
                text_visual = expanded(text);
            }
        }
        // Fit (wrap, line spacing, tracking, size, condensed) has already run using Qt metrics.
        // Then remove excessive padding. No movement is attempted if it would uncover the
        // original glyphs; an impossible field is skipped instead of overlapping the game HUD.
        let background = item.background_rect.clipped_to(&safe);
        let visual = background.union(&text_visual);
        // The text box is checked as it is, not as clipped: a translation hanging over the edge of the
        // window or the screen would be cut there.
        let impossible = !safe.contains(&text) || !safe.contains(&text_visual)
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
    fn c(id: &str, x: f32, y: f32, w: f32, h: f32, pad: f32, bounds: Rect) -> Candidate {
        let text = Rect::in_space(x, y, w, h);
        Candidate { id: id.into(), source_rect: text, text_rect: text, bounds,
            background_rect: Rect::in_space(x - pad, y - pad, w + 2.0 * pad, h + 2.0 * pad), effect_margin: 0.0, image_background: false, allow_text_shift: true }
    }
    #[test]
    fn separate_name_and_dialogue_keep_separate_visuals() {
        let b = Rect::in_space(0.0, 0.0, 800.0, 200.0);
        let entries = [c("name", 40.0, 30.0, 100.0, 20.0, 8.0, b), c("dialogue", 40.0, 64.0, 340.0, 52.0, 8.0, b)];
        let out = resolve(&entries);
        assert!(out.iter().all(Option::is_some));
        assert_eq!(out[0].as_ref().unwrap().visual_rect.intersection(&out[1].as_ref().unwrap().visual_rect), 0.0);
    }
    #[test]
    fn oversized_background_is_clipped_before_neighbor() {
        let b = Rect::in_space(0.0, 0.0, 300.0, 100.0);
        let entries = [c("button1", 20.0, 30.0, 70.0, 20.0, 30.0, b), c("button2", 120.0, 30.0, 70.0, 20.0, 5.0, b)];
        let out = resolve(&entries);
        assert!(out.iter().all(Option::is_some));
        assert!(out[0].as_ref().unwrap().background_rect.right() <= out[1].as_ref().unwrap().safe_rect.x);
    }
    #[test]
    fn overlapping_text_is_never_painted() {
        let b = Rect::in_space(0.0, 0.0, 300.0, 100.0);
        let entries = [c("subtitle", 20.0, 30.0, 140.0, 20.0, 2.0, b), c("hud", 150.0, 30.0, 80.0, 20.0, 2.0, b)];
        let out = resolve(&entries);
        assert!(out.iter().any(Option::is_none));
    }

    #[test]
    fn negative_monitor_coordinates_are_preserved() {
        let b = Rect::in_space(-900.0, 0.0, 900.0, 600.0);
        let entries = [c("subtitle", -800.0, 200.0, 250.0, 40.0, 10.0, b),
            c("hud", -400.0, 200.0, 120.0, 40.0, 10.0, b)];
        let out = resolve(&entries);
        assert!(out.iter().all(Option::is_some));
        assert!(out[0].as_ref().unwrap().visual_rect.x < 0.0);
    }
    #[test]
    fn small_text_shift_can_save_an_outline() {
        let b = Rect::in_space(0.0, 0.0, 100.0, 100.0);
        let mut first = c("left", 20.0, 20.0, 20.0, 20.0, 0.0, b);
        first.effect_margin = 4.0;
        let second = c("right", 45.0, 20.0, 20.0, 20.0, 0.0, b);
        let out = resolve(&[first, second]);
        assert!(out[0].is_some());
        assert!(out[0].as_ref().unwrap().text_rect.x < 20.0);
    }

    #[test]
    fn text_hanging_over_the_bounds_is_refused_not_cut() {
        // Before, the outline-expanded text was clipped to the bounds first and then found "inside".
        let b = Rect::in_space(0.0, 0.0, 300.0, 100.0);
        let mut field = c("edge", 250.0, 40.0, 40.0, 20.0, 0.0, b);
        field.text_rect = Rect::in_space(250.0, 40.0, 120.0, 20.0);
        field.effect_margin = 2.0;
        assert!(resolve(&[field.clone()])[0].is_none(), "70 px of the text would be outside the window");
        // A hair over the edge is moved back in instead.
        field.text_rect = Rect::in_space(252.0, 40.0, 50.0, 20.0);
        let placed = resolve(&[field])[0].clone().expect("shifted inside");
        assert!(placed.text_rect.right() <= 300.0 + 0.5, "{:?}", placed.text_rect);
    }

    #[test]
    fn each_field_keeps_to_its_own_bounds() {
        // Two monitors side by side: a field on the right one may not reach over to the left one.
        let left = Rect::in_space(0.0, 0.0, 100.0, 100.0);
        let right = Rect::in_space(100.0, 0.0, 100.0, 100.0);
        let mut field = c("right", 110.0, 40.0, 40.0, 20.0, 15.0, right);
        let placed = resolve(&[field.clone()])[0].clone().expect("fits");
        assert!(placed.background_rect.x >= 100.0, "the plate is cut at the monitor edge: {:?}", placed.background_rect);
        field.bounds = left;
        assert!(resolve(&[field])[0].is_none(), "text outside its bounds is never accepted");
    }

    #[test]
    fn the_room_is_split_halfway_between_neighbours() {
        let b = Rect::in_space(0.0, 0.0, 400.0, 200.0);
        let entries = [c("top", 20.0, 20.0, 100.0, 20.0, 0.0, b), c("bottom", 20.0, 100.0, 100.0, 20.0, 0.0, b), c("beside", 300.0, 20.0, 50.0, 20.0, 0.0, b)];
        let top = safe_rect(&entries, 0);
        assert_eq!((top.y, top.bottom()), (0.0, 70.0), "halfway to the field below");
        assert_eq!(top.right(), 210.0, "halfway to the field beside");
        let duplicate = [c("a", 20.0, 20.0, 100.0, 20.0, 0.0, b), c("b", 25.0, 22.0, 100.0, 20.0, 0.0, b)];
        assert_eq!(safe_rect(&duplicate, 1).area(), 0.0, "the second reading of the same text gets no room");
    }
}
