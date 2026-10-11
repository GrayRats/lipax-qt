//! Diagnostic: prints the detector's blocks and report for a PNG/JPEG (no OCR, no claims of accuracy).
use lipa_core::layout::block_detector::BlockDetector;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let limit = args.iter().find_map(|a| a.strip_prefix("--max=")).and_then(|v| v.parse().ok());
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let image = image::open(path).expect("image").into_rgba8();
        let mut mask = BlockDetector::ink_mask(&image);
        let mut d = BlockDetector::default();
        if let Some(l) = limit { d.max_blocks = l; }
        let (blocks, report) = if args.iter().any(|a| a == "--local") { d.detect_with_local(&image, &mut mask) } else { d.detect(&image, &mask) };
        println!("{path} {}x{} ink_dark={} {report:?}", image.width(), image.height(), mask.ink_is_dark);
        for b in &blocks {
            println!("  {:?} {:>4.0},{:>4.0} {:>4.0}x{:<4.0} lines={} h={:.0}", b.block_type, b.rect.x, b.rect.y, b.rect.w, b.rect.h, b.lines.len(), b.line_height());
        }
    }
}
