//! Движок режима «перевод поверх оригинала» (`translation_display = "inplace"`).
//!
//! Кадр области — набор устойчивых текстовых полей. Для каждого поля:
//!
//! ```text
//! BlockDetector → TextBlockTracker (стабильный id) → OCR → FontClassifier + FontMatcher
//! (один раз, шрифт блокируется за полем) → перевод → TypographyEstimator →
//! BackgroundAnalyzer/BackgroundInpainter → fit (метрики Qt в приложении) → QML
//! ```
//!
//! Модули независимы: фон не выбирает шрифт, типографика не строит фон, детектор не знает об OCR.
//! Оркестратор — [`engine::InplaceEngine`]; он пересчитывает только изменившиеся стадии.

pub mod background;
pub mod block_detector;
pub mod engine;
pub mod fit;
pub mod font_classifier;
pub mod font_database;
pub mod font_matcher;
pub mod tracker;
pub mod typography;

use serde::{Deserialize, Serialize};

/// Прямоугольник в пикселях кадра области.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn intersection(&self, o: &Rect) -> f32 {
        let w = self.right().min(o.right()) - self.x.max(o.x);
        let h = self.bottom().min(o.bottom()) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }
    pub fn iou(&self, o: &Rect) -> f32 {
        let i = self.intersection(o);
        let u = self.area() + o.area() - i;
        if u > 0.0 { i / u } else { 0.0 }
    }
    /// Расширить на `pad` со всех сторон, не выходя за `w`×`h`.
    pub fn expand(&self, pad: f32, w: f32, h: f32) -> Rect {
        let (x0, y0) = ((self.x - pad).max(0.0), (self.y - pad).max(0.0));
        let (x1, y1) = ((self.right() + pad).min(w), (self.bottom() + pad).min(h));
        Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }
    /// Целочисленный прямоугольник внутри `w`×`h`: x, y, ширина, высота (не меньше 1).
    pub fn pixels(&self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let x0 = (self.x.floor().max(0.0) as u32).min(w.saturating_sub(1));
        let y0 = (self.y.floor().max(0.0) as u32).min(h.saturating_sub(1));
        let x1 = (self.right().ceil().max(0.0) as u32).clamp(x0 + 1, w);
        let y1 = (self.bottom().ceil().max(0.0) as u32).clamp(y0 + 1, h);
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// Смысловой тип блока. `Unknown`, если признаков мало: уверенность не выдумывается.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextBlockType {
    Dialogue,
    Subtitle,
    CharacterName,
    UiLabel,
    Button,
    MenuItem,
    Notification,
    Tooltip,
    Unknown,
}

/// Начертание по признакам глифов. Точное имя игрового шрифта не определяется.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FontCategory {
    SansSerif,
    HumanistSans,
    GeometricSans,
    NeoGrotesqueSans,
    Serif,
    TransitionalSerif,
    OldStyleSerif,
    ModernSerif,
    SlabSerif,
    Monospace,
    Rounded,
    Condensed,
    Display,
    CjkSans,
    CjkSerif,
    Unknown,
}

impl FontCategory {
    pub fn is_sans(self) -> bool {
        matches!(self, Self::SansSerif | Self::HumanistSans | Self::GeometricSans | Self::NeoGrotesqueSans | Self::Rounded)
    }
    pub fn is_serif(self) -> bool {
        matches!(self, Self::Serif | Self::TransitionalSerif | Self::OldStyleSerif | Self::ModernSerif)
    }
    pub fn is_cjk(self) -> bool {
        matches!(self, Self::CjkSans | Self::CjkSerif)
    }
    /// Общее семейство для последнего уровня запасных вариантов.
    pub fn generic_family(self) -> &'static str {
        match self {
            c if c.is_serif() || c == Self::SlabSerif || c == Self::CjkSerif => "serif",
            Self::Monospace => "monospace",
            _ => "sans-serif",
        }
    }
}

/// Насыщенность шрифта; значения совпадают со шкалой Qt 6 (`Font.Weight`, 100–900).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    DemiBold,
    Bold,
    ExtraBold,
    Black,
}

impl FontWeight {
    pub const ALL: [FontWeight; 9] = [Self::Thin, Self::ExtraLight, Self::Light, Self::Normal, Self::Medium,
        Self::DemiBold, Self::Bold, Self::ExtraBold, Self::Black];
    pub fn value(self) -> i32 {
        (Self::ALL.iter().position(|w| *w == self).unwrap() as i32 + 1) * 100
    }
    /// Ближайшая насыщенность по шкале 100–900 (fontconfig 0–210 переводится заранее).
    pub fn from_value(v: i32) -> Self {
        Self::ALL[((v as f32 / 100.0).round() as i32 - 1).clamp(0, 8) as usize]
    }
    /// Ближайшая из доступных у шрифта.
    pub fn closest(self, available: &[FontWeight]) -> FontWeight {
        available.iter().copied().min_by_key(|w| (w.value() - self.value()).abs()).unwrap_or(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapMode {
    WordWrap,
    WrapAnywhere,
    NoWrap,
    Elide,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Padding {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Padding {
    pub fn uniform(v: f32) -> Self {
        Self { left: v, right: v, top: v, bottom: v }
    }
}

/// Письменность, глифы которой нужны для перевода (из языка перевода, не из длины текста).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
    ChineseSimplified,
    ChineseTraditional,
    Japanese,
    Korean,
    Other,
}

impl Script {
    pub fn is_cjk(self) -> bool {
        matches!(self, Self::ChineseSimplified | Self::ChineseTraditional | Self::Japanese | Self::Korean)
    }
    /// Языковой тег fontconfig, покрытие которого обязательно.
    pub fn fontconfig_lang(self) -> &'static str {
        match self {
            Self::Latin | Self::Other => "en",
            Self::Cyrillic => "ru",
            Self::Greek => "el",
            Self::ChineseSimplified => "zh-cn",
            Self::ChineseTraditional => "zh-tw",
            Self::Japanese => "ja",
            Self::Korean => "ko",
        }
    }
    /// По коду языка перевода (ISO 639-1 или Tesseract).
    pub fn from_lang(code: &str) -> Self {
        let code = code.to_ascii_lowercase();
        match code.as_str() {
            "ru" | "rus" | "uk" | "ukr" | "be" | "bel" | "bg" | "bul" | "sr" | "srp" | "kk" | "kaz" | "mk" | "mkd" => Self::Cyrillic,
            "el" | "ell" => Self::Greek,
            "ja" | "jpn" => Self::Japanese,
            "ko" | "kor" => Self::Korean,
            "zh-tw" | "zh-hk" | "chi_tra" => Self::ChineseTraditional,
            c if c.starts_with("zh") || c == "chi_sim" => Self::ChineseSimplified,
            "en" | "eng" | "de" | "deu" | "fr" | "fra" | "es" | "spa" | "it" | "ita" | "pt" | "por" | "pl" | "pol"
            | "nl" | "nld" | "cs" | "ces" | "sv" | "swe" | "tr" | "tur" | "vi" | "vie" | "fi" | "fin" | "ro" | "ron" => Self::Latin,
            _ => Self::Other,
        }
    }
    /// По самому тексту: преобладающая письменность среди букв.
    pub fn of_text(text: &str) -> Self {
        let (mut latin, mut cyr, mut greek, mut kana, mut hangul, mut han) = (0, 0, 0, 0, 0, 0);
        for c in text.chars() {
            match c as u32 {
                0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => latin += 1,
                0x400..=0x52F => cyr += 1,
                0x370..=0x3FF => greek += 1,
                0x3040..=0x30FF => kana += 1,
                0xAC00..=0xD7AF | 0x1100..=0x11FF => hangul += 1,
                0x4E00..=0x9FFF | 0x3400..=0x4DBF => han += 1,
                _ => {}
            }
        }
        let counts = [(latin, Self::Latin), (cyr, Self::Cyrillic), (greek, Self::Greek), (kana + han / 4, Self::Japanese),
            (hangul, Self::Korean), (han, Self::ChineseSimplified)];
        counts.into_iter().max_by_key(|(n, _)| *n).filter(|(n, _)| *n > 0).map(|(_, s)| s).unwrap_or(Self::Other)
    }
}

/// Маска «чернил» кадра: текст — меньший по площади класс яркости после порога Оцу.
pub struct InkMask {
    pub w: usize,
    pub h: usize,
    pub ink: Vec<bool>,
    pub ink_is_dark: bool,
}

impl InkMask {
    pub fn at(&self, x: usize, y: usize) -> bool {
        self.ink[y * self.w + x]
    }
}

pub(crate) fn luma(p: &[u8]) -> u8 {
    ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8
}

/// Средний цвет пикселей, для которых `pick` истинно, внутри `r`.
pub(crate) fn mean_color(img: &image::RgbaImage, r: &Rect, mut pick: impl FnMut(usize, usize) -> bool) -> Option<[u8; 3]> {
    let (x0, y0, w, h) = r.pixels(img.width(), img.height());
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            if pick(x as usize, y as usize) {
                let p = img.get_pixel(x, y).0;
                for c in 0..3 { sum[c] += p[c] as u64; }
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum.map(|v| (v / n) as u8))
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Относительная яркость (WCAG) для оценки контраста.
pub fn relative_luminance(c: [u8; 3]) -> f32 {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
}

pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
pub(crate) mod testing {
    use image::{DynamicImage, Rgba, RgbaImage};

    /// Холст цвета `bg`.
    pub fn canvas(w: u32, h: u32, bg: [u8; 3]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([bg[0], bg[1], bg[2], 255]))
    }

    /// «Строка текста»: буквы вида «Н» шириной `glyph`, через `gap`, высотой `height` (одна связная
    /// компонента, как настоящая буква); `serifs` — засечки сверху и снизу. Слова разделены 3×gap.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_line(img: &mut RgbaImage, x: u32, y: u32, chars: usize, glyph: u32, gap: u32, height: u32, stroke: u32, ink: [u8; 3], serifs: bool) -> u32 {
        let mut gx = x;
        for i in 0..chars {
            if i > 0 && i % 5 == 0 { gx += gap * 3; }
            for yy in y..y + height {
                for xx in gx..gx + glyph {
                    let edge = xx < gx + stroke || xx >= gx + glyph - stroke;
                    let bar = serifs && (yy < y + stroke || yy >= y + height - stroke);
                    let middle = yy >= y + height / 2 && yy < y + height / 2 + stroke;
                    if edge || bar || middle { img.put_pixel(xx, yy, Rgba([ink[0], ink[1], ink[2], 255])); }
                }
            }
            gx += glyph + gap;
        }
        gx
    }

    pub fn dynamic(img: RgbaImage) -> DynamicImage {
        DynamicImage::ImageRgba8(img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_geometry() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), 25.0);
        assert!((a.iou(&b) - 25.0 / 175.0).abs() < 1e-6);
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.expand(3.0, 12.0, 100.0), Rect::new(0.0, 0.0, 12.0, 13.0));
        assert_eq!(Rect::new(2.5, 1.2, 3.0, 2.0).pixels(10, 10), (2, 1, 4, 3));
    }

    #[test]
    fn weights_and_scripts() {
        assert_eq!(FontWeight::Bold.value(), 700);
        assert_eq!(FontWeight::from_value(640), FontWeight::DemiBold);
        assert_eq!(FontWeight::DemiBold.closest(&[FontWeight::Normal, FontWeight::Medium, FontWeight::Bold]), FontWeight::Medium);
        assert_eq!(Script::from_lang("ru"), Script::Cyrillic);
        assert_eq!(Script::from_lang("zh-TW"), Script::ChineseTraditional);
        assert_eq!(Script::of_text("Привет, world"), Script::Cyrillic);
        assert_eq!(Script::of_text("こんにちは世界"), Script::Japanese);
        assert_eq!(Script::of_text("你好世界"), Script::ChineseSimplified);
        assert!(contrast_ratio([255, 255, 255], [0, 0, 0]) > 20.0);
    }
}
