//! What the machine and the settings allow: display mode, capabilities, fallback and window placement.

use super::*;

/// Положение свободного окна сохраняется после перемещения — не во время него.
pub(super) fn store_floating_geometry(shared: &Shared, g: lipa_core::settings::FloatingGeometry, reason: &str) {
    tracing::debug!(target: "translation_window.geometry", reason, x = g.x, y = g.y, width = g.w, height = g.h, screen = %g.output, "floating geometry");
    // With "remember window geometry" off the saved geometry is left untouched.
    if !shared.settings.borrow().general.remember_window_geometry { return; }
    shared.update(|s| if s.general.remember_window_geometry { s.translation_window.floating_geometry = Some(g) });
}

/// JSON для скрипта KWin: где поставить свободное окно при появлении. Сначала — где его
/// оставили; без сохранённой геометрии `null` — пусть место выберет KWin.
pub(super) fn floating_placement(s: &Settings) -> String {
    match s.translation_window.floating_geometry.as_ref().filter(|_| s.general.remember_window_geometry) {
        Some(g) => {
            let (rx, ry) = g.relative();
            serde_json::json!({ "x": g.x, "y": g.y, "output": g.output, "rx": rx, "ry": ry }).to_string()
        }
        None => "null".into(),
    }
}

/// Какой рендер перевода работает: выбранный в настройках, кроме случая, когда для
/// «поверх оригинала» нет положения окна на экране (захват через portal).
pub(super) fn inplace_availability(s: &Settings) -> lipa_core::capture::FeatureAvailability {
    use lipa_core::capture::FeatureAvailability;
    let Some(window) = s.capture.window.as_ref() else {
        return FeatureAvailability::unavailable("Окно захвата ещё не выбрано.", "Выберите окно в разделе «Источник изображения».");
    };
    if let Some(reason) = capabilities_for(window, s).inplace_blocker() {
        return FeatureAvailability::unavailable(format!("{}: {reason}", backend_name(window)),
            "Выберите захват KWin либо укажите экран и подтвердите, что portal захватывает полноэкранное окно на этом экране.");
    }
    let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
    let regions = s.capture_regions();
    let langs: Vec<&str> = if regions.is_empty() { vec![s.translation.target_language.as_str()] }
        else { regions.iter().map(|r| if r.target_language.is_empty() { s.translation.target_language.as_str() } else { r.target_language.as_str() }).collect() };
    if langs.into_iter().any(|lang| !db.fonts().iter().any(|font| font.covers(lipa_core::layout::Script::from_lang(lang).fontconfig_lang()))) {
        return FeatureAvailability::unavailable("Нет встроенного шрифта для языка перевода.", "Выберите поддерживаемый язык перевода или отдельное окно.");
    }
    FeatureAvailability::available()
}

pub(super) fn effective_display(s: &Settings) -> (&'static str, String) {
    if s.display_mode == lipa_core::settings::TranslationDisplayMode::Window { return ("window", String::new()); }
    let availability = inplace_availability(s);
    if availability.available { ("inplace", String::new()) } else { ("window", availability.reason) }
}

/// What did not go over the game as planned, for the note in the main window. Nothing here is shown in another window.
pub(super) fn fallback_json(outcome: &lipa_core::layout::place::PlacementOutcome, regions: &[lipa_core::settings::CaptureRegion]) -> String {
    let name = |id: &str| regions.iter().find(|r| r.id == id).map_or_else(|| id.to_owned(), |r| r.name.clone());
    let texts: Vec<serde_json::Value> = outcome.dropped.iter().map(|f| serde_json::json!({ "region": name(&f.region_id), "text": f.text, "reason": f.reason })).collect();
    serde_json::json!({ "dropped": texts, "degraded": outcome.placed.iter().filter(|p| p.degraded.is_some()).count() }).to_string()
}

/// Возможности способа захвата выбранного окна для QML; без окна — как у KWin (ограничений нет).
pub(super) fn capabilities_json(s: &Settings) -> String {
    let caps = s.capture.window.as_ref().map(|w| capabilities_for(w, s)).unwrap_or(lipa_core::capture::CaptureCapabilities::KWIN);
    let mut value = serde_json::to_value(caps).unwrap_or_default();
    let inplace = inplace_availability(s);
    value["inplaceBlocker"] = inplace.reason.clone().into();
    value["inplaceTranslation"] = serde_json::to_value(inplace).unwrap();
    let frames = if s.capture.window.is_none() {
        lipa_core::capture::FeatureAvailability::unavailable("Окно захвата ещё не выбрано.", "Выберите окно в разделе «Источник изображения».")
    } else if let Some(reason) = caps.frame_blocker() {
        lipa_core::capture::FeatureAvailability::unavailable(reason, "Выберите захват KWin или настройте полноэкранный portal.")
    } else { lipa_core::capture::FeatureAvailability::available() };
    value["frameBlocker"] = frames.reason.clone().into();
    value["captureRegionFrame"] = serde_json::to_value(frames).unwrap();
    value.to_string()
}

/// Снимок Tesseract в JSON; тяжёлые вызовы (tesseract, pacman/dpkg) выполняются вне GUI-потока.
pub(super) fn tesseract_snapshot() -> TesseractInfo {
    TesseractManager::system().detect()
}


/// `LIPAX_REDUCE_MOTION` (anything but empty/`0`) or `AnimationDurationFactor=0` in the `[KDE]` group of kdeglobals.
pub(super) fn reduced_motion_from(env: Option<&str>, kdeglobals: &str) -> bool {
    if env.is_some_and(|v| !v.is_empty() && v != "0") { return true; }
    let mut in_kde = false;
    for line in kdeglobals.lines().map(str::trim) {
        if line.starts_with('[') { in_kde = line == "[KDE]"; continue; }
        if let (true, Some(v)) = (in_kde, line.strip_prefix("AnimationDurationFactor=")) {
            return v.trim().parse::<f64>().is_ok_and(|f| f <= 0.0);
        }
    }
    false
}
