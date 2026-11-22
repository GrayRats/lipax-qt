//! Small value types of the settings: rectangles, window keys, capture sources, OCR engines.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowKey {
    pub uuid: String,
    pub resource_class: String,
    pub caption: String,
    /// The program of the window's process, lower case without the path (`crossxmars.exe` for a game under Proton);
    /// empty when it could not be read (portal, no KWin script, settings of an older version).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub program: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureSource {
    /// KWin, а если он недоступен — portal.
    Auto,
    Kwin,
    Portal,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslationService {
    Google,
    Yandex,
    Custom,
    DeepL,
    Microsoft,
    Bergamot,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureRegion {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub rect: Option<NormRect>,
    /// Empty values inherit the global OCR/translation settings.
    #[serde(alias = "source_lang")]
    pub recognition_language: String,
    #[serde(alias = "target_lang")]
    pub target_language: String,
    #[serde(alias = "ocr_engine", deserialize_with = "optional_ocr_engine")]
    pub engine: Option<OcrEngine>,
    pub interval_ms: u64,
    pub debounce_ms: u64,
    /// Outline of this region in the game; empty inherits `CaptureSettings::region_frame_mode`.
    pub frame_mode: String,
}
impl Default for CaptureRegion {
    fn default() -> Self {
        Self { id: "subtitles".into(), name: "Субтитры".into(), enabled: true, rect: None,
            recognition_language: String::new(), target_language: String::new(), engine: None,
            interval_ms: 500, debounce_ms: 400, frame_mode: String::new() }
    }
}

pub(super) fn optional_ocr_engine<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<OcrEngine>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()).map(OcrEngine::from))
}

/// How the translation window draws its background.
/// Application themes of the «General» tab. `system` leaves the choice to Qt and the desktop; `dark`/`light` are the
/// Universal style (and switch live), `breeze`/`fusion` are other Qt Quick styles chosen at start.
pub const THEMES: [&str; 5] = ["system", "breeze", "fusion", "dark", "light"];
pub const LOG_LEVELS: [&str; 4] = ["error", "warn", "info", "debug"];
/// What fills the text areas of the main window: a standard grey or the palette of the current theme.
pub const MAIN_WINDOW_BACKGROUNDS: [&str; 2] = ["gray", "system"];

pub const WINDOW_BACKGROUND_STYLES: [&str; 4] = [
    "blur",        // compositor blur with an adjustable dark tint
    "transparent", // no background: white text with a light shadow
    "dim",         // light dark tint over a faint blur, or its light inverse
    "solid",       // `background_color` at `opacity`
];

/// Outline around a capture region in the game.
pub const REGION_FRAME_MODES: [&str; 4] = [
    "pattern",   // purple/black "error texture", always shown
    "solid",     // plain outline in `frame_color`, always shown
    "off",
    "selection", // shown on selection, fades out after `frame_seconds`
];

/// No more translation regions than this can exist at once.
pub const MAX_REGIONS: usize = 3;

/// A single active region; more are added from the settings, up to `MAX_REGIONS`.
pub fn default_regions() -> Vec<CaptureRegion> {
    vec![CaptureRegion::default()]
}

/// Explicit user preference. Surface roles remain separate QML objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationWindowMode { Pinned, #[default] Floating }
impl TranslationWindowMode {
    pub fn is_pinned(self) -> bool { self == Self::Pinned }
    pub fn from_pinned(pinned: bool) -> Self { if pinned { Self::Pinned } else { Self::Floating } }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "language")]
pub enum TranslationSourceLanguage { #[default] RecognitionLanguage, Explicit(String) }

/// Unknown engines are retained so diagnostics can explain an invalid configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum OcrEngine { Tesseract, PaddleOcr, RapidOcr, MeikiOcr, Auto, Consensus, Unknown(String) }
impl OcrEngine {
    pub fn as_str(&self) -> &str { match self { Self::Tesseract => "tesseract", Self::PaddleOcr => "paddleocr", Self::RapidOcr => "rapidocr", Self::MeikiOcr => "meikiocr", Self::Auto => "auto", Self::Consensus => "consensus", Self::Unknown(name) => name } }
}
impl From<&str> for OcrEngine {
    fn from(name: &str) -> Self { match name { "tesseract" => Self::Tesseract, "paddleocr" => Self::PaddleOcr, "rapidocr" => Self::RapidOcr, "meikiocr" => Self::MeikiOcr, "auto" => Self::Auto, "consensus" => Self::Consensus, _ => Self::Unknown(name.into()) } }
}
impl From<String> for OcrEngine { fn from(name: String) -> Self { Self::from(name.as_str()) } }
impl From<OcrEngine> for String { fn from(engine: OcrEngine) -> Self { engine.as_str().into() } }
impl PartialEq<str> for OcrEngine { fn eq(&self, other: &str) -> bool { self.as_str() == other } }
impl PartialEq<&str> for OcrEngine { fn eq(&self, other: &&str) -> bool { self.as_str() == *other } }
