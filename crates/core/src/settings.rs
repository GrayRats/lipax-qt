use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}
impl Default for RegionProfile {
    fn default() -> Self {
        Self { id: "subtitles".into(), name: "Субтитры".into(), enabled: true, rect: None,
            source_lang: String::new(), target_lang: String::new(), ocr_engine: String::new(),
            interval_ms: 500, debounce_ms: 400 }
    }
}

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
    /// Рамка вокруг выбранного окна/области: цвет `#rrggbb`, толщина в пикселях, секунд показа (0 — не показывать).
    pub frame_color: String,
    pub frame_width: u32,
    pub frame_seconds: u32,
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
            font_family: String::new(),
            font_bold: false,
            font_italic: false,
            text_color: "#ffffff".into(),
            background_color: "#181818".into(),
            border_color: "#ff00ff".into(),
            border_opacity: 0.65,
            border_width: 2,
            border_pattern: true,
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
        let mut settings: Self = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        // Migrate the old single rectangle into the first named region.
        if settings.regions.iter().all(|r| r.rect.is_none()) {
            if let Some(first) = settings.regions.first_mut() { first.rect = settings.region; }
        }
        settings.sanitize();
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
        for (value, fallback) in [(&mut self.text_color, defaults.text_color), (&mut self.background_color, defaults.background_color), (&mut self.border_color, defaults.border_color),
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
            self.yandex_api_key, self.yandex_folder_id, self.interval_ms, self.debounce_ms, self.sensitivity]).to_string()
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
    fn partial_config_uses_defaults() {
        let s: Settings = toml::from_str("target_lang = \"de\"").unwrap();
        assert_eq!(s.target_lang, "de");
        assert_eq!(s.interval_ms, 500);
    }
}
