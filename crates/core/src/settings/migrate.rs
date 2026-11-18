//! Reading older configuration files: legacy flat keys and schema migrations.

/// Legacy key mapping is confined to file migration; the runtime and UI use canonical paths.
pub(super) const LEGACY_KEYS: &[(&str, &str)] = &[
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
pub(super) fn insert_legacy(table: &mut toml::Table, path: &str, value: toml::Value) {
    if let Some((head, tail)) = path.split_once('.') {
        let entry = table.entry(head).or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(child) = entry.as_table_mut() { insert_legacy(child, tail, value); }
    } else if let Some(existing) = table.get_mut(path).and_then(toml::Value::as_table_mut)
        && let Some(legacy) = value.as_table() {
        for (key, value) in legacy { insert_legacy(existing, key, value.clone()); }
    } else { table.entry(path).or_insert(value); }
}
pub(super) fn migrate_flat_keys(table: &mut toml::Table) {
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
pub(super) fn migrate(table: &mut toml::Table, from: u32) {
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
