//! Diagnostic: writes the global ink mask of an image as black/white PNG, with the detector's blocks outlined.
use lipa_core::layout::block_detector::BlockDetector;
fn main() {
    let mut a = std::env::args().skip(1);
    let (input, out) = (a.next().unwrap(), a.next().unwrap());
    let img = image::open(&input).unwrap().to_rgba8();
    let mask = BlockDetector::ink_mask(&img);
    eprintln!("ink_is_dark={}", mask.ink_is_dark);
    let mut o = image::RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() { for x in 0..img.width() {
        let v = if mask.at(x as usize, y as usize) { 255 } else { 0 };
        o.put_pixel(x, y, image::Rgba([v, v, v, 255]));
    } }
    o.save(out).unwrap();
}
