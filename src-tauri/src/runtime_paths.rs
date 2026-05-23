//! Runtime path resolution for the Tauri application shell.
//!
//! Owns: repository-root detection, `AppPaths` construction from an `AppHandle`,
//!       and the resource-root decision for debug vs release builds.
//! Does not own: template listing, render dispatch, or encoding — those live
//!       in `ovrley_core::commands` and the render pipeline.
//!
//! Allowed dependencies: `ovrley_core`, `tauri`.
//! Forbidden dependencies: none (this is a utility module for the shell layer).

use ovrley_core::paths::AppPaths;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Returns the repository root when running from the Tauri crate.
///
/// `CARGO_MANIFEST_DIR` points at `src-tauri`, so this walks one directory up
/// to the project root used by bundled tooling and development assets.
pub(crate) fn source_repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root
}

/// Builds the backend path configuration for the current runtime mode.
///
/// Development builds read resources from the source checkout. Packaged builds
/// read resources from Tauri's resource directory. User templates always live
/// under the user's documents directory in `OVRLEY`.
pub(crate) fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let repo_root = source_repo_root();
    // cfg!() is intentional here, not #[cfg(debug_assertions)]. Both branches
    // must compile because this function serves both debug and release builds.
    // #[cfg] would exclude the release branch during debug compilation, causing
    // compile errors when a debug build of the app runs in dev mode.
    let resource_root = if cfg!(debug_assertions) {
        repo_root.clone()
    } else {
        app.path().resource_dir().map_err(|e| e.to_string())?
    };
    let mut paths = AppPaths::from_resource_root(repo_root, resource_root);
    paths.user_templates_dir = app
        .path()
        .document_dir()
        .map_err(|e| e.to_string())?
        .join("OVRLEY");
    paths.ensure_dirs().map_err(|e| e.to_string())?;
    Ok(paths)
}
