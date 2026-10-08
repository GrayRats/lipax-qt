use crate::layout::{FontWeight, Padding, TextAlignment, WrapMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Последнее положение свободного окна перевода (логические координаты рабочего стола)
/// и выход, на котором оно было. Это состояние для восстановления, а не ограничение:
/// во время перемещения позицию определяет композитор.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FloatingGeometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub output: String,
    pub output_x: f64,
    pub output_y: f64,
}

impl FloatingGeometry {
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.w, self.h, self.output_x, self.output_y].iter().all(|v| v.is_finite()) && self.w > 0.0 && self.h > 0.0
    }
    /// Положение относительно своего выхода: для закреплённого окна на том же месте.
    pub fn relative(&self) -> (i32, i32) {
        ((self.x - self.output_x).round() as i32, (self.y - self.output_y).round() as i32)
    }
}

/// Где показывается перевод. Два независимых рендера со своим состоянием видимости.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslationDisplayMode {
    /// Окно перевода (`TranslationWindow.qml`): закреплённое или свободное.
    #[default]
    Window,
    /// Перевод поверх найденных полей текста (`InplaceTranslation.qml`).
    Inplace,
}

impl<'de> Deserialize<'de> for TranslationDisplayMode {
    /// Старое значение `"overlay"` и неизвестные строки — окно перевода: одна незнакомая
    /// строка не должна сбрасывать весь конфиг.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(if String::deserialize(d)? == "inplace" { Self::Inplace } else { Self::Window })
    }
}

/// Свойство, которое оценивается автоматически или задано пользователем. Каждое свойство
/// переключается отдельно: ручной шрифт не отключает автоматический размер и т. п.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
#[derive(Default)]
pub enum PropertyMode<T> {
    #[default]
    Auto,
    Manual(T),
}


impl<T: Clone> PropertyMode<T> {
    pub fn manual(&self) -> Option<&T> {
        match self { Self::Manual(v) => Some(v), Self::Auto => None }
    }
    /// Ручное значение или результат автоматической оценки.
    pub fn resolve(&self, auto: T) -> T {
        self.manual().cloned().unwrap_or(auto)
    }
}

/// Чем закрывается оригинал под переводом. Не связано с фоном окна перевода (`WindowAppearance::background_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InplaceBackgroundMode {
    #[default]
    Auto,
    #[serde(alias = "inpaint_blur", alias = "solid_fill")]
    TextReplacement,
    #[serde(alias = "transparent")]
    TransparentOutline,
    #[serde(alias = "adaptive_padding_fill")]
    PaddedFill,
}

/// Настройки режима «перевод поверх оригинала».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InplaceSettings {
    pub background_mode: InplaceBackgroundMode,
    pub font_family: PropertyMode<String>,
    /// Кегль, px экрана.
    pub font_size: PropertyMode<f32>,
    pub font_weight: PropertyMode<FontWeight>,
    pub italic: PropertyMode<bool>,
    /// Межстрочный интервал, множитель высоты строки.
    pub line_height: PropertyMode<f32>,
    /// Дополнительный трекинг, px экрана.
    pub letter_spacing: PropertyMode<f32>,
    pub alignment: PropertyMode<TextAlignment>,
    pub wrap_mode: PropertyMode<WrapMode>,
    /// `#rrggbb`.
    pub text_color: PropertyMode<String>,
    pub outline_color: PropertyMode<String>,
    pub outline_width: f32,
    pub shadow: bool,
    pub text_opacity: f32,
    pub fill_color: PropertyMode<String>,
    pub fill_opacity: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub extra_margin: f32,
    pub corner_radius: f32,
    /// Поля, px экрана.
    pub padding: PropertyMode<Padding>,
    pub minimum_font_size: f32,
    pub maximum_font_size: f32,
    /// Разрешить узкий вариант шрифта, если перевод не помещается.
    pub allow_condensed_fallback: bool,
    /// Замена шрифта для категории: `"serif" = "PT Serif"` (ключ — категория в snake_case).
    pub font_overrides: BTreeMap<String, String>,
    /// Семейства, которые проверяются первыми при автоматическом выборе.
    pub preferred_fonts: Vec<String>,
    /// Блок делится там, где шаг между строками (центр–центр) больше `k` средних высот строки; см. `layout::split`.
    /// Меньше — дробнее (склеенные имя и реплика разделяются раньше), больше — крупнее блоки.
    pub line_gap_factor: f32,
    /// The overlay is shown only while the game window is the active one (not minimised, not behind another window).
    pub only_when_active: bool,
}

impl Default for InplaceSettings {
    fn default() -> Self {
        Self {
            background_mode: InplaceBackgroundMode::Auto,
            font_family: PropertyMode::Auto,
            font_size: PropertyMode::Auto,
            font_weight: PropertyMode::Auto,
            italic: PropertyMode::Auto,
            line_height: PropertyMode::Auto,
            letter_spacing: PropertyMode::Auto,
            alignment: PropertyMode::Auto,
            wrap_mode: PropertyMode::Auto,
            text_color: PropertyMode::Auto,
            outline_color: PropertyMode::Auto,
            outline_width: 1.0,
            shadow: false,
            text_opacity: 1.0,
            fill_color: PropertyMode::Auto,
            fill_opacity: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            extra_margin: 0.0,
            corner_radius: 4.0,
            padding: PropertyMode::Auto,
            minimum_font_size: 8.0,
            maximum_font_size: 96.0,
            allow_condensed_fallback: true,
            font_overrides: BTreeMap::new(),
            preferred_fonts: Vec::new(),
            line_gap_factor: crate::layout::split::DEFAULT_LINE_GAP_FACTOR,
            only_when_active: false,
        }
    }
}

impl InplaceSettings {
    fn sanitize(&mut self) {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        // Below 1.3 ordinary line spacing (1.2–1.5 heights) would be cut; above 4 nothing would ever be.
        self.line_gap_factor = finite(self.line_gap_factor, crate::layout::split::DEFAULT_LINE_GAP_FACTOR).clamp(1.3, 4.0);
        self.minimum_font_size = finite(self.minimum_font_size, 8.0).clamp(4.0, 200.0);
        self.maximum_font_size = finite(self.maximum_font_size, 96.0).clamp(self.minimum_font_size, 400.0);
        if let PropertyMode::Manual(v) = &mut self.font_size { *v = finite(*v, 20.0).clamp(4.0, 400.0); }
        if let PropertyMode::Manual(v) = &mut self.line_height { *v = finite(*v, 1.0).clamp(0.7, 3.0); }
        if let PropertyMode::Manual(v) = &mut self.letter_spacing { *v = finite(*v, 0.0).clamp(-3.0, 20.0); }
        if let PropertyMode::Manual(p) = &mut self.padding {
            for v in [&mut p.left, &mut p.right, &mut p.top, &mut p.bottom] { *v = finite(*v, 0.0).clamp(0.0, 64.0); }
        }
        if let PropertyMode::Manual(c) = &self.text_color
            && (c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())) { self.text_color = PropertyMode::Auto; }
        if let PropertyMode::Manual(f) = &self.font_family && f.trim().is_empty() { self.font_family = PropertyMode::Auto; }
        self.outline_width = finite(self.outline_width, 1.0).clamp(0.0, 8.0);
        self.text_opacity = finite(self.text_opacity, 1.0).clamp(0.0, 1.0);
        self.fill_opacity = finite(self.fill_opacity, 1.0).clamp(0.0, 1.0);
        self.padding_x = finite(self.padding_x, 0.0).clamp(0.0, 64.0);
        self.padding_y = finite(self.padding_y, 0.0).clamp(0.0, 64.0);
        self.extra_margin = finite(self.extra_margin, 0.0).clamp(0.0, 64.0);
        self.corner_radius = finite(self.corner_radius, 4.0).clamp(0.0, 64.0);
        for color in [&mut self.outline_color, &mut self.fill_color] {
            if let PropertyMode::Manual(c) = color && (c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())) { *color = PropertyMode::Auto; }
        }
        self.preferred_fonts.retain(|f| !f.trim().is_empty());
        self.font_overrides.retain(|_, f| !f.trim().is_empty());
    }
}

/// Upper bound for any screen side in logical pixels (8K, rotated or not).
pub const MAX_SCREEN_SIDE: u32 = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowKey {
    pub uuid: String,
    pub resource_class: String,
    pub caption: String,
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

fn optional_ocr_engine<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<OcrEngine>, D::Error> {
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
pub enum OcrEngine { Tesseract, PaddleOcr, Auto, Unknown(String) }
impl OcrEngine {
    pub fn as_str(&self) -> &str { match self { Self::Tesseract => "tesseract", Self::PaddleOcr => "paddleocr", Self::Auto => "auto", Self::Unknown(name) => name } }
}
impl From<&str> for OcrEngine {
    fn from(name: &str) -> Self { match name { "tesseract" => Self::Tesseract, "paddleocr" => Self::PaddleOcr, "auto" => Self::Auto, _ => Self::Unknown(name.into()) } }
}
impl From<String> for OcrEngine { fn from(name: String) -> Self { Self::from(name.as_str()) } }
impl From<OcrEngine> for String { fn from(engine: OcrEngine) -> Self { engine.as_str().into() } }
impl PartialEq<str> for OcrEngine { fn eq(&self, other: &str) -> bool { self.as_str() == other } }
impl PartialEq<&str> for OcrEngine { fn eq(&self, other: &&str) -> bool { self.as_str() == *other } }

/// Legacy key mapping is confined to file migration; the runtime and UI use canonical paths.
const LEGACY_KEYS: &[(&str, &str)] = &[
    ("capture_backend", "capture.source"),
    ("portal_token", "capture.portal_token"),
    ("portal_fills_monitor", "capture.portal_fills_monitor"),
    ("regions", "capture.regions"),
    ("active_region", "capture.active_region"),
    ("allow_multiple_regions", "capture.allow_multiple_regions"),
    ("frame_color", "capture.frame_color"),
    ("frame_width", "capture.frame_width"),
    ("frame_seconds", "capture.frame_seconds"),
    ("region_frame_mode", "capture.region_frame_mode"),
    ("region_frame_pinned", "capture.region_frame_pinned"),
    ("window", "capture.window"),
    ("region", "capture.region"),
    ("source_lang", "recognition.language"),
    ("ocr_engine", "recognition.engine"),
    ("paddle_python", "recognition.paddle_python"),
    ("ocr_min_confidence", "recognition.minimum_confidence"),
    ("interval_ms", "recognition.interval_ms"),
    ("sensitivity", "recognition.sensitivity"),
    ("debounce_ms", "recognition.debounce_ms"),
    ("target_lang", "translation.target_language"),
    ("translator", "translation.service"),
    ("yandex_api_key", "translation.yandex_api_key"),
    ("yandex_folder_id", "translation.yandex_folder_id"),
    ("custom_url", "translation.custom_url"),
    ("custom_api_key", "translation.custom_api_key"),
    ("auto_translate", "translation.auto_translate"),
    ("overlay_screen", "translation_window.screen"),
    ("overlay_pinned", "translation_window.mode"),
    ("overlay_corner_radius", "translation_window.corner_radius"),
    ("overlay_pinned_corner_radius", "translation_window.pinned_corner_radius"),
    ("border_color", "translation_window.border_color"),
    ("border_opacity", "translation_window.border_opacity"),
    ("border_width", "translation_window.border_width"),
    ("border_pattern", "translation_window.border_pattern"),
    ("border_always", "translation_window.border_always"),
    ("border_seconds", "translation_window.border_seconds"),
    ("max_width_enabled", "translation_window.max_width_enabled"),
    ("overlay_max_width", "translation_window.maximum_width"),
    ("overlay_auto_shrink", "translation_window.auto_shrink"),
    ("opacity", "translation_window.opacity"),
    ("click_through", "translation_window.click_through"),
    ("overlay_pos", "translation_window.position"),
    ("floating_geometry", "translation_window.floating_geometry"),
    ("overlay_size", "translation_window.size"),
    ("overlay_style", "appearance.window.background_style"),
    ("blur_enabled", "appearance.window.blur_enabled"),
    ("blur_tint", "appearance.window.blur_tint"),
    ("dim_inverse", "appearance.window.dim_inverse"),
    ("font_family", "appearance.window.font_family"),
    ("font_bold", "appearance.window.font_bold"),
    ("font_italic", "appearance.window.font_italic"),
    ("text_color", "appearance.window.text_color"),
    ("background_color", "appearance.window.background_color"),
    ("overlay_padding", "appearance.window.padding"),
    ("text_alignment", "appearance.window.text_alignment"),
    ("text_wrap", "appearance.window.text_wrap"),
    ("text_outline", "appearance.window.text_outline"),
    ("outline_color", "appearance.window.outline_color"),
    ("line_spacing", "appearance.window.line_spacing"),
    ("show_original", "appearance.window.show_original"),
    ("original_font_family", "appearance.window.original_font_family"),
    ("original_font_size", "appearance.window.original_font_size"),
    ("original_color", "appearance.window.original_color"),
    ("font_size", "appearance.window.font_size"),
    ("translation_display", "display_mode"),
    ("inplace", "appearance.inplace"),
];
fn insert_legacy(table: &mut toml::Table, path: &str, value: toml::Value) {
    if let Some((head, tail)) = path.split_once('.') {
        let entry = table.entry(head).or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(child) = entry.as_table_mut() { insert_legacy(child, tail, value); }
    } else if let Some(existing) = table.get_mut(path).and_then(toml::Value::as_table_mut)
        && let Some(legacy) = value.as_table() {
        for (key, value) in legacy { insert_legacy(existing, key, value.clone()); }
    } else { table.entry(path).or_insert(value); }
}
fn migrate_flat_keys(table: &mut toml::Table) {
    for &(old, path) in LEGACY_KEYS {
        if let Some(mut value) = table.remove(old) {
            if old == "overlay_pinned" { value = if value.as_bool() == Some(true) { "pinned" } else { "floating" }.into(); }
            insert_legacy(table, path, value);
        }
    }
    table.remove("overlay_mode");
    if let Some(hotkeys) = table.get_mut("hotkeys").and_then(toml::Value::as_table_mut)
        && let Some(value) = hotkeys.remove("toggle_overlay") {
        hotkeys.entry("toggle_translation").or_insert(value);
    }
    if let Some(regions) = table.get_mut("capture").and_then(toml::Value::as_table_mut)
        .and_then(|capture| capture.get_mut("regions")).and_then(toml::Value::as_array_mut) {
        for region in regions.iter_mut().filter_map(toml::Value::as_table_mut) {
            for (old, new) in [("source_lang", "recognition_language"), ("target_lang", "target_language"), ("ocr_engine", "engine")] {
                if let Some(value) = region.remove(old) { region.entry(new).or_insert(value); }
            }
        }
    }
    if let Some(profiles) = table.get_mut("game_profiles").and_then(toml::Value::as_table_mut) {
        for profile in profiles.iter_mut().map(|(_, v)| v).filter_map(toml::Value::as_table_mut) { migrate_flat_keys(profile); }
    }
}

/// Read a canonical settings path, including structured leaves such as property modes.
pub fn value_at<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').try_fold(value, |value, key| value.get(key))
}

/// Version of the config layout. A file without `schema_version` is version 0 (everything
/// written before versions existed). Each step in [`migrate`] lifts a file by exactly one version.
pub const SCHEMA_VERSION: u32 = 2;

/// Lift a parsed config from `from` to [`SCHEMA_VERSION`], one step at a time, so a file of any age
/// goes through every step it missed. Steps work on the raw table: they may rename or drop
/// keys the current structs no longer know.
fn migrate(table: &mut toml::Table, from: u32) {
    for version in from..SCHEMA_VERSION {
        match version {
            // 0 -> 1: before frame modes, `frame_seconds = 0` meant "never show the frame".
            0 => {
                let never = table.get("frame_seconds").and_then(toml::Value::as_integer) == Some(0);
                if never && !table.contains_key("region_frame_mode") {
                    table.insert("region_frame_mode".into(), "off".into());
                }
            }
            1 => migrate_flat_keys(table),
            _ => unreachable!("no migration from schema {version}"),
        }
    }
    table.insert("schema_version".into(), i64::from(SCHEMA_VERSION).into());
}

/// What a changed setting needs in order to take effect. Every key of [`Settings`] is declared in
/// [`REACTIONS`] (a test fails when one is missing or stale), so no setting is saved without a
/// known way of reaching the running application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Reaction {
    /// Read live by the windows or the backend: nothing to restart.
    Immediate,
    /// The in-place layout is rebuilt from the last frame (no new capture, no OCR).
    Layout,
    /// The pipeline is reset: what was recognised so far no longer describes the new setting.
    Pipeline,
    /// The global shortcuts are registered again.
    Hotkeys,
    /// Takes effect when the game window is selected again; the user is told so.
    Reselect,
    /// Written by the backend only (window selection, capture, saving state); the form cannot change it.
    Managed,
}

/// Declaration of what every setting needs. `Managed` keys are refused in form patches.
pub const REACTIONS: &[(&str, Reaction)] = &{
    use Reaction::*;
    [
        ("schema_version", Managed), ("capture.portal_token", Managed), ("game_profiles", Managed), ("translation_window.floating_geometry", Managed),
        ("capture.window", Managed), ("capture.region", Managed),
        // Recognition and translation: the pipeline starts over.
        ("recognition.language", Pipeline), ("translation.target_language", Pipeline), ("translation.source_language", Pipeline), ("recognition.engine", Pipeline), ("recognition.paddle_python", Pipeline),
        ("recognition.minimum_confidence", Pipeline), ("recognition.binarize", Pipeline), ("recognition.auto_invert", Pipeline), ("recognition.contrast", Pipeline), ("recognition.sharpen", Pipeline), ("recognition.filter_noise", Pipeline), ("recognition.auto_filters", Pipeline), ("translation.service", Pipeline), ("translation.yandex_api_key", Pipeline), ("translation.yandex_folder_id", Pipeline),
        ("translation.custom_url", Pipeline), ("capture.portal_fills_monitor", Pipeline), ("translation.custom_api_key", Pipeline), ("translation.deepl_api_key", Pipeline), ("translation.microsoft_api_key", Pipeline), ("translation.microsoft_region", Pipeline), ("translation.bergamot_binary", Pipeline), ("translation.bergamot_models_dir", Pipeline), ("recognition.interval_ms", Pipeline), ("recognition.sensitivity", Pipeline),
        ("recognition.debounce_ms", Pipeline), ("display_mode", Pipeline), ("capture.regions", Pipeline),
        ("appearance.inplace.background_mode", Layout), ("appearance.inplace.font_family", Layout), ("appearance.inplace.font_size", Layout), ("appearance.inplace.font_weight", Layout), ("appearance.inplace.italic", Layout), ("appearance.inplace.line_height", Layout), ("appearance.inplace.letter_spacing", Layout), ("appearance.inplace.alignment", Layout), ("appearance.inplace.wrap_mode", Layout), ("appearance.inplace.text_color", Layout), ("appearance.inplace.outline_color", Layout), ("appearance.inplace.outline_width", Layout), ("appearance.inplace.shadow", Layout), ("appearance.inplace.text_opacity", Layout), ("appearance.inplace.fill_color", Layout), ("appearance.inplace.fill_opacity", Layout), ("appearance.inplace.padding_x", Layout), ("appearance.inplace.padding_y", Layout), ("appearance.inplace.extra_margin", Layout), ("appearance.inplace.corner_radius", Layout), ("appearance.inplace.padding", Layout), ("appearance.inplace.minimum_font_size", Layout), ("appearance.inplace.maximum_font_size", Layout), ("appearance.inplace.allow_condensed_fallback", Layout), ("appearance.inplace.font_overrides", Layout), ("appearance.inplace.preferred_fonts", Layout), ("appearance.inplace.line_gap_factor", Pipeline), ("appearance.inplace.only_when_active", Immediate), ("general.theme", Immediate), ("general.autostart", Immediate), ("general.notify_errors", Immediate), ("general.notify_retries", Immediate), ("general.log_level", Immediate), ("appearance.main_window.font_family", Immediate), ("appearance.main_window.cjk_font_family", Immediate), ("appearance.main_window.font_size", Immediate), ("appearance.main_window.background", Immediate), ("translation.changes_only", Pipeline),
        ("hotkeys.toggle", Hotkeys), ("hotkeys.select_region", Hotkeys), ("hotkeys.translate_once", Hotkeys), ("hotkeys.toggle_translation", Hotkeys), ("hotkeys.toggle_pin", Hotkeys),
        // The backend is chosen by the window key at selection time.
        ("capture.source", Reselect),
        ("translation_window.screen", Pipeline), ("translation_window.mode", Immediate), ("appearance.window.background_style", Immediate),
        ("appearance.window.blur_enabled", Immediate), ("appearance.window.blur_tint", Immediate), ("appearance.window.dim_inverse", Immediate), ("translation_window.corner_radius", Immediate),
        ("translation_window.pinned_corner_radius", Immediate), ("appearance.window.font_family", Immediate), ("appearance.window.font_bold", Immediate), ("appearance.window.font_italic", Immediate),
        ("appearance.window.text_color", Immediate), ("appearance.window.background_color", Immediate), ("translation_window.border_color", Immediate), ("translation_window.border_opacity", Immediate),
        ("translation_window.border_width", Immediate), ("translation_window.border_pattern", Immediate), ("translation_window.border_always", Immediate), ("translation_window.border_seconds", Immediate),
        ("appearance.window.padding", Immediate), ("appearance.window.text_alignment", Immediate), ("appearance.window.text_wrap", Immediate), ("appearance.window.text_outline", Immediate),
        ("appearance.window.outline_color", Immediate), ("appearance.window.line_spacing", Immediate), ("appearance.window.show_original", Immediate), ("appearance.window.original_font_family", Immediate),
        ("appearance.window.original_font_size", Immediate), ("appearance.window.original_color", Immediate), ("translation_window.max_width_enabled", Immediate), ("translation_window.maximum_width", Immediate),
        ("history_enabled", Immediate), ("history_persist", Immediate), ("history_limit", Immediate), ("close_to_tray", Immediate),
        ("translation.auto_translate", Immediate), ("capture.active_region", Immediate), ("capture.allow_multiple_regions", Immediate), ("appearance.window.font_size", Immediate),
        ("translation_window.auto_shrink", Immediate), ("translation_window.opacity", Immediate), ("translation_window.click_through", Immediate), ("translation_window.position", Immediate),
        ("translation_window.size", Immediate), ("capture.frame_color", Immediate), ("capture.frame_width", Immediate), ("capture.frame_seconds", Immediate),
        ("capture.region_frame_mode", Immediate), ("capture.region_frame_pinned", Immediate), ("game_profiles_enabled", Immediate),
    ]
};

/// Reaction of a setting; `None` for a key that does not exist.
pub fn reaction(key: &str) -> Option<Reaction> {
    REACTIONS.iter().find(|(k, _)| *k == key).map(|(_, r)| *r)
}

/// Most games remembered; a new one beyond this is simply not stored.
pub const MAX_GAME_PROFILES: usize = 64;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub auto_translate: bool,
    /// Translate what changed: the text of the translation window is split into paragraphs and only the
    /// paragraphs not translated before are sent; the others come from the cache.
    pub changes_only: bool,
    pub source_language: TranslationSourceLanguage,
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

/// Behaviour of the application itself, not of translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub theme: String,
    /// Start LipaX when the user logs in (an autostart entry in `~/.config/autostart`).
    pub autostart: bool,
    /// A desktop notification when an area stops with an error and automatic retries are over.
    pub notify_errors: bool,
    /// A desktop notification at every failure that will be retried.
    pub notify_retries: bool,
    /// `error`, `warn`, `info` or `debug`; applied at once. `RUST_LOG` wins at start.
    pub log_level: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self { theme: "dark".into(), autostart: false, notify_errors: false, notify_retries: false, log_level: "info".into() }
    }
}

impl GeneralSettings {
    fn sanitize(&mut self) {
        if !THEMES.contains(&self.theme.as_str()) { self.theme = "dark".into(); }
        if !LOG_LEVELS.contains(&self.log_level.as_str()) { self.log_level = "info".into(); }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub history_enabled: bool,
    pub history_persist: bool,
    pub history_limit: usize,
    pub close_to_tray: bool,
    pub general: GeneralSettings,
    pub hotkeys: Hotkeys,
    pub game_profiles_enabled: bool,
    pub game_profiles: BTreeMap<String, GameProfile>,
    pub capture: CaptureSettings,
    pub recognition: TextRecognitionSettings,
    pub translation: TranslationSettings,
    pub translation_window: TranslationWindowSettings,
    pub display_mode: TranslationDisplayMode,
    pub appearance: TranslationAppearance,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            history_enabled: true,
            history_persist: false,
            history_limit: 200,
            close_to_tray: false,
            general: GeneralSettings::default(),
            hotkeys: Hotkeys::default(),
            game_profiles_enabled: true,
            game_profiles: BTreeMap::new(),
            capture: CaptureSettings::default(),
            recognition: TextRecognitionSettings::default(),
            translation: TranslationSettings::default(),
            translation_window: TranslationWindowSettings::default(),
            display_mode: TranslationDisplayMode::Window,
            appearance: TranslationAppearance::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub toggle: String,
    pub select_region: String,
    pub translate_once: String,
    #[serde(alias = "toggle_overlay")]
    pub toggle_translation: String,
    pub toggle_pin: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+P".into(),
            select_region: "Ctrl+Alt+R".into(),
            translate_once: "Ctrl+Alt+Y".into(),
            toggle_translation: "Ctrl+Alt+H".into(),
            toggle_pin: "Ctrl+Alt+U".into(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_default().join("lipa/config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => { tracing::debug!(path = %path.display(), "Настройки прочитаны"); text },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("Файл настроек отсутствует, используются значения по умолчанию");
                String::new()
            },
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "Не удалось прочитать настройки");
                String::new()
            },
        };
        // The next save rewrites the file in this version's layout: keep what a newer one wrote.
        let version = Self::file_schema_version(&text);
        if version > SCHEMA_VERSION {
            let backup = path.with_extension(format!("toml.v{version}.bak"));
            if !backup.exists() && let Err(e) = std::fs::copy(&path, &backup) {
                tracing::error!(path = %backup.display(), error = %e, "Не удалось сохранить копию настроек новой версии");
            }
        }
        let mut settings = Self::from_toml(&text);
        // Migrate the old single rectangle into the first named region.
        if settings.capture.regions.iter().all(|r| r.rect.is_none())
            && let Some(first) = settings.capture.regions.first_mut() { first.rect = settings.capture.region; }
        settings.sanitize();
        settings
    }

    /// Parse a saved config, migrating older formats. A file from a newer LipaX is read as far as
    /// this version understands it (unknown keys are ignored); `load` keeps a copy of it first.
    pub fn from_toml(text: &str) -> Self {
        let parsed = text.parse::<toml::Table>().map_err(|e| e.message().to_owned()).and_then(|mut table| {
            let version = table.get("schema_version").and_then(toml::Value::as_integer).map_or(0, |v| v.clamp(0, i64::from(u32::MAX)) as u32);
            if version > SCHEMA_VERSION {
                tracing::warn!(file = version, supported = SCHEMA_VERSION, "Настройки записаны более новой версией LipaX; неизвестные поля будут потеряны при сохранении");
                table.insert("schema_version".into(), i64::from(SCHEMA_VERSION).into());
            } else {
                migrate(&mut table, version);
            }
            migrate_flat_keys(&mut table);
            table.try_into::<Self>().map_err(|e| e.message().to_owned())
        });
        parsed.unwrap_or_else(|reason| {
            // Do not print the TOML excerpt: configuration may contain API keys.
            tracing::error!(reason, "Некорректный TOML настроек; используются значения по умолчанию");
            Self::default()
        })
    }

    /// Version number written in the file at `path`, if it can be read.
    fn file_schema_version(text: &str) -> u32 {
        text.parse::<toml::Table>().ok().and_then(|t| t.get("schema_version").and_then(toml::Value::as_integer)).map_or(0, |v| v.clamp(0, i64::from(u32::MAX)) as u32)
    }

    pub fn sanitize(&mut self) {
        self.appearance.window.font_size = self.appearance.window.font_size.clamp(8, 96);
        self.recognition.minimum_confidence = self.recognition.minimum_confidence.min(95);
        self.recognition.contrast = self.recognition.contrast.clamp(-100, 100);
        // The translation window uses only fonts shipped with LipaX. Old configurations may
        // name a system font, which must never silently resolve through Qt/fontconfig.
        let bundled = crate::layout::font_database::InstalledFontDatabase::bundled();
        let available = bundled.fonts();
        if !available.iter().any(|font| font.family == self.appearance.window.font_family) {
            self.appearance.window.font_family = available.first().map(|font| font.family.clone()).unwrap_or_else(|| "Inter".into());
        }
        let main = &mut self.appearance.main_window;
        if !available.iter().any(|font| font.family == main.font_family) { main.font_family = "Inter".into(); }
        if !main.cjk_font_family.is_empty() && !available.iter().any(|font| font.family == main.cjk_font_family) { main.cjk_font_family.clear(); }
        main.font_size = main.font_size.clamp(8, 72);
        if !MAIN_WINDOW_BACKGROUNDS.contains(&main.background.as_str()) { main.background = "gray".into(); }
        self.general.sanitize();
        if !self.appearance.window.original_font_family.is_empty()
            && !available.iter().any(|font| font.family == self.appearance.window.original_font_family) {
            self.appearance.window.original_font_family.clear();
        }
        self.appearance.window.original_font_size = self.appearance.window.original_font_size.clamp(8, 96);
        self.appearance.window.line_spacing = if self.appearance.window.line_spacing.is_finite() { self.appearance.window.line_spacing.clamp(0.8, 2.5) } else { 1.0 };
        if !["left", "center", "right"].contains(&self.appearance.window.text_alignment.as_str()) { self.appearance.window.text_alignment = "center".into(); }
        self.translation_window.border_width = self.translation_window.border_width.clamp(1, 16);
        self.translation_window.border_opacity = self.translation_window.border_opacity.clamp(0.0, 1.0);
        self.translation_window.border_seconds = self.translation_window.border_seconds.clamp(1, 120);
        self.capture.frame_seconds = self.capture.frame_seconds.clamp(1, 60);
        self.appearance.inplace.sanitize();
        if self.translation_window.floating_geometry.as_ref().is_some_and(|g| !g.is_valid()) { self.translation_window.floating_geometry = None; }
        if !WINDOW_BACKGROUND_STYLES.contains(&self.appearance.window.background_style.as_str()) { self.appearance.window.background_style = "solid".into(); }
        self.translation_window.corner_radius = self.translation_window.corner_radius.min(32);
        self.translation_window.pinned_corner_radius = self.translation_window.pinned_corner_radius.min(32);
        self.appearance.window.blur_tint = if self.appearance.window.blur_tint.is_finite() { self.appearance.window.blur_tint.clamp(0.0, 0.8) } else { 0.3 };
        self.capture.frame_width = self.capture.frame_width.clamp(1, 12);
        if !REGION_FRAME_MODES.contains(&self.capture.region_frame_mode.as_str()) { self.capture.region_frame_mode = "selection".into(); }
        if !["dim", "hide"].contains(&self.capture.region_frame_pinned.as_str()) { self.capture.region_frame_pinned = "dim".into(); }
        self.translation_window.opacity = self.translation_window.opacity.clamp(0.0, 1.0);
        self.appearance.window.padding = self.appearance.window.padding.min(64);
        // Only sanity bounds: the translation window itself is clamped to the size of its actual screen.
        self.translation_window.maximum_width = self.translation_window.maximum_width.clamp(200, MAX_SCREEN_SIDE);
        self.translation_window.size.0 = self.translation_window.size.0.clamp(200, MAX_SCREEN_SIDE);
        self.translation_window.size.1 = self.translation_window.size.1.clamp(60, MAX_SCREEN_SIDE);
        self.history_limit = self.history_limit.clamp(10, 1000);
        if self.capture.regions.is_empty() { self.capture.regions = default_regions(); }
        self.capture.regions.truncate(MAX_REGIONS);
        let mut ids = std::collections::HashSet::new();
        for (index, r) in self.capture.regions.iter_mut().enumerate() {
            if r.id.is_empty() || !ids.insert(r.id.clone()) { r.id = format!("region-{index}"); ids.insert(r.id.clone()); }
            r.interval_ms = r.interval_ms.clamp(100, 10000);
            r.debounce_ms = r.debounce_ms.min(5000);
            if !r.frame_mode.is_empty() && !REGION_FRAME_MODES.contains(&r.frame_mode.as_str()) { r.frame_mode.clear(); }
            if r.rect.is_some_and(|r| ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) || r.w <= 0.0 || r.h <= 0.0 || r.x < 0.0 || r.y < 0.0 || r.x+r.w > 1.000001 || r.y+r.h > 1.000001) { r.rect = None; }
        }
        if !self.capture.regions.iter().any(|r| r.id == self.capture.active_region) { self.capture.active_region = self.capture.regions[0].id.clone(); }
        if !self.capture.allow_multiple_regions {
            // Keep the active region if it is on, otherwise the first enabled one.
            let keep = self.capture.regions.iter().position(|r| r.enabled && r.id == self.capture.active_region)
                .or_else(|| self.capture.regions.iter().position(|r| r.enabled));
            for (i, r) in self.capture.regions.iter_mut().enumerate() { r.enabled = Some(i) == keep; }
        }
        let defaults = Self::default();
        for (value, fallback) in [(&mut self.appearance.window.text_color, defaults.appearance.window.text_color), (&mut self.appearance.window.background_color, defaults.appearance.window.background_color), (&mut self.translation_window.border_color, defaults.translation_window.border_color), (&mut self.capture.frame_color, defaults.capture.frame_color),
            (&mut self.appearance.window.outline_color, defaults.appearance.window.outline_color), (&mut self.appearance.window.original_color, defaults.appearance.window.original_color)] {
            if value.len() != 7 || !value.starts_with('#') || !value[1..].bytes().all(|b| b.is_ascii_hexdigit()) { *value = fallback; }
        }
    }

    pub fn capture_regions(&self) -> Vec<CaptureRegion> {
        if self.capture.regions.iter().any(|r| r.rect.is_some()) {
            self.capture.regions.iter().filter(|r| r.enabled && r.rect.is_some()).cloned().collect()
        } else {
            self.capture.region.map(|rect| vec![CaptureRegion { rect: Some(rect), interval_ms: self.recognition.interval_ms, debounce_ms: self.recognition.debounce_ms, ..Default::default() }]).unwrap_or_default()
        }
    }

    /// Everything that makes what was recognised so far stale: the `Pipeline` settings and what the
    /// backend selected (window, area). Display-only edits must not interrupt OCR or reset its retry budget.
    pub fn translation_source_language(&self) -> &str {
        match &self.translation.source_language {
            TranslationSourceLanguage::RecognitionLanguage => crate::translate::tess_to_iso(crate::tesseract::primary_lang(&self.recognition.language)),
            TranslationSourceLanguage::Explicit(language) => language,
        }
    }

    /// The confidence below which a reading is rejected: the configured threshold, raised to the CJK floor while a
    /// Chinese, Japanese or Korean model is in use (their garbage on a textured background scores 28–30).
    pub fn effective_minimum_confidence(&self) -> u32 {
        // PaddleOCR reads the first language only; Tesseract (and `auto`, which starts with it) reads all of them.
        let language = match self.recognition.engine.as_str() {
            "paddleocr" => crate::tesseract::primary_lang(&self.recognition.language),
            _ => self.recognition.language.as_str(),
        };
        crate::ocr::filter::effective_min_confidence(language, self.recognition.minimum_confidence)
    }

    pub fn processing_key(&self) -> String {
        let all = serde_json::to_value(self).unwrap_or_default();
        let part: Vec<&serde_json::Value> = REACTIONS.iter()
            .filter(|(key, reaction)| *reaction == Reaction::Pipeline || *key == "capture.window" || *key == "capture.region")
            .filter_map(|(key, _)| value_at(&all, key)).collect();
        serde_json::to_string(&part).unwrap_or_default()
    }

    /// Top-level settings that differ between two states.
    pub fn changed_keys(&self, other: &Self) -> Vec<String> {
        let (a, b) = (serde_json::to_value(self).unwrap_or_default(), serde_json::to_value(other).unwrap_or_default());
        REACTIONS.iter().filter(|(key, _)| value_at(&a, key) != value_at(&b, key)).map(|(key, _)| (*key).to_owned()).collect()
    }

    /// What the user has to do for the changes between `self` and `other` to take full effect;
    /// empty if everything applies at once.
    pub fn pending_actions(&self, other: &Self) -> Vec<&'static str> {
        let mut notes = Vec::new();
        for key in self.changed_keys(other) {
            if reaction(&key) == Some(Reaction::Reselect) {
                notes.push("Способ захвата сменится при следующем выборе окна игры");
            }
        }
        notes.dedup();
        notes
    }

    /// Key of the selected game: its window class. Portal windows have no class of their own.
    pub fn game_key(&self) -> Option<String> {
        let window = self.capture.window.as_ref().filter(|w| !crate::capture::portal::is_portal_window(w))?;
        let class = window.resource_class.trim().to_lowercase();
        (!class.is_empty()).then_some(class)
    }

    fn game_profile(&self, caption: &str) -> GameProfile {
        GameProfile {
            caption: caption.to_owned(),
            display_mode: self.display_mode,
            capture: GameCaptureSettings {
                regions: self.capture.regions.clone(),
                active_region: self.capture.active_region.clone(),
                allow_multiple_regions: self.capture.allow_multiple_regions,
            },
            recognition: GameTextRecognitionSettings {
                language: self.recognition.language.clone(),
                engine: self.recognition.engine.clone(),
                binarize: self.recognition.binarize,
                auto_invert: self.recognition.auto_invert,
                contrast: self.recognition.contrast,
                sharpen: self.recognition.sharpen,
            },
            translation: GameTranslationSettings {
                target_language: self.translation.target_language.clone(),
                source_language: self.translation.source_language.clone(),
            },
            translation_window: GameTranslationWindowSettings {
                mode: self.translation_window.mode,
                screen: self.translation_window.screen.clone(),
                position: self.translation_window.position,
                size: self.translation_window.size,
                floating_geometry: self.translation_window.floating_geometry.clone(),
            },
        }
    }

    /// Store the current state of the selected game. Called after every change, so a game
    /// never needs an explicit «save profile».
    pub fn remember_game(&mut self) {
        let Some(key) = self.game_key().filter(|_| self.game_profiles_enabled) else { return };
        if !self.game_profiles.contains_key(&key) && self.game_profiles.len() >= MAX_GAME_PROFILES {
            tracing::warn!(component = "settings", limit = MAX_GAME_PROFILES, "Слишком много профилей игр: новый не сохранён");
            return;
        }
        let caption = self.capture.window.as_ref().map(|w| w.caption.clone()).unwrap_or_default();
        let profile = self.game_profile(&caption);
        self.game_profiles.insert(key, profile);
    }

    /// Bring back what was remembered for the selected game. `false`: the game is new.
    pub fn apply_game_profile(&mut self) -> bool {
        let Some(profile) = self.game_key().filter(|_| self.game_profiles_enabled).and_then(|k| self.game_profiles.get(&k)).cloned() else { return false };
        self.recognition.language = profile.recognition.language;
        self.translation.target_language = profile.translation.target_language;
        self.translation.source_language = profile.translation.source_language;
        self.recognition.engine = profile.recognition.engine;
        self.recognition.binarize = profile.recognition.binarize;
        self.recognition.auto_invert = profile.recognition.auto_invert;
        self.recognition.contrast = profile.recognition.contrast.clamp(-100, 100);
        self.recognition.sharpen = profile.recognition.sharpen;
        self.capture.regions = profile.capture.regions;
        self.capture.active_region = profile.capture.active_region;
        self.capture.allow_multiple_regions = profile.capture.allow_multiple_regions;
        self.display_mode = profile.display_mode;
        self.translation_window.mode = profile.translation_window.mode;
        self.translation_window.screen = profile.translation_window.screen;
        self.translation_window.position = profile.translation_window.position;
        self.translation_window.size = profile.translation_window.size;
        self.translation_window.floating_geometry = profile.translation_window.floating_geometry;
        true
    }

    pub fn save(&self) -> std::io::Result<()> {
        static SAVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = SAVE.lock().unwrap();
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        // Атомарная запись, чтобы не повредить конфиг при падении.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, s)?;
        // В файле могут лежать API-ключи: доступ только владельцу.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_legacy_settings_and_profiles_keep_new_values_and_legacy_siblings() {
        let s = Settings::from_toml(r#"
schema_version = 1
source_lang = "eng+jpn"
overlay_pinned = true
overlay_pos = [20, 30]
font_size = 25
[recognition]
language = "deu"
[inplace]
shadow = true
fill_opacity = 0.4
[appearance.inplace]
shadow = false
[hotkeys]
toggle_overlay = "Ctrl+Alt+H"
[[regions]]
id = "dialogue"
source_lang = "jpn"
ocr_engine = "paddleocr"
[game_profiles.game]
source_lang = "fra"
target_lang = "de"
overlay_pinned = true
overlay_size = [800, 200]
"#);
        assert_eq!(s.recognition.language, "deu");
        assert_eq!(s.translation_window.mode, TranslationWindowMode::Pinned);
        assert_eq!(s.translation_window.position, (20, 30));
        assert_eq!(s.appearance.window.font_size, 25);
        assert!(!s.appearance.inplace.shadow);
        assert_eq!(s.appearance.inplace.fill_opacity, 0.4);
        assert_eq!(s.capture.regions[0].recognition_language, "jpn");
        assert_eq!(s.capture.regions[0].engine, Some(OcrEngine::PaddleOcr));
        assert_eq!(s.game_profiles["game"].recognition.language, "fra");
        assert_eq!(s.game_profiles["game"].translation_window.size, (800, 200));
        let saved = toml::to_string(&s).unwrap();
        for old in ["overlay_pinned", "source_lang =", "target_lang =", "ocr_engine", "toggle_overlay"] {
            assert!(!saved.contains(old), "legacy name written: {old}");
        }
        assert_eq!(Settings::from_toml(&saved), s);
    }

    #[test]
    fn translation_language_inherits_recognition_or_remembers_an_explicit_override() {
        let mut s = Settings::default();
        s.recognition.language = "jpn+eng".into();
        assert_eq!(s.translation_source_language(), "ja");
        let inherited = s.processing_key();
        s.translation.source_language = TranslationSourceLanguage::Explicit("fr".into());
        assert_eq!(s.translation_source_language(), "fr");
        assert_ne!(s.processing_key(), inherited);
        assert_eq!(s.recognition.language, "jpn+eng");
        s.capture.window = game("game", "u");
        s.remember_game();
        s.translation.source_language = TranslationSourceLanguage::RecognitionLanguage;
        assert!(s.apply_game_profile());
        assert_eq!(s.translation_source_language(), "fr");
    }

    fn game(class: &str, uuid: &str) -> Option<WindowKey> {
        Some(WindowKey { uuid: uuid.into(), resource_class: class.into(), caption: format!("{class} window") })
    }

    #[test]
    fn general_and_main_window_settings_are_checked_and_old_files_get_defaults() {
        let s = Settings::from_toml("");
        assert_eq!((s.general.theme.as_str(), s.general.log_level.as_str(), s.general.autostart), ("dark", "info", false));
        let m = &s.appearance.main_window;
        assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Inter", "", 16, "gray"));
        let mut bad = Settings::default();
        bad.general.theme = "neon".into();
        bad.general.log_level = "loud".into();
        bad.appearance.main_window = MainWindowAppearance { font_family: "Some System Font".into(), cjk_font_family: "Another".into(), font_size: 500, background: "plaid".into() };
        bad.sanitize();
        assert_eq!((bad.general.theme.as_str(), bad.general.log_level.as_str()), ("dark", "info"));
        let m = &bad.appearance.main_window;
        assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Inter", "", 72, "gray"), "a system font is never used");
        let mut good = Settings::default();
        good.general.theme = "light".into();
        good.general.log_level = "debug".into();
        good.appearance.main_window = MainWindowAppearance { font_family: "Lora".into(), cjk_font_family: "Noto Sans CJK SC".into(), font_size: 24, background: "system".into() };
        good.sanitize();
        let m = &good.appearance.main_window;
        assert_eq!((good.general.theme.as_str(), good.general.log_level.as_str()), ("light", "debug"));
        assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Lora", "Noto Sans CJK SC", 24, "system"));
    }

    #[test]
    fn the_cjk_floor_follows_the_language_the_engine_reads() {
        let mut s = Settings::default();
        s.recognition.language = "eng+jpn".into();
        assert_eq!(s.effective_minimum_confidence(), 38, "Tesseract reads both");
        s.recognition.engine = "paddleocr".into();
        assert_eq!(s.effective_minimum_confidence(), 30, "PaddleOCR reads only English here");
        s.recognition.language = "jpn+eng".into();
        assert_eq!(s.effective_minimum_confidence(), 38);
    }

    #[test]
    fn ocr_filters_belong_to_the_game() {
        let mut s = { let mut value = Settings::default(); value.capture.window = game("dark.exe", "u1"); value };
        s.recognition.auto_invert = true;
        s.recognition.contrast = 30;
        s.remember_game();
        // A light game next: no filters, and it keeps them off.
        s.capture.window = game("light.exe", "u2");
        s.recognition.auto_invert = false;
        s.recognition.contrast = 0;
        s.remember_game();
        s.capture.window = game("dark.exe", "u1");
        assert!(s.apply_game_profile());
        assert!(s.recognition.auto_invert && s.recognition.contrast == 30 && !s.recognition.binarize);
        s.capture.window = game("light.exe", "u2");
        assert!(s.apply_game_profile());
        assert!(!s.recognition.auto_invert && s.recognition.contrast == 0);
    }

    #[test]
    fn a_game_gets_back_its_areas_languages_and_window() {
        let rect = NormRect { x: 0.1, y: 0.7, w: 0.8, h: 0.2 };
        let mut s = { let mut value = Settings::default(); value.capture.window = game("Witcher3.exe", "u1"); value };
        s.capture.regions[0].rect = Some(rect);
        s.recognition.language = "jpn".into();
        s.translation_window.position = (40, 50);
        s.remember_game();
        assert_eq!(s.game_profiles.len(), 1);
        assert_eq!(s.game_profiles["witcher3.exe"].caption, "Witcher3.exe window");

        // Another game starts clean...
        s.capture.window = game("Other", "u2");
        s.capture.regions[0].rect = None;
        s.recognition.language = "eng".into();
        assert!(!s.apply_game_profile(), "unknown game");
        s.remember_game();
        assert_eq!(s.game_profiles.len(), 2);

        // ...and the first one comes back, even from a new window (new UUID, other letter case).
        s.capture.window = game("witcher3.EXE", "u3");
        assert!(s.apply_game_profile());
        assert_eq!((s.capture.regions[0].rect, s.recognition.language.as_str(), s.translation_window.position), (Some(rect), "jpn", (40, 50)));
    }

    #[test]
    fn profiles_ignore_portal_windows_and_can_be_turned_off() {
        let mut s = { let mut value = Settings::default(); value.capture.window = Some(crate::capture::portal::portal_window_key()); value };
        s.remember_game();
        assert!(s.game_profiles.is_empty(), "a portal window has no class of its own");
        s.capture.window = game("Game", "u1");
        s.game_profiles_enabled = false;
        s.remember_game();
        assert!(s.game_profiles.is_empty());
        assert!(!s.apply_game_profile());
        s.capture.window = game("", "u2");
        s.game_profiles_enabled = true;
        s.remember_game();
        assert!(s.game_profiles.is_empty(), "no class, no key");
    }

    #[test]
    fn profile_count_is_capped_and_existing_ones_still_update() {
        let mut s = Settings::default();
        for i in 0..MAX_GAME_PROFILES + 5 {
            s.capture.window = game(&format!("game{i}"), "u");
            s.remember_game();
        }
        assert_eq!(s.game_profiles.len(), MAX_GAME_PROFILES);
        s.capture.window = game("game0", "u");
        s.translation.target_language = "de".into();
        s.remember_game();
        assert_eq!(s.game_profiles["game0"].translation.target_language, "de");
    }

    #[test]
    fn profiles_survive_the_config_file() {
        let mut s = { let mut value = Settings::default(); value.capture.window = game("Game", "u1"); value };
        s.capture.regions[0].rect = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 });
        s.remember_game();
        let saved = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::from_toml(&saved).game_profiles, s.game_profiles);
        assert!(Settings::from_toml("").game_profiles_enabled, "on by default, also for old configs");
    }

    #[test]
    fn an_old_config_is_lifted_step_by_step_and_stamped() {
        // Version 0: no stamp, frame_seconds = 0 meant "never".
        let old = Settings::from_toml("frame_seconds = 0\ntarget_lang = \"de\"");
        assert_eq!((old.schema_version, old.capture.region_frame_mode.as_str(), old.translation.target_language.as_str()), (SCHEMA_VERSION, "off", "de"));
        // The same text already stamped with the current version is taken as it is.
        let current = Settings::from_toml("schema_version = 1\nframe_seconds = 0");
        assert_ne!(current.capture.region_frame_mode, "off");
        // A new config always carries its version.
        let saved = toml::to_string_pretty(&Settings::default()).unwrap();
        assert!(saved.contains(&format!("schema_version = {SCHEMA_VERSION}")));
    }

    #[test]
    fn a_config_from_a_newer_version_is_read_not_rejected() {
        let s = Settings::from_toml("schema_version = 99\ntarget_lang = \"fr\"\nsome_future_key = true");
        assert_eq!((s.translation.target_language.as_str(), s.schema_version), ("fr", SCHEMA_VERSION));
        assert_eq!(Settings::file_schema_version("schema_version = 99"), 99);
        assert_eq!(Settings::file_schema_version("target_lang = \"fr\""), 0);
        assert_eq!(Settings::file_schema_version("not toml ["), 0);
    }

    #[test]
    fn a_broken_config_falls_back_to_defaults() {
        assert_eq!(Settings::from_toml("this is [not toml"), Settings::default());
        assert_eq!(Settings::from_toml("target_lang = 5"), Settings::default(), "a wrong type is not a reason to crash");
    }

    #[test]
    fn every_setting_declares_what_it_needs() {
        let all = serde_json::to_value(Settings::default()).unwrap();
        let declared: Vec<&str> = REACTIONS.iter().map(|(k, _)| *k).collect();
        fn visit(value: &serde_json::Value, path: &str, declared: &[&str]) {
            if declared.contains(&path) { return; }
            let object = value.as_object().unwrap_or_else(|| panic!("setting without a reaction: {path}"));
            assert!(!object.is_empty(), "empty setting without a reaction: {path}");
            for (key, child) in object {
                visit(child, &if path.is_empty() { key.clone() } else { format!("{path}.{key}") }, declared);
            }
        }
        visit(&all, "", &declared);
        for path in &declared { assert!(value_at(&all, path).is_some(), "stale reaction: {path}"); }
        let mut sorted = declared.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), declared.len(), "a setting is declared twice");
    }

    #[test]
    fn only_pipeline_settings_and_the_selection_reset_the_pipeline() {
        let base = Settings::default();
        let key = base.processing_key();
        for change in [
            |s: &mut Settings| s.translation.target_language = "de".into(),
            |s: &mut Settings| s.recognition.language = "jpn".into(),
            |s: &mut Settings| s.recognition.engine = "auto".into(),
            |s: &mut Settings| s.recognition.minimum_confidence = 50,
            |s: &mut Settings| s.recognition.binarize = true,
            |s: &mut Settings| s.recognition.contrast = 20,
            |s: &mut Settings| s.recognition.interval_ms += 100,
            |s: &mut Settings| s.display_mode = TranslationDisplayMode::Inplace,
            |s: &mut Settings| s.capture.regions[0].enabled = false,
            |s: &mut Settings| s.capture.window = Some(WindowKey { uuid: "u".into(), resource_class: "c".into(), caption: "t".into() }),
        ] {
            let mut changed = base.clone();
            change(&mut changed);
            assert_ne!(changed.processing_key(), key);
        }
        for change in [
            |s: &mut Settings| s.appearance.window.font_size += 1,
            |s: &mut Settings| s.translation_window.mode = TranslationWindowMode::from_pinned(!s.translation_window.mode.is_pinned()),
            |s: &mut Settings| s.close_to_tray = true,
            |s: &mut Settings| s.history_limit += 1,
            |s: &mut Settings| s.hotkeys.toggle = "Ctrl+Alt+Q".into(),
            |s: &mut Settings| s.capture.source = CaptureSource::Portal,
            |s: &mut Settings| s.game_profiles_enabled = false,
            |s: &mut Settings| s.appearance.inplace.fill_opacity = 0.5,
        ] {
            let mut changed = base.clone();
            change(&mut changed);
            assert_eq!(changed.processing_key(), key, "an appearance or behaviour edit must not reset the pipeline");
        }
    }

    #[test]
    fn changed_keys_and_pending_actions() {
        let a = Settings::default();
        let mut b = a.clone();
        b.appearance.window.font_size += 2;
        assert_eq!(a.changed_keys(&b), ["appearance.window.font_size"]);
        assert!(a.pending_actions(&b).is_empty(), "applies at once");
        b.capture.source = CaptureSource::Portal;
        assert_eq!(a.changed_keys(&b), ["capture.source", "appearance.window.font_size"]);
        assert_eq!(a.pending_actions(&b), ["Способ захвата сменится при следующем выборе окна игры"]);
        assert_eq!(reaction("hotkeys.toggle"), Some(Reaction::Hotkeys));
        assert_eq!(reaction("nonsense"), None);
    }

    #[test]
    fn roundtrip() {
        let s = { let mut value = Settings::default(); value.capture.region = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 }); value };
        let t = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&t).unwrap(), s);
    }

    #[test]
    fn pinned_window_radius_is_saved_and_bounded() {
        let mut s = { let mut value = Settings::default(); value.translation_window.pinned_corner_radius = 18; value };
        let saved = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::from_toml(&saved).translation_window.pinned_corner_radius, 18);
        s.translation_window.pinned_corner_radius = 100;
        s.sanitize();
        assert_eq!(s.translation_window.pinned_corner_radius, 32);
        s.translation_window.pinned_corner_radius = 0;
        s.sanitize();
        assert_eq!(s.translation_window.pinned_corner_radius, 0);
    }

    #[test]
    fn old_system_font_falls_back_to_a_bundled_family() {
        let mut settings = Settings::from_toml("font_family = 'DejaVu Sans'\noriginal_font_family = 'Liberation Serif'\noverlay_auto_shrink = false");
        settings.sanitize();
        assert_eq!(settings.appearance.window.font_family, "Inter");
        assert!(settings.appearance.window.original_font_family.is_empty(), "original follows the bundled translation font");
        assert!(!settings.translation_window.auto_shrink);
        let restored = Settings::from_toml(&toml::to_string(&settings).unwrap());
        assert_eq!(restored.appearance.window.font_family, "Inter");
        assert!(!restored.translation_window.auto_shrink);
    }

    #[test]
    fn legacy_background_modes_migrate_to_four_public_choices() {
        for (old, expected) in [("inpaint_blur", InplaceBackgroundMode::TextReplacement),
            ("solid_fill", InplaceBackgroundMode::TextReplacement),
            ("adaptive_padding_fill", InplaceBackgroundMode::PaddedFill),
            ("transparent", InplaceBackgroundMode::TransparentOutline)] {
            let value: InplaceBackgroundMode = serde_json::from_value(serde_json::json!(old)).unwrap();
            assert_eq!(value, expected);
        }
    }

    fn region(id: &str, enabled: bool) -> CaptureRegion {
        CaptureRegion { id: id.into(), name: id.into(), enabled, ..Default::default() }
    }

    #[test]
    fn regions_are_limited_and_single_active_by_default() {
        let mut s = Settings::default();
        assert_eq!(s.capture.regions.len(), 1);
        s.capture.regions = ["a", "b", "c", "d"].map(|id| region(id, true)).to_vec();
        s.capture.active_region = "b".into();
        s.sanitize();
        assert_eq!(s.capture.regions.len(), MAX_REGIONS, "a fourth region is dropped");
        let active: Vec<_> = s.capture.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
        assert_eq!(active, ["b"], "only the active region stays on");

        s.capture.active_region = "c".into();
        s.capture.regions[2].enabled = false;
        s.capture.regions[0].enabled = true;
        s.sanitize();
        let active: Vec<_> = s.capture.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
        assert_eq!(active, ["a"], "an inactive target falls back to the first enabled region");
    }

    #[test]
    fn several_active_regions_when_allowed() {
        let mut s = { let mut value = Settings::default(); value.capture.allow_multiple_regions = true; value };
        s.capture.regions = ["a", "b", "c"].map(|id| region(id, true)).to_vec();
        s.sanitize();
        assert_eq!(s.capture.regions.iter().filter(|r| r.enabled).count(), 3);
    }

    #[test]
    fn region_frame_defaults_and_migration() {
        let s = Settings::default();
        assert!(!s.translation_window.border_pattern, "the error pattern is off by default");
        assert_eq!((s.capture.region_frame_mode.as_str(), s.capture.frame_seconds), ("selection", 3));
        assert_eq!(Settings::from_toml("frame_seconds = 0").capture.region_frame_mode, "off", "old 'never show' is kept");
        assert_eq!(Settings::from_toml("frame_seconds = 0\nregion_frame_mode = \"solid\"").capture.region_frame_mode, "solid");
        let mut bad = { let mut value = Settings::default(); value.capture.region_frame_mode = "blink".into(); value };
        bad.capture.regions[0].frame_mode = "blink".into();
        bad.sanitize();
        assert_eq!(bad.capture.region_frame_mode, "selection");
        assert_eq!(bad.capture.regions[0].frame_mode, "", "unknown per-region mode falls back to the global one");
    }

    #[test]
    fn inplace_properties_are_independent_and_roundtrip() {
        let mut s = Settings::default();
        assert_eq!(s.appearance.inplace.font_family, PropertyMode::Auto);
        s.appearance.inplace.font_family = PropertyMode::Manual("PT Serif".into());
        s.appearance.inplace.letter_spacing = PropertyMode::Manual(1.5);
        s.appearance.inplace.padding = PropertyMode::Manual(crate::layout::Padding::uniform(4.0));
        s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
        s.appearance.inplace.font_overrides.insert("serif".into(), "Noto Serif".into());
        let t = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&t).unwrap();
        assert_eq!(back.appearance.inplace, s.appearance.inplace);
        assert_eq!(back.appearance.inplace.font_size, PropertyMode::Auto, "other properties stay automatic");
        let json = serde_json::to_value(&s.appearance.inplace).unwrap();
        assert_eq!(json["font_family"], serde_json::json!({"mode": "manual", "value": "PT Serif"}));
        assert_eq!(json["font_size"], serde_json::json!({"mode": "auto"}));

        let mut bad = Settings::default();
        bad.appearance.inplace.text_color = PropertyMode::Manual("red".into());
        bad.appearance.inplace.minimum_font_size = 50.0;
        bad.appearance.inplace.maximum_font_size = 10.0;
        bad.sanitize();
        assert_eq!(bad.appearance.inplace.text_color, PropertyMode::Auto);
        assert!(bad.appearance.inplace.maximum_font_size >= bad.appearance.inplace.minimum_font_size);
    }

    #[test]
    fn translation_display_reads_old_and_unknown_values() {
        assert_eq!(Settings::from_toml("translation_display = \"overlay\"").display_mode, TranslationDisplayMode::Window);
        assert_eq!(Settings::from_toml("translation_display = \"inplace\"").display_mode, TranslationDisplayMode::Inplace);
        let odd = Settings::from_toml("translation_display = \"hologram\"\ntarget_lang = \"de\"");
        assert_eq!((odd.display_mode, odd.translation.target_language.as_str()), (TranslationDisplayMode::Window, "de"), "the rest of the config survives");
        assert_eq!(serde_json::to_value(TranslationDisplayMode::Inplace).unwrap(), "inplace");
    }

    #[test]
    fn partial_config_uses_defaults() {
        let s = Settings::from_toml("target_lang = \"de\"");
        assert_eq!(s.translation.target_language, "de");
        assert_eq!(s.recognition.interval_ms, 500);
    }
}
