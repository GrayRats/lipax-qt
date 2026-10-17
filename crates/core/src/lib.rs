//! Ядро переводчика без зависимости от Qt: настройки, change detection,
//! сравнение текста, кэш. Остальные модули (capture, ocr, translate, pipeline)
//! добавляются поэтапно.

pub mod autostart;
pub mod cache;
pub mod capture;
pub mod detect;
pub mod hotkeys;
pub mod layout;
pub mod ocr;
pub mod pipeline;
pub mod settings;
pub mod tesseract;
pub mod text;
pub mod translate;

pub mod history;
pub mod diagnostics;

/// Base directory for runtime files written by live runs (backdrops, previews, KWin scripts).
///
/// `$LIPAX_RUNTIME_DIR` (when non-empty) is used as-is and created best-effort with mode 0700,
/// so tests can keep every artefact inside the project `tmp/`. Otherwise the previous default
/// is returned unchanged: `$XDG_RUNTIME_DIR`, falling back to the system temp dir. Callers join
/// their own sub-path exactly as before.
pub fn runtime_dir() -> std::path::PathBuf {
    match std::env::var_os("LIPAX_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => {
            let dir = std::path::PathBuf::from(dir);
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            let _ = builder.create(&dir);
            dir
        }
        None => dirs::runtime_dir().unwrap_or_else(std::env::temp_dir),
    }
}

#[cfg(test)]
mod runtime_dir_tests {
    use super::runtime_dir;
    use std::sync::Mutex;

    static ENV: Mutex<()> = Mutex::new(());

    #[test]
    fn override_is_created_private_and_default_is_unchanged() {
        let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var_os("LIPAX_RUNTIME_DIR");
        let target = std::env::temp_dir().join(format!("lipax-rt-{}", std::process::id())).join("a/b");
        let _ = std::fs::remove_dir_all(target.parent().unwrap().parent().unwrap());

        unsafe { std::env::set_var("LIPAX_RUNTIME_DIR", &target) };
        assert_eq!(runtime_dir(), target);
        assert!(target.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o700);
        }

        unsafe { std::env::set_var("LIPAX_RUNTIME_DIR", "") };
        assert_eq!(runtime_dir(), dirs::runtime_dir().unwrap_or_else(std::env::temp_dir));
        unsafe { std::env::remove_var("LIPAX_RUNTIME_DIR") };
        assert_eq!(runtime_dir(), dirs::runtime_dir().unwrap_or_else(std::env::temp_dir));

        let _ = std::fs::remove_dir_all(target.parent().unwrap().parent().unwrap());
        if let Some(v) = saved { unsafe { std::env::set_var("LIPAX_RUNTIME_DIR", v) } }
    }
}
