//! Image filters before OCR and noise removal after it.
//!
//! Both are cheap, run in the OCR task (a Tokio worker, never in the Qt thread) and are off or
//! harmless by default: the filters are opt-in, the noise removal only drops what cannot be text.

use super::{OcrLine, OcrResult};
use crate::settings::TextRecognitionSettings;
use image::{DynamicImage, GrayImage};

/// What is done with a frame before the engine sees it. The order is fixed: invert, contrast, sharpen, binarize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Preprocess {
    /// Bright text on a dark background is turned into dark on bright (the engines are trained on it).
    pub auto_invert: bool,
    /// −100…100, 0 is off.
    pub contrast: i32,
    /// Unsharp mask: helps small, soft fonts.
    pub sharpen: bool,
    /// Otsu's threshold: black text on white.
    pub binarize: bool,
}

impl Preprocess {
    pub fn of(s: &TextRecognitionSettings) -> Self {
        Self { auto_invert: s.auto_invert, contrast: s.contrast.clamp(-100, 100), sharpen: s.sharpen, binarize: s.binarize }
    }

    pub fn is_identity(&self) -> bool {
        !self.auto_invert && self.contrast == 0 && !self.sharpen && !self.binarize
    }

    /// The frame as the engine will see it (before Tesseract's own enlargement). Unchanged if no filter is on.
    pub fn apply(&self, img: &DynamicImage) -> DynamicImage {
        if self.is_identity() { return img.clone(); }
        let mut gray = img.to_luma8();
        if self.auto_invert && mean(&gray) < 128.0 {
            for p in gray.pixels_mut() { p.0[0] = 255 - p.0[0]; }
        }
        if self.contrast != 0 {
            let factor = (100 + self.contrast) as f32 / 100.0;
            for p in gray.pixels_mut() { p.0[0] = ((p.0[0] as f32 - 128.0) * factor + 128.0).round().clamp(0.0, 255.0) as u8; }
        }
        if self.sharpen { gray = image::imageops::unsharpen(&gray, 1.0, 1); }
        if self.binarize {
            let t = otsu(&gray);
            for p in gray.pixels_mut() { p.0[0] = if p.0[0] > t { 255 } else { 0 }; }
        }
        DynamicImage::ImageLuma8(gray)
    }
}

fn mean(img: &GrayImage) -> f32 {
    let n = (img.width() as u64 * img.height() as u64).max(1);
    img.pixels().map(|p| p.0[0] as u64).sum::<u64>() as f32 / n as f32
}

/// Otsu's threshold: the level that maximises the variance between the dark and the bright class.
pub fn otsu(img: &GrayImage) -> u8 {
    let mut hist = [0u64; 256];
    for p in img.pixels() { hist[p.0[0] as usize] += 1; }
    let total: u64 = hist.iter().sum();
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut below, mut below_sum, mut best, mut threshold) = (0u64, 0f64, 0f64, 0u8);
    for t in 0..256usize {
        below += hist[t];
        if below == 0 { continue; }
        let above = total - below;
        if above == 0 { break; }
        below_sum += t as f64 * hist[t] as f64;
        let (m1, m2) = (below_sum / below as f64, (sum - below_sum) / above as f64);
        let between = below as f64 * above as f64 * (m1 - m2) * (m1 - m2);
        if between > best { best = between; threshold = t as u8; }
    }
    threshold
}

/// Marks that high-contrast textures leave in the recognised text; never part of game text on their own.
fn is_noise_char(c: char) -> bool {
    matches!(c, '~' | '|' | '°' | '¦' | '^' | '`' | '¬' | '‖' | '_' | '\\')
}

/// The line without stray noise marks: a word made only of them is dropped, a mark glued to the start or the
/// end of a word is trimmed (`|Hello~` → `Hello`). Marks inside a word stay.
pub fn strip_noise(line: &str) -> String {
    line.split_whitespace()
        .map(|word| word.trim_matches(is_noise_char))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Removes noise marks and, when a line is clearly below `min_confidence` while another one is not, that line.
/// If every line is below the threshold the result is left as it is: the pipeline then rejects the whole
/// text and says why, instead of showing an empty frame.
pub fn clean(mut result: OcrResult, min_confidence: u32, filter_noise: bool) -> OcrResult {
    if result.lines.is_empty() {
        if filter_noise {
            result.text = result.text.lines().map(strip_noise).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        }
        return result;
    }
    let before = result.lines.len();
    let min = min_confidence as f32;
    let any_sure = min_confidence == 0 || result.lines.iter().any(|l| l.confidence >= min);
    let mut kept: Vec<OcrLine> = Vec::with_capacity(before);
    for mut line in std::mem::take(&mut result.lines) {
        if any_sure && line.confidence < min { continue; }
        if filter_noise {
            line.text = strip_noise(&line.text);
            if line.text.is_empty() { continue; }
        }
        kept.push(line);
    }
    // Nothing but noise was left: an empty result is the honest answer.
    let changed = kept.len() != before || kept.iter().zip(result.text.lines()).any(|(l, t)| l.text != t);
    if changed {
        result.text = kept.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        result.confidence = (!kept.is_empty()).then(|| kept.iter().map(|l| l.confidence).sum::<f32>() / kept.len() as f32);
    }
    result.lines = kept;
    result
}

/// A reading below this confidence is rejected while a CJK model is active, whatever the configured value (unless it
/// is 0 = no check). Garbage from a textured background scores 28–30 there, which the default 30 lets through.
pub const CJK_MIN_CONFIDENCE: u32 = 38;

/// Tesseract (`jpn`, `chi_sim`, `kor`, `jpn+eng`, …) and PaddleOCR (`japan`, `ch`, `korean`, …) language codes of Chinese,
/// Japanese and Korean. A combination counts if any of its parts does.
pub fn is_cjk_language(language: &str) -> bool {
    language.split('+').map(str::trim).any(|part| {
        let part = part.to_ascii_lowercase();
        ["jpn", "chi", "kor", "japan", "korean", "ch", "zh", "ja", "ko"].iter().any(|code| part == *code || part.strip_prefix(code).is_some_and(|rest| rest.starts_with('_') || rest.starts_with('-')))
    })
}

/// The threshold in force: `configured`, raised to `CJK_MIN_CONFIDENCE` for a CJK language; 0 stays 0.
pub fn effective_min_confidence(language: &str, configured: u32) -> u32 {
    if configured > 0 && is_cjk_language(language) { configured.max(CJK_MIN_CONFIDENCE) } else { configured }
}

/// What a frame looks like before any filter: brightness and how many different levels it uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    /// Mean luminance, 0–255.
    pub mean: f32,
    /// Shannon entropy of the luminance histogram in bits (0–8). Text on a flat background uses two or three
    /// levels (low); a gradient with a texture behind it uses many (high).
    pub entropy: f32,
}

/// At most this many pixels are looked at, whatever the size of the frame.
const STATS_SAMPLES: u32 = 40_000;
/// An entropy above this is a noisy background (flat ones measure below 3 bits in `tests/ocr_languages.rs`).
pub const NOISY_ENTROPY: f32 = 4.0;

impl FrameStats {
    pub fn of(img: &DynamicImage) -> Self {
        let (w, h) = (img.width().max(1), img.height().max(1));
        let step = (((w as u64 * h as u64) / STATS_SAMPLES as u64) as f32).sqrt().ceil().max(1.0) as u32;
        let mut hist = [0u32; 256];
        let mut count = 0u32;
        let mut sum = 0u64;
        let mut y = 0;
        while y < h {
            let mut x = 0;
            while x < w {
                let [r, g, b, _] = image::GenericImageView::get_pixel(img, x, y).0;
                let luma = ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) >> 8) as usize;
                hist[luma.min(255)] += 1;
                sum += luma as u64;
                count += 1;
                x += step;
            }
            y += step;
        }
        let n = count.max(1) as f32;
        let entropy = hist.iter().filter(|&&c| c > 0).map(|&c| { let p = c as f32 / n; -p * p.log2() }).sum();
        Self { mean: sum as f32 / n, entropy }
    }

    pub fn is_noisy(&self) -> bool {
        self.entropy > NOISY_ENTROPY
    }
}

/// How the filters of a reading are decided: the ones switched on by hand win; otherwise, with `auto`, a noisy
/// frame gets Otsu binarization with inversion (inversion itself acts only on a dark frame) and a clean one none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterPlan {
    pub manual: Preprocess,
    pub auto: bool,
}

impl FilterPlan {
    pub fn of(r: &TextRecognitionSettings) -> Self {
        Self { manual: Preprocess::of(r), auto: r.auto_filters }
    }

    /// The filters for `img`, and whether they were chosen by the program (so the reading may be checked against the raw frame).
    pub fn resolve(&self, img: &DynamicImage) -> (Preprocess, bool) {
        if !self.manual.is_identity() || !self.auto { return (self.manual, false); }
        if FrameStats::of(img).is_noisy() {
            (Preprocess { binarize: true, auto_invert: true, ..Preprocess::default() }, true)
        } else {
            (Preprocess::default(), false)
        }
    }
}

/// The combinations tried by auto-tuning, in the order of preference: on a tie the earlier one wins.
pub const PRESETS: [(&str, Preprocess); 6] = [
    ("Без фильтров", Preprocess { auto_invert: false, contrast: 0, sharpen: false, binarize: false }),
    ("Бинаризация", Preprocess { auto_invert: false, contrast: 0, sharpen: false, binarize: true }),
    ("Бинаризация + инверсия", Preprocess { auto_invert: true, contrast: 0, sharpen: false, binarize: true }),
    ("Резкость + контраст", Preprocess { auto_invert: false, contrast: 30, sharpen: true, binarize: false }),
    ("Инверсия + резкость", Preprocess { auto_invert: true, contrast: 0, sharpen: true, binarize: false }),
    ("Инверсия + контраст", Preprocess { auto_invert: true, contrast: 30, sharpen: false, binarize: false }),
];

/// Quality of a recognition: mean confidence of the lines × the number of lines that are text and not noise.
/// An engine without geometry is judged by the lines of its text and the confidence it reports (50 if none).
pub fn score(result: &OcrResult) -> f32 {
    let valid = |text: &str| crate::text::is_meaningful(&strip_noise(text));
    let (sum, count) = if result.lines.is_empty() {
        let n = result.text.lines().filter(|l| valid(l)).count();
        (result.confidence.unwrap_or(50.0) * n as f32, n)
    } else {
        result.lines.iter().filter(|l| valid(&l.text)).fold((0.0, 0), |(sum, n), l| (sum + l.confidence, n + 1))
    };
    if count == 0 { 0.0 } else { sum / count as f32 * count as f32 }
}

/// The index of the best result. A filter must beat «no filters» (the first) by `MARGIN`, otherwise the
/// frame is left alone: a gain within the noise is not worth a changed picture.
pub fn pick_best(scores: &[f32]) -> usize {
    const MARGIN: f32 = 1.05;
    let Some((best, top)) = scores.iter().copied().enumerate().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(&a.0))) else { return 0 };
    if best != 0 && top <= scores[0] * MARGIN { 0 } else { best }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::CropRect;
    use image::{GrayImage, Luma};

    fn line(text: &str, confidence: f32) -> OcrLine {
        OcrLine { rect: CropRect::in_space(0.0, 0.0, 10.0, 10.0), text: text.into(), confidence }
    }

    fn result(lines: Vec<OcrLine>) -> OcrResult {
        OcrResult {
            text: lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"),
            confidence: Some(lines.iter().map(|l| l.confidence).sum::<f32>() / lines.len() as f32),
            lines,
            engine: "tesseract",
        }
    }

    #[test]
    fn otsu_separates_text_from_background() {
        let mut img = GrayImage::from_pixel(20, 20, Luma([30]));
        for x in 0..20 { for y in 0..5 { img.put_pixel(x, y, Luma([220])); } }
        let t = otsu(&img);
        assert!((30..220).contains(&t), "{t}");
    }

    #[test]
    fn identity_filter_returns_the_frame_untouched() {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 200, 30, 255])));
        assert!(Preprocess::default().is_identity());
        assert_eq!(Preprocess::default().apply(&img).to_rgba8(), img.to_rgba8());
    }

    #[test]
    fn bright_text_on_dark_is_inverted_and_binarized() {
        let mut img = GrayImage::from_pixel(30, 30, Luma([20]));
        for x in 5..25 { for y in 12..18 { img.put_pixel(x, y, Luma([240])); } }
        let out = Preprocess { auto_invert: true, binarize: true, ..Default::default() }.apply(&DynamicImage::ImageLuma8(img)).to_luma8();
        assert_eq!(out.get_pixel(0, 0).0[0], 255, "background became white");
        assert_eq!(out.get_pixel(10, 15).0[0], 0, "text became black");
        assert!(out.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
    }

    #[test]
    fn dark_text_on_bright_is_not_inverted() {
        let mut img = GrayImage::from_pixel(30, 30, Luma([235]));
        for x in 5..25 { for y in 12..18 { img.put_pixel(x, y, Luma([20])); } }
        let out = Preprocess { auto_invert: true, ..Default::default() }.apply(&DynamicImage::ImageLuma8(img)).to_luma8();
        assert_eq!(out.get_pixel(0, 0).0[0], 235);
    }

    #[test]
    fn contrast_moves_values_away_from_the_middle() {
        let img = DynamicImage::ImageLuma8(GrayImage::from_pixel(2, 2, Luma([160])));
        let out = Preprocess { contrast: 50, ..Default::default() }.apply(&img).to_luma8();
        assert_eq!(out.get_pixel(0, 0).0[0], 176);
    }

    #[test]
    fn stray_marks_are_trimmed_and_marks_alone_vanish() {
        assert_eq!(strip_noise("| Hello ~ world° ~~"), "Hello world");
        assert_eq!(strip_noise("snake_case"), "snake_case");
        assert_eq!(strip_noise("... — 20 / 30"), "... — 20 / 30", "punctuation of real text stays");
        assert_eq!(strip_noise("|~°"), "");
    }

    #[test]
    fn low_confidence_lines_go_when_another_line_is_sure() {
        let r = clean(result(vec![line("Welcome back", 90.0), line("xq", 12.0)]), 30, true);
        assert_eq!(r.text, "Welcome back");
        assert_eq!(r.lines.len(), 1);
        assert_eq!(r.confidence, Some(90.0));
    }

    #[test]
    fn if_every_line_is_unsure_the_result_stays_for_the_pipeline_to_reject() {
        let r = clean(result(vec![line("xq", 12.0), line("zz", 20.0)]), 30, true);
        assert_eq!(r.lines.len(), 2);
        assert_eq!(r.confidence, Some(16.0));
    }

    #[test]
    fn noise_lines_are_removed_and_the_confidence_follows() {
        let r = clean(result(vec![line("Hello there", 80.0), line("| ~", 70.0)]), 0, true);
        assert_eq!(r.text, "Hello there");
        assert_eq!(r.confidence, Some(80.0));
        let off = clean(result(vec![line("Hello there", 80.0), line("| ~", 70.0)]), 0, false);
        assert_eq!(off.lines.len(), 2, "the filter can be switched off");
    }

    #[test]
    fn engines_without_geometry_are_cleaned_by_text() {
        let r = clean(OcrResult::text_only("| Hi\n~°\nthere".into(), "paddleocr"), 30, true);
        assert_eq!(r.text, "Hi\nthere");
    }

    #[test]
    fn score_counts_valid_lines_and_ignores_noise() {
        let good = result(vec![line("Welcome back", 80.0), line("Press any key", 60.0)]);
        assert_eq!(score(&good), 140.0);
        let noisy = result(vec![line("Welcome back", 80.0), line("| ~", 90.0), line("°", 95.0)]);
        assert_eq!(score(&noisy), 80.0, "noise lines add nothing");
        assert_eq!(score(&OcrResult::default()), 0.0);
        assert_eq!(score(&OcrResult::text_only("Hello world\n~".into(), "paddleocr")), 50.0);
    }

    #[test]
    fn a_filter_must_clearly_beat_no_filter() {
        assert_eq!(pick_best(&[100.0, 103.0, 90.0]), 0, "within the margin: keep the frame as it is");
        assert_eq!(pick_best(&[100.0, 130.0, 90.0]), 1);
        assert_eq!(pick_best(&[0.0, 0.0, 0.0]), 0);
        assert_eq!(pick_best(&[50.0, 200.0, 200.0]), 1, "a tie goes to the earlier preset");
        assert_eq!(pick_best(&[]), 0);
    }

    #[test]
    fn presets_start_with_no_filters_and_are_all_different() {
        assert!(PRESETS[0].1.is_identity());
        for (i, (_, a)) in PRESETS.iter().enumerate() { for (_, b) in &PRESETS[i + 1..] { assert_ne!(a, b); } }
    }

    #[test]
    fn cjk_languages_are_recognised_in_tesseract_and_paddle_codes() {
        for cjk in ["jpn", "jpn_vert", "chi_sim", "chi_tra", "kor", "jpn+eng", "eng+kor", "japan", "korean", "ch", "zh", "ja", "ko"] {
            assert!(is_cjk_language(cjk), "{cjk}");
        }
        for other in ["eng", "rus", "deu", "eng+rus", "chr", "kan", "khm", "fra", "osd", ""] {
            assert!(!is_cjk_language(other), "{other}");
        }
    }

    #[test]
    fn the_threshold_is_raised_only_for_cjk_and_only_when_it_is_on() {
        assert_eq!(effective_min_confidence("eng", 30), 30);
        assert_eq!(effective_min_confidence("jpn", 30), CJK_MIN_CONFIDENCE);
        assert_eq!(effective_min_confidence("eng+chi_sim", 30), CJK_MIN_CONFIDENCE);
        assert_eq!(effective_min_confidence("jpn", 50), 50, "a higher choice of the user stays");
        assert_eq!(effective_min_confidence("jpn", 0), 0, "0 switches the check off");
        assert!((35..=40).contains(&CJK_MIN_CONFIDENCE));
    }

    fn noisy_frame() -> DynamicImage {
        let mut state = 12345u32;
        let img = image::RgbaImage::from_fn(200, 80, |x, y| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (70.0 + 25.0 * ((x as f32) / 9.0 + (y as f32) / 5.0).sin() + ((state >> 24) % 40) as f32 - 20.0) as u8;
            image::Rgba([v, v, v.saturating_add(20), 255])
        });
        DynamicImage::ImageRgba8(img)
    }

    fn clean_frame() -> DynamicImage {
        let mut img = image::RgbaImage::from_pixel(200, 80, image::Rgba([22, 26, 36, 255]));
        for x in 40..160 { for y in 30..44 { img.put_pixel(x, y, image::Rgba([240, 240, 240, 255])); } }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn a_flat_frame_is_clean_and_a_textured_one_is_noisy() {
        let (clean, noisy) = (FrameStats::of(&clean_frame()), FrameStats::of(&noisy_frame()));
        assert!(!clean.is_noisy() && clean.entropy < 2.0, "{clean:?}");
        assert!(noisy.is_noisy() && noisy.entropy > 5.0, "{noisy:?}");
        assert!(clean.mean < 80.0, "dark frame: {clean:?}");
        let big = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(3840, 2160, image::Rgba([10, 10, 10, 255])));
        assert!(FrameStats::of(&big).entropy < 0.01, "a large frame is sampled, not walked through");
    }

    #[test]
    fn automatic_filters_apply_to_noise_only_and_never_override_the_users_choice() {
        let auto = FilterPlan { manual: Preprocess::default(), auto: true };
        assert_eq!(auto.resolve(&clean_frame()), (Preprocess::default(), false), "clean text is read as it is");
        let (filters, chosen) = auto.resolve(&noisy_frame());
        assert!(chosen && filters.binarize && filters.auto_invert && filters.contrast == 0 && !filters.sharpen, "{filters:?}");
        let off = FilterPlan { manual: Preprocess::default(), auto: false };
        assert_eq!(off.resolve(&noisy_frame()), (Preprocess::default(), false), "switched off");
        let by_hand = Preprocess { sharpen: true, ..Preprocess::default() };
        assert_eq!(FilterPlan { manual: by_hand, auto: true }.resolve(&noisy_frame()), (by_hand, false), "the user's filters win");
    }
}
