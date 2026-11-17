//! The native OVRLEY binary. With `--mcp` it is instead an MCP server on stdio for an agent,
//! forwarding to the running window.
//!
//! Startup, in order: logging; runtime paths and distribution; the preferences file; the language
//! and drawer hydration; backend selection with the shared GPU device; then the window
//! (`app::run`). A failure before the window exists is shown in a native message dialog. A failure
//! once it exists (a malformed `lng`) is shown in the window, as `main.jsx` does.

use ovrley_app::lib::preferences_store::PreferencesStore;
use ovrley_app::store::Store;
use ovrley_services::runtime_paths::RuntimeRoots;
use std::path::PathBuf;
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
    let roots = match RuntimeRoots::from_platform(installed_resource_dir) {
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
    let roots = RuntimeRoots::from_platform(installed_resource_dir)?;
    let distribution = ovrley_services::distribution::detect()?;
    log::info!("{distribution:?} distribution; settings in {}", roots.app_data_dir.display());

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

/// The installed build's resources: `Contents/Resources` in a macOS bundle, the executable's
/// folder elsewhere. Phase 9 settles packaging.
fn installed_resource_dir() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable.parent().ok_or("the executable has no folder")?;
    Ok(if cfg!(target_os = "macos") {
        directory.join("../Resources")
    } else {
        directory.to_path_buf()
    })
}
