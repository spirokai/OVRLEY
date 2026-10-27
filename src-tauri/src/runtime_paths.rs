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
use std::path::{Path, PathBuf};
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
/// read resources from Tauri's resource directory. User data lives under the
/// user's documents directory in `OVRLEY`, with templates in `templates/`,
/// render outputs at the root, and runtime scratch data in `.runtime/`.
pub(crate) fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let repo_root = source_repo_root();
    // cfg!() is intentional here, not #[cfg(debug_assertions)]. Both branches
    // must compile because this function serves both debug and release builds.
    // #[cfg] would exclude the release branch during debug compilation, causing
    // compile errors when a debug build of the app runs in dev mode.
    let resource_root = if cfg!(debug_assertions) {
        repo_root.clone()
    } else {
        portable_resource_root().unwrap_or(app.path().resource_dir().map_err(|e| e.to_string())?)
    };
    let paths = AppPaths::from_runtime_roots(repo_root, resource_root, user_data_root(app)?);
    paths.ensure_dirs().map_err(|e| e.to_string())?;
    Ok(paths)
}

/// Returns the `OVRLEY` user data root inside the user's documents directory.
///
/// The XDG documents entry is optional on Linux (`~/.config/user-dirs.dirs`
/// may not define it), so its absence resolves to `$HOME/Documents`.
pub(crate) fn user_data_root(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path();
    resolve_user_data_root(path.document_dir().ok(), || path.home_dir()).map_err(|e| e.to_string())
}

fn resolve_user_data_root(
    document_dir: Option<PathBuf>,
    home_dir: impl FnOnce() -> tauri::Result<PathBuf>,
) -> tauri::Result<PathBuf> {
    let documents_dir = match document_dir {
        Some(dir) => dir,
        None => home_dir()?.join("Documents"),
    };
    Ok(documents_dir.join("OVRLEY"))
}

fn portable_resource_root() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    has_bundled_ffmpeg(&exe_dir).then_some(exe_dir)
}

fn has_bundled_ffmpeg(root: &Path) -> bool {
    let binary_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    root.join("vendor")
        .join("ffmpeg")
        .join("bin")
        .join(binary_name)
        .is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_root_lives_in_documents_dir() {
        let root = resolve_user_data_root(Some(PathBuf::from("/home/rider/Docs")), || {
            panic!("home dir must not be resolved when documents dir exists")
        })
        .unwrap();

        assert_eq!(root, PathBuf::from("/home/rider/Docs/OVRLEY"));
    }

    #[test]
    fn user_data_root_uses_home_documents_when_documents_dir_is_undefined() {
        let root = resolve_user_data_root(None, || Ok(PathBuf::from("/home/rider"))).unwrap();

        assert_eq!(root, PathBuf::from("/home/rider/Documents/OVRLEY"));
    }

    #[test]
    fn user_data_root_fails_without_home_dir() {
        let result = resolve_user_data_root(None, || Err(tauri::Error::UnknownPath));

        assert!(matches!(result, Err(tauri::Error::UnknownPath)));
    }
}
