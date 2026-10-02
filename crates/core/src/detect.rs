//! Дешёвое определение изменения кадра: уменьшенная копия в градациях серого
//! и средняя абсолютная разница с предыдущей.

use image::{DynamicImage, imageops::FilterType};

const SIDE: u32 = 64;

pub struct ChangeDetector {
    prev: Option<Vec<u8>>,
}

impl ChangeDetector {
    pub fn new() -> Self {
        Self { prev: None }
    }

    pub fn reset(&mut self) {
        self.prev = None;
    }

    fn thumb(img: &DynamicImage) -> Vec<u8> {
        img.resize_exact(SIDE, SIDE, FilterType::Triangle).to_luma8().into_raw()
    }

    /// Возвращает true, если кадр заметно отличается от предыдущего
    /// (первый кадр всегда считается изменённым).
    pub fn changed(&mut self, img: &DynamicImage, threshold: f32) -> bool {
        let cur = Self::thumb(img);
        let changed = match &self.prev {
            None => true,
            Some(p) => {
                let sum: u64 = p.iter().zip(&cur).map(|(a, b)| a.abs_diff(*b) as u64).sum();
                (sum as f32 / cur.len() as f32) >= threshold
            }
        };
        if changed {
            self.prev = Some(cur);
        }
        changed
    }
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn solid(v: u8) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 100, Rgba([v, v, v, 255])))
    }

    #[test]
    fn identical_frames_not_changed() {
        let mut d = ChangeDetector::new();
        assert!(d.changed(&solid(10), 2.0));
        assert!(!d.changed(&solid(10), 2.0));
    }

    #[test]
    fn different_frames_changed() {
        let mut d = ChangeDetector::new();
        d.changed(&solid(10), 2.0);
        assert!(d.changed(&solid(200), 2.0));
    }

    #[test]
    fn small_noise_below_threshold() {
        let mut d = ChangeDetector::new();
        d.changed(&solid(100), 2.0);
        assert!(!d.changed(&solid(101), 2.0));
    }
}
