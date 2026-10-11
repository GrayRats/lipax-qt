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

/// Box mean of a luma plane with edge clamping, through an integral image.
fn local_mean(luma: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
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
            let sum = integral[y1 * stride + x1] + integral[y0 * stride + x0]
                - integral[y0 * stride + x1] - integral[y1 * stride + x0];
            out[y * w + x] = (sum / ((x1 - x0) * (y1 - y0)) as u64) as u8;
        }
    }
    out
}

/// Ink of one polarity: brighter (`bright`) or darker than the local mean by more than `CONTRAST`.
pub(super) fn contrast_masks(luma: &[u8], w: usize, h: usize) -> [InkMask; 2] {
    let mean = local_mean(luma, w, h, window_radius(w, h));
    let make = |bright: bool| {
        let ink = luma.iter().zip(&mean).map(|(&l, &m)| {
            let d = l as i32 - m as i32;
            if bright { d > CONTRAST } else { -d > CONTRAST }
        }).collect();
        InkMask { w, h, ink, ink_is_dark: !bright }
    };
    [make(true), make(false)]
}
