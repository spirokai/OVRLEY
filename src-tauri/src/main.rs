//! OVRLEY desktop application binary entry point.
//!
//! This file is intentionally minimal — all setup logic lives in [`app_lib::run`].
//! The `windows_subsystem` attribute suppresses the console window on Windows
//! release builds so the app appears as a native GUI application.
//!
//! Does not own: any application logic.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
