//! Tauri glue for `ovrley_services::runtime_paths`: fills the runtime roots from the `AppHandle`
//! path resolver.

use ovrley_core::paths::AppPaths;
use ovrley_services::runtime_paths::{PlatformDirs, RuntimeRoots};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

pub(crate) fn runtime_roots<R: Runtime>(app: &AppHandle<R>) -> Result<RuntimeRoots, String> {
    let path = app.path();
    RuntimeRoots::resolve(
        PlatformDirs {
            document_dir: path.document_dir().ok(),
            home_dir: path.home_dir().ok(),
            data_dir: path.data_dir().ok(),
            cache_dir: path.cache_dir().ok(),
        },
        || path.resource_dir().map_err(|e| e.to_string()),
    )
}

pub(crate) fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    runtime_roots(app)?.app_paths()
}

pub(crate) fn user_data_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(runtime_roots(app)?.user_data_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tauri's desktop resolver and the `dirs` crate agree, so both apps share one settings file
    /// and one tile cache.
    #[test]
    fn tauri_roots_match_the_native_roots() {
        let app = tauri::test::mock_app();
        let tauri_roots = runtime_roots(app.handle()).unwrap();
        let native_roots = RuntimeRoots::from_platform(|| unreachable!("debug build")).unwrap();
        assert_eq!(tauri_roots, native_roots);
    }

    #[test]
    fn app_identifier_is_the_bundle_identifier() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], ovrley_services::runtime_paths::APP_IDENTIFIER);
    }
}
