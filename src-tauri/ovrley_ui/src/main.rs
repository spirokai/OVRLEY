//! The native OVRLEY binary.
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
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            ovrley_ui::platform::show_startup_failure(&message);
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), String> {
    let roots = RuntimeRoots::from_platform(installed_resource_dir)?;
    let distribution = ovrley_services::distribution::detect()?;
    log::info!("{distribution:?} distribution; settings in {}", roots.app_data_dir.display());

    let preferences = PreferencesStore::open(&roots.app_data_dir)?;
    let store = Store::hydrate(&preferences).unwrap_or_else(|error| {
        log::error!("startup failed: {error}");
        let mut store = Store::default();
        store.bootstrap.startup_error = Some(error);
        store
    });

    let gpu = ovrley_ui::platform::select_backend()?;
    ovrley_ui::app::run(store, preferences, gpu)
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
