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
pub enum TranslationDisplay {
    /// Окно перевода (`TranslationOverlay.qml`): закреплённое или свободное.
    #[default]
    Window,
    /// Перевод поверх найденных полей текста (`InplaceText.qml`).
    Inplace,
}

impl<'de> Deserialize<'de> for TranslationDisplay {
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

/// Чем закрывается оригинал под переводом. Не связано с фоном окна перевода (`overlay_style`).
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
        }
    }
}

impl InplaceSettings {
    fn sanitize(&mut self) {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
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
pub enum CaptureBackendKind {
    /// KWin, а если он недоступен — portal.
    Auto,
    Kwin,
    Portal,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslatorKind {
    Google,
    Yandex,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayMode {
    Overlay,
    Window,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegionProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub rect: Option<NormRect>,
    /// Empty values inherit the global OCR/translation settings.
    pub source_lang: String,
    pub target_lang: String,
    pub ocr_engine: String,
    pub interval_ms: u64,
    pub debounce_ms: u64,
    /// Outline of this region in the game; empty inherits `Settings::region_frame_mode`.
    pub frame_mode: String,
}
impl Default for RegionProfile {
    fn default() -> Self {
        Self { id: "subtitles".into(), name: "Субтитры".into(), enabled: true, rect: None,
            source_lang: String::new(), target_lang: String::new(), ocr_engine: String::new(),
            interval_ms: 500, debounce_ms: 400, frame_mode: String::new() }
    }
}

/// How the translation overlay draws its background.
pub const OVERLAY_STYLES: [&str; 4] = [
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
pub fn default_regions() -> Vec<RegionProfile> {
    vec![RegionProfile::default()]
}

/// Version of the config layout. A file without `schema_version` is version 0 (everything
/// written before versions existed). Each step in [`migrate`] lifts a file by exactly one version.
pub const SCHEMA_VERSION: u32 = 1;

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
        ("schema_version", Managed), ("portal_token", Managed), ("game_profiles", Managed), ("floating_geometry", Managed),
        ("window", Managed), ("region", Managed),
        // Recognition and translation: the pipeline starts over.
        ("source_lang", Pipeline), ("target_lang", Pipeline), ("ocr_engine", Pipeline), ("paddle_python", Pipeline),
        ("ocr_min_confidence", Pipeline), ("translator", Pipeline), ("yandex_api_key", Pipeline), ("yandex_folder_id", Pipeline),
        ("custom_url", Pipeline), ("portal_fills_monitor", Pipeline), ("custom_api_key", Pipeline), ("interval_ms", Pipeline), ("sensitivity", Pipeline),
        ("debounce_ms", Pipeline), ("translation_display", Pipeline), ("regions", Pipeline),
        ("inplace", Layout),
        ("hotkeys", Hotkeys),
        // The backend is chosen by the window key at selection time.
        ("capture_backend", Reselect),
        ("overlay_screen", Immediate), ("overlay_mode", Immediate), ("overlay_pinned", Immediate), ("overlay_style", Immediate),
        ("blur_enabled", Immediate), ("blur_tint", Immediate), ("dim_inverse", Immediate), ("overlay_corner_radius", Immediate),
        ("overlay_pinned_corner_radius", Immediate), ("font_family", Immediate), ("font_bold", Immediate), ("font_italic", Immediate),
        ("text_color", Immediate), ("background_color", Immediate), ("border_color", Immediate), ("border_opacity", Immediate),
        ("border_width", Immediate), ("border_pattern", Immediate), ("border_always", Immediate), ("border_seconds", Immediate),
        ("overlay_padding", Immediate), ("text_alignment", Immediate), ("text_wrap", Immediate), ("text_outline", Immediate),
        ("outline_color", Immediate), ("line_spacing", Immediate), ("show_original", Immediate), ("original_font_family", Immediate),
        ("original_font_size", Immediate), ("original_color", Immediate), ("max_width_enabled", Immediate), ("overlay_max_width", Immediate),
        ("history_enabled", Immediate), ("history_persist", Immediate), ("history_limit", Immediate), ("close_to_tray", Immediate),
        ("auto_translate", Immediate), ("active_region", Immediate), ("allow_multiple_regions", Immediate), ("font_size", Immediate),
        ("overlay_auto_shrink", Immediate), ("opacity", Immediate), ("click_through", Immediate), ("overlay_pos", Immediate),
        ("overlay_size", Immediate), ("frame_color", Immediate), ("frame_width", Immediate), ("frame_seconds", Immediate),
        ("region_frame_mode", Immediate), ("region_frame_pinned", Immediate), ("game_profiles_enabled", Immediate),
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
    /// Window title when the profile was last saved: only to recognise it in the list.
    pub caption: String,
    pub source_lang: String,
    pub target_lang: String,
    pub ocr_engine: String,
    pub regions: Vec<RegionProfile>,
    pub active_region: String,
    pub allow_multiple_regions: bool,
    pub translation_display: TranslationDisplay,
    pub overlay_pinned: bool,
    pub overlay_screen: String,
    pub overlay_pos: (i32, i32),
    pub overlay_size: (u32, u32),
    pub floating_geometry: Option<FloatingGeometry>,
}

impl Default for GameProfile {
    fn default() -> Self { Settings::default().game_profile("") }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Layout of the config file, see [`SCHEMA_VERSION`]; managed by the backend.
    pub schema_version: u32,
    pub source_lang: String,
    pub target_lang: String,
    /// "tesseract", "paddleocr" or "auto" (Tesseract first, PaddleOCR when Tesseract is unsure).
    pub ocr_engine: String,
    pub paddle_python: String,
    /// Text the engine itself is less sure about than this (0–100) is ignored instead of translated;
    /// 0 turns the check off. Applies only to engines that report confidence.
    pub ocr_min_confidence: u32,
    /// Empty: follow the selected game; otherwise a Qt screen name.
    pub overlay_screen: String,
    pub translator: TranslatorKind,
    /// Yandex Cloud Translate v2: API-ключ сервисного аккаунта и ID каталога.
    pub yandex_api_key: String,
    pub yandex_folder_id: String,
    /// Свой API (формат LibreTranslate): POST {q, source, target, format, api_key} -> {translatedText}.
    pub custom_url: String,
    pub custom_api_key: String,
    pub capture_backend: CaptureBackendKind,
    /// Токен восстановления xdg-desktop-portal: повторный запуск без диалога выбора окна.
    pub portal_token: String,
    /// The window chosen through the portal fills a whole monitor (a fullscreen game): the portal does not
    /// say where a window is, so the monitor stands for it, and the translation can go over the original.
    pub portal_fills_monitor: bool,
    pub interval_ms: u64,
    /// Чувствительность детектора смены текста: порог контраста краёв букв 40 + 8·value (ниже — чувствительнее).
    pub sensitivity: f32,
    pub debounce_ms: u64,
    pub auto_translate: bool,
    pub overlay_mode: OverlayMode,
    pub overlay_pinned: bool,
    /// "overlay": translation window; "inplace": translation drawn over the original text.
    pub translation_display: TranslationDisplay,
    pub inplace: InplaceSettings,
    /// One of `OVERLAY_STYLES`.
    pub overlay_style: String,
    /// "blur" style: compositor blur behind the overlay and the opacity of its dark tint (0–0.8).
    pub blur_enabled: bool,
    pub blur_tint: f64,
    /// "dim" style: light background with dark text instead of dark with white.
    pub dim_inverse: bool,
    /// Corner radius of the unpinned (floating) translation window, px.
    pub overlay_corner_radius: u32,
    /// Corner radius of the pinned layer-shell translation window, px.
    pub overlay_pinned_corner_radius: u32,
    pub font_family: String,
    pub font_bold: bool,
    pub font_italic: bool,
    pub text_color: String,
    pub background_color: String,
    pub border_color: String,
    pub border_opacity: f64,
    pub border_width: u32,
    pub border_pattern: bool,
    /// false: the frame shows for `border_seconds` after a region is selected (and while unpinned).
    pub border_always: bool,
    pub border_seconds: u32,
    pub overlay_padding: u32,
    pub text_alignment: String,
    pub text_wrap: bool,
    pub text_outline: bool,
    pub outline_color: String,
    /// Line height multiplier for the translated text.
    pub line_spacing: f64,
    /// Show the recognized original above the translation.
    pub show_original: bool,
    pub original_font_family: String,
    pub original_font_size: u32,
    pub original_color: String,
    pub max_width_enabled: bool,
    pub overlay_max_width: u32,
    pub history_enabled: bool,
    pub history_persist: bool,
    pub history_limit: usize,
    /// Closing the main window hides it to the system tray instead of quitting; the capture
    /// and translation keep running. Off: closing the main window quits the application.
    pub close_to_tray: bool,
    pub regions: Vec<RegionProfile>,
    /// Region that "select area" targets; without `allow_multiple_regions` it is the only active one.
    pub active_region: String,
    /// Off: activating a region deactivates the others.
    pub allow_multiple_regions: bool,
    pub font_size: u32,
    /// Shrink translated text in the translation window until it fits, but never below 14 px.
    pub overlay_auto_shrink: bool,
    pub opacity: f64,
    pub click_through: bool,
    /// Закреплённое окно: положение относительно экрана `overlay_screen`.
    pub overlay_pos: (i32, i32),
    /// Свободное окно: где пользователь оставил его в последний раз.
    pub floating_geometry: Option<FloatingGeometry>,
    pub overlay_size: (u32, u32),
    pub hotkeys: Hotkeys,
    /// Рамка вокруг выбранного окна и областей: цвет `#rrggbb` простой обводки, толщина в пикселях
    /// и время показа в режиме «при выделении».
    pub frame_color: String,
    pub frame_width: u32,
    pub frame_seconds: u32,
    /// One of `REGION_FRAME_MODES`.
    pub region_frame_mode: String,
    /// Region outlines while the translation is pinned: "dim" (semi-transparent) or "hide".
    pub region_frame_pinned: String,
    pub window: Option<WindowKey>,
    pub region: Option<NormRect>,
    /// Remember the areas, languages, engine and translation position of every game and bring
    /// them back when its window is chosen again.
    pub game_profiles_enabled: bool,
    /// Keyed by lower-case window class; managed by the backend (`remember_game`).
    pub game_profiles: BTreeMap<String, GameProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub toggle: String,
    pub select_region: String,
    pub translate_once: String,
    pub toggle_overlay: String,
    pub toggle_pin: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+P".into(),
            select_region: "Ctrl+Alt+R".into(),
            translate_once: "Ctrl+Alt+Y".into(),
            toggle_overlay: "Ctrl+Alt+H".into(),
            toggle_pin: "Ctrl+Alt+U".into(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            source_lang: "eng".into(),
            target_lang: "ru".into(),
            ocr_engine: "tesseract".into(),
            ocr_min_confidence: 30,
            paddle_python: "python3".into(),
            overlay_screen: String::new(),
            translator: TranslatorKind::Google,
            yandex_api_key: String::new(),
            yandex_folder_id: String::new(),
            custom_url: String::new(),
            custom_api_key: String::new(),
            capture_backend: CaptureBackendKind::Auto,
            portal_token: String::new(),
            portal_fills_monitor: false,
            interval_ms: 500,
            sensitivity: 2.0,
            debounce_ms: 400,
            auto_translate: true,
            overlay_mode: OverlayMode::Overlay,
            overlay_pinned: false,
            translation_display: TranslationDisplay::Window,
            inplace: InplaceSettings::default(),
            overlay_style: "solid".into(),
            blur_enabled: true,
            blur_tint: 0.3,
            dim_inverse: false,
            overlay_corner_radius: 12,
            overlay_pinned_corner_radius: 0,
            font_family: "Inter".into(),
            font_bold: false,
            font_italic: false,
            text_color: "#ffffff".into(),
            background_color: "#181818".into(),
            border_color: "#ff00ff".into(),
            border_opacity: 0.65,
            border_width: 2,
            border_pattern: false,
            border_always: true,
            border_seconds: 5,
            overlay_padding: 16,
            text_alignment: "center".into(),
            text_wrap: true,
            text_outline: true,
            outline_color: "#000000".into(),
            line_spacing: 1.0,
            show_original: false,
            original_font_family: String::new(),
            original_font_size: 14,
            original_color: "#b0b0b0".into(),
            max_width_enabled: true,
            overlay_max_width: 900,
            history_enabled: true,
            history_persist: false,
            history_limit: 200,
            close_to_tray: false,
            regions: default_regions(),
            active_region: "subtitles".into(),
            allow_multiple_regions: false,
            font_size: 20,
            overlay_auto_shrink: true,
            opacity: 0.85,
            click_through: true,
            overlay_pos: (100, 100),
            floating_geometry: None,
            overlay_size: (700, 120),
            hotkeys: Hotkeys::default(),
            frame_color: "#ff0000".into(),
            frame_width: 2,
            frame_seconds: 3,
            region_frame_mode: "selection".into(),
            region_frame_pinned: "dim".into(),
            window: None,
            region: None,
            game_profiles_enabled: true,
            game_profiles: BTreeMap::new(),
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
        if settings.regions.iter().all(|r| r.rect.is_none())
            && let Some(first) = settings.regions.first_mut() { first.rect = settings.region; }
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
        self.font_size = self.font_size.clamp(8, 96);
        self.ocr_min_confidence = self.ocr_min_confidence.min(95);
        // The translation window uses only fonts shipped with LipaX. Old configurations may
        // name a system font, which must never silently resolve through Qt/fontconfig.
        let bundled = crate::layout::font_database::InstalledFontDatabase::bundled();
        let available = bundled.fonts();
        if !available.iter().any(|font| font.family == self.font_family) {
            self.font_family = available.first().map(|font| font.family.clone()).unwrap_or_else(|| "Inter".into());
        }
        if !self.original_font_family.is_empty()
            && !available.iter().any(|font| font.family == self.original_font_family) {
            self.original_font_family.clear();
        }
        self.original_font_size = self.original_font_size.clamp(8, 96);
        self.line_spacing = if self.line_spacing.is_finite() { self.line_spacing.clamp(0.8, 2.5) } else { 1.0 };
        if !["left", "center", "right"].contains(&self.text_alignment.as_str()) { self.text_alignment = "center".into(); }
        self.border_width = self.border_width.clamp(1, 16);
        self.border_opacity = self.border_opacity.clamp(0.0, 1.0);
        self.border_seconds = self.border_seconds.clamp(1, 120);
        self.frame_seconds = self.frame_seconds.clamp(1, 60);
        self.inplace.sanitize();
        if self.floating_geometry.as_ref().is_some_and(|g| !g.is_valid()) { self.floating_geometry = None; }
        if !OVERLAY_STYLES.contains(&self.overlay_style.as_str()) { self.overlay_style = "solid".into(); }
        self.overlay_corner_radius = self.overlay_corner_radius.min(32);
        self.overlay_pinned_corner_radius = self.overlay_pinned_corner_radius.min(32);
        self.blur_tint = if self.blur_tint.is_finite() { self.blur_tint.clamp(0.0, 0.8) } else { 0.3 };
        self.frame_width = self.frame_width.clamp(1, 12);
        if !REGION_FRAME_MODES.contains(&self.region_frame_mode.as_str()) { self.region_frame_mode = "selection".into(); }
        if !["dim", "hide"].contains(&self.region_frame_pinned.as_str()) { self.region_frame_pinned = "dim".into(); }
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.overlay_padding = self.overlay_padding.min(64);
        // Only sanity bounds: the overlay itself is clamped to the size of its actual screen.
        self.overlay_max_width = self.overlay_max_width.clamp(200, MAX_SCREEN_SIDE);
        self.overlay_size.0 = self.overlay_size.0.clamp(200, MAX_SCREEN_SIDE);
        self.overlay_size.1 = self.overlay_size.1.clamp(60, MAX_SCREEN_SIDE);
        self.history_limit = self.history_limit.clamp(10, 1000);
        if self.regions.is_empty() { self.regions = default_regions(); }
        self.regions.truncate(MAX_REGIONS);
        let mut ids = std::collections::HashSet::new();
        for (index, r) in self.regions.iter_mut().enumerate() {
            if r.id.is_empty() || !ids.insert(r.id.clone()) { r.id = format!("region-{index}"); ids.insert(r.id.clone()); }
            r.interval_ms = r.interval_ms.clamp(100, 10000);
            r.debounce_ms = r.debounce_ms.min(5000);
            if !r.frame_mode.is_empty() && !REGION_FRAME_MODES.contains(&r.frame_mode.as_str()) { r.frame_mode.clear(); }
            if r.rect.is_some_and(|r| ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) || r.w <= 0.0 || r.h <= 0.0 || r.x < 0.0 || r.y < 0.0 || r.x+r.w > 1.000001 || r.y+r.h > 1.000001) { r.rect = None; }
        }
        if !self.regions.iter().any(|r| r.id == self.active_region) { self.active_region = self.regions[0].id.clone(); }
        if !self.allow_multiple_regions {
            // Keep the active region if it is on, otherwise the first enabled one.
            let keep = self.regions.iter().position(|r| r.enabled && r.id == self.active_region)
                .or_else(|| self.regions.iter().position(|r| r.enabled));
            for (i, r) in self.regions.iter_mut().enumerate() { r.enabled = Some(i) == keep; }
        }
        let defaults = Self::default();
        for (value, fallback) in [(&mut self.text_color, defaults.text_color), (&mut self.background_color, defaults.background_color), (&mut self.border_color, defaults.border_color), (&mut self.frame_color, defaults.frame_color),
            (&mut self.outline_color, defaults.outline_color), (&mut self.original_color, defaults.original_color)] {
            if value.len() != 7 || !value.starts_with('#') || !value[1..].bytes().all(|b| b.is_ascii_hexdigit()) { *value = fallback; }
        }
    }

    pub fn capture_regions(&self) -> Vec<RegionProfile> {
        if self.regions.iter().any(|r| r.rect.is_some()) {
            self.regions.iter().filter(|r| r.enabled && r.rect.is_some()).cloned().collect()
        } else {
            self.region.map(|rect| vec![RegionProfile { rect: Some(rect), interval_ms: self.interval_ms, debounce_ms: self.debounce_ms, ..Default::default() }]).unwrap_or_default()
        }
    }

    /// Everything that makes what was recognised so far stale: the `Pipeline` settings and what the
    /// backend selected (window, area). Display-only edits must not interrupt OCR or reset its retry budget.
    pub fn processing_key(&self) -> String {
        let all = serde_json::to_value(self).unwrap_or_default();
        let part: Vec<&serde_json::Value> = REACTIONS.iter()
            .filter(|(key, reaction)| *reaction == Reaction::Pipeline || *key == "window" || *key == "region")
            .filter_map(|(key, _)| all.get(*key)).collect();
        serde_json::to_string(&part).unwrap_or_default()
    }

    /// Top-level settings that differ between two states.
    pub fn changed_keys(&self, other: &Self) -> Vec<String> {
        let (a, b) = (serde_json::to_value(self).unwrap_or_default(), serde_json::to_value(other).unwrap_or_default());
        REACTIONS.iter().filter(|(key, _)| a.get(*key) != b.get(*key)).map(|(key, _)| (*key).to_owned()).collect()
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
        let window = self.window.as_ref().filter(|w| !crate::capture::portal::is_portal_window(w))?;
        let class = window.resource_class.trim().to_lowercase();
        (!class.is_empty()).then_some(class)
    }

    fn game_profile(&self, caption: &str) -> GameProfile {
        GameProfile {
            caption: caption.to_owned(),
            source_lang: self.source_lang.clone(),
            target_lang: self.target_lang.clone(),
            ocr_engine: self.ocr_engine.clone(),
            regions: self.regions.clone(),
            active_region: self.active_region.clone(),
            allow_multiple_regions: self.allow_multiple_regions,
            translation_display: self.translation_display,
            overlay_pinned: self.overlay_pinned,
            overlay_screen: self.overlay_screen.clone(),
            overlay_pos: self.overlay_pos,
            overlay_size: self.overlay_size,
            floating_geometry: self.floating_geometry.clone(),
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
        let caption = self.window.as_ref().map(|w| w.caption.clone()).unwrap_or_default();
        let profile = self.game_profile(&caption);
        self.game_profiles.insert(key, profile);
    }

    /// Bring back what was remembered for the selected game. `false`: the game is new.
    pub fn apply_game_profile(&mut self) -> bool {
        let Some(profile) = self.game_key().filter(|_| self.game_profiles_enabled).and_then(|k| self.game_profiles.get(&k)).cloned() else { return false };
        self.source_lang = profile.source_lang;
        self.target_lang = profile.target_lang;
        self.ocr_engine = profile.ocr_engine;
        self.regions = profile.regions;
        self.active_region = profile.active_region;
        self.allow_multiple_regions = profile.allow_multiple_regions;
        self.translation_display = profile.translation_display;
        self.overlay_pinned = profile.overlay_pinned;
        self.overlay_screen = profile.overlay_screen;
        self.overlay_pos = profile.overlay_pos;
        self.overlay_size = profile.overlay_size;
        self.floating_geometry = profile.floating_geometry;
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

    fn game(class: &str, uuid: &str) -> Option<WindowKey> {
        Some(WindowKey { uuid: uuid.into(), resource_class: class.into(), caption: format!("{class} window") })
    }

    #[test]
    fn a_game_gets_back_its_areas_languages_and_overlay() {
        let rect = NormRect { x: 0.1, y: 0.7, w: 0.8, h: 0.2 };
        let mut s = Settings { window: game("Witcher3.exe", "u1"), ..Settings::default() };
        s.regions[0].rect = Some(rect);
        s.source_lang = "jpn".into();
        s.overlay_pos = (40, 50);
        s.remember_game();
        assert_eq!(s.game_profiles.len(), 1);
        assert_eq!(s.game_profiles["witcher3.exe"].caption, "Witcher3.exe window");

        // Another game starts clean...
        s.window = game("Other", "u2");
        s.regions[0].rect = None;
        s.source_lang = "eng".into();
        assert!(!s.apply_game_profile(), "unknown game");
        s.remember_game();
        assert_eq!(s.game_profiles.len(), 2);

        // ...and the first one comes back, even from a new window (new UUID, other letter case).
        s.window = game("witcher3.EXE", "u3");
        assert!(s.apply_game_profile());
        assert_eq!((s.regions[0].rect, s.source_lang.as_str(), s.overlay_pos), (Some(rect), "jpn", (40, 50)));
    }

    #[test]
    fn profiles_ignore_portal_windows_and_can_be_turned_off() {
        let mut s = Settings { window: Some(crate::capture::portal::portal_window_key()), ..Settings::default() };
        s.remember_game();
        assert!(s.game_profiles.is_empty(), "a portal window has no class of its own");
        s.window = game("Game", "u1");
        s.game_profiles_enabled = false;
        s.remember_game();
        assert!(s.game_profiles.is_empty());
        assert!(!s.apply_game_profile());
        s.window = game("", "u2");
        s.game_profiles_enabled = true;
        s.remember_game();
        assert!(s.game_profiles.is_empty(), "no class, no key");
    }

    #[test]
    fn profile_count_is_capped_and_existing_ones_still_update() {
        let mut s = Settings::default();
        for i in 0..MAX_GAME_PROFILES + 5 {
            s.window = game(&format!("game{i}"), "u");
            s.remember_game();
        }
        assert_eq!(s.game_profiles.len(), MAX_GAME_PROFILES);
        s.window = game("game0", "u");
        s.target_lang = "de".into();
        s.remember_game();
        assert_eq!(s.game_profiles["game0"].target_lang, "de");
    }

    #[test]
    fn profiles_survive_the_config_file() {
        let mut s = Settings { window: game("Game", "u1"), ..Settings::default() };
        s.regions[0].rect = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 });
        s.remember_game();
        let saved = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::from_toml(&saved).game_profiles, s.game_profiles);
        assert!(Settings::from_toml("").game_profiles_enabled, "on by default, also for old configs");
    }

    #[test]
    fn an_old_config_is_lifted_step_by_step_and_stamped() {
        // Version 0: no stamp, frame_seconds = 0 meant "never".
        let old = Settings::from_toml("frame_seconds = 0\ntarget_lang = \"de\"");
        assert_eq!((old.schema_version, old.region_frame_mode.as_str(), old.target_lang.as_str()), (SCHEMA_VERSION, "off", "de"));
        // The same text already stamped with the current version is taken as it is.
        let current = Settings::from_toml("schema_version = 1\nframe_seconds = 0");
        assert_ne!(current.region_frame_mode, "off");
        // A new config always carries its version.
        let saved = toml::to_string_pretty(&Settings::default()).unwrap();
        assert!(saved.contains(&format!("schema_version = {SCHEMA_VERSION}")));
    }

    #[test]
    fn a_config_from_a_newer_version_is_read_not_rejected() {
        let s = Settings::from_toml("schema_version = 99\ntarget_lang = \"fr\"\nsome_future_key = true");
        assert_eq!((s.target_lang.as_str(), s.schema_version), ("fr", SCHEMA_VERSION));
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
        let keys: Vec<&str> = all.as_object().unwrap().keys().map(String::as_str).collect();
        let declared: Vec<&str> = REACTIONS.iter().map(|(k, _)| *k).collect();
        let missing: Vec<&&str> = keys.iter().filter(|k| !declared.contains(k)).collect();
        let stale: Vec<&&str> = declared.iter().filter(|k| !keys.contains(k)).collect();
        assert!(missing.is_empty(), "settings without a declared reaction (add them to REACTIONS): {missing:?}");
        assert!(stale.is_empty(), "REACTIONS names settings that do not exist: {stale:?}");
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
            |s: &mut Settings| s.target_lang = "de".into(),
            |s: &mut Settings| s.source_lang = "jpn".into(),
            |s: &mut Settings| s.ocr_engine = "auto".into(),
            |s: &mut Settings| s.ocr_min_confidence = 50,
            |s: &mut Settings| s.interval_ms += 100,
            |s: &mut Settings| s.translation_display = TranslationDisplay::Inplace,
            |s: &mut Settings| s.regions[0].enabled = false,
            |s: &mut Settings| s.window = Some(WindowKey { uuid: "u".into(), resource_class: "c".into(), caption: "t".into() }),
        ] {
            let mut changed = base.clone();
            change(&mut changed);
            assert_ne!(changed.processing_key(), key);
        }
        for change in [
            |s: &mut Settings| s.font_size += 1,
            |s: &mut Settings| s.overlay_pinned = !s.overlay_pinned,
            |s: &mut Settings| s.close_to_tray = true,
            |s: &mut Settings| s.history_limit += 1,
            |s: &mut Settings| s.hotkeys.toggle = "Ctrl+Alt+Q".into(),
            |s: &mut Settings| s.capture_backend = CaptureBackendKind::Portal,
            |s: &mut Settings| s.game_profiles_enabled = false,
            |s: &mut Settings| s.inplace.fill_opacity = 0.5,
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
        b.font_size += 2;
        assert_eq!(a.changed_keys(&b), ["font_size"]);
        assert!(a.pending_actions(&b).is_empty(), "applies at once");
        b.capture_backend = CaptureBackendKind::Portal;
        assert_eq!(a.changed_keys(&b), ["capture_backend", "font_size"]);
        assert_eq!(a.pending_actions(&b), ["Способ захвата сменится при следующем выборе окна игры"]);
        assert_eq!(reaction("hotkeys"), Some(Reaction::Hotkeys));
        assert_eq!(reaction("nonsense"), None);
    }

    #[test]
    fn roundtrip() {
        let s = Settings { region: Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 }), ..Settings::default() };
        let t = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&t).unwrap(), s);
    }

    #[test]
    fn pinned_overlay_radius_is_saved_and_bounded() {
        let mut s = Settings { overlay_pinned_corner_radius: 18, ..Settings::default() };
        let saved = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::from_toml(&saved).overlay_pinned_corner_radius, 18);
        s.overlay_pinned_corner_radius = 100;
        s.sanitize();
        assert_eq!(s.overlay_pinned_corner_radius, 32);
        s.overlay_pinned_corner_radius = 0;
        s.sanitize();
        assert_eq!(s.overlay_pinned_corner_radius, 0);
    }

    #[test]
    fn old_system_font_falls_back_to_a_bundled_family() {
        let mut settings = Settings::from_toml("font_family = 'DejaVu Sans'\noriginal_font_family = 'Liberation Serif'\noverlay_auto_shrink = false");
        settings.sanitize();
        assert_eq!(settings.font_family, "Inter");
        assert!(settings.original_font_family.is_empty(), "original follows the bundled translation font");
        assert!(!settings.overlay_auto_shrink);
        let restored = Settings::from_toml(&toml::to_string(&settings).unwrap());
        assert_eq!(restored.font_family, "Inter");
        assert!(!restored.overlay_auto_shrink);
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

    fn region(id: &str, enabled: bool) -> RegionProfile {
        RegionProfile { id: id.into(), name: id.into(), enabled, ..Default::default() }
    }

    #[test]
    fn regions_are_limited_and_single_active_by_default() {
        let mut s = Settings::default();
        assert_eq!(s.regions.len(), 1);
        s.regions = ["a", "b", "c", "d"].map(|id| region(id, true)).to_vec();
        s.active_region = "b".into();
        s.sanitize();
        assert_eq!(s.regions.len(), MAX_REGIONS, "a fourth region is dropped");
        let active: Vec<_> = s.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
        assert_eq!(active, ["b"], "only the active region stays on");

        s.active_region = "c".into();
        s.regions[2].enabled = false;
        s.regions[0].enabled = true;
        s.sanitize();
        let active: Vec<_> = s.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
        assert_eq!(active, ["a"], "an inactive target falls back to the first enabled region");
    }

    #[test]
    fn several_active_regions_when_allowed() {
        let mut s = Settings { allow_multiple_regions: true, ..Settings::default() };
        s.regions = ["a", "b", "c"].map(|id| region(id, true)).to_vec();
        s.sanitize();
        assert_eq!(s.regions.iter().filter(|r| r.enabled).count(), 3);
    }

    #[test]
    fn region_frame_defaults_and_migration() {
        let s = Settings::default();
        assert!(!s.border_pattern, "the error pattern is off by default");
        assert_eq!((s.region_frame_mode.as_str(), s.frame_seconds), ("selection", 3));
        assert_eq!(Settings::from_toml("frame_seconds = 0").region_frame_mode, "off", "old 'never show' is kept");
        assert_eq!(Settings::from_toml("frame_seconds = 0\nregion_frame_mode = \"solid\"").region_frame_mode, "solid");
        let mut bad = Settings { region_frame_mode: "blink".into(), ..Settings::default() };
        bad.regions[0].frame_mode = "blink".into();
        bad.sanitize();
        assert_eq!(bad.region_frame_mode, "selection");
        assert_eq!(bad.regions[0].frame_mode, "", "unknown per-region mode falls back to the global one");
    }

    #[test]
    fn inplace_properties_are_independent_and_roundtrip() {
        let mut s = Settings::default();
        assert_eq!(s.inplace.font_family, PropertyMode::Auto);
        s.inplace.font_family = PropertyMode::Manual("PT Serif".into());
        s.inplace.letter_spacing = PropertyMode::Manual(1.5);
        s.inplace.padding = PropertyMode::Manual(crate::layout::Padding::uniform(4.0));
        s.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
        s.inplace.font_overrides.insert("serif".into(), "Noto Serif".into());
        let t = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&t).unwrap();
        assert_eq!(back.inplace, s.inplace);
        assert_eq!(back.inplace.font_size, PropertyMode::Auto, "other properties stay automatic");
        let json = serde_json::to_value(&s.inplace).unwrap();
        assert_eq!(json["font_family"], serde_json::json!({"mode": "manual", "value": "PT Serif"}));
        assert_eq!(json["font_size"], serde_json::json!({"mode": "auto"}));

        let mut bad = Settings::default();
        bad.inplace.text_color = PropertyMode::Manual("red".into());
        bad.inplace.minimum_font_size = 50.0;
        bad.inplace.maximum_font_size = 10.0;
        bad.sanitize();
        assert_eq!(bad.inplace.text_color, PropertyMode::Auto);
        assert!(bad.inplace.maximum_font_size >= bad.inplace.minimum_font_size);
    }

    #[test]
    fn translation_display_reads_old_and_unknown_values() {
        assert_eq!(Settings::from_toml("translation_display = \"overlay\"").translation_display, TranslationDisplay::Window);
        assert_eq!(Settings::from_toml("translation_display = \"inplace\"").translation_display, TranslationDisplay::Inplace);
        let odd = Settings::from_toml("translation_display = \"hologram\"\ntarget_lang = \"de\"");
        assert_eq!((odd.translation_display, odd.target_lang.as_str()), (TranslationDisplay::Window, "de"), "the rest of the config survives");
        assert_eq!(serde_json::to_value(TranslationDisplay::Inplace).unwrap(), "inplace");
    }

    #[test]
    fn partial_config_uses_defaults() {
        let s: Settings = toml::from_str("target_lang = \"de\"").unwrap();
        assert_eq!(s.target_lang, "de");
        assert_eq!(s.interval_ms, 500);
    }
}
