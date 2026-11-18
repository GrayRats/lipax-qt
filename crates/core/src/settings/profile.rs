//! Per-game profiles: what is remembered for each game window.

use super::*;

/// Most games remembered; a new one beyond this is simply not stored.
pub const MAX_GAME_PROFILES: usize = 64;

/// Window classes that many games share: Proton (`steam_app_default`, also `steam_app_<id>` for the key of 1.4.1),
/// plain Wine. Their profile key takes the program of the window.
pub(super) fn shared_class(class: &str) -> bool {
    class.starts_with("steam_app_") || class == "wine" || class == "explorer.exe"
}

/// What is remembered per game (window class): where the text is, how it is read and where the
/// translation sits. Region rectangles are fractions of the client frame, so they survive
/// restarts, window moves and resolution changes. Appearance and hotkeys stay global.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameProfile {
    pub caption: String,
    pub display_mode: TranslationDisplayMode,
    pub capture: GameCaptureSettings,
    pub recognition: GameTextRecognitionSettings,
    pub translation: GameTranslationSettings,
    pub translation_window: GameTranslationWindowSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameCaptureSettings {
    pub regions: Vec<CaptureRegion>,
    pub active_region: String,
    pub allow_multiple_regions: bool,
}

impl Default for GameCaptureSettings {
    fn default() -> Self {
        Self {
            regions: default_regions(),
            active_region: "subtitles".into(),
            allow_multiple_regions: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameTextRecognitionSettings {
    pub language: String,
    pub engine: OcrEngine,
    /// Image filters are a property of the game: dark interfaces want inversion, light ones do not.
    pub binarize: bool,
    pub auto_invert: bool,
    pub contrast: i32,
    pub sharpen: bool,
}

impl Default for GameTextRecognitionSettings {
    fn default() -> Self {
        Self {
            language: "eng".into(),
            engine: "tesseract".into(),
            binarize: false,
            auto_invert: false,
            contrast: 0,
            sharpen: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameTranslationSettings {
    pub target_language: String,
    pub source_language: TranslationSourceLanguage,
}

impl Default for GameTranslationSettings {
    fn default() -> Self {
        Self {
            target_language: "ru".into(),
            source_language: TranslationSourceLanguage::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameTranslationWindowSettings {
    pub mode: TranslationWindowMode,
    pub screen: String,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub floating_geometry: Option<FloatingGeometry>,
}

impl Default for GameTranslationWindowSettings {
    fn default() -> Self {
        Self {
            mode: TranslationWindowMode::Floating,
            screen: String::new(),
            position: (100, 100),
            size: (700, 120),
            floating_geometry: None,
        }
    }
}

impl Default for GameProfile {
    fn default() -> Self { Settings::default().game_profile("") }
}
