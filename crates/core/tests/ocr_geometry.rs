//! The chain "detected block -> crop -> real Tesseract -> lines in frame coordinates -> layout"
//! on a rendered two-line text. Skipped where Tesseract or the font is missing.

mod common;

use image::DynamicImage;
use lipa_core::layout::engine::InplaceEngine;
use lipa_core::layout::font_database::InstalledFontDatabase;
use lipa_core::ocr::Tesseract;
use lipa_core::settings::{Settings, TranslationDisplayMode};
use lipa_core::tesseract::TesseractManager;
use std::time::Instant;

fn render() -> Option<image::RgbImage> {
    let text = common::Text::load("Inter.ttf", 34)?;
    let mut im = image::RgbImage::from_pixel(900, 300, image::Rgb([22, 26, 36]));
    text.draw(&mut im, 120.0, 90.0, "Where are you going,", [240, 240, 240]);
    text.draw(&mut im, 120.0, 140.0, "stranger? Follow me.", [240, 240, 240]);
    Some(im)
}

#[tokio::test]
async fn real_ocr_lines_land_on_the_detected_block_in_frame_coordinates() {
    let Some(frame) = render().filter(|_| TesseractManager::system().detect().installed) else {
        eprintln!("skipped: no Tesseract or font");
        return;
    };
    let settings = { let mut value = Settings::default(); value.translation.target_language = "ru".into(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let mut engine = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = engine.begin(DynamicImage::from(DynamicImage::ImageRgb8(frame).to_rgba8()), &settings, Instant::now(), false);
    assert_eq!(jobs.len(), 1, "one block of dialogue");
    let job = &jobs[0];
    // The engine reads the crop; its lines are in crop pixels and move into the frame by the crop origin.
    let result = Tesseract.run_detailed(&job.image, "eng").await.unwrap();
    assert!(result.text.contains("Where are you going"), "{:?}", result.text);
    assert_eq!(result.lines.len(), 2, "{:?}", result.lines);
    let in_frame: Vec<_> = result.lines.iter().map(|l| l.rect.in_frame(job.origin)).collect();
    let (first, block) = (&in_frame[0], &job.rect);
    assert!((first.x - block.x).abs() < 6.0 && (first.y - block.y).abs() < 6.0, "the first line starts where the block does: {first:?} vs {block:?}");
    assert!(in_frame[1].y > first.y + 20.0, "the second line is below the first");

    engine.observe_ocr_lines(job.id, &in_frame, &settings);
    engine.complete(job.id, Some((result.text.clone(), "Куда ты идёшь, незнакомец? Следуй за мной.".into())), &settings);
    let block = engine.finish(&settings).expect("the field").blocks.remove(0);
    assert_eq!(block.style.source_lines, 2);
    assert!(block.lines_note.contains("согласны") || block.lines_note.contains("по OCR"), "{}", block.lines_note);
    assert!(!block.lines_note.contains("не совпала"), "real geometry must lie on the real block: {}", block.lines_note);
}
