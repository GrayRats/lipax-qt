//! Check the actual shipped files, including on a clean checkout before the Qt build unpacks them.
use ab_glyph::{Font, FontVec, ScaleFont, VariableFont};
use lipa_core::layout::font_database::BUNDLED;
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};

fn unpack(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/assets/fonts").join(format!("{name}.xz"));
    let output = Command::new("xz").arg("-dc").arg(path)
        .output().expect("xz is required to unpack bundled fonts");
    assert!(output.status.success(), "cannot unpack {name}");
    output.stdout
}

#[test]
fn imported_fonts_match_checksums_and_declared_glyph_coverage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("packaging/kizurium-fonts.json")).unwrap(),
    ).unwrap();
    for record in manifest["fonts"].as_array().unwrap() {
        let Some(family) = record["family"].as_str() else { continue };
        let name = record["file"].as_str().unwrap();
        let entry = BUNDLED.iter().find(|f| f.family == family).unwrap();
        assert!(entry.files.contains(&name), "{name} missing from registry");
        let bytes = unpack(name);
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), record["sha256"].as_str().unwrap(), "{name}");
        let font = FontVec::try_from_vec(bytes).unwrap();
        for (lang, alphabet) in [
            ("en", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"),
            ("ru", "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя"),
            ("uk", "ҐґЄєІіЇї"),
            ("el", "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩαβγδεζηθικλμνξοπρστυφχψω"),
        ] {
            if entry.langs.contains(&lang) {
                assert!(alphabet.chars().all(|c| font.glyph_id(c).0 != 0), "{name} lacks declared {lang} glyphs");
            }
        }
        assert!(root.join("crates/app/assets/fonts").join(record["license"].as_str().unwrap()).is_file());
    }
}

#[test]
fn imported_pixel_narrow_and_italic_faces_are_recognized_from_pixels() {
    use image::{Rgba, RgbaImage};
    use lipa_core::layout::{FontCategory, Script, TextBlockType};
    use lipa_core::layout::block_detector::BlockDetector;
    use lipa_core::layout::font_classifier::FontClassifier;
    use lipa_core::layout::font_database::InstalledFontDatabase;
    use lipa_core::layout::font_matcher::FontMatcher;

    for (file, category, italic) in [
        ("PressStart2P.ttf", FontCategory::Monospace, false),
        ("Oswald.ttf", FontCategory::Condensed, false),
        ("PTSerif-Italic.ttf", FontCategory::Serif, true),
    ] {
        for dark in [false, true] {
            let mut font = FontVec::try_from_vec(unpack(file)).unwrap();
            font.set_variation(b"wght", 400.0);
            let scaled = font.as_scaled(40.0 * font.height_unscaled() / font.units_per_em().unwrap());
            let (background, ink) = if dark { (20u8, 240u8) } else { (240, 20) };
            let mut img = RgbaImage::from_pixel(1800, 120, Rgba([background, background, background, 255]));
            let mut x = 20.0;
            let mut previous = None;
            for c in "Translation settings: Quick brown fox".chars() {
                let id = scaled.glyph_id(c);
                if let Some(prev) = previous { x += scaled.kern(prev, id); }
                let glyph = id.with_scale_and_position(scaled.scale(), ab_glyph::point(x, 75.0));
                if let Some(outline) = font.outline_glyph(glyph) {
                    let bounds = outline.px_bounds();
                    outline.draw(|gx, gy, coverage| {
                        let value = (background as f32 + coverage * (ink as f32 - background as f32)).round() as u8;
                        img.put_pixel(bounds.min.x as u32 + gx, bounds.min.y as u32 + gy, Rgba([value, value, value, 255]));
                    });
                }
                x += scaled.h_advance(id);
                previous = Some(id);
            }
            let mask = BlockDetector::ink_mask(&img);
            let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
            let block = blocks.iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area())).unwrap();
            let analysis = FontClassifier.classify(&img, &mask, block);
            assert_eq!(analysis.category, category, "{file}, dark={dark}: {analysis:?}");
            assert_eq!(analysis.italic, italic, "{file}, dark={dark}");
            let selected = FontMatcher.select_font(&analysis, Script::Cyrillic, TextBlockType::Unknown,
                &InstalledFontDatabase::bundled(), &Default::default());
            if file == "PressStart2P.ttf" { assert_eq!(selected.family, "Press Start 2P"); }
            if italic { assert!(selected.italic_available, "{selected:?}"); }
        }
    }
}
