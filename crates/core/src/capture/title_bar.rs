//! A hint for windows that draw their own title bar (GTK/libadwaita client-side decorations).
//! KWin cuts the server-side decoration off the frame, but a header bar painted by the application
//! is part of its client area and nothing in the protocol tells where it ends. It is recognised from
//! the picture: a band at the top that stays the same colour across the whole width and ends at a
//! full-width edge. This is advice for choosing the region, never a crop.

use image::{DynamicImage, GrayImage};

/// Header bars are at least this tall (px of the frame); anything thinner is a border or a line.
const MIN_HEIGHT: u32 = 24;
/// …and at most this tall, or at most a fifth of the frame.
const MAX_HEIGHT: u32 = 140;
/// Brightness step that counts as an edge.
const EDGE_STEP: i16 = 10;
/// Share of the width an edge must span to separate the bar from the content.
const EDGE_SHARE: f32 = 0.9;
/// Share of the bar's pixels that must have the bar's own colour (the rest is title, icons, buttons).
const BAR_UNIFORMITY: f32 = 0.6;
/// How much brighter or darker than the bar the content below must be, on average.
const CONTENT_CONTRAST: f32 = 8.0;

/// Height in pixels of a title bar the application draws itself at the top of the frame, if there is one.
pub fn detect(frame: &DynamicImage) -> Option<u32> {
    let gray = frame.to_luma8();
    let (w, h) = gray.dimensions();
    if w < 200 || h < 200 { return None; }
    let limit = MAX_HEIGHT.min(h / 5);
    (MIN_HEIGHT..=limit).find(|&y| is_bar_bottom(&gray, y))
}

fn row_mean(gray: &GrayImage, y: u32) -> f32 {
    let w = gray.width();
    (0..w).map(|x| gray.get_pixel(x, y)[0] as f32).sum::<f32>() / w as f32
}

/// Is the horizontal line between rows `y - 1` and `y` the bottom of a header bar?
fn is_bar_bottom(gray: &GrayImage, y: u32) -> bool {
    let w = gray.width();
    // A full-width edge here...
    let edge = (0..w).filter(|&x| (gray.get_pixel(x, y)[0] as i16 - gray.get_pixel(x, y - 1)[0] as i16).abs() >= EDGE_STEP).count();
    if (edge as f32) < EDGE_SHARE * w as f32 { return false; }
    // ...and above it a band of one colour (the bar), whose median is taken over its rows.
    let mut bar: Vec<u8> = (0..y).flat_map(|row| (0..w).map(move |x| (row, x))).map(|(row, x)| gray.get_pixel(x, row)[0]).collect();
    bar.sort_unstable();
    let median = bar[bar.len() / 2] as i16;
    let uniform = bar.iter().filter(|v| (**v as i16 - median).abs() <= 6).count() as f32 / bar.len() as f32;
    if uniform < BAR_UNIFORMITY { return false; }
    // The edge must separate two different things: the content below is not the bar's colour.
    let below = (y..(y + 8).min(gray.height())).map(|row| row_mean(gray, row)).sum::<f32>() / (y + 8).min(gray.height()).saturating_sub(y).max(1) as f32;
    (below - median as f32).abs() >= CONTENT_CONTRAST
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// `bar` rows of `bar_color` with a title and buttons drawn in it, then a textured scene.
    fn window(bar: u32, bar_color: [u8; 3], scene: [u8; 3]) -> DynamicImage {
        let (w, h) = (900, 500);
        let mut img = RgbImage::from_fn(w, h, |x, y| {
            if y < bar { Rgb(bar_color) } else {
                // A little texture so the scene is not a flat colour.
                let n = ((x * 7 + y * 13) % 17) as u8;
                Rgb([scene[0].saturating_add(n), scene[1].saturating_add(n), scene[2].saturating_add(n)])
            }
        });
        for x in 20..220 { for y in 12..28.min(bar) { img.put_pixel(x, y, Rgb([235, 235, 235])); } } // the title
        for x in 800..880 { for y in 10..30.min(bar) { img.put_pixel(x, y, Rgb([200, 200, 205])); } } // buttons
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn a_header_bar_drawn_by_the_application_is_found_with_its_height() {
        assert_eq!(detect(&window(40, [48, 48, 52], [18, 18, 22])), Some(40));
        // A light theme, a taller bar.
        assert!(matches!(detect(&window(47, [236, 236, 238], [250, 250, 252])), Some(h) if h.abs_diff(47) <= 1));
    }

    #[test]
    fn a_scene_without_a_bar_is_left_alone() {
        let scene = RgbImage::from_fn(900, 500, |x, y| { let n = ((x * 31 + y * 17) % 90) as u8; Rgb([40 + n, 60 + n / 2, 30 + n]) });
        assert_eq!(detect(&DynamicImage::ImageRgb8(scene)), None);
        assert_eq!(detect(&DynamicImage::new_rgb8(100, 100)), None, "too small to judge");
    }

    #[test]
    fn a_thin_border_is_not_a_title_bar() {
        assert_eq!(detect(&window(10, [60, 60, 66], [18, 18, 22])), None);
    }

    #[test]
    fn a_bar_of_the_same_colour_as_the_content_is_invisible_and_not_reported() {
        assert_eq!(detect(&window(40, [20, 20, 24], [18, 18, 22])), None);
    }
}
