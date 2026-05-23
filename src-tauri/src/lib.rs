//! Tauri application shell — command registration, preview server lifecycle,
//! and platform resource resolution.
//!
//! Owns: module wiring, `BackendState` management, `tauri::generate_handler!`
//!       registration, the `video_server` lifecycle, and the `run()` entry
//!       point that glues the Rust core to the Tauri window.
//! Does not own: rendering, encoding, activity parsing, or config validation —
//!       those live in `ovrley_core`. Command wrappers live in `tauri_commands`,
//!       file-system commands live in `file_ops`, path resolution lives in
//!       `runtime_paths`, and preview helpers live in `preview_import`.
//!
//! Allowed dependencies: `ovrley_core`, `tauri`, `video_server`, and all
//!       sibling shell modules.
//! Forbidden dependencies: none (this is the outermost layer — it may import
//!       anything from `ovrley_core` but should not implement domain logic).
//!
//! ## Thread Safety
//! `BackendState` is managed by Tauri as app-level state (Send + Sync via Tauri's
//! `manage`). The `RenderController` inside it is the shared coordination point
//! for all render progress and cancellation. The video server runs on a dedicated
//! thread spawned at startup and joined on app teardown.
//!
//! ## Performance
//! Not a hot path — called once at application startup. Resource path resolution
//! and plugin registration happen before the frontend loads.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub mod video_server; // test seam

mod file_ops;
mod preview_import;
mod runtime_paths;
mod tauri_commands;

use ovrley_core::encode::video::RenderController;
use tauri::Manager;

pub(crate) struct BackendState {
    pub(crate) render_controller: RenderController,
}

/// Builds and runs the Tauri application.
///
/// The setup hook installs development logging when appropriate and starts the
/// loopback preview video server before the frontend can invoke commands.
pub fn run() {
    let video_server = video_server::VideoServerHandle::new();

    tauri::Builder::default()
        .manage(BackendState {
            render_controller: RenderController::default(),
        })
        .manage(video_server)
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            tauri_commands::backend_health,
            tauri_commands::backend_current_os,
            tauri_commands::backend_list_system_fonts,
            tauri_commands::backend_render,
            tauri_commands::backend_progress,
            tauri_commands::backend_cancel,
            tauri_commands::backend_list_templates,
            tauri_commands::backend_get_template,
            tauri_commands::backend_open_downloads,
            tauri_commands::backend_open_video,
            tauri_commands::backend_probe_video,
            tauri_commands::backend_import_preview_video,
            tauri_commands::backend_clear_preview_video,
            tauri_commands::backend_get_video_state,
            tauri_commands::backend_detect_codecs,
            file_ops::default_template_save_path,
            file_ops::write_template_file,
            file_ops::write_parse_debug_file
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            app.state::<video_server::VideoServerHandle>().start()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
