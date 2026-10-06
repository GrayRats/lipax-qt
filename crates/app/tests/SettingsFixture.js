.pragma library
function assign(object, path, value) {
    const parts = path.split(".")
    let node = object
    for (let i = 0; i < parts.length - 1; ++i) {
        if (!node[parts[i]]) node[parts[i]] = ({})
        node = node[parts[i]]
    }
    node[parts[parts.length - 1]] = value
}
function make(values) {
    const out = {capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}}
    for (const key of Object.keys(values || ({}))) assign(out, key, values[key])
    return out
}
function patch(base, changes) {
    const out = JSON.parse(JSON.stringify(base))
    for (const key of Object.keys(changes)) assign(out, key, changes[key])
    return out
}
function paths() { return ["recognition.language", "translation.target_language", "translation.source_language", "recognition.engine", "recognition.paddle_python", "recognition.minimum_confidence", "translation.service", "translation.yandex_api_key", "translation.yandex_folder_id", "translation.custom_url", "capture.portal_fills_monitor", "translation.custom_api_key", "recognition.interval_ms", "recognition.sensitivity", "recognition.debounce_ms", "display_mode", "capture.regions", "appearance.inplace.background_mode", "appearance.inplace.font_family", "appearance.inplace.font_size", "appearance.inplace.font_weight", "appearance.inplace.italic", "appearance.inplace.line_height", "appearance.inplace.letter_spacing", "appearance.inplace.alignment", "appearance.inplace.wrap_mode", "appearance.inplace.text_color", "appearance.inplace.outline_color", "appearance.inplace.outline_width", "appearance.inplace.shadow", "appearance.inplace.text_opacity", "appearance.inplace.fill_color", "appearance.inplace.fill_opacity", "appearance.inplace.padding_x", "appearance.inplace.padding_y", "appearance.inplace.extra_margin", "appearance.inplace.corner_radius", "appearance.inplace.padding", "appearance.inplace.minimum_font_size", "appearance.inplace.maximum_font_size", "appearance.inplace.allow_condensed_fallback", "appearance.inplace.font_overrides", "appearance.inplace.preferred_fonts", "appearance.inplace.line_gap_factor", "hotkeys.toggle", "hotkeys.select_region", "hotkeys.translate_once", "hotkeys.toggle_translation", "hotkeys.toggle_pin", "capture.source", "translation_window.screen", "translation_window.mode", "appearance.window.background_style", "appearance.window.blur_enabled", "appearance.window.blur_tint", "appearance.window.dim_inverse", "translation_window.corner_radius", "translation_window.pinned_corner_radius", "appearance.window.font_family", "appearance.window.font_bold", "appearance.window.font_italic", "appearance.window.text_color", "appearance.window.background_color", "translation_window.border_color", "translation_window.border_opacity", "translation_window.border_width", "translation_window.border_pattern", "translation_window.border_always", "translation_window.border_seconds", "appearance.window.padding", "appearance.window.text_alignment", "appearance.window.text_wrap", "appearance.window.text_outline", "appearance.window.outline_color", "appearance.window.line_spacing", "appearance.window.show_original", "appearance.window.original_font_family", "appearance.window.original_font_size", "appearance.window.original_color", "translation_window.max_width_enabled", "translation_window.maximum_width", "history_enabled", "history_persist", "history_limit", "close_to_tray", "translation.auto_translate", "capture.active_region", "capture.allow_multiple_regions", "appearance.window.font_size", "translation_window.auto_shrink", "translation_window.opacity", "translation_window.click_through", "translation_window.position", "translation_window.size", "capture.frame_color", "capture.frame_width", "capture.frame_seconds", "capture.region_frame_mode", "capture.region_frame_pinned", "game_profiles_enabled"] }
