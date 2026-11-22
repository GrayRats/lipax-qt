//! What a changed setting needs in order to take effect.

use super::*;

/// What a changed setting needs in order to take effect. Every key of [`Settings`] is declared in
/// [`REACTIONS`] (a test fails when one is missing or stale), so no setting is saved without a
/// known way of reaching the running application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Reaction {
    /// Read live by the windows or the backend: nothing to restart.
    Immediate,
    /// Qt Quick Controls style is selected before loading QML; palette-only changes may be live.
    ApplicationStyle,
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
        ("schema_version", Managed), ("capture.portal_token", Managed), ("game_profiles", Managed), ("translation_window.floating_geometry", Managed), ("general.main_window", Managed),
        ("capture.window", Managed), ("capture.region", Managed),
        // Recognition and translation: the pipeline starts over.
        ("recognition.language", Pipeline), ("translation.target_language", Pipeline), ("translation.source_language", Pipeline), ("recognition.engine", Pipeline), ("recognition.paddle_python", Pipeline),
        ("recognition.rapid_threads", Pipeline), ("recognition.rapid_use_gpu", Pipeline), ("recognition.rapid_variant", Pipeline),
        ("recognition.minimum_confidence", Pipeline), ("recognition.binarize", Pipeline), ("recognition.auto_invert", Pipeline), ("recognition.contrast", Pipeline), ("recognition.sharpen", Pipeline), ("recognition.filter_noise", Pipeline), ("recognition.auto_filters", Pipeline), ("translation.service", Pipeline), ("translation.yandex_api_key", Pipeline), ("translation.yandex_folder_id", Pipeline),
        ("translation.custom_url", Pipeline), ("capture.portal_fills_monitor", Pipeline), ("translation.custom_api_key", Pipeline), ("translation.deepl_api_key", Pipeline), ("translation.microsoft_api_key", Pipeline), ("translation.microsoft_region", Pipeline), ("translation.bergamot_binary", Pipeline), ("translation.bergamot_models_dir", Pipeline), ("translation.bergamot_model_paths", Pipeline), ("recognition.interval_ms", Pipeline), ("recognition.sensitivity", Pipeline),
        ("recognition.debounce_ms", Pipeline), ("display_mode", Pipeline), ("capture.regions", Pipeline),
        ("appearance.inplace.background_mode", Layout), ("appearance.inplace.font_family", Layout), ("appearance.inplace.font_size", Layout), ("appearance.inplace.font_weight", Layout), ("appearance.inplace.italic", Layout), ("appearance.inplace.line_height", Layout), ("appearance.inplace.letter_spacing", Layout), ("appearance.inplace.alignment", Layout), ("appearance.inplace.wrap_mode", Layout), ("appearance.inplace.text_color", Layout), ("appearance.inplace.outline_color", Layout), ("appearance.inplace.outline_width", Layout), ("appearance.inplace.shadow", Layout), ("appearance.inplace.text_opacity", Layout), ("appearance.inplace.fill_color", Layout), ("appearance.inplace.fill_opacity", Layout), ("appearance.inplace.padding_x", Layout), ("appearance.inplace.padding_y", Layout), ("appearance.inplace.extra_margin", Layout), ("appearance.inplace.corner_radius", Layout), ("appearance.inplace.padding", Layout), ("appearance.inplace.minimum_font_size", Layout), ("appearance.inplace.maximum_font_size", Layout), ("appearance.inplace.allow_condensed_fallback", Layout), ("appearance.inplace.font_overrides", Layout), ("appearance.inplace.preferred_fonts", Layout), ("appearance.inplace.line_gap_factor", Pipeline), ("appearance.inplace.max_blocks", Pipeline), ("appearance.inplace.only_when_active", Immediate), ("appearance.inplace.click_through", Immediate), ("appearance.inplace.skip_controls", Pipeline), ("appearance.inplace.skip_names", Pipeline), ("appearance.inplace.extend_up", Pipeline), ("general.theme", ApplicationStyle), ("general.language", Immediate), ("general.keys_in_wallet", Immediate), ("general.ui_text_scale", Immediate), ("general.autostart", Immediate), ("general.remember_window_geometry", Immediate), ("general.notify_errors", Immediate), ("general.notify_retries", Immediate), ("general.log_level", Immediate), ("appearance.main_window.font_family", Immediate), ("appearance.main_window.cjk_font_family", Immediate), ("appearance.main_window.font_size", Immediate), ("appearance.main_window.background", Immediate), ("translation.changes_only", Pipeline),
        ("hotkeys.gamepad.enabled", Hotkeys), ("hotkeys.gamepad.toggle", Hotkeys), ("hotkeys.gamepad.select_region", Hotkeys), ("hotkeys.gamepad.translate_once", Hotkeys), ("hotkeys.gamepad.toggle_translation", Hotkeys), ("hotkeys.gamepad.toggle_pin", Hotkeys), ("hotkeys.toggle", Hotkeys), ("hotkeys.select_region", Hotkeys), ("hotkeys.translate_once", Hotkeys), ("hotkeys.toggle_translation", Hotkeys), ("hotkeys.toggle_pin", Hotkeys),
        // The backend is chosen by the window key at selection time.
        ("capture.source", Reselect),
        ("translation_window.screen", Pipeline), ("translation_window.mode", Immediate), ("appearance.window.background_style", Immediate),
        ("appearance.window.blur_enabled", Immediate), ("appearance.window.blur_tint", Immediate), ("appearance.window.dim_inverse", Immediate), ("translation_window.corner_radius", Immediate),
        ("translation_window.pinned_corner_radius", Immediate), ("appearance.window.font_family", Immediate), ("appearance.window.font_bold", Immediate), ("appearance.window.font_italic", Immediate),
        ("appearance.window.text_color", Immediate), ("appearance.window.background_color", Immediate), ("translation_window.border_color", Immediate), ("translation_window.border_opacity", Immediate),
        ("translation_window.border_width", Immediate), ("translation_window.border_pattern", Immediate), ("translation_window.border_always", Immediate), ("translation_window.border_seconds", Immediate),
        ("appearance.window.padding", Immediate), ("appearance.window.text_alignment", Immediate), ("appearance.window.text_wrap", Immediate), ("appearance.window.text_outline", Immediate),
        ("appearance.window.outline_color", Immediate), ("appearance.window.line_spacing", Immediate), ("appearance.window.show_original", Immediate), ("appearance.window.log_mode", Immediate), ("appearance.window.log_entries", Immediate), ("appearance.window.original_font_family", Immediate),
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
