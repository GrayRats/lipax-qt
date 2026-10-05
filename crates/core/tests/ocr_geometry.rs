//! The chain "detected block -> crop -> real Tesseract -> lines in frame coordinates -> layout"
//! on a rendered two-line text. Skipped where Tesseract, Python/PIL or the font are missing.

use image::DynamicImage;
use lipa_core::layout::engine::InplaceEngine;
use lipa_core::layout::font_database::InstalledFontDatabase;
use lipa_core::ocr::Tesseract;
use lipa_core::settings::{Settings, TranslationDisplayMode};
use lipa_core::tesseract::TesseractManager;
use std::path::Path;
use std::time::Instant;

fn render(png: &Path) -> bool {
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts/Inter.ttf");
    let script = "import sys\nfrom PIL import Image, ImageDraw, ImageFont\nim = Image.new('RGB', (900, 300), (22, 26, 36))\nd = ImageDraw.Draw(im)\nf = ImageFont.truetype(sys.argv[1], 34)\nd.text((120, 90), 'Where are you going,', font=f, fill=(240, 240, 240))\nd.text((120, 140), 'stranger? Follow me.', font=f, fill=(240, 240, 240))\nim.save(sys.argv[2])";
    std::process::Command::new("python3").args(["-c", script]).arg(&font).arg(png).status().is_ok_and(|s| s.success())
}

#[tokio::test]
async fn real_ocr_lines_land_on_the_detected_block_in_frame_coordinates() {
    let png = std::env::temp_dir().join(format!("lipax-geometry-{}.png", std::process::id()));
    if !render(&png) || !TesseractManager::system().detect().installed {
        eprintln!("skipped: no Tesseract, PIL or font");
        return;
    }
    let frame = image::open(&png).unwrap();
    let _ = std::fs::remove_file(&png);
    let settings = { let mut value = Settings::default(); value.translation.target_language = "ru".into(); value.display_mode = TranslationDisplayMode::Inplace; value };
    let mut engine = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let jobs = engine.begin(DynamicImage::from(frame.to_rgba8()), &settings, Instant::now(), false);
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
