//! Text for the tests that read rendered frames: the bundled fonts drawn by a pure Rust rasterizer (no Python).
#![allow(dead_code)]

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use image::RgbImage;
use std::path::{Path, PathBuf};

pub fn fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts")
}

/// A bundled font at a size in pixels (the size of the em square, like the `size` of a typical imaging library).
pub struct Text {
    font: FontVec,
    scale: PxScale,
}

impl Text {
    pub fn load(file: &str, size: u32) -> Option<Self> {
        let font = FontVec::try_from_vec(std::fs::read(fonts_dir().join(file)).ok()?).ok()?;
        let scale = PxScale::from(size as f32 * font.height_unscaled() / font.units_per_em()?);
        Some(Self { font, scale })
    }

    pub fn width(&self, text: &str) -> f32 {
        let scaled = self.font.as_scaled(self.scale);
        let mut width = 0.0;
        let mut previous = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                width += scaled.kern(p, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        width
    }

    /// Draws `text` with its top-left corner (the top of the ascent) at `x, y`, blending `color` over the image.
    pub fn draw(&self, img: &mut RgbImage, x: f32, y: f32, text: &str, color: [u8; 3]) {
        let scaled = self.font.as_scaled(self.scale);
        let baseline = y + scaled.ascent();
        let mut caret = x;
        let mut previous = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                caret += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(self.scale, point(caret, baseline));
            if let Some(outline) = self.font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let (px, py) = (bounds.min.x as i64 + gx as i64, bounds.min.y as i64 + gy as i64);
                    if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        for (channel, ink) in pixel.0.iter_mut().zip(color) {
                            *channel = (*channel as f32 * (1.0 - coverage) + ink as f32 * coverage).round() as u8;
                        }
                    }
                });
            }
            caret += scaled.h_advance(id);
            previous = Some(id);
        }
    }
}

/// A repeatable pseudo-random sequence (the textured backgrounds must not change between runs).
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// An integer in `-range..=range`.
    pub fn next(&mut self, range: i32) -> i32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as i32 % (2 * range + 1) - range
    }
}
