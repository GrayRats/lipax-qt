//! The sections of the settings: capture, recognition, translation, translation window and appearance.

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureSettings {
    pub source: CaptureSource,
    pub portal_token: String,
    pub portal_fills_monitor: bool,
    pub regions: Vec<CaptureRegion>,
    pub active_region: String,
    pub allow_multiple_regions: bool,
    pub frame_color: String,
    pub frame_width: u32,
    pub frame_seconds: u32,
    pub region_frame_mode: String,
    pub region_frame_pinned: String,
    pub window: Option<WindowKey>,
    pub region: Option<NormRect>,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            source: CaptureSource::Auto,
            portal_token: String::new(),
            portal_fills_monitor: false,
            regions: default_regions(),
            active_region: "subtitles".into(),
            allow_multiple_regions: false,
            frame_color: "#ff0000".into(),
            frame_width: 2,
            frame_seconds: 3,
            region_frame_mode: "selection".into(),
            region_frame_pinned: "dim".into(),
            window: None,
            region: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextRecognitionSettings {
    pub language: String,
    pub engine: OcrEngine,
    pub paddle_python: String,
    /// Threads of ONNX Runtime for RapidOCR; 0 is half of the cores, at most 4 (`ocr::rapid::effective_threads`).
    pub rapid_threads: u32,
    /// Use a GPU execution provider of ONNX Runtime when the installed library has one; otherwise the CPU.
    pub rapid_use_gpu: bool,
    /// PP-OCRv5 models: `mobile` or `server` (where the language has one).
    pub rapid_variant: String,
    pub minimum_confidence: u32,
    /// Filters before OCR (`ocr::filter::Preprocess`) and removal of stray marks after it.
    pub binarize: bool,
    pub auto_invert: bool,
    /// −100…100, 0 is off.
    pub contrast: i32,
    pub sharpen: bool,
    pub filter_noise: bool,
    /// With no filter switched on by hand, the frame is looked at and Otsu binarization with inversion is applied to
    /// a noisy one (`ocr::filter::FilterPlan`); a clean frame is read as it is.
    pub auto_filters: bool,
    pub interval_ms: u64,
    pub sensitivity: f32,
    pub debounce_ms: u64,
}

impl Default for TextRecognitionSettings {
    fn default() -> Self {
        Self {
            language: "eng".into(),
            engine: "tesseract".into(),
            paddle_python: "python3".into(),
            rapid_threads: 0,
            rapid_use_gpu: false,
            rapid_variant: "mobile".into(),
            minimum_confidence: 30,
            binarize: false,
            auto_invert: false,
            contrast: 0,
            sharpen: false,
            filter_noise: true,
            auto_filters: true,
            interval_ms: 500,
            sensitivity: 2.0,
            debounce_ms: 400,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslationSettings {
    pub target_language: String,
    pub service: TranslationService,
    pub yandex_api_key: String,
    pub yandex_folder_id: String,
    pub custom_url: String,
    pub custom_api_key: String,
    pub deepl_api_key: String,
    pub microsoft_api_key: String,
    pub microsoft_region: String,
    pub bergamot_binary: String,
    pub bergamot_models_dir: String,
    pub bergamot_model_paths: BTreeMap<String, String>,
    pub auto_translate: bool,
    /// Translate what changed: the text of the translation window is split into paragraphs and only the
    /// paragraphs not translated before are sent; the others come from the cache.
    pub changes_only: bool,
    pub source_language: TranslationSourceLanguage,
}

/// An API key as `Debug` shows it: only whether one is set, never the value.
pub(crate) fn redacted(key: &str) -> &'static str {
    if key.is_empty() { "<unset>" } else { "<redacted>" }
}

impl std::fmt::Debug for TranslationSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranslationSettings")
            .field("target_language", &self.target_language)
            .field("service", &self.service)
            .field("yandex_api_key", &redacted(&self.yandex_api_key))
            .field("yandex_folder_id", &self.yandex_folder_id)
            .field("custom_url", &self.custom_url)
            .field("custom_api_key", &redacted(&self.custom_api_key))
            .field("deepl_api_key", &redacted(&self.deepl_api_key))
            .field("microsoft_api_key", &redacted(&self.microsoft_api_key))
            .field("microsoft_region", &self.microsoft_region)
            .field("bergamot_binary", &self.bergamot_binary)
            .field("bergamot_models_dir", &self.bergamot_models_dir)
            .field("bergamot_model_paths", &self.bergamot_model_paths)
            .field("auto_translate", &self.auto_translate)
            .field("changes_only", &self.changes_only)
            .field("source_language", &self.source_language)
            .finish()
    }
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            target_language: "ru".into(),
            service: TranslationService::Google,
            yandex_api_key: String::new(),
            yandex_folder_id: String::new(),
            custom_url: String::new(),
            custom_api_key: String::new(),
            deepl_api_key: String::new(),
            microsoft_api_key: String::new(),
            microsoft_region: String::new(),
            bergamot_binary: String::new(),
            bergamot_models_dir: String::new(),
            bergamot_model_paths: BTreeMap::new(),
            auto_translate: true,
            changes_only: false,
            source_language: TranslationSourceLanguage::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslationWindowSettings {
    pub screen: String,
    pub mode: TranslationWindowMode,
    pub corner_radius: u32,
    pub pinned_corner_radius: u32,
    pub border_color: String,
    pub border_opacity: f64,
    pub border_width: u32,
    pub border_pattern: bool,
    pub border_always: bool,
    pub border_seconds: u32,
    pub max_width_enabled: bool,
    pub maximum_width: u32,
    pub auto_shrink: bool,
    pub opacity: f64,
    pub click_through: bool,
    pub position: (i32, i32),
    pub floating_geometry: Option<FloatingGeometry>,
    pub size: (u32, u32),
}

impl Default for TranslationWindowSettings {
    fn default() -> Self {
        Self {
            screen: String::new(),
            mode: TranslationWindowMode::Floating,
            corner_radius: 12,
            pinned_corner_radius: 0,
            border_color: "#ff00ff".into(),
            border_opacity: 0.65,
            border_width: 2,
            border_pattern: false,
            border_always: true,
            border_seconds: 5,
            max_width_enabled: true,
            maximum_width: 900,
            auto_shrink: true,
            opacity: 0.85,
            click_through: true,
            position: (100, 100),
            floating_geometry: None,
            size: (700, 120),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowAppearance {
    pub background_style: String,
    pub blur_enabled: bool,
    pub blur_tint: f64,
    pub dim_inverse: bool,
    pub font_family: String,
    pub font_bold: bool,
    pub font_italic: bool,
    pub text_color: String,
    pub background_color: String,
    pub padding: u32,
    pub text_alignment: String,
    pub text_wrap: bool,
    pub text_outline: bool,
    pub outline_color: String,
    pub line_spacing: f64,
    pub show_original: bool,
    /// A rolling log: the previous translations stay above the current one, fading with age.
    pub log_mode: bool,
    /// How many entries the log keeps (the current one included), 2..=10.
    pub log_entries: u32,
    pub original_font_family: String,
    pub original_font_size: u32,
    pub original_color: String,
    pub font_size: u32,
}

impl Default for WindowAppearance {
    fn default() -> Self {
        Self {
            background_style: "solid".into(),
            blur_enabled: true,
            blur_tint: 0.3,
            dim_inverse: false,
            font_family: "Inter".into(),
            font_bold: false,
            font_italic: false,
            text_color: "#ffffff".into(),
            background_color: "#181818".into(),
            padding: 16,
            text_alignment: "center".into(),
            text_wrap: true,
            text_outline: true,
            outline_color: "#000000".into(),
            line_spacing: 1.0,
            show_original: false,
            log_mode: false,
            log_entries: 5,
            original_font_family: String::new(),
            original_font_size: 14,
            original_color: "#b0b0b0".into(),
            font_size: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslationAppearance {
    pub window: WindowAppearance,
    pub inplace: InplaceSettings,
    pub main_window: MainWindowAppearance,
}

/// The «Original» and «Translation» areas of the main LipaX window: its own font and background, independent of the
/// floating translation window (`window`) and of the text drawn over the game (`inplace`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MainWindowAppearance {
    /// A font shipped with LipaX, for Latin and Cyrillic text.
    pub font_family: String,
    /// For Chinese, Japanese and Korean glyphs; empty — the bundled Noto Sans CJK.
    pub cjk_font_family: String,
    pub font_size: u32,
    /// `gray` (standard) or `system` (the palette of the theme).
    pub background: String,
}

impl Default for MainWindowAppearance {
    fn default() -> Self {
        Self { font_family: "Inter".into(), cjk_font_family: String::new(), font_size: 16, background: "gray".into() }
    }
}
