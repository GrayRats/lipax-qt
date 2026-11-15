//! Repeating backgrounds (checks, argyle, dots, stripes) under "text replacement".
//!
//! The exemplar search of `background.rs` compares a 5×5 neighbourhood and copies one pixel at a time: on a regular
//! pattern whose cells are larger than that context it loses the phase and leaves blotches. A repeating pattern is
//! described better by its lattice: the shifts that map the picture onto itself. The lattice is estimated from the
//! clean pixels around (and between) the letters, and every pixel of the hole takes the colour of the nearest clean
//! pixels one or more periods away. Where those copies do not agree (the pattern is not exactly regular there, or
//! the lattice is wrong) the pixel is left to the exemplar search and the diffusion.

use image::RgbaImage;

/// The shortest period searched for (px, the larger of the two components). A shorter "period" is merely the
/// smoothness of the picture.
const MIN_PERIOD: i32 = 3;
/// Sampled clean pixels per error estimate (a grid of the clean pixels thinned to about this many).
const MAX_SAMPLES: usize = 2500;
/// Fewer clean pixels than this: no estimate.
const MIN_SAMPLES: usize = 200;
/// A shift counts only if it maps at least this share of the samples onto clean pixels.
const MIN_OVERLAP: f32 = 0.2;
/// A lattice shift must match within this share of the error of two unrelated pixels (twice the variance).
const MATCH_SHARE: f32 = 0.12;
/// The shift by half a period must miss by at least this share of the error of two unrelated pixels.
const VALLEY_DEPTH: f32 = 0.25;
/// Copies of one pixel taken from different periods may differ by at most this many levels in any channel.
const AGREEMENT: f32 = 36.0;
/// Copies looked at per pixel; the colour is their per-channel median.
const COPIES: usize = 3;

/// A piece of the frame (`x0`, `y0` in frame px) and which of its pixels are background without text.
pub(super) struct Context {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    colour: Vec<[f32; 3]>,
    clean: Vec<bool>,
}

impl Context {
    pub(super) fn new(img: &RgbaImage, (x0, y0, w, h): (u32, u32, u32, u32), clean: impl Fn(u32, u32) -> bool) -> Self {
        let mut colour = Vec::with_capacity((w * h) as usize);
        let mut is_clean = Vec::with_capacity((w * h) as usize);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let p = img.get_pixel(x, y).0;
                colour.push([p[0] as f32, p[1] as f32, p[2] as f32]);
                is_clean.push(clean(x, y));
            }
        }
        Self { x0: x0 as i32, y0: y0 as i32, w: w as i32, h: h as i32, colour, clean: is_clean }
    }

    /// The index of the context pixel at (x, y) in context coordinates, if it is in the context and clean.
    fn clean_at(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| (y * self.w + x) as usize).filter(|&i| self.clean[i])
    }
}

/// The shifts that map the pattern onto itself, nearest first (combinations of one or two basis vectors).
pub(super) struct Lattice {
    pub(super) basis: Vec<(i32, i32)>,
    offsets: Vec<(i32, i32)>,
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// The lattice of a repeating pattern in the clean pixels of `ctx`, searched up to `max_shift` px. `None` for a
/// picture that does not repeat (a photo, clouds, a gradient) or has too little clean background to tell.
pub(super) fn find_lattice(ctx: &Context, max_shift: i32) -> Option<Lattice> {
    let clean_count = ctx.clean.iter().filter(|&&c| c).count();
    if clean_count < MIN_SAMPLES { return None; }
    let step = ((clean_count as f32 / MAX_SAMPLES as f32).sqrt().ceil() as i32).max(1);
    let samples: Vec<(i32, i32)> = (0..ctx.h).step_by(step as usize)
        .flat_map(|y| (0..ctx.w).step_by(step as usize).map(move |x| (x, y)))
        .filter(|&(x, y)| ctx.clean_at(x, y).is_some()).collect();
    if samples.len() < MIN_SAMPLES { return None; }
    let n = samples.len() as f32;
    let mean = samples.iter().fold([0.0f32; 3], |acc, &(x, y)| {
        let c = ctx.colour[(y * ctx.w + x) as usize];
        [acc[0] + c[0] / n, acc[1] + c[1] / n, acc[2] + c[2] / n]
    });
    let variance = samples.iter().map(|&(x, y)| distance2(ctx.colour[(y * ctx.w + x) as usize], mean)).sum::<f32>() / n;
    // A flat background has nothing to repeat; any fill will do.
    if variance < 25.0 { return None; }
    let unrelated = 2.0 * variance;
    let min_overlap = (MIN_OVERLAP * n) as usize;
    let error = |dx: i32, dy: i32| -> Option<f32> {
        let (mut sum, mut count) = (0.0f32, 0usize);
        for &(x, y) in &samples {
            if let Some(j) = ctx.clean_at(x + dx, y + dy) {
                sum += distance2(ctx.colour[(y * ctx.w + x) as usize], ctx.colour[j]);
                count += 1;
            }
        }
        (count >= min_overlap.max(1)).then(|| sum / count as f32)
    };
    let max_shift = max_shift.min(ctx.w.max(ctx.h) - 1);
    // Half of the plane: a shift and its opposite match equally.
    let shifts: Vec<(i32, i32)> = (0..=max_shift).flat_map(|dy| (-max_shift..=max_shift).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| (dy > 0 || dx > 0) && dx.abs().max(dy) >= MIN_PERIOD).collect();
    let errors: Vec<((i32, i32), f32)> = std::thread::scope(|scope| {
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).div_ceil(2).clamp(1, 6);
        let piece = shifts.len().div_ceil(workers).max(1);
        let error = &error;
        let handles: Vec<_> = shifts.chunks(piece)
            .map(|part| scope.spawn(move || part.iter().filter_map(|&(dx, dy)| error(dx, dy).map(|e| ((dx, dy), e))).collect::<Vec<_>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("lattice search panicked")).collect()
    });
    // A smooth picture (large cells of a pattern, too) matches itself at every short shift, so the smallest error says
    // nothing. A period is a valley of the error: lower than at the neighbouring shifts, and the shift by half of it
    // matches badly (it lands on the other colour of the pattern).
    let map: std::collections::HashMap<(i32, i32), f32> = errors.iter().copied().collect();
    let at = |(dx, dy): (i32, i32)| map.get(&(dx, dy)).or_else(|| map.get(&(-dx, -dy))).copied();
    let valley = |&&((dx, dy), e): &&((i32, i32), f32)| {
        e <= MATCH_SHARE * unrelated
            && (-1..=1).all(|oy| (-1..=1).all(|ox| (ox, oy) == (0, 0) || at((dx + ox, dy + oy)).is_none_or(|n| n >= e)))
            && {
                let half = ((dx as f32 / 2.0).round() as i32, (dy as f32 / 2.0).round() as i32);
                error(half.0, half.1).is_some_and(|h| h >= (4.0 * e).max(VALLEY_DEPTH * unrelated))
            }
    };
    let periods: Vec<((i32, i32), f32)> = errors.iter().filter(valley).copied().collect();
    let best = periods.iter().map(|&(_, e)| e).fold(f32::INFINITY, f32::min);
    if !best.is_finite() { return None; }
    // The shortest period that matches about as well as the best one: the best one may be a multiple of it.
    let good = (2.0 * best).max(best + 0.02 * unrelated);
    let norm = |(dx, dy): (i32, i32)| dx * dx + dy * dy;
    let first = periods.iter().filter(|&&(_, e)| e <= good).map(|&(d, _)| d).min_by_key(|&d| norm(d))?;
    let length = (norm(first) as f32).sqrt();
    let second = periods.iter()
        .filter(|&&(d, e)| e <= good && ((d.0 * first.1 - d.1 * first.0).abs() as f32) >= length * MIN_PERIOD as f32)
        .map(|&(d, _)| d).min_by_key(|&d| norm(d));
    let basis: Vec<(i32, i32)> = std::iter::once(first).chain(second).collect();
    // Combinations of the basis within about three periods, nearest first.
    let k = 3;
    let mut offsets: Vec<(i32, i32)> = Vec::new();
    for i in -k..=k {
        for j in if basis.len() > 1 { -k..=k } else { 0..=0 } {
            let o = (i * first.0 + j * basis.get(1).map_or(0, |b| b.0), i * first.1 + j * basis.get(1).map_or(0, |b| b.1));
            if o != (0, 0) && !offsets.contains(&o) { offsets.push(o); }
        }
    }
    offsets.sort_by_key(|&o| norm(o));
    Some(Lattice { basis, offsets })
}

/// The colour of the frame pixel (`fx`, `fy`) continued along the lattice: the per-channel median of the nearest
/// clean copies. `None` without two agreeing copies.
pub(super) fn copy_along(ctx: &Context, lattice: &Lattice, fx: i32, fy: i32) -> Option<[f32; 3]> {
    let (x, y) = (fx - ctx.x0, fy - ctx.y0);
    let copies: Vec<[f32; 3]> = lattice.offsets.iter()
        .filter_map(|&(dx, dy)| ctx.clean_at(x + dx, y + dy).map(|j| ctx.colour[j]))
        .take(COPIES).collect();
    if copies.len() < 2 { return None; }
    if (0..3).any(|c| (copies[0][c] - copies[1][c]).abs() > AGREEMENT) { return None; }
    Some(std::array::from_fn(|c| {
        let mut v: Vec<f32> = copies.iter().map(|p| p[c]).collect();
        v.sort_by(f32::total_cmp);
        if v.len() % 2 == 1 { v[v.len() / 2] } else { 0.5 * (v[v.len() / 2 - 1] + v[v.len() / 2]) }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Diagonal checks of two blues, cells of `cell` px.
    fn checks(w: u32, h: u32, cell: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| if ((x + y) / cell + (x + 2 * cell * 64 - y) / cell).is_multiple_of(2) { Rgba([20, 120, 230, 255]) } else { Rgba([0, 170, 240, 255]) })
    }

    #[test]
    fn the_period_of_checks_is_found_and_a_hole_is_filled_in_phase() {
        let img = checks(300, 120, 15);
        // A band through the middle is "text": not clean.
        let hole = |x: u32, y: u32| (40..80).contains(&y) && (30..270).contains(&x);
        let ctx = Context::new(&img, (0, 0, 300, 120), |x, y| !hole(x, y));
        let lattice = find_lattice(&ctx, 48).expect("checks repeat");
        let mut wrong = 0;
        for y in 40..80 {
            for x in 30..270 {
                let got = copy_along(&ctx, &lattice, x as i32, y as i32).expect("a copy");
                let want = img.get_pixel(x, y).0;
                if (0..3).any(|c| (got[c] - want[c] as f32).abs() > 8.0) { wrong += 1; }
            }
        }
        assert!(wrong < 240 * 40 / 50, "{wrong} of {} pixels out of phase, basis {:?}", 240 * 40, lattice.basis);
    }

    #[test]
    fn a_gradient_or_a_flat_colour_has_no_lattice() {
        let flat = RgbaImage::from_pixel(200, 80, Rgba([40, 40, 60, 255]));
        assert!(find_lattice(&Context::new(&flat, (0, 0, 200, 80), |_, _| true), 32).is_none());
        let gradient = RgbaImage::from_fn(200, 80, |x, y| Rgba([(x / 2) as u8, (y * 2) as u8, 128, 255]));
        assert!(find_lattice(&Context::new(&gradient, (0, 0, 200, 80), |_, _| true), 32).is_none());
    }
}
