#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("app_icon.h");
        fn configureLipaApplication();
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        fn configureOverlayInput(pinned: bool, passthrough: bool, edge: i32);
        fn copyLipaText(text: &QString);
    }
}

pub fn configure() {
    ffi::configureLipaApplication();
}

pub fn overlay_input(pinned: bool, passthrough: bool, edge: i32) { ffi::configureOverlayInput(pinned, passthrough, edge); }
pub fn copy_text(text: &cxx_qt_lib::QString) { ffi::copyLipaText(text); }
