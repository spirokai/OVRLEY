//! Tauri build script.
//!
//! On Windows, links `msvcprt` so the Skia C++ bindings compiled by
//! `ovrley_core` can resolve the Microsoft C++ standard library. On all
//! platforms, delegates to `tauri_build::build` for resource bundling and
//! platform-specific manifest generation.
//!
//! This file must remain minimal — all build logic belongs in the core crate.

fn main() {
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=msvcprt");
    }
    tauri_build::build()
}
