//! Local-contrast ink: pixels that stand out from their own neighbourhood.
//!
//! A single global threshold (Otsu) follows the dominant brightness split of the whole frame. Text that is
//! only slightly brighter or darker than the panel it sits on (a translucent HUD, text over a gradient) has
//! no weight in that split and disappears. Here each pixel is compared with the mean of a window around
//! it instead. This mask is noisy on its own (every edge of the scene shows up), so the detector only
//! accepts regions of it that are shaped like lines of text.

use super::InkMask;

/// Window radius in pixels for a frame of this size: larger than a stroke, about a glyph height.
pub(super) fn window_radius(w: usize, h: usize) -> usize {
    (w.min(h) / 90).clamp(6, 20)
}

/// Minimum luma difference from the local mean for a pixel to count as ink.
pub(super) const CONTRAST: i32 = 22;

/// Box mean of a luma plane with edge clamping: the window shrinks at the edges and the sum is divided by its real
/// area. Two sliding sums (along the rows, then down the columns) in `u32`: the old integral image of `u64` was 22 MB for
/// a 2560×1080 frame and its divisions took most of the time of the detector. The values are the same, bit for bit.
fn local_mean(luma: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 { return Vec::new(); }
    // Sums along the rows: the window [x − r, x + r] of every pixel.
    let mut rows = vec![0u32; w * h];
    for y in 0..h {
        let row = &luma[y * w..(y + 1) * w];
        let out = &mut rows[y * w..(y + 1) * w];
        let mut sum: u32 = row[..(r + 1).min(w)].iter().map(|&v| v as u32).sum();
        for x in 0..w {
            out[x] = sum;
            if x + r + 1 < w { sum += row[x + r + 1] as u32; }
            if x >= r { sum -= row[x - r] as u32; }
        }
    }
    // Interior pixels share one divisor: `n / d` as a multiplication (exact: n < 2^20, d < 2^12, 2^40 / d > n).
    let interior = 2 * r + 1;
    let magic = (1u64 << 40).div_ceil((interior * interior) as u64);
    let width_of = |x: usize| (x + r + 1).min(w) - x.saturating_sub(r);
    let mut columns: Vec<u32> = (0..w).map(|x| (0..(r + 1).min(h)).map(|y| rows[y * w + x]).sum()).collect();
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let height = (y + r + 1).min(h) - y.saturating_sub(r);
        let line = &mut out[y * w..(y + 1) * w];
        if height == interior && w > 2 * r {
            for x in 0..r { line[x] = (columns[x] / (width_of(x) * height) as u32) as u8; }
            for x in r..w - r { line[x] = ((columns[x] as u64 * magic) >> 40) as u8; }
            for x in w - r..w { line[x] = (columns[x] / (width_of(x) * height) as u32) as u8; }
        } else {
            for x in 0..w { line[x] = (columns[x] / (width_of(x) * height) as u32) as u8; }
        }
        // Down one row: the row entering the window is added, the one leaving it taken off.
        if y + r + 1 < h { for x in 0..w { columns[x] += rows[(y + r + 1) * w + x]; } }
        if y >= r { for x in 0..w { columns[x] -= rows[(y - r) * w + x]; } }
    }
    out
}

/// Ink of one polarity: brighter (`bright`) or darker than the local mean by more than `CONTRAST`.
///
/// The frame is cut into bands of rows that are computed on separate threads. A band needs the rows `window_radius`
/// above and below it for its means; with them the means of its own rows are exactly those of the whole frame (the
/// window of a row never reaches further). The threads are shared with the three planes the detector searches at
/// once, so a plane gets its share of half the cores: the game keeps the rest.
pub(super) fn contrast_masks(luma: &[u8], w: usize, h: usize) -> [InkMask; 2] {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    contrast_masks_in_bands(luma, w, h, ((cores / 2) / 3).clamp(1, 4))
}

pub(super) fn contrast_masks_in_bands(luma: &[u8], w: usize, h: usize, bands: usize) -> [InkMask; 2] {
    let r = window_radius(w, h);
    // A band of fewer than 128 rows is not worth a thread.
    let bands = bands.min(h / 128).max(1);
    let rows_per_band = h.div_ceil(bands);
    let band = |y0: usize, y1: usize| -> (Vec<bool>, Vec<bool>) {
        let (top, bottom) = (y0.saturating_sub(r), (y1 + r).min(h));
        let plane = &luma[top * w..bottom * w];
        let mean = local_mean(plane, w, bottom - top, r);
        // Two plain passes over the band: the compiler turns each of them into vector code.
        let (rows, band_mean) = (&luma[y0 * w..y1 * w], &mean[(y0 - top) * w..(y1 - top) * w]);
        let bright: Vec<bool> = rows.iter().zip(band_mean).map(|(&l, &m)| l as i32 - m as i32 > CONTRAST).collect();
        let dark: Vec<bool> = rows.iter().zip(band_mean).map(|(&l, &m)| (m as i32 - l as i32) > CONTRAST).collect();
        (bright, dark)
    };
    let pieces: Vec<(Vec<bool>, Vec<bool>)> = if bands == 1 {
        vec![band(0, h)]
    } else {
        std::thread::scope(|scope| {
            let band = &band;
            let workers: Vec<_> = (0..bands).map(|i| (i * rows_per_band, ((i + 1) * rows_per_band).min(h)))
                .filter(|(y0, y1)| y0 < y1)
                .map(|(y0, y1)| scope.spawn(move || band(y0, y1))).collect();
            workers.into_iter().map(|worker| worker.join().expect("local-contrast band panicked")).collect()
        })
    };
    let (mut bright, mut dark) = (Vec::with_capacity(w * h), Vec::with_capacity(w * h));
    for (b, d) in pieces { bright.extend(b); dark.extend(d); }
    [InkMask { w, h, ink: bright, ink_is_dark: false }, InkMask { w, h, ink: dark, ink_is_dark: true }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integral-image version this replaced: the reference for the exact values.
    fn reference(luma: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
        let stride = w + 1;
        let mut integral = vec![0u64; stride * (h + 1)];
        for y in 0..h {
            let mut row = 0u64;
            for x in 0..w {
                row += luma[y * w + x] as u64;
                integral[(y + 1) * stride + x + 1] = integral[y * stride + x + 1] + row;
            }
        }
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
            for x in 0..w {
                let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
                let sum = integral[y1 * stride + x1] + integral[y0 * stride + x0] - integral[y0 * stride + x1] - integral[y1 * stride + x0];
                out[y * w + x] = (sum / ((x1 - x0) * (y1 - y0)) as u64) as u8;
            }
        }
        out
    }

    fn noise(w: usize, h: usize, seed: u32) -> Vec<u8> {
        let mut state = seed;
        (0..w * h).map(|_| { state = state.wrapping_mul(1664525).wrapping_add(1013904223); (state >> 24) as u8 }).collect()
    }

    #[test]
    fn the_sliding_box_mean_equals_the_integral_image_one_bit_for_bit() {
        // Frames smaller than the window, thin ones, odd sizes and the sizes the detector really uses.
        for (w, h, r) in [(1, 1, 6), (5, 9, 6), (13, 40, 6), (64, 64, 7), (101, 77, 20), (200, 31, 6), (333, 255, 20), (1920, 1080, 12), (40, 300, 6)] {
            let luma = noise(w, h, (w * 31 + h) as u32);
            assert_eq!(local_mean(&luma, w, h, r), reference(&luma, w, h, r), "{w}×{h} r={r}");
        }
        // The extremes: all black and all white (the largest sums).
        for value in [0u8, 255] {
            let flat = vec![value; 300 * 200];
            assert_eq!(local_mean(&flat, 300, 200, 20), vec![value; 300 * 200]);
        }
    }

    #[test]
    fn bands_of_rows_give_the_same_masks_as_the_whole_frame() {
        // Frames that cut into bands unevenly, thin ones, and the size the detector really sees.
        for (w, h) in [(200, 80), (300, 300), (333, 517), (640, 1031), (1920, 1080)] {
            let plane = noise(w, h, (w + 7 * h) as u32);
            let whole = contrast_masks_in_bands(&plane, w, h, 1);
            for bands in [2, 3, 4, 7] {
                let cut = contrast_masks_in_bands(&plane, w, h, bands);
                assert!(cut[0].ink == whole[0].ink && cut[1].ink == whole[1].ink, "{w}×{h} in {bands} bands");
            }
        }
    }

    #[test]
    fn ink_is_what_differs_from_its_neighbourhood() {
        let (w, h) = (200, 80);
        let mut plane = vec![60u8; w * h];
        for y in 30..40 { for x in 50..150 { plane[y * w + x] = 200; } }
        let [bright, dark] = contrast_masks(&plane, w, h);
        assert!(bright.at(100, 35) && !dark.at(100, 35));
        assert!(!bright.at(10, 10) && !dark.at(10, 10));
    }
}
