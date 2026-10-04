#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("app_icon.h");
        fn createLipaApplication(args: &Vec<String>);
        fn execLipaApplication() -> i32;
        fn destroyLipaApplication();
        fn configureLipaApplication();
        fn loadLipaFonts(directory: &QString) -> QString;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        fn configureOverlayInput(passthrough: bool, rects: &[i32]);
        fn configureOverlayBlur(enable: bool, radius: i32);
        fn copyLipaText(text: &QString);
        fn activateLipaWindow(object_name: &QString, token: &QString);
        #[allow(dead_code)]
        fn scheduleLipaTestClose(delay_ms: i32);
        #[allow(clippy::too_many_arguments)]
        fn measureLipaText(family: &str, weight: i32, italic: bool, px: f64, letter_spacing: f64, width: f64, wrap: i32, text: &str, out: &mut [f64]);
        fn lipaCapRatio(family: &str, weight: i32, italic: bool) -> f64;
    }
}

use lipa_core::layout::WrapMode;
use lipa_core::layout::fit::{FontSpec, Measured, TextMeasure};
use cxx_qt_lib::QString;

/// Метрики Qt (`QFontMetricsF`) выбранного шрифта для подгонки перевода. Только в GUI-потоке.
pub struct QtMeasure;

impl TextMeasure for QtMeasure {
    fn measure(&self, text: &str, font: &FontSpec, px: f32, letter_spacing: f32, width: f32, wrap: WrapMode) -> Measured {
        let wrap = match wrap { WrapMode::WordWrap => 0, WrapMode::WrapAnywhere => 1, WrapMode::NoWrap | WrapMode::Elide => 2 };
        let mut out = [0.0f64; 4];
        ffi::measureLipaText(&font.family, font.weight.value(), font.italic, px as f64, letter_spacing as f64, width as f64, wrap, text, &mut out);
        Measured { width: out[0] as f32, height: out[1] as f32, line_spacing: out[2] as f32, lines: out[3] as u32 }
    }
    fn cap_ratio(&self, font: &FontSpec) -> f32 {
        ffi::lipaCapRatio(&font.family, font.weight.value(), font.italic) as f32
    }
}

/// Создаёт `QApplication` (см. `app_icon.h`: трей на теме KDE строится из виджетов).
pub fn create_application() { ffi::createLipaApplication(&std::env::args().collect()); }
pub fn exec_application() -> i32 { ffi::execLipaApplication() }
pub fn destroy_application() { ffi::destroyLipaApplication(); }

pub fn configure() {
    ffi::configureLipaApplication();
    let directory = std::env::var_os("LIPAX_FONT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| {
        let installed = std::path::PathBuf::from("/usr/share/lipax/fonts");
        if installed.is_dir() { return installed; }
        std::env::current_exe().ok().into_iter().flat_map(|path| path.ancestors().map(std::path::Path::to_path_buf).collect::<Vec<_>>())
            .map(|root| root.join("crates/app/assets/fonts"))
            .find(|fonts| fonts.is_dir()).unwrap_or(installed)
    });
    let names = ffi::loadLipaFonts(&QString::from(directory.to_string_lossy().as_ref())).to_string();
    lipa_core::layout::font_database::set_bundled_available(names.lines().map(str::to_owned));
}

pub fn overlay_blur(enable: bool, radius: i32) { ffi::configureOverlayBlur(enable, radius); }
pub fn overlay_input(passthrough: bool, rects: &[i32]) { ffi::configureOverlayInput(passthrough, rects); }
pub fn copy_text(text: &cxx_qt_lib::QString) { ffi::copyLipaText(text); }
pub fn activate_window(object_name: &cxx_qt_lib::QString, token: &cxx_qt_lib::QString) { ffi::activateLipaWindow(object_name, token); }

#[cfg(feature = "lifecycle-test")]
pub fn test_close_after(delay_ms: i32) { ffi::scheduleLipaTestClose(delay_ms); }
