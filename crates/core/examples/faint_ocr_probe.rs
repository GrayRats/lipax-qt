//! Diagnostic: reads faint crops of an image with RapidOCR in several variants (raw, contrast-stretched, enlarged).
//!   faint_ocr_probe IMAGE X,Y,W,H [X,Y,W,H ...]
use image::{DynamicImage, imageops::FilterType};
use lipa_core::{ocr::{Ocr, rapid::RapidOcr, rapid_models}, settings::Settings};

fn stretch(img: &DynamicImage) -> DynamicImage {
    let mut g = img.to_luma8();
    let mut v: Vec<u8> = g.pixels().map(|p| p.0[0]).collect();
    v.sort_unstable();
    let (lo, hi) = (v[v.len() / 50] as f32, v[v.len() - 1 - v.len() / 50] as f32);
    for p in g.pixels_mut() { p.0[0] = ((p.0[0] as f32 - lo) / (hi - lo).max(8.0) * 255.0).clamp(0.0, 255.0) as u8; }
    DynamicImage::ImageLuma8(g)
}

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let img = image::open(a.next().unwrap()).unwrap();
    let mut s = Settings::default();
    s.recognition.engine = "rapidocr".into();
    s.recognition.language = "eng".into();
    s.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(rapid_models::cache_root(), None);
    for spec in a {
        let n: Vec<u32> = spec.split(',').map(|v| v.parse().unwrap()).collect();
        let pad = 8;
        let (x, y) = (n[0].saturating_sub(pad), n[1].saturating_sub(pad));
        let crop = img.crop_imm(x, y, (n[2] + 2 * pad).min(img.width() - x), (n[3] + 2 * pad).min(img.height() - y));
        let stretched = stretch(&crop);
        let big = stretched.resize_exact(stretched.width() * 3, stretched.height() * 3, FilterType::Lanczos3);
        let big_raw = crop.resize_exact(crop.width() * 3, crop.height() * 3, FilterType::Lanczos3);
        for (name, v) in [("raw", &crop), ("stretch", &stretched), ("stretch x3", &big), ("raw x3", &big_raw)] {
            let r = rapid.recognize_detailed(v, &s).await;
            println!("{spec:>16} {name:<11} {:?}", r.map(|r| (r.text.replace('\n', " | "), r.confidence)));
        }
    }
}
