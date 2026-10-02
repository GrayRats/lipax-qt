use crate::layout::{FontWeight, Padding, TextAlignment, WrapMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
    /// По фону вокруг текста: однородный — заливка, сложный — восстановление с размытием.
    #[default]
    Auto,
    InpaintBlur,
    SolidFill,
    AdaptivePaddingFill,
    Transparent,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub source_lang: String,
    pub target_lang: String,
    pub ocr_engine: String,
    pub paddle_python: String,
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
    pub interval_ms: u64,
    /// Чувствительность детектора смены текста: порог контраста краёв букв 40 + 8·value (ниже — чувствительнее).
    pub sensitivity: f32,
    pub debounce_ms: u64,
    pub auto_translate: bool,
    pub overlay_mode: OverlayMode,
    pub overlay_pinned: bool,
    /// "overlay": translation window; "inplace": translation drawn over the original text.
    pub translation_display: String,
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
    pub regions: Vec<RegionProfile>,
    /// Region that "select area" targets; without `allow_multiple_regions` it is the only active one.
    pub active_region: String,
    /// Off: activating a region deactivates the others.
    pub allow_multiple_regions: bool,
    pub font_size: u32,
    pub opacity: f64,
    pub click_through: bool,
    pub overlay_pos: (i32, i32),
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
            source_lang: "eng".into(),
            target_lang: "ru".into(),
            ocr_engine: "tesseract".into(),
            paddle_python: "python3".into(),
            overlay_screen: String::new(),
            translator: TranslatorKind::Google,
            yandex_api_key: String::new(),
            yandex_folder_id: String::new(),
            custom_url: String::new(),
            custom_api_key: String::new(),
            capture_backend: CaptureBackendKind::Auto,
            portal_token: String::new(),
            interval_ms: 500,
            sensitivity: 2.0,
            debounce_ms: 400,
            auto_translate: true,
            overlay_mode: OverlayMode::Overlay,
            overlay_pinned: false,
            translation_display: "overlay".into(),
            inplace: InplaceSettings::default(),
            overlay_style: "solid".into(),
            blur_enabled: true,
            blur_tint: 0.3,
            dim_inverse: false,
            overlay_corner_radius: 12,
            font_family: String::new(),
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
            regions: default_regions(),
            active_region: "subtitles".into(),
            allow_multiple_regions: false,
            font_size: 20,
            opacity: 0.85,
            click_through: true,
            overlay_pos: (100, 100),
            overlay_size: (700, 120),
            hotkeys: Hotkeys::default(),
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

impl Settings {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_default().join("lipa/config.toml")
    }

    pub fn load() -> Self {
        let text = std::fs::read_to_string(Self::path()).unwrap_or_default();
        let mut settings = Self::from_toml(&text);
        // Migrate the old single rectangle into the first named region.
        if settings.regions.iter().all(|r| r.rect.is_none())
            && let Some(first) = settings.regions.first_mut() { first.rect = settings.region; }
        settings.sanitize();
        settings
    }

    /// Parse a saved config, migrating older formats.
    pub fn from_toml(text: &str) -> Self {
        let mut settings: Self = toml::from_str(text).unwrap_or_default();
        // Before frame modes, `frame_seconds = 0` meant "never show the frame".
        if settings.frame_seconds == 0 && !text.contains("region_frame_mode") {
            settings.region_frame_mode = "off".into();
        }
        settings
    }

    pub fn sanitize(&mut self) {
        self.font_size = self.font_size.clamp(8, 96);
        self.original_font_size = self.original_font_size.clamp(8, 96);
        self.line_spacing = if self.line_spacing.is_finite() { self.line_spacing.clamp(0.8, 2.5) } else { 1.0 };
        if !["left", "center", "right"].contains(&self.text_alignment.as_str()) { self.text_alignment = "center".into(); }
        self.border_width = self.border_width.clamp(1, 16);
        self.border_opacity = self.border_opacity.clamp(0.0, 1.0);
        self.border_seconds = self.border_seconds.clamp(1, 120);
        self.frame_seconds = self.frame_seconds.clamp(1, 60);
        if !["overlay", "inplace"].contains(&self.translation_display.as_str()) { self.translation_display = "overlay".into(); }
        self.inplace.sanitize();
        if !OVERLAY_STYLES.contains(&self.overlay_style.as_str()) { self.overlay_style = "solid".into(); }
        self.overlay_corner_radius = self.overlay_corner_radius.min(32);
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

    /// Display-only edits must not interrupt OCR or reset its retry budget.
    pub fn processing_key(&self) -> String {
        serde_json::json!([self.window, self.region, self.regions, self.source_lang, self.target_lang,
            self.ocr_engine, self.paddle_python, self.translator, self.custom_url, self.custom_api_key,
            self.yandex_api_key, self.yandex_folder_id, self.interval_ms, self.debounce_ms, self.sensitivity,
            // Switching to "over the original" re-reads the text so its layout is known.
            self.translation_display]).to_string()
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
    fn roundtrip() {
        let mut s = Settings::default();
        s.region = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 });
        let t = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&t).unwrap(), s);
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
        s.inplace.background_mode = InplaceBackgroundMode::InpaintBlur;
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
    fn partial_config_uses_defaults() {
        let s: Settings = toml::from_str("target_lang = \"de\"").unwrap();
        assert_eq!(s.target_lang, "de");
        assert_eq!(s.interval_ms, 500);
    }
}
