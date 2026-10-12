//! Writes "before | patch | composite" for each detected block of an image (visual check of text replacement).
use lipa_core::layout::{background::*, block_detector::BlockDetector, TextBlockType};
use lipa_core::settings::{InplaceBackgroundMode, InplaceSettings};

fn main() {
    let mut args = std::env::args().skip(1);
    let (input, out) = (args.next().expect("input image"), args.next().expect("output png"));
    let img = image::open(&input).expect("image").to_rgba8();
    let mask = BlockDetector::ink_mask(&img);
    let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
    let s = InplaceSettings { background_mode: InplaceBackgroundMode::TextReplacement, ..Default::default() };
    let mut composite = img.clone();
    // Only the lower part: the detector also reports artwork, which OCR would reject.
    let t = std::time::Instant::now();
    for b in blocks.iter().filter(|b| b.rect.y > img.height() as f32 * 0.8) {
        let lh = b.line_height();
        let a = BackgroundAnalyzer.analyze(&img, &mask, &b.rect, BackgroundAnalyzer::margin(lh));
        let r = BackgroundInpainter.render(&img, &mask, &b.rect, lh, a, &s, TextBlockType::Unknown);
        let patch = r.image.expect("patch");
        let (x0, y0, _, _) = r.rect.pixels(img.width(), img.height());
        for (x, y, p) in patch.enumerate_pixels() {
            let a = p.0[3] as f32 / 255.0;
            let d = composite.get_pixel_mut(x0 + x, y0 + y);
            for c in 0..3 { d.0[c] = (p.0[c] as f32 * a + d.0[c] as f32 * (1.0 - a)) as u8; }
        }
    }
    eprintln!("{} blocks, patches built in {:?}", blocks.len(), t.elapsed());
    let mut sheet = image::RgbaImage::new(img.width(), img.height() * 2);
    image::imageops::replace(&mut sheet, &img, 0, 0);
    image::imageops::replace(&mut sheet, &composite, 0, img.height() as i64);
    sheet.save(out).unwrap();
}
