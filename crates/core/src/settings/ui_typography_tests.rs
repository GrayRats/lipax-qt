use super::*;
#[test]
fn ui_scale_migrates_and_does_not_invalidate_recognition() {
    let mut settings = Settings::from_toml("schema_version = 2\n[general]\ntheme = 'system'\n");
    assert_eq!(settings.general.ui_text_scale, "normal");
    let processing = settings.processing_key();
    let appearance = settings.appearance.clone();
    settings.general.ui_text_scale = "extra_large".into();
    settings.sanitize();
    assert_eq!(settings.processing_key(), processing);
    assert_eq!(settings.appearance, appearance);
    let restored = Settings::from_toml(&toml::to_string(&settings).expect("serialize settings"));
    assert_eq!(restored.general.ui_text_scale, "extra_large");
    settings.general.ui_text_scale = "invalid".into();
    settings.sanitize();
    assert_eq!(settings.general.ui_text_scale, "normal");
}
