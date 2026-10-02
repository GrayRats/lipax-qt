use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
pub struct Settings {
    pub source_lang: String,
    pub target_lang: String,
    pub ocr_engine: String,
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
    /// Порог change detection, средняя разница яркости 0..255.
    pub sensitivity: f32,
    pub debounce_ms: u64,
    pub auto_translate: bool,
    pub overlay_mode: OverlayMode,
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
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+P".into(),
            select_region: "Ctrl+Alt+R".into(),
            translate_once: "Ctrl+Alt+Y".into(),
            toggle_overlay: "Ctrl+Alt+H".into(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            source_lang: "eng".into(),
            target_lang: "ru".into(),
            ocr_engine: "tesseract".into(),
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
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
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

    #[test]
    fn partial_config_uses_defaults() {
        let s: Settings = toml::from_str("target_lang = \"de\"").unwrap();
        assert_eq!(s.target_lang, "de");
        assert_eq!(s.interval_ms, 500);
    }
}
