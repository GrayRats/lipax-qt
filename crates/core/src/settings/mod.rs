use crate::layout::{FontWeight, Padding, TextAlignment, WrapMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod inplace;
mod migrate;
mod profile;
mod reaction;
mod sections;
mod types;
pub use inplace::*;
pub use profile::*;
pub use reaction::*;
pub use sections::*;
pub use types::*;
use migrate::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_typography_tests;


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

/// Behaviour of the application itself, not of translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub theme: String,
    /// UI typography only; does not alter translation fonts.
    pub ui_text_scale: String,
    /// Start LipaX when the user logs in (an autostart entry in `~/.config/autostart`).
    pub autostart: bool,
    /// A desktop notification when an area stops with an error and automatic retries are over.
    pub notify_errors: bool,
    /// A desktop notification at every failure that will be retried.
    pub notify_retries: bool,
    /// `error`, `warn`, `info` or `debug`; applied at once. `RUST_LOG` wins at start.
    pub log_level: String,
    /// Remember the position, size and screen of the main and the translation window. When off, nothing is
    /// saved and the saved geometry is neither applied nor overwritten.
    pub remember_window_geometry: bool,
    /// Where the main window was left (set by the application, never by the settings form).
    pub main_window: Option<WindowRecord>,
}

/// A top-level window as it was left: logical desktop pixels, the screen it was on and that screen's geometry
/// at that time (to tell a moved or rescaled monitor from the same one). Position is advisory: Wayland
/// compositors may refuse to place a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WindowRecord {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub screen: String,
    pub screen_x: f64,
    pub screen_y: f64,
    pub screen_w: f64,
    pub screen_h: f64,
    pub scale: f64,
}

impl WindowRecord {
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.w, self.h, self.screen_x, self.screen_y, self.screen_w, self.screen_h, self.scale].iter().all(|v| v.is_finite())
            && self.w >= 1.0 && self.h >= 1.0 && self.w <= 65536.0 && self.h <= 65536.0
    }
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self { theme: "dark".into(), ui_text_scale: "normal".into(), autostart: false, notify_errors: false, notify_retries: false, log_level: "info".into(),
            remember_window_geometry: true, main_window: None }
    }
}

impl GeneralSettings {
    fn sanitize(&mut self) {
        if !["small", "normal", "large", "extra_large"].contains(&self.ui_text_scale.as_str()) { self.ui_text_scale = "normal".into(); }
        if !THEMES.contains(&self.theme.as_str()) { self.theme = "dark".into(); }
        if !LOG_LEVELS.contains(&self.log_level.as_str()) { self.log_level = "info".into(); }
        if self.main_window.as_ref().is_some_and(|g| !g.is_valid()) { self.main_window = None; }
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
        let mut settings = match Self::try_from_toml(&text) {
            Ok(settings) => settings,
            Err(reason) => {
                // The next save overwrites the file: keep what the user wrote (API keys, profiles) beside it.
                let kept = crate::files::keep_copy(&path, "broken");
                // Do not print the TOML excerpt: configuration may contain API keys.
                tracing::error!(reason, copy = ?kept, "Некорректный TOML настроек; используются значения по умолчанию");
                Self::default()
            }
        };
        // Migrate the old single rectangle into the first named region.
        if settings.capture.regions.iter().all(|r| r.rect.is_none())
            && let Some(first) = settings.capture.regions.first_mut() { first.rect = settings.capture.region; }
        settings.sanitize();
        settings
    }

    /// Parse a saved config, migrating older formats. A file from a newer LipaX is read as far as
    /// this version understands it (unknown keys are ignored); `load` keeps a copy of it first.
    pub fn from_toml(text: &str) -> Self {
        Self::try_from_toml(text).unwrap_or_else(|reason| {
            // Do not print the TOML excerpt: configuration may contain API keys.
            tracing::error!(reason, "Некорректный TOML настроек; используются значения по умолчанию");
            Self::default()
        })
    }

    /// As [`Settings::from_toml`], but a file that cannot be understood is an error (the message of the parser).
    pub fn try_from_toml(text: &str) -> Result<Self, String> {
        text.parse::<toml::Table>().map_err(|e| e.message().to_owned()).and_then(|mut table| {
            let version = table.get("schema_version").and_then(toml::Value::as_integer).map_or(0, |v| v.clamp(0, i64::from(u32::MAX)) as u32);
            if version > SCHEMA_VERSION {
                tracing::warn!(file = version, supported = SCHEMA_VERSION, "Настройки записаны более новой версией LipaX; неизвестные поля будут потеряны при сохранении");
                table.insert("schema_version".into(), i64::from(SCHEMA_VERSION).into());
            } else {
                migrate(&mut table, version);
            }
            migrate_flat_keys(&mut table);
            table.try_into::<Self>().map_err(|e| e.message().to_owned())
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
        self.recognition.rapid_threads = self.recognition.rapid_threads.min(16);
        if !["mobile", "server"].contains(&self.recognition.rapid_variant.as_str()) { self.recognition.rapid_variant = "mobile".into(); }
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
        // PaddleOCR, RapidOCR and MeikiOCR read the first language only; Tesseract (and `auto`, which starts with it) reads all of them.
        let language = match self.recognition.engine.as_str() {
            "paddleocr" | "rapidocr" | "meikiocr" => crate::tesseract::primary_lang(&self.recognition.language),
            _ => self.recognition.language.as_str(),
        };
        let floor = crate::ocr::filter::effective_min_confidence(language, self.recognition.minimum_confidence);
        // PP-OCR is sure of its garbage more than Tesseract: its own pipelines drop lines below 0.5 (`drop_score`).
        if self.recognition.engine == OcrEngine::RapidOcr && floor > 0 { floor.max(crate::ocr::filter::RAPID_MIN_CONFIDENCE) } else { floor }
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
    /// Proton and Wine give many games the same class (`steam_app_default`, `steam_app_<id>`, `wine`), so there the
    /// program of the window joins the key (`steam_app_default|crossxmars.exe`): it stays the same when the title
    /// shows a score or a level. Without a known program a Proton class takes the normalised caption, as before 1.4.2
    /// (`steam_app_default|cross x mars`).
    pub fn game_key(&self) -> Option<String> {
        let window = self.capture.window.as_ref().filter(|w| !crate::capture::portal::is_portal_window(w))?;
        let class = window.resource_class.trim().to_lowercase();
        if class.is_empty() { return None; }
        let program = window.program.trim().to_lowercase();
        if shared_class(&class) && !program.is_empty() { return Some(format!("{class}|{program}")); }
        self.caption_game_key()
    }

    /// The key of 1.4.1 and earlier (class, with the caption for Proton classes). A profile stored under it is moved to
    /// [`Self::game_key`] when the game is met with its program known.
    fn caption_game_key(&self) -> Option<String> {
        let window = self.capture.window.as_ref().filter(|w| !crate::capture::portal::is_portal_window(w))?;
        let class = window.resource_class.trim().to_lowercase();
        if class.is_empty() { return None; }
        if class.starts_with("steam_app_") {
            let caption = window.caption.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
            if !caption.is_empty() { return Some(format!("{class}|{caption}")); }
        }
        Some(class)
    }

    /// Moves the profile of the selected game from its pre-1.4.2 key (by caption) to the key by program, if only the
    /// old one exists. A profile keyed by a bare shared class (`wine`) may belong to several games: it is copied, not
    /// moved.
    fn migrate_game_profile(&mut self) {
        let (Some(key), Some(old)) = (self.game_key(), self.caption_game_key()) else { return };
        if key == old || self.game_profiles.contains_key(&key) { return; }
        let profile = if old.contains('|') { self.game_profiles.remove(&old) } else { self.game_profiles.get(&old).cloned() };
        if let Some(profile) = profile {
            tracing::info!(component = "settings", from = %old, to = %key, "Профиль игры перенесён на ключ по программе");
            self.game_profiles.insert(key, profile);
        }
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
        if self.game_profiles_enabled { self.migrate_game_profile(); }
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
        if self.game_profiles_enabled { self.migrate_game_profile(); }
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
        let _guard = SAVE.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        // Атомарная запись (в файле могут лежать API-ключи: доступ только владельцу с первого байта).
        crate::files::write_atomic(&path, s.as_bytes(), 0o600)
    }
}

