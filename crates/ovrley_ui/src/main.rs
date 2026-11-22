//! The native OVRLEY binary. With `--mcp` it is instead an MCP server on stdio for an agent,
//! forwarding to the running window.
//!
//! Startup, in order: logging; runtime paths and distribution; the preferences file; the language
//! and drawer hydration; backend selection with the shared GPU device; then the window
//! (`app::run`). A failure before the window exists is shown in a native message dialog. A failure
//! once it exists (a malformed `lng`) is shown in the window, as `main.jsx` does.

// A release build is a GUI app without a console. `--mcp` uses the pipes its client passes.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ovrley_app::lib::preferences_store::PreferencesStore;
use ovrley_app::store::Store;
use ovrley_services::runtime_paths::RuntimeRoots;
use std::process::ExitCode;

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // MCP over stdio for an agent (D13), before anything touches Slint or winit: no window, Dock
    // icon or GPU device. Standard output carries only the protocol; logs go to standard error.
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        return serve_mcp();
    }
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            ovrley_ui::platform::show_startup_failure(&message);
            ExitCode::FAILURE
        }
    }
}

fn serve_mcp() -> ExitCode {
    let roots = match RuntimeRoots::from_platform() {
        Ok(roots) => roots,
        Err(error) => {
            log::error!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut client = ovrley_mcp::link::LinkClient::new(ovrley_mcp::link::SocketLocation::for_user(&roots.app_data_dir));
    match ovrley_mcp::protocol::run_stdio(&mut client) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("the MCP connection failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), String> {
    let roots = RuntimeRoots::from_platform()?;
    let distribution = ovrley_services::distribution::detect()?;
    log::info!("{distribution:?} distribution; settings in {}", roots.app_data_dir.display());
    #[cfg(target_os = "linux")]
    ovrley_ui::platform::appimage_integration::register();

    let preferences = open_preferences(&roots.app_data_dir)?;
    let store = Store::hydrate(&preferences).unwrap_or_else(|error| {
        log::error!("startup failed: {error}");
        let mut store = Store::default();
        store.bootstrap.startup_error = Some(error);
        store
    });

    let gpu = ovrley_ui::platform::select_backend()?;
    let agent_socket = ovrley_mcp::link::SocketLocation::for_user(&roots.app_data_dir);
    let paths = roots.app_paths()?;
    // The installed map styles and the tile cache, as the Tauri shell opens them (phase 5 D9).
    let maps = std::sync::Arc::new(ovrley_app::store::services::NativeMapServices::open(&roots, &preferences)?);
    ovrley_ui::app::run(store, paths, &roots.app_cache_dir, preferences, maps, gpu, agent_socket)
}

/// The settings file. The debug snapshot tour (`OVRLEY_SNAPSHOT`) changes the language and the
/// drawer, so it reads the file but never writes it: it is shared with the React app.
fn open_preferences(app_data_dir: &std::path::Path) -> Result<PreferencesStore, String> {
    if cfg!(debug_assertions) && std::env::var_os("OVRLEY_SNAPSHOT").is_some() {
        let path = app_data_dir.join(ovrley_app::lib::preferences_store::PREFERENCES_FILE_NAME);
        let values = ovrley_app::lib::preferences_store::read_preferences_file(&path)?;
        return Ok(PreferencesStore::with_sink(values, Box::new(ovrley_app::lib::preferences_store::MemorySink::default())));
    }
    PreferencesStore::open(app_data_dir)
}
