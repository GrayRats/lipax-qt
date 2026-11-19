//! Diagnostic: prints the font features and the classifier's decision for every detected block of an image.
//!   font_features_dump <image>... [--min-width=<px>]
use lipa_core::layout::{block_detector::BlockDetector, font_classifier::FontClassifier};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let min_w: f32 = args.iter().find_map(|a| a.strip_prefix("--min-width=")).and_then(|v| v.parse().ok()).unwrap_or(150.0);
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let image = image::open(path).expect("image").into_rgba8();
        let mut mask = BlockDetector::ink_mask(&image);
        let (blocks, _) = BlockDetector::default().detect_with_local(&image, &mut mask);
        println!("{path}");
        for b in blocks.iter().filter(|b| b.rect.w >= min_w) {
            let f = FontClassifier.features(&image, &mask, b);
            let a = FontClassifier.classify(&image, &mask, b);
            println!("  {:>4.0},{:>4.0} {:>4.0}x{:<4.0} glyphs={:>3} cap={:>4.1} x_ratio={:.2} width_ratio={:.3} stem={:.2} contrast={:.2} foot={:.2} pitch_err={:.2} -> {:?} ({:.2})",
                b.rect.x, b.rect.y, b.rect.w, b.rect.h, f.glyphs, f.cap_h, f.x_ratio, f.width_ratio, f.stem_w, f.contrast, f.foot, f.pitch_error, a.category, a.confidence);
        }
    }
}
