use super::*;

#[test]
fn mixed_legacy_settings_and_profiles_keep_new_values_and_legacy_siblings() {
    let s = Settings::from_toml(r#"
schema_version = 1
source_lang = "eng+jpn"
overlay_pinned = true
overlay_pos = [20, 30]
font_size = 25
[recognition]
language = "deu"
[inplace]
shadow = true
fill_opacity = 0.4
[appearance.inplace]
shadow = false
[hotkeys]
toggle_overlay = "Ctrl+Alt+H"
[[regions]]
id = "dialogue"
source_lang = "jpn"
ocr_engine = "paddleocr"
[game_profiles.game]
source_lang = "fra"
target_lang = "de"
overlay_pinned = true
overlay_size = [800, 200]
"#);
    assert_eq!(s.recognition.language, "deu");
    assert_eq!(s.translation_window.mode, TranslationWindowMode::Pinned);
    assert_eq!(s.translation_window.position, (20, 30));
    assert_eq!(s.appearance.window.font_size, 25);
    assert!(!s.appearance.inplace.shadow);
    assert_eq!(s.appearance.inplace.fill_opacity, 0.4);
    assert_eq!(s.capture.regions[0].recognition_language, "jpn");
    assert_eq!(s.capture.regions[0].engine, Some(OcrEngine::PaddleOcr));
    assert_eq!(s.game_profiles["game"].recognition.language, "fra");
    assert_eq!(s.game_profiles["game"].translation_window.size, (800, 200));
    let saved = toml::to_string(&s).unwrap();
    for old in ["overlay_pinned", "source_lang =", "target_lang =", "ocr_engine", "toggle_overlay"] {
        assert!(!saved.contains(old), "legacy name written: {old}");
    }
    assert_eq!(Settings::from_toml(&saved), s);
}

#[test]
fn window_geometry_memory_is_on_by_default_and_a_damaged_record_is_dropped() {
    let s = Settings::default();
    assert!(s.general.remember_window_geometry && s.general.main_window.is_none());
    let mut s = Settings::from_toml("[general]\nremember_window_geometry = false\n[general.main_window]\nx = 10.0\ny = 20.0\nw = 640.0\nh = 480.0\nscreen = \"DP-1\"\nscale = 1.0\n");
    assert!(!s.general.remember_window_geometry);
    let g = s.general.main_window.clone().expect("record kept");
    assert_eq!((g.x, g.y, g.w, g.h, g.screen.as_str()), (10.0, 20.0, 640.0, 480.0, "DP-1"));
    s.general.main_window.as_mut().unwrap().w = 0.0;
    s.sanitize();
    assert!(s.general.main_window.is_none(), "a zero-sized window record is not kept");
    assert!(!REACTIONS.iter().any(|(k, r)| *k == "general.main_window" && !matches!(r, Reaction::Managed)), "the form cannot patch the geometry record");
}

#[test]
fn translation_language_inherits_recognition_or_remembers_an_explicit_override() {
    let mut s = Settings::default();
    s.recognition.language = "jpn+eng".into();
    assert_eq!(s.translation_source_language(), "ja");
    let inherited = s.processing_key();
    s.translation.source_language = TranslationSourceLanguage::Explicit("fr".into());
    assert_eq!(s.translation_source_language(), "fr");
    assert_ne!(s.processing_key(), inherited);
    assert_eq!(s.recognition.language, "jpn+eng");
    s.capture.window = game("game", "u");
    s.remember_game();
    s.translation.source_language = TranslationSourceLanguage::RecognitionLanguage;
    assert!(s.apply_game_profile());
    assert_eq!(s.translation_source_language(), "fr");
}

fn game(class: &str, uuid: &str) -> Option<WindowKey> {
    Some(WindowKey { uuid: uuid.into(), resource_class: class.into(), caption: format!("{class} window"), program: String::new() })
}

#[test]
fn general_and_main_window_settings_are_checked_and_old_files_get_defaults() {
    let s = Settings::from_toml("");
    assert_eq!((s.general.theme.as_str(), s.general.log_level.as_str(), s.general.autostart), ("dark", "info", false));
    let m = &s.appearance.main_window;
    assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Inter", "", 16, "gray"));
    let mut bad = Settings::default();
    bad.general.theme = "neon".into();
    bad.general.log_level = "loud".into();
    bad.appearance.main_window = MainWindowAppearance { font_family: "Some System Font".into(), cjk_font_family: "Another".into(), font_size: 500, background: "plaid".into() };
    bad.sanitize();
    assert_eq!((bad.general.theme.as_str(), bad.general.log_level.as_str()), ("dark", "info"));
    let m = &bad.appearance.main_window;
    assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Inter", "", 72, "gray"), "a system font is never used");
    let mut good = Settings::default();
    good.general.theme = "light".into();
    good.general.log_level = "debug".into();
    good.appearance.main_window = MainWindowAppearance { font_family: "Lora".into(), cjk_font_family: "Noto Sans CJK SC".into(), font_size: 24, background: "system".into() };
    good.sanitize();
    let m = &good.appearance.main_window;
    assert_eq!((good.general.theme.as_str(), good.general.log_level.as_str()), ("light", "debug"));
    assert_eq!((m.font_family.as_str(), m.cjk_font_family.as_str(), m.font_size, m.background.as_str()), ("Lora", "Noto Sans CJK SC", 24, "system"));
}

#[test]
fn the_cjk_floor_follows_the_language_the_engine_reads() {
    let mut s = Settings::default();
    s.recognition.language = "eng+jpn".into();
    assert_eq!(s.effective_minimum_confidence(), 38, "Tesseract reads both");
    s.recognition.engine = "paddleocr".into();
    assert_eq!(s.effective_minimum_confidence(), 30, "PaddleOCR reads only English here");
    s.recognition.language = "jpn+eng".into();
    assert_eq!(s.effective_minimum_confidence(), 38);
    // RapidOCR reads the first language too, and keeps PP-OCR's own floor; 0 still switches the check off.
    s.recognition.engine = OcrEngine::RapidOcr;
    assert_eq!(s.effective_minimum_confidence(), 50);
    s.recognition.minimum_confidence = 70;
    assert_eq!(s.effective_minimum_confidence(), 70);
    s.recognition.minimum_confidence = 0;
    assert_eq!(s.effective_minimum_confidence(), 0);
}

#[test]
fn rapidocr_is_an_engine_with_its_own_settings() {
    assert_eq!(OcrEngine::from("rapidocr"), OcrEngine::RapidOcr);
    assert_eq!(OcrEngine::RapidOcr.as_str(), "rapidocr");
    assert_eq!(String::from(OcrEngine::RapidOcr), "rapidocr");
    let r = TextRecognitionSettings::default();
    assert_eq!((r.rapid_threads, r.rapid_use_gpu, r.rapid_variant.as_str()), (0, false, "mobile"));
    let s = Settings::from_toml("[recognition]\nengine = \"rapidocr\"\nrapid_threads = 3\nrapid_use_gpu = true\nrapid_variant = \"server\"\n[[capture.regions]]\nid = \"menu\"\nengine = \"rapidocr\"\n");
    assert_eq!(s.recognition.engine, OcrEngine::RapidOcr);
    assert_eq!((s.recognition.rapid_threads, s.recognition.rapid_use_gpu, s.recognition.rapid_variant.as_str()), (3, true, "server"));
    assert_eq!(s.capture.regions[0].engine, Some(OcrEngine::RapidOcr));
    let saved = toml::to_string(&s).unwrap();
    assert!(saved.contains("engine = \"rapidocr\"") && saved.contains("rapid_threads = 3"), "{saved}");
    assert_eq!(Settings::from_toml(&saved), s);
    // Every one of them restarts the pipeline: the next frame is read with the new engine settings.
    for key in ["recognition.rapid_threads", "recognition.rapid_use_gpu", "recognition.rapid_variant"] {
        assert_eq!(reaction(key), Some(Reaction::Pipeline), "{key}");
    }
    let mut wild = Settings::default();
    wild.recognition.rapid_threads = 1000;
    wild.recognition.rapid_variant = "huge".into();
    wild.sanitize();
    assert_eq!((wild.recognition.rapid_threads, wild.recognition.rapid_variant.as_str()), (16, "mobile"));
}

#[test]
fn ocr_filters_belong_to_the_game() {
    let mut s = { let mut value = Settings::default(); value.capture.window = game("dark.exe", "u1"); value };
    s.recognition.auto_invert = true;
    s.recognition.contrast = 30;
    s.remember_game();
    // A light game next: no filters, and it keeps them off.
    s.capture.window = game("light.exe", "u2");
    s.recognition.auto_invert = false;
    s.recognition.contrast = 0;
    s.remember_game();
    s.capture.window = game("dark.exe", "u1");
    assert!(s.apply_game_profile());
    assert!(s.recognition.auto_invert && s.recognition.contrast == 30 && !s.recognition.binarize);
    s.capture.window = game("light.exe", "u2");
    assert!(s.apply_game_profile());
    assert!(!s.recognition.auto_invert && s.recognition.contrast == 0);
}

#[test]
fn a_game_gets_back_its_areas_languages_and_window() {
    let rect = NormRect { x: 0.1, y: 0.7, w: 0.8, h: 0.2 };
    let mut s = { let mut value = Settings::default(); value.capture.window = game("Witcher3.exe", "u1"); value };
    s.capture.regions[0].rect = Some(rect);
    s.recognition.language = "jpn".into();
    s.translation_window.position = (40, 50);
    s.remember_game();
    assert_eq!(s.game_profiles.len(), 1);
    assert_eq!(s.game_profiles["witcher3.exe"].caption, "Witcher3.exe window");

    // Another game starts clean...
    s.capture.window = game("Other", "u2");
    s.capture.regions[0].rect = None;
    s.recognition.language = "eng".into();
    assert!(!s.apply_game_profile(), "unknown game");
    s.remember_game();
    assert_eq!(s.game_profiles.len(), 2);

    // ...and the first one comes back, even from a new window (new UUID, other letter case).
    s.capture.window = game("witcher3.EXE", "u3");
    assert!(s.apply_game_profile());
    assert_eq!((s.capture.regions[0].rect, s.recognition.language.as_str(), s.translation_window.position), (Some(rect), "jpn", (40, 50)));
}

#[test]
fn profiles_ignore_portal_windows_and_can_be_turned_off() {
    let mut s = { let mut value = Settings::default(); value.capture.window = Some(crate::capture::portal::portal_window_key()); value };
    s.remember_game();
    assert!(s.game_profiles.is_empty(), "a portal window has no class of its own");
    s.capture.window = game("Game", "u1");
    s.game_profiles_enabled = false;
    s.remember_game();
    assert!(s.game_profiles.is_empty());
    assert!(!s.apply_game_profile());
    s.capture.window = game("", "u2");
    s.game_profiles_enabled = true;
    s.remember_game();
    assert!(s.game_profiles.is_empty(), "no class, no key");
}

fn proton(class: &str, caption: &str) -> Option<WindowKey> {
    Some(WindowKey { uuid: "u".into(), resource_class: class.into(), caption: caption.into(), program: String::new() })
}

#[test]
fn proton_windows_with_a_generic_class_get_keys_by_caption() {
    let mut s = Settings::default();
    s.capture.window = proton("steam_app_default", "Cross x Mars");
    let a = s.game_key().unwrap();
    assert_eq!(a, "steam_app_default|cross x mars");
    s.capture.window = proton("steam_app_default", "Hold Me Tight All Night, Onii-chan!");
    assert_ne!(s.game_key().unwrap(), a);
    s.capture.window = proton("Steam_App_1234", "  Cross \t x   Mars ");
    assert_eq!(s.game_key().unwrap(), "steam_app_1234|cross x mars");
}

fn proton_program(class: &str, caption: &str, program: &str) -> Option<WindowKey> {
    Some(WindowKey { uuid: "u".into(), resource_class: class.into(), caption: caption.into(), program: program.into() })
}

#[test]
fn a_shared_class_is_keyed_by_the_program_and_survives_a_changed_title() {
    let mut s = Settings::default();
    s.capture.window = proton_program("steam_app_default", "Cross x Mars", "crossxmars.exe");
    assert_eq!(s.game_key().unwrap(), "steam_app_default|crossxmars.exe");
    s.recognition.language = "jpn".into();
    s.remember_game();
    // The title now shows a score: the same game, the same profile.
    s.capture.window = proton_program("steam_app_default", "Cross x Mars — 1200 pts", "crossxmars.exe");
    s.recognition.language = "eng".into();
    assert!(s.apply_game_profile());
    assert_eq!(s.recognition.language, "jpn");
    // Plain Wine and its explorer class are shared too; a class of its own is not touched.
    s.capture.window = proton_program("wine", "Game", "game.exe");
    assert_eq!(s.game_key().unwrap(), "wine|game.exe");
    s.capture.window = proton_program("CrossxMars.exe", "Game", "crossxmars.exe");
    assert_eq!(s.game_key().unwrap(), "crossxmars.exe");
    // The program is optional in the TOML: older settings and windows without it read as before.
    let back = Settings::from_toml(&toml::to_string_pretty(&s).unwrap());
    assert_eq!(back.capture.window.as_ref().unwrap().program, "crossxmars.exe");
    let mut old = Settings::default();
    old.capture.window = proton("steam_app_default", "Cross x Mars");
    assert!(!toml::to_string_pretty(&old).unwrap().contains("program"), "an empty program is not written");
}

#[test]
fn a_profile_of_1_4_1_is_moved_to_the_key_by_program() {
    let mut s = Settings::default();
    // Stored by 1.4.1: keyed by the caption.
    s.capture.window = proton("steam_app_default", "Cross x Mars");
    s.recognition.language = "jpn".into();
    s.remember_game();
    assert!(s.game_profiles.contains_key("steam_app_default|cross x mars"));
    // 1.4.2 meets the game with its program known.
    s.capture.window = proton_program("steam_app_default", "Cross x Mars", "crossxmars.exe");
    s.recognition.language = "eng".into();
    assert!(s.apply_game_profile(), "the old profile is found");
    assert_eq!(s.recognition.language, "jpn");
    assert!(s.game_profiles.contains_key("steam_app_default|crossxmars.exe"));
    assert!(!s.game_profiles.contains_key("steam_app_default|cross x mars"), "moved, not copied");
    // A profile of a bare shared class may belong to several games: copied.
    let mut w = Settings::default();
    w.capture.window = proton("wine", "Some game");
    w.remember_game();
    w.capture.window = proton_program("wine", "Some game", "some.exe");
    assert!(w.apply_game_profile());
    assert!(w.game_profiles.contains_key("wine") && w.game_profiles.contains_key("wine|some.exe"));
}

#[test]
fn a_normal_class_keeps_its_old_key_and_an_empty_caption_falls_back_to_the_class() {
    let mut s = Settings::default();
    s.capture.window = game("CrossxMars.exe", "u1");
    assert_eq!(s.game_key().unwrap(), "crossxmars.exe");
    s.capture.window = proton("Inversion", "Some caption");
    assert_eq!(s.game_key().unwrap(), "inversion");
    s.capture.window = proton("steam_app_default", "");
    assert_eq!(s.game_key().unwrap(), "steam_app_default");
    s.capture.window = proton("steam_app_default", "  \t ");
    assert_eq!(s.game_key().unwrap(), "steam_app_default");
}

#[test]
fn a_proton_profile_is_not_applied_to_another_proton_game_and_survives_toml() {
    let mut s = Settings::default();
    s.capture.window = proton("steam_app_default", "Cross x Mars");
    s.recognition.language = "jpn".into();
    s.remember_game();
    s.capture.window = proton("steam_app_default", "Hold Me Tight All Night, Onii-chan!");
    s.recognition.language = "eng".into();
    assert!(!s.apply_game_profile(), "the other Proton game is new");
    assert_eq!(s.recognition.language, "eng");
    s.capture.window = proton("steam_app_default", "Игра ёлка");
    s.remember_game();
    assert_eq!(s.game_profiles.len(), 2);

    let saved = toml::to_string_pretty(&s).unwrap();
    let mut back = Settings::from_toml(&saved);
    assert_eq!(back.game_profiles, s.game_profiles);
    assert!(back.game_profiles.contains_key("steam_app_default|cross x mars"));
    assert!(back.game_profiles.contains_key("steam_app_default|игра ёлка"));
    back.capture.window = proton("steam_app_default", "cross X mars");
    back.recognition.language = "eng".into();
    assert!(back.apply_game_profile());
    assert_eq!(back.recognition.language, "jpn");
}

#[test]
fn profile_count_is_capped_and_existing_ones_still_update() {
    let mut s = Settings::default();
    for i in 0..MAX_GAME_PROFILES + 5 {
        s.capture.window = game(&format!("game{i}"), "u");
        s.remember_game();
    }
    assert_eq!(s.game_profiles.len(), MAX_GAME_PROFILES);
    s.capture.window = game("game0", "u");
    s.translation.target_language = "de".into();
    s.remember_game();
    assert_eq!(s.game_profiles["game0"].translation.target_language, "de");
}

#[test]
fn profiles_survive_the_config_file() {
    let mut s = { let mut value = Settings::default(); value.capture.window = game("Game", "u1"); value };
    s.capture.regions[0].rect = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 });
    s.remember_game();
    let saved = toml::to_string_pretty(&s).unwrap();
    assert_eq!(Settings::from_toml(&saved).game_profiles, s.game_profiles);
    assert!(Settings::from_toml("").game_profiles_enabled, "on by default, also for old configs");
}

#[test]
fn an_old_config_is_lifted_step_by_step_and_stamped() {
    // Version 0: no stamp, frame_seconds = 0 meant "never".
    let old = Settings::from_toml("frame_seconds = 0\ntarget_lang = \"de\"");
    assert_eq!((old.schema_version, old.capture.region_frame_mode.as_str(), old.translation.target_language.as_str()), (SCHEMA_VERSION, "off", "de"));
    // The same text already stamped with the current version is taken as it is.
    let current = Settings::from_toml("schema_version = 1\nframe_seconds = 0");
    assert_ne!(current.capture.region_frame_mode, "off");
    // A new config always carries its version.
    let saved = toml::to_string_pretty(&Settings::default()).unwrap();
    assert!(saved.contains(&format!("schema_version = {SCHEMA_VERSION}")));
}

#[test]
fn a_config_from_a_newer_version_is_read_not_rejected() {
    let s = Settings::from_toml("schema_version = 99\ntarget_lang = \"fr\"\nsome_future_key = true");
    assert_eq!((s.translation.target_language.as_str(), s.schema_version), ("fr", SCHEMA_VERSION));
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
    let declared: Vec<&str> = REACTIONS.iter().map(|(k, _)| *k).collect();
    fn visit(value: &serde_json::Value, path: &str, declared: &[&str]) {
        if declared.contains(&path) { return; }
        let object = value.as_object().unwrap_or_else(|| panic!("setting without a reaction: {path}"));
        assert!(!object.is_empty(), "empty setting without a reaction: {path}");
        for (key, child) in object {
            visit(child, &if path.is_empty() { key.clone() } else { format!("{path}.{key}") }, declared);
        }
    }
    visit(&all, "", &declared);
    for path in &declared { assert!(value_at(&all, path).is_some(), "stale reaction: {path}"); }
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
        |s: &mut Settings| s.translation.target_language = "de".into(),
        |s: &mut Settings| s.recognition.language = "jpn".into(),
        |s: &mut Settings| s.recognition.engine = "auto".into(),
        |s: &mut Settings| s.recognition.minimum_confidence = 50,
        |s: &mut Settings| s.recognition.binarize = true,
        |s: &mut Settings| s.recognition.contrast = 20,
        |s: &mut Settings| s.recognition.interval_ms += 100,
        |s: &mut Settings| s.display_mode = TranslationDisplayMode::Inplace,
        |s: &mut Settings| s.capture.regions[0].enabled = false,
        |s: &mut Settings| s.capture.window = Some(WindowKey { uuid: "u".into(), resource_class: "c".into(), caption: "t".into(), program: String::new() }),
    ] {
        let mut changed = base.clone();
        change(&mut changed);
        assert_ne!(changed.processing_key(), key);
    }
    for change in [
        |s: &mut Settings| s.appearance.window.font_size += 1,
        |s: &mut Settings| s.translation_window.mode = TranslationWindowMode::from_pinned(!s.translation_window.mode.is_pinned()),
        |s: &mut Settings| s.close_to_tray = true,
        |s: &mut Settings| s.history_limit += 1,
        |s: &mut Settings| s.hotkeys.toggle = "Ctrl+Alt+Q".into(),
        |s: &mut Settings| s.capture.source = CaptureSource::Portal,
        |s: &mut Settings| s.game_profiles_enabled = false,
        |s: &mut Settings| s.appearance.inplace.fill_opacity = 0.5,
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
    b.appearance.window.font_size += 2;
    assert_eq!(a.changed_keys(&b), ["appearance.window.font_size"]);
    assert!(a.pending_actions(&b).is_empty(), "applies at once");
    b.capture.source = CaptureSource::Portal;
    assert_eq!(a.changed_keys(&b), ["capture.source", "appearance.window.font_size"]);
    assert_eq!(a.pending_actions(&b), ["Способ захвата сменится при следующем выборе окна игры"]);
    assert_eq!(reaction("hotkeys.toggle"), Some(Reaction::Hotkeys));
    assert_eq!(reaction("nonsense"), None);
}

#[test]
fn roundtrip() {
    let s = { let mut value = Settings::default(); value.capture.region = Some(NormRect { x: 0.1, y: 0.6, w: 0.8, h: 0.3 }); value };
    let t = toml::to_string_pretty(&s).unwrap();
    assert_eq!(toml::from_str::<Settings>(&t).unwrap(), s);
}

#[test]
fn pinned_window_radius_is_saved_and_bounded() {
    let mut s = { let mut value = Settings::default(); value.translation_window.pinned_corner_radius = 18; value };
    let saved = toml::to_string_pretty(&s).unwrap();
    assert_eq!(Settings::from_toml(&saved).translation_window.pinned_corner_radius, 18);
    s.translation_window.pinned_corner_radius = 100;
    s.sanitize();
    assert_eq!(s.translation_window.pinned_corner_radius, 32);
    s.translation_window.pinned_corner_radius = 0;
    s.sanitize();
    assert_eq!(s.translation_window.pinned_corner_radius, 0);
}

#[test]
fn old_system_font_falls_back_to_a_bundled_family() {
    let mut settings = Settings::from_toml("font_family = 'DejaVu Sans'\noriginal_font_family = 'Liberation Serif'\noverlay_auto_shrink = false");
    settings.sanitize();
    assert_eq!(settings.appearance.window.font_family, "Inter");
    assert!(settings.appearance.window.original_font_family.is_empty(), "original follows the bundled translation font");
    assert!(!settings.translation_window.auto_shrink);
    let restored = Settings::from_toml(&toml::to_string(&settings).unwrap());
    assert_eq!(restored.appearance.window.font_family, "Inter");
    assert!(!restored.translation_window.auto_shrink);
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

fn region(id: &str, enabled: bool) -> CaptureRegion {
    CaptureRegion { id: id.into(), name: id.into(), enabled, ..Default::default() }
}

#[test]
fn regions_are_limited_and_single_active_by_default() {
    let mut s = Settings::default();
    assert_eq!(s.capture.regions.len(), 1);
    s.capture.regions = ["a", "b", "c", "d"].map(|id| region(id, true)).to_vec();
    s.capture.active_region = "b".into();
    s.sanitize();
    assert_eq!(s.capture.regions.len(), MAX_REGIONS, "a fourth region is dropped");
    let active: Vec<_> = s.capture.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
    assert_eq!(active, ["b"], "only the active region stays on");

    s.capture.active_region = "c".into();
    s.capture.regions[2].enabled = false;
    s.capture.regions[0].enabled = true;
    s.sanitize();
    let active: Vec<_> = s.capture.regions.iter().filter(|r| r.enabled).map(|r| r.id.as_str()).collect();
    assert_eq!(active, ["a"], "an inactive target falls back to the first enabled region");
}

#[test]
fn several_active_regions_when_allowed() {
    let mut s = { let mut value = Settings::default(); value.capture.allow_multiple_regions = true; value };
    s.capture.regions = ["a", "b", "c"].map(|id| region(id, true)).to_vec();
    s.sanitize();
    assert_eq!(s.capture.regions.iter().filter(|r| r.enabled).count(), 3);
}

#[test]
fn region_frame_defaults_and_migration() {
    let s = Settings::default();
    assert!(!s.translation_window.border_pattern, "the error pattern is off by default");
    assert_eq!((s.capture.region_frame_mode.as_str(), s.capture.frame_seconds), ("selection", 3));
    assert_eq!(Settings::from_toml("frame_seconds = 0").capture.region_frame_mode, "off", "old 'never show' is kept");
    assert_eq!(Settings::from_toml("frame_seconds = 0\nregion_frame_mode = \"solid\"").capture.region_frame_mode, "solid");
    let mut bad = { let mut value = Settings::default(); value.capture.region_frame_mode = "blink".into(); value };
    bad.capture.regions[0].frame_mode = "blink".into();
    bad.sanitize();
    assert_eq!(bad.capture.region_frame_mode, "selection");
    assert_eq!(bad.capture.regions[0].frame_mode, "", "unknown per-region mode falls back to the global one");
}

#[test]
fn inplace_properties_are_independent_and_roundtrip() {
    let mut s = Settings::default();
    assert_eq!(s.appearance.inplace.font_family, PropertyMode::Auto);
    s.appearance.inplace.font_family = PropertyMode::Manual("PT Serif".into());
    s.appearance.inplace.letter_spacing = PropertyMode::Manual(1.5);
    s.appearance.inplace.padding = PropertyMode::Manual(crate::layout::Padding::uniform(4.0));
    s.appearance.inplace.background_mode = InplaceBackgroundMode::TextReplacement;
    s.appearance.inplace.font_overrides.insert("serif".into(), "Noto Serif".into());
    let t = toml::to_string_pretty(&s).unwrap();
    let back: Settings = toml::from_str(&t).unwrap();
    assert_eq!(back.appearance.inplace, s.appearance.inplace);
    assert_eq!(back.appearance.inplace.font_size, PropertyMode::Auto, "other properties stay automatic");
    let json = serde_json::to_value(&s.appearance.inplace).unwrap();
    assert_eq!(json["font_family"], serde_json::json!({"mode": "manual", "value": "PT Serif"}));
    assert_eq!(json["font_size"], serde_json::json!({"mode": "auto"}));

    let mut bad = Settings::default();
    bad.appearance.inplace.text_color = PropertyMode::Manual("red".into());
    bad.appearance.inplace.minimum_font_size = 50.0;
    bad.appearance.inplace.maximum_font_size = 10.0;
    bad.sanitize();
    assert_eq!(bad.appearance.inplace.text_color, PropertyMode::Auto);
    assert!(bad.appearance.inplace.maximum_font_size >= bad.appearance.inplace.minimum_font_size);
}

#[test]
fn translation_display_reads_old_and_unknown_values() {
    assert_eq!(Settings::from_toml("translation_display = \"overlay\"").display_mode, TranslationDisplayMode::Window);
    assert_eq!(Settings::from_toml("translation_display = \"inplace\"").display_mode, TranslationDisplayMode::Inplace);
    let odd = Settings::from_toml("translation_display = \"hologram\"\ntarget_lang = \"de\"");
    assert_eq!((odd.display_mode, odd.translation.target_language.as_str()), (TranslationDisplayMode::Window, "de"), "the rest of the config survives");
    assert_eq!(serde_json::to_value(TranslationDisplayMode::Inplace).unwrap(), "inplace");
}

#[test]
fn partial_config_uses_defaults() {
    let s = Settings::from_toml("target_lang = \"de\"");
    assert_eq!(s.translation.target_language, "de");
    assert_eq!(s.recognition.interval_ms, 500);
}
