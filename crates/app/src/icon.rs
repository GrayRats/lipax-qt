#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("app_icon.h");
        fn configureLipaApplication();
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        fn configureOverlayInput(passthrough: bool, rects: &[i32]);
        fn configureOverlayBlur(enable: bool, radius: i32);
        fn copyLipaText(text: &QString);
        #[allow(clippy::too_many_arguments)]
        fn measureLipaText(family: &str, weight: i32, italic: bool, px: f64, letter_spacing: f64, width: f64, wrap: i32, text: &str, out: &mut [f64]);
        fn lipaCapRatio(family: &str, weight: i32, italic: bool) -> f64;
    }
}

use lipa_core::layout::WrapMode;
use lipa_core::layout::fit::{FontSpec, Measured, TextMeasure};

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

pub fn configure() {
    ffi::configureLipaApplication();
}

pub fn overlay_blur(enable: bool, radius: i32) { ffi::configureOverlayBlur(enable, radius); }
pub fn overlay_input(passthrough: bool, rects: &[i32]) { ffi::configureOverlayInput(passthrough, rects); }
pub fn copy_text(text: &cxx_qt_lib::QString) { ffi::copyLipaText(text); }
