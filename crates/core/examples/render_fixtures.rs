//! Renders real text with real fonts into the PNG fixtures of the font and scene tests, with known labels. Maintainers
//! only: the tests read the committed PNGs and `manifest.json`; this is needed to regenerate or extend them.
//!
//!   cargo run --release -p lipa-core --example render_fixtures -- fonts [--all] [OUT_DIR]
//!   cargo run --release -p lipa-core --example render_fixtures -- scenes [OUT_DIR]
//!
//! The fonts are the bundled ones (`crates/app/assets/fonts`, unpacked by a cargo build) and a few system fonts that
//! stand in for the unknown fonts of a game. Only freely licensed fonts go into the committed set; `--all` adds the
//! MS core fonts as a local, never committed hold-out check. Fonts that are not installed are skipped.
use ab_glyph::{Font, FontVec, PxScale, ScaleFont, VariableFont, point};
use image::{GrayImage, Luma, Rgb, RgbImage, codecs::png};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const BUNDLED: &str = "../app/assets/fonts";
const SYSTEM: &str = "/usr/share/fonts/TTF";
const LIBERATION: &str = "/usr/share/fonts/liberation";

const LATIN: &str = "Hello, how are you today? Quick brown fox";
const CYRILLIC: &str = "Привет, как твои дела сегодня? Быстрая лиса";
const CJK: &str = "你好，世界，这是一个测试文本，请看这里";

const PROPRIETARY: [&str; 12] = ["Arial", "Arial-bold", "Arial-italic", "Verdana", "Trebuchet", "Times", "Times-bold", "Times-italic", "Georgia", "Courier", "Courier-bold", "Impact"];
/// No Cyrillic glyphs: the renderer would draw identical .notdef boxes, which look monospaced.
const LATIN_ONLY: [&str; 3] = ["Vera", "VeraSerif", "VeraMono"];

/// name, folder, file, group (sans | serif | slab | mono | condensed | display | cjk), weight axis, real italic file.
type FontEntry = (&'static str, &'static str, &'static str, &'static str, Option<f32>, bool);
#[rustfmt::skip]
const FONTS: &[FontEntry] = &[
    ("Inter", BUNDLED, "Inter.ttf", "sans", Some(400.0), false), ("Inter-bold", BUNDLED, "Inter.ttf", "sans", Some(700.0), false),
    ("Inter-light", BUNDLED, "Inter.ttf", "sans", Some(300.0), false), ("Roboto", BUNDLED, "Roboto.ttf", "sans", Some(400.0), false),
    ("Roboto-bold", BUNDLED, "Roboto.ttf", "sans", Some(700.0), false), ("NotoSans", BUNDLED, "NotoSans.ttf", "sans", Some(400.0), false),
    ("OpenSans", BUNDLED, "OpenSans.ttf", "sans", Some(400.0), false), ("Montserrat", BUNDLED, "Montserrat.ttf", "sans", Some(400.0), false),
    ("FiraSans", BUNDLED, "FiraSans-Regular.ttf", "sans", None, false), ("FiraSans-bold", BUNDLED, "FiraSans-Bold.ttf", "sans", None, false),
    ("NotoSerif", BUNDLED, "NotoSerif.ttf", "serif", Some(400.0), false), ("NotoSerif-bold", BUNDLED, "NotoSerif.ttf", "serif", Some(700.0), false),
    ("SourceSerif4", BUNDLED, "SourceSerif4.ttf", "serif", Some(400.0), false), ("Literata", BUNDLED, "Literata.ttf", "serif", Some(400.0), false),
    ("Lora", BUNDLED, "Lora.ttf", "serif", Some(400.0), false), ("EBGaramond", BUNDLED, "EBGaramond.ttf", "serif", Some(400.0), false),
    ("PTSerif", BUNDLED, "PTSerif-Regular.ttf", "serif", None, false), ("PTSerif-bold", BUNDLED, "PTSerif-Bold.ttf", "serif", None, false),
    ("RobotoSlab", BUNDLED, "RobotoSlab.ttf", "slab", Some(400.0), false), ("RobotoSlab-bold", BUNDLED, "RobotoSlab.ttf", "slab", Some(700.0), false),
    ("Bitter", BUNDLED, "Bitter.ttf", "slab", Some(400.0), false), ("JetBrainsMono", BUNDLED, "JetBrainsMono.ttf", "mono", Some(400.0), false),
    ("FiraCode", BUNDLED, "FiraCode.ttf", "mono", Some(400.0), false), ("SourceCodePro", BUNDLED, "SourceCodePro.ttf", "mono", Some(400.0), false),
    ("RobotoCondensed", BUNDLED, "RobotoCondensed.ttf", "condensed", Some(400.0), false),
    ("Arial", SYSTEM, "Arial.TTF", "sans", None, false), ("Arial-bold", SYSTEM, "Arialbd.TTF", "sans", None, false),
    ("Arial-italic", SYSTEM, "Ariali.TTF", "sans", None, true), ("Verdana", SYSTEM, "Verdana.TTF", "sans", None, false),
    ("Trebuchet", SYSTEM, "trebuc.ttf", "sans", None, false), ("LiberationSans", LIBERATION, "LiberationSans-Regular.ttf", "sans", None, false),
    ("DejaVuSans", SYSTEM, "DejaVuSans.ttf", "sans", None, false), ("DejaVuSans-bold", SYSTEM, "DejaVuSans-Bold.ttf", "sans", None, false),
    ("Times", SYSTEM, "Times.TTF", "serif", None, false), ("Times-bold", SYSTEM, "Timesbd.TTF", "serif", None, false),
    ("Times-italic", SYSTEM, "Timesi.TTF", "serif", None, true), ("Georgia", SYSTEM, "Georgia.TTF", "serif", None, false),
    ("LiberationSerif", LIBERATION, "LiberationSerif-Regular.ttf", "serif", None, false), ("DejaVuSerif", SYSTEM, "DejaVuSerif.ttf", "serif", None, false),
    ("Courier", SYSTEM, "cour.ttf", "mono", None, false), ("Courier-bold", SYSTEM, "courbd.ttf", "mono", None, false),
    ("LiberationMono", LIBERATION, "LiberationMono-Regular.ttf", "mono", None, false), ("DejaVuSansMono", SYSTEM, "DejaVuSansMono.ttf", "mono", None, false),
    ("Hack", SYSTEM, "Hack-Regular.ttf", "mono", None, false), ("Impact", SYSTEM, "Impact.TTF", "display", None, false),
    ("Vera", SYSTEM, "Vera.ttf", "sans", None, false), ("VeraSerif", SYSTEM, "VeraSe.ttf", "serif", None, false), ("VeraMono", SYSTEM, "VeraMono.ttf", "mono", None, false),
    ("FantasqueMono", SYSTEM, "FantasqueSansMNerdFontMono-Regular.ttf", "mono", None, false), ("Meslo", SYSTEM, "MesloLGMNerdFontMono-Regular.ttf", "mono", None, false),
    ("LiberationSans-bold", LIBERATION, "LiberationSans-Bold.ttf", "sans", None, false), ("LiberationSerif-bold", LIBERATION, "LiberationSerif-Bold.ttf", "serif", None, false),
    ("LiberationSerif-italic", LIBERATION, "LiberationSerif-Italic.ttf", "serif", None, true), ("LiberationSans-italic", LIBERATION, "LiberationSans-Italic.ttf", "sans", None, true),
    ("FiraSansCondensed", SYSTEM, "FiraSansCondensed-Regular.ttf", "condensed", None, false), ("FiraSansCompressed", SYSTEM, "FiraSansCompressed-Regular.ttf", "condensed", None, false),
    ("OpenSansCondensed", SYSTEM, "OpenSans-CondensedRegular.ttf", "condensed", None, false), ("DejaVuSansCondensed", SYSTEM, "DejaVuSansCondensed.ttf", "sans", None, false),
    ("FiraSans-light", SYSTEM, "FiraSans-Light.ttf", "sans", None, false), ("FiraSans-heavy", SYSTEM, "FiraSans-Heavy.ttf", "sans", None, false),
    ("OpenSans-light", SYSTEM, "OpenSans-Light.ttf", "sans", None, false), ("NotoSansCJK", BUNDLED, "NotoSansCJK-VF.otf", "cjk", Some(400.0), false),
];

/// A font at a size in pixels (the em square), optionally set to a weight on a variable font.
struct Face {
    font: FontVec,
    scale: PxScale,
}

impl Face {
    fn load(path: &Path, size: f32, weight: Option<f32>) -> Option<Self> {
        let mut font = FontVec::try_from_vec(fs::read(path).ok()?).ok()?;
        if let Some(weight) = weight {
            font.set_variation(b"wght", weight);
        }
        let scale = PxScale::from(size * font.height_unscaled() / font.units_per_em()?);
        Some(Self { font, scale })
    }

    fn width(&self, text: &str) -> f32 {
        let scaled = self.font.as_scaled(self.scale);
        let mut previous = None;
        text.chars().map(|c| {
            let id = scaled.glyph_id(c);
            let kern = previous.map_or(0.0, |p| scaled.kern(p, id));
            previous = Some(id);
            kern + scaled.h_advance(id)
        }).sum()
    }

    /// Calls `plot(x, y, coverage)` for the pixels of `text` with its baseline at `baseline` starting at `x`.
    fn glyphs(&self, text: &str, x: f32, baseline: f32, mut plot: impl FnMut(i64, i64, f32)) {
        let scaled = self.font.as_scaled(self.scale);
        let (mut caret, mut previous) = (x, None);
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                caret += scaled.kern(p, id);
            }
            if let Some(outline) = self.font.outline_glyph(id.with_scale_and_position(self.scale, point(caret, baseline))) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| plot(bounds.min.x as i64 + gx as i64, bounds.min.y as i64 + gy as i64, coverage));
            }
            caret += scaled.h_advance(id);
            previous = Some(id);
        }
    }
}

fn save_png(img: &image::DynamicImage, path: &Path) -> Result<()> {
    let mut out = Vec::new();
    img.write_with_encoder(png::PngEncoder::new_with_quality(&mut out, png::CompressionType::Best, png::FilterType::Adaptive))?;
    fs::write(path, out)?;
    Ok(())
}

/// A JSON object that keeps the order of its entries.
struct Ordered<T>(Vec<(&'static str, T)>);
impl<T: Serialize> Serialize for Ordered<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

fn write_manifest(path: &Path, manifest: &impl Serialize) -> Result<()> {
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b" "));
    manifest.serialize(&mut serializer)?;
    fs::write(path, out)?;
    Ok(())
}

// --- font fixtures ---

#[derive(Serialize)]
struct FontFixture {
    file: String,
    font: String,
    group: String,
    weight: u32,
    italic: bool,
    script: String,
    light_on_dark: bool,
}

/// A small greyscale frame with a soft vertical gradient (game backgrounds are never flat): the files stay a few KiB.
fn font_frame(face: &Face, text: &str, light_on_dark: bool, shear: f32) -> GrayImage {
    let (w, h) = (660u32, 76u32);
    let base: i32 = if light_on_dark { 34 } else { 228 };
    let ink: f32 = if light_on_dark { 245.0 } else { 24.0 };
    let mut img = GrayImage::from_fn(w, h, |_, y| Luma([(base + (10.0 * (y as f32 / h as f32)) as i32 - 5).clamp(0, 255) as u8]));
    face.glyphs(text, 14.0, 54.0, |x, y, coverage| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let p = img.get_pixel_mut(x as u32, y as u32);
            p.0[0] = (p.0[0] as f32 * (1.0 - coverage) + ink * coverage).round() as u8;
        }
    });
    if shear == 0.0 {
        return img;
    }
    // A synthetic slant: output (x, y) takes the input at (x + shear·y − shear·54, y), the baseline stays in place.
    let source = img.clone();
    GrayImage::from_fn(w, h, |x, y| {
        let sx = x as f32 + shear * y as f32 - shear * 54.0;
        if sx < 0.0 || sx > (w - 1) as f32 {
            return Luma([base as u8]);
        }
        let (x0, f) = (sx.floor() as u32, sx.fract());
        let a = source.get_pixel(x0, y).0[0] as f32;
        let b = source.get_pixel((x0 + 1).min(w - 1), y).0[0] as f32;
        Luma([(a * (1.0 - f) + b * f).round() as u8])
    })
}

fn fonts(all: bool, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let (mut manifest, mut missing, mut total) = (Vec::new(), Vec::new(), 0u64);
    for &(name, folder, file, group, axis, real_italic) in FONTS {
        if PROPRIETARY.contains(&name) && !all {
            continue;
        }
        let path = if folder == BUNDLED { Path::new(env!("CARGO_MANIFEST_DIR")).join(folder).join(file) } else { Path::new(folder).join(file) };
        let Some(face) = Face::load(&path, 30.0, axis) else {
            missing.push(path);
            continue;
        };
        let mut weight = axis.unwrap_or(400.0) as u32;
        if name.ends_with("-bold") { weight = 700; }
        if name.ends_with("-light") { weight = 300; }
        if name.ends_with("-italic") && !name.contains("bold") { weight = 400; }
        if name == "Impact" { weight = 700; }
        if name == "FiraSans-heavy" { weight = 800; }
        let mut texts = vec![("latin", if group == "cjk" { CJK } else { LATIN })];
        if group != "cjk" && !LATIN_ONLY.contains(&name) {
            texts.push(("cyr", CYRILLIC));
        }
        for (script, text) in texts {
            for dark in if script == "latin" { vec![true, false] } else { vec![true] } {
                // Synthetic slant on the regular files too: the game may use italics.
                let mut variants = vec![("", 0.0, real_italic)];
                if script == "latin" && dark && !real_italic && group != "cjk" {
                    variants.push(("-oblique", 0.21, true));
                }
                for (suffix, shear, italic) in variants {
                    let fixture = format!("{name}{suffix}-{script}-{}.png", if dark { "dark" } else { "light" });
                    save_png(&image::DynamicImage::ImageLuma8(font_frame(&face, text, dark, shear)), &out.join(&fixture))?;
                    total += fs::metadata(out.join(&fixture))?.len();
                    manifest.push(FontFixture { file: fixture, font: name.into(), group: group.into(), weight, italic, script: script.into(), light_on_dark: dark });
                }
            }
        }
    }
    write_manifest(&out.join("manifest.json"), &manifest)?;
    println!("{} fixtures, {} KiB in {}", manifest.len(), total / 1024, out.display());
    for path in missing {
        println!("missing font (skipped): {}", path.display());
    }
    Ok(())
}

// --- scene fixtures ---

const W: u32 = 960;
const H: u32 = 300;

fn blend(img: &mut RgbImage, x: i64, y: i64, color: [u8; 3], alpha: f32) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        let p = img.get_pixel_mut(x as u32, y as u32);
        for (channel, ink) in p.0.iter_mut().zip(color) {
            *channel = (*channel as f32 * (1.0 - alpha) + ink as f32 * alpha).round() as u8;
        }
    }
}

fn sky(top: [f32; 3], bottom: [f32; 3]) -> RgbImage {
    RgbImage::from_fn(W, H, |x, y| {
        let t = y as f32 / H as f32;
        // Soft hills so the background is not a flat gradient.
        let hill = 1.0 + 0.06 * (x as f32 / 70.0 + y as f32 / 90.0).sin();
        Rgb(std::array::from_fn(|i| ((top[i] * (1.0 - t) + bottom[i] * t) * hill).clamp(0.0, 255.0) as u8))
    })
}

/// Whether the pixel centre is inside the rectangle `(x0, y0, x1, y1)` (inclusive edges) with rounded corners.
fn in_rounded(x: f32, y: f32, r: (f32, f32, f32, f32), radius: f32) -> bool {
    let (x0, y0, x1, y1) = r;
    if x < x0 || x > x1 + 1.0 || y < y0 || y > y1 + 1.0 {
        return false;
    }
    let (cx, cy) = (x.clamp(x0 + radius, x1 + 1.0 - radius), y.clamp(y0 + radius, y1 + 1.0 - radius));
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

fn rounded_rect(img: &mut RgbImage, r: (u32, u32, u32, u32), radius: f32, fill: Option<([u8; 3], f32)>, border: Option<([u8; 3], f32)>) {
    let outer = (r.0 as f32, r.1 as f32, r.2 as f32, r.3 as f32);
    let inner = (outer.0 + 2.0, outer.1 + 2.0, outer.2 - 2.0, outer.3 - 2.0);
    for y in r.1..=r.3 {
        for x in r.0..=r.2 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            if !in_rounded(px, py, outer, radius) {
                continue;
            }
            match (border, in_rounded(px, py, inner, (radius - 2.0).max(0.0))) {
                (Some((color, alpha)), false) => blend(img, x as i64, y as i64, color, alpha),
                _ => if let Some((color, alpha)) = fill { blend(img, x as i64, y as i64, color, alpha) },
            }
        }
    }
}

/// Text with its baseline at `y`; `x` is the left edge, or the middle with `centered`. A stroke is drawn first.
struct Line<'a> {
    text: &'a str,
    x: f32,
    baseline: f32,
    centered: bool,
    color: [u8; 3],
    stroke: Option<(i64, [u8; 3])>,
}

fn text(img: &mut RgbImage, face: &Face, line: Line) {
    let Line { text: s, x, baseline: y, centered, color, stroke } = line;
    let x = if centered { x - face.width(s) / 2.0 } else { x };
    if let Some((width, stroke_color)) = stroke {
        for dy in -width..=width {
            for dx in -width..=width {
                if dx * dx + dy * dy <= width * width {
                    face.glyphs(s, x + dx as f32, y + dy as f32, |px, py, c| blend(img, px, py, stroke_color, c));
                }
            }
        }
    }
    face.glyphs(s, x, y, |px, py, c| blend(img, px, py, color, c));
}

fn bundled(file: &str, size: f32, weight: Option<f32>) -> Result<Face> {
    Face::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(BUNDLED).join(file), size, weight).ok_or_else(|| format!("font {file} is not available (build the application once to unpack the bundled fonts)").into())
}

fn dialogue() -> Result<RgbImage> {
    let mut img = sky([92.0, 118.0, 150.0], [42.0, 52.0, 40.0]);
    rounded_rect(&mut img, (40, 40, W - 40, H - 30), 14.0, Some(([18, 20, 32], 215.0 / 255.0)), Some(([190, 160, 90], 1.0)));
    text(&mut img, &bundled("Lora.ttf", 28.0, Some(700.0))?, Line { text: "Elder Maren", x: 70.0, baseline: 82.0, centered: false, color: [240, 196, 92], stroke: None });
    let body = bundled("PTSerif-Regular.ttf", 26.0, None)?;
    for (i, line) in ["The road north is closed. Bandits have taken", "the old bridge, and no caravan has passed", "through the valley in days. Will you help us?"].iter().enumerate() {
        text(&mut img, &body, Line { text: line, x: 70.0, baseline: 128.0 + i as f32 * 34.0, centered: false, color: [238, 236, 228], stroke: None });
    }
    let label = bundled("Inter.ttf", 22.0, Some(600.0))?;
    for (x, name) in [(70u32, "Accept"), (250, "Decline")] {
        rounded_rect(&mut img, (x, 232, x + 150, 272), 8.0, Some(([52, 60, 84], 1.0)), Some(([150, 160, 190], 1.0)));
        text(&mut img, &label, Line { text: name, x: x as f32 + 75.0, baseline: 260.0, centered: true, color: [236, 240, 250], stroke: None });
    }
    Ok(img)
}

fn subtitles() -> Result<RgbImage> {
    let mut img = sky([196.0, 214.0, 232.0], [120.0, 140.0, 96.0]);
    let face = bundled("Roboto.ttf", 34.0, Some(500.0))?;
    for (i, line) in ["I never thought we would", "make it out of the canyon alive."].iter().enumerate() {
        text(&mut img, &face, Line { text: line, x: W as f32 / 2.0, baseline: 226.0 + i as f32 * 42.0, centered: true, color: [255, 255, 255], stroke: Some((3, [0, 0, 0])) });
    }
    Ok(img)
}

fn menu(selected: usize) -> Result<RgbImage> {
    let mut img = sky([22.0, 26.0, 44.0], [10.0, 12.0, 22.0]);
    rounded_rect(&mut img, (60, 30, 420, H - 30), 6.0, Some(([0, 0, 0], 120.0 / 255.0)), None);
    let face = bundled("JetBrainsMono.ttf", 26.0, Some(400.0))?;
    for (i, label) in ["> New Game", "  Load Game", "  Options", "  Quit"].iter().enumerate() {
        text(&mut img, &face, Line { text: label, x: 100.0, baseline: 84.0 + i as f32 * 56.0, centered: false, color: if i == selected { [120, 230, 170] } else { [200, 205, 215] }, stroke: None });
    }
    Ok(img)
}

#[derive(Serialize)]
struct SceneExpectation {
    fields: u32,
    multiline: u32,
    kinds: Vec<&'static str>,
}

fn scenes(out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    // The number of fields, which of them are multi-line, and what the font should look like.
    let list: Vec<(&str, RgbImage, SceneExpectation)> = vec![
        ("dialogue", dialogue()?, SceneExpectation { fields: 4, multiline: 1, kinds: vec!["serif", "serif", "sans", "sans"] }),
        ("subtitles", subtitles()?, SceneExpectation { fields: 1, multiline: 1, kinds: vec!["sans"] }),
        ("menu", menu(0)?, SceneExpectation { fields: 4, multiline: 0, kinds: vec!["mono"; 4] }),
        ("menu_selected_quit", menu(3)?, SceneExpectation { fields: 4, multiline: 0, kinds: vec!["mono"; 4] }),
    ];
    let mut manifest = Ordered(Vec::new());
    let mut total = 0;
    for (name, img, expect) in list {
        let path = out.join(format!("{name}.png"));
        save_png(&image::DynamicImage::ImageRgb8(img), &path)?;
        total += fs::metadata(&path)?.len();
        manifest.0.push((name, expect));
    }
    write_manifest(&out.join("manifest.json"), &manifest)?;
    println!("{} scenes, {} KiB in {}", manifest.0.len(), total / 1024, out.display());
    Ok(())
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let all = args.iter().position(|a| a == "--all").map(|i| args.remove(i)).is_some();
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = |default: &str| args.get(1).map_or_else(|| tests.join(default), PathBuf::from);
    match args.first().map(String::as_str) {
        Some("fonts") => fonts(all, &out("fonts")),
        Some("scenes") => scenes(&out("scenes")),
        _ => Err("usage: render_fixtures fonts [--all] [OUT_DIR] | scenes [OUT_DIR]".into()),
    }
}
