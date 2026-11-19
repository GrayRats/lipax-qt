use super::*;

#[test]
fn reduced_motion_follows_the_environment_and_the_kde_animation_factor() {
    assert!(!reduced_motion_from(None, ""));
    assert!(reduced_motion_from(Some("1"), ""));
    assert!(!reduced_motion_from(Some("0"), "") && !reduced_motion_from(Some(""), ""));
    assert!(reduced_motion_from(None, "[KDE]\nAnimationDurationFactor=0\n"));
    assert!(!reduced_motion_from(None, "[KDE]\nAnimationDurationFactor=0.35\n"));
    assert!(!reduced_motion_from(None, "[General]\nAnimationDurationFactor=0\n"), "only the [KDE] group counts");
}

use lipa_core::settings::{FloatingGeometry, TranslationDisplayMode, WindowKey};

#[test]
fn appearance_patch_preserves_live_capture_settings() {
    let mut current = Settings::default();
    current.capture.window = Some(WindowKey { uuid: "game-now".into(), resource_class: "Game".into(), caption: "Game".into(), program: String::new() });
    current.capture.source = CaptureSource::Portal;
    let before = current.processing_key();
    let patch = serde_json::json!({ "appearance.window.font_size": 30, "translation_window.pinned_corner_radius": 18 });
    let updated = merge_settings_patch(&current, patch.as_object().unwrap()).unwrap();
    assert_eq!(updated.capture.window, current.capture.window);
    assert_eq!(updated.capture.source, CaptureSource::Portal);
    assert_eq!(updated.processing_key(), before, "appearance must not reset the capture pipeline");
    assert_eq!((updated.appearance.window.font_size, updated.translation_window.pinned_corner_radius), (30, 18));
    // Anything the backend manages is refused, found by the declaration and not by a list kept here.
    for key in ["translation_window.floating_geometry", "schema_version", "capture.portal_token"] {
        let managed = serde_json::json!({ key: null });
        assert!(merge_settings_patch(&current, managed.as_object().unwrap()).is_err(), "{key}");
    }
    let forbidden = serde_json::json!({ "capture.window": null });
    assert!(merge_settings_patch(&current, forbidden.as_object().unwrap()).is_err());
    // Game profiles are written by the backend only; a stale form must not overwrite them.
    let stale = serde_json::json!({ "game_profiles": {} });
    assert!(merge_settings_patch(&current, stale.as_object().unwrap()).is_err());
    let switch = serde_json::json!({ "game_profiles_enabled": false });
    assert!(!merge_settings_patch(&current, switch.as_object().unwrap()).unwrap().game_profiles_enabled);
    current.appearance.inplace.shadow = true;
    let patch = serde_json::json!({ "appearance.inplace.fill_opacity": 0.4 });
    let updated = merge_settings_patch(&current, patch.as_object().unwrap()).unwrap();
    assert!(updated.appearance.inplace.shadow, "a delayed edit keeps a newly changed sibling");
    assert_eq!(updated.appearance.inplace.fill_opacity, 0.4);
    for patch in [serde_json::json!({"capture": {"window": null}}), serde_json::json!({"appearance.inplace": {}})] {
        assert!(merge_settings_patch(&current, patch.as_object().unwrap()).is_err(), "only registered leaves may be patched");
    }
}

#[test]
fn the_ui_gets_the_backend_capabilities_and_the_reasons() {
    let mut s = Settings::default();
    let caps = |s: &Settings| serde_json::from_str::<serde_json::Value>(&capabilities_json(s)).unwrap();
    let none = caps(&s);
    assert_eq!(none["inplaceTranslation"]["available"], false, "select a capture window first");
    s.capture.window = Some(WindowKey { uuid: "portal:window".into(), resource_class: String::new(), caption: String::new(), program: String::new() });
    let portal = caps(&s);
    assert_eq!((portal["window_geometry"].as_bool(), portal["inplace_translation"].as_bool()), (Some(false), Some(false)));
    assert!(portal["inplaceBlocker"].as_str().unwrap().contains("глобальные координаты"));
    assert!(portal["frameBlocker"].as_str().unwrap().contains("рамка"));
    s.capture.portal_fills_monitor = true;
    s.translation_window.screen = "DP-1".into();
    let fullscreen = caps(&s);
    assert_eq!((fullscreen["window_geometry"].as_bool(), fullscreen["inplace_translation"].as_bool()), (Some(true), Some(true)));
    assert_eq!((fullscreen["inplaceBlocker"].as_str(), fullscreen["frameBlocker"].as_str()), (Some(""), Some("")));
}

#[test]
fn fields_left_out_are_reported_for_the_note_only() {
    use lipa_core::layout::place::{DroppedText, PlacementOutcome};
    let regions = vec![lipa_core::settings::CaptureRegion { id: "dialogue".into(), name: "Диалоги".into(), ..Default::default() }];
    let none = serde_json::from_str::<serde_json::Value>(&fallback_json(&PlacementOutcome::default(), &regions)).unwrap();
    assert_eq!((none["dropped"].as_array().map(Vec::len), none["degraded"].as_u64()), (Some(0), Some(0)), "nothing to report: the note stays hidden");
    assert!(none.get("texts").is_none(), "no texts for another window");
    let outcome = PlacementOutcome { placed: Vec::new(), dropped: vec![DroppedText { region_id: "dialogue".into(), block_id: 3, text: "Привет".into(), original: "Hello".into(), reason: "для перевода нет места поверх игры" }] };
    let shown = serde_json::from_str::<serde_json::Value>(&fallback_json(&outcome, &regions)).unwrap();
    assert_eq!((shown["dropped"][0]["region"].as_str(), shown["dropped"][0]["text"].as_str()), (Some("Диалоги"), Some("Привет")));
}

#[test]
fn inplace_does_not_depend_on_the_translation_window() {
    let mut s = Settings { display_mode: TranslationDisplayMode::Inplace, ..Default::default() };
    s.capture.window = Some(WindowKey { uuid: "kwin-game".into(), resource_class: "game".into(), caption: "Game".into(), program: String::new() });
    assert_eq!(effective_display(&s), ("inplace", String::new()));
    // Portal capture has no window position: logged fallback to the translation window.
    s.capture.window = Some(WindowKey { uuid: "portal:window".into(), resource_class: String::new(), caption: String::new(), program: String::new() });
    let (mode, note) = effective_display(&s);
    assert_eq!(mode, "window");
    assert!(note.contains("portal"));
    s.capture.portal_fills_monitor = true;
    s.translation_window.screen = "DP-1".into();
    assert_eq!(effective_display(&s), ("inplace", String::new()));
    s.display_mode = TranslationDisplayMode::Window;
    assert_eq!(effective_display(&s), ("window", String::new()));
}

#[test]
fn floating_placement_restores_where_the_user_left_it() {
    let mut s = Settings::default();
    assert_eq!(floating_placement(&s), "null", "first start: KWin chooses");
    s.translation_window.screen = "DP-1".into();
    s.translation_window.position = (40, 50);
    assert_eq!(floating_placement(&s), "null", "pinned placement must never constrain floating placement");
    s.translation_window.floating_geometry = Some(FloatingGeometry { x: 2140.0, y: 382.0, w: 740.0, h: 240.0, output: "DP-2".into(), output_x: 2560.0, output_y: -200.0 });
    let p: serde_json::Value = serde_json::from_str(&floating_placement(&s)).unwrap();
    assert_eq!((p["x"].as_f64(), p["y"].as_f64(), p["output"].as_str()), (Some(2140.0), Some(382.0), Some("DP-2")));
    assert_eq!((p["rx"].as_i64(), p["ry"].as_i64()), (Some(-420), Some(582)), "negative layout offsets are kept");
    s.general.remember_window_geometry = false;
    assert_eq!(floating_placement(&s), "null", "disabled persistence leaves placement to KWin");
}
