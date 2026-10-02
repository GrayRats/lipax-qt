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
    }
}

pub fn configure() {
    ffi::configureLipaApplication();
}

pub fn overlay_blur(enable: bool, radius: i32) { ffi::configureOverlayBlur(enable, radius); }
pub fn overlay_input(passthrough: bool, rects: &[i32]) { ffi::configureOverlayInput(passthrough, rects); }
pub fn copy_text(text: &cxx_qt_lib::QString) { ffi::copyLipaText(text); }
