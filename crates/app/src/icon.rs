#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("app_icon.h");
        fn configureLipaApplication();
    }
}

pub fn configure() {
    ffi::configureLipaApplication();
}
