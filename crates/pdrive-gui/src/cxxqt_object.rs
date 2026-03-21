// AppController QObject bridge — cxx-qt 0.8.1 + rustc ≥1.87 compatibility note:
//
// The `#[cxx_qt::bridge]` macro emits `include!(<QtCore/QObject>)` inside an
// `unsafe extern "C++"` block as part of its signal boilerplate.  Rustc ≥1.87
// enforces the `non_foreign_item_macro` lint before nested proc macros (such as
// `cxx::bridge`) can consume those tokens, causing an unconditional build failure.
// See: https://github.com/KDAB/cxx-qt/issues/ (upstream tracking issue)
//
// When that is fixed, reinstate `mod cxxqt_object;` in main.rs and uncomment the
// bridge below.

/*
use std::pin::Pin;

#[derive(Default)]
pub struct AppControllerRust {
    status: i32,
}

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, status)]
        type AppController = super::AppControllerRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn request_browse(self: Pin<&mut AppController>, path: i32);

        #[qinvokable]
        fn request_upload(self: Pin<&mut AppController>, local: i32, remote: i32);
    }
}

impl ffi::AppController {
    fn request_browse(self: Pin<&mut Self>, _path: i32) {
        tracing::info!("browse requested");
    }

    fn request_upload(self: Pin<&mut Self>, _local: i32, _remote: i32) {
        tracing::info!("upload requested");
    }
}
*/
