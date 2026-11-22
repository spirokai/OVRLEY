//! Runtime path resolution.
//!
//! Owns: repository-root detection, the four runtime roots (resources, user data, app data,
//!       app cache), `AppPaths` construction from them, and the one rule for where each package
//!       keeps its resources.

use crate::map_styles::MapStyleDirs;
use ovrley_core::paths::AppPaths;
use std::path::{Path, PathBuf};

/// The bundle identifier, under which app data and caches are kept.
pub const APP_IDENTIFIER: &str = "com.ovrley.desktop";

/// Returns the repository root.
///
/// `CARGO_MANIFEST_DIR` points at `crates/ovrley_services`, so this walks two directories up to
/// the project root used by bundled tooling and development assets.
pub fn source_repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root
}

/// The platform directories. Absent entries are the platform's documented gaps.
pub struct PlatformDirs {
    /// The user's documents folder. Optional on Linux (`~/.config/user-dirs.dirs` may not define it).
    pub document_dir: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    /// The per-user application data root (macOS `Application Support`, Windows Roaming
    /// `AppData`, Linux `$XDG_DATA_HOME`).
    pub data_dir: Option<PathBuf>,
    /// The per-user cache root (macOS `Caches`, Windows Local `AppData`, Linux `$XDG_CACHE_HOME`).
    pub cache_dir: Option<PathBuf>,
}

/// The four roots every runtime path derives from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeRoots {
    /// The checkout in development, the build's resources otherwise.
    pub resource_root: PathBuf,
    /// `Documents/OVRLEY`.
    pub user_data_root: PathBuf,
    /// `<data dir>/com.ovrley.desktop`, where the settings file lives.
    pub app_data_dir: PathBuf,
    /// `<cache dir>/com.ovrley.desktop`, where the map tile cache lives.
    pub app_cache_dir: PathBuf,
}

impl RuntimeRoots {
    /// Resolves the roots from the platform's directories and the executable's place.
    pub fn resolve(dirs: PlatformDirs) -> Result<Self, String> {
        let documents_dir = match dirs.document_dir {
            Some(dir) => dir,
            None => dirs
                .home_dir
                .ok_or_else(|| "Could not resolve the home directory".to_string())?
                .join("Documents"),
        };
        Ok(Self {
            resource_root: resource_root()?,
            user_data_root: documents_dir.join("OVRLEY"),
            app_data_dir: dirs
                .data_dir
                .ok_or_else(|| "Could not resolve the application data directory".to_string())?
                .join(APP_IDENTIFIER),
            app_cache_dir: dirs
                .cache_dir
                .ok_or_else(|| "Could not resolve the application cache directory".to_string())?
                .join(APP_IDENTIFIER),
        })
    }

    /// Resolves the roots from the `dirs` crate, as the app does.
    pub fn from_platform() -> Result<Self, String> {
        Self::resolve(
            PlatformDirs {
                document_dir: dirs::document_dir(),
                home_dir: dirs::home_dir(),
                data_dir: dirs::data_dir(),
                cache_dir: dirs::cache_dir(),
            },
        )
    }

    /// Builds the backend path configuration and creates its directories.
    ///
    /// Development builds read resources from the source checkout. Packaged builds read resources
    /// from the resource root. User data lives under the user's documents directory in `OVRLEY`,
    /// with templates in `templates/`, render outputs at the root, and runtime scratch data in
    /// `.runtime/`.
    pub fn app_paths(&self) -> Result<AppPaths, String> {
        let paths = AppPaths::from_runtime_roots(source_repo_root(), self.resource_root.clone(), self.user_data_root.clone());
        paths.ensure_dirs().map_err(|e| e.to_string())?;
        Ok(paths)
    }

    /// The installed map styles' folders: `map-styles` in the build, which a development build
    /// reads from `assets/`, and `map-styles` in the user's OVRLEY folder.
    pub fn map_style_dirs(&self) -> MapStyleDirs {
        let bundled = if cfg!(debug_assertions) {
            source_repo_root().join("assets").join("map-styles")
        } else {
            self.resource_root.join("map-styles")
        };
        MapStyleDirs {
            bundled,
            user: self.user_data_root.join("map-styles"),
        }
    }

    /// The bundled map fonts' folder: `fonts/map` in the checkout in development, in the build's
    /// resources otherwise.
    pub fn map_font_dir(&self) -> PathBuf {
        self.resource_root.join("fonts").join("map")
    }
}

/// The checkout in development, the build's resources otherwise.
fn resource_root() -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return Ok(source_repo_root());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable.parent().ok_or("the executable has no folder")?;
    Ok(packaged_resource_root(directory, std::env::consts::OS, has_bundled_ffmpeg(directory)))
}

/// Where a package keeps its resources, by the executable's folder:
/// - beside it wherever FFmpeg is beside it: the Windows packages and the AppImage;
/// - `Contents/Resources` in the macOS bundle;
/// - `<exe>/../lib/OVRLEY` on Linux: the Debian package and the Flatpak.
fn packaged_resource_root(directory: &Path, os: &str, ffmpeg_beside: bool) -> PathBuf {
    match os {
        _ if ffmpeg_beside => directory.to_path_buf(),
        "macos" => directory.join("../Resources"),
        "linux" => directory.join("../lib/OVRLEY"),
        _ => directory.to_path_buf(),
    }
}

fn has_bundled_ffmpeg(root: &Path) -> bool {
    let binary_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    root.join("vendor").join("ffmpeg").join("bin").join(binary_name).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform_dirs(document_dir: Option<&str>, home_dir: Option<&str>) -> PlatformDirs {
        PlatformDirs {
            document_dir: document_dir.map(PathBuf::from),
            home_dir: home_dir.map(PathBuf::from),
            data_dir: Some(PathBuf::from("/home/rider/.local/share")),
            cache_dir: Some(PathBuf::from("/home/rider/.cache")),
        }
    }

    #[test]
    fn source_repo_root_is_the_checkout() {
        assert!(source_repo_root().join("crates").join("ovrley_services").is_dir());
    }

    #[test]
    fn each_package_layout_finds_its_resources() {
        for (layout, exe_dir, os, ffmpeg_beside, root) in [
            ("installer", "C:/Users/rider/AppData/Local/OVRLEY", "windows", true, "C:/Users/rider/AppData/Local/OVRLEY"),
            ("portable and Store", "D:/OVRLEY", "windows", true, "D:/OVRLEY"),
            ("app bundle", "/Applications/OVRLEY.app/Contents/MacOS", "macos", false, "/Applications/OVRLEY.app/Contents/MacOS/../Resources"),
            ("AppImage", "/tmp/.mount/usr/lib/ovrley", "linux", true, "/tmp/.mount/usr/lib/ovrley"),
            ("deb", "/usr/bin", "linux", false, "/usr/bin/../lib/OVRLEY"),
            ("Flatpak", "/app/bin", "linux", false, "/app/bin/../lib/OVRLEY"),
        ] {
            assert_eq!(packaged_resource_root(Path::new(exe_dir), os, ffmpeg_beside), PathBuf::from(root), "{layout}");
        }
    }

    #[test]
    fn user_data_root_lives_in_documents_dir() {
        let roots = RuntimeRoots::resolve(platform_dirs(Some("/home/rider/Docs"), None)).unwrap();

        assert_eq!(roots.user_data_root, PathBuf::from("/home/rider/Docs/OVRLEY"));
    }

    #[test]
    fn user_data_root_uses_home_documents_when_documents_dir_is_undefined() {
        let roots = RuntimeRoots::resolve(platform_dirs(None, Some("/home/rider"))).unwrap();

        assert_eq!(roots.user_data_root, PathBuf::from("/home/rider/Documents/OVRLEY"));
    }

    #[test]
    fn user_data_root_fails_without_home_dir() {
        let result = RuntimeRoots::resolve(platform_dirs(None, None));

        assert!(result.is_err());
    }

    #[test]
    fn app_data_and_cache_dirs_carry_the_bundle_identifier() {
        let roots = RuntimeRoots::resolve(platform_dirs(Some("/home/rider/Documents"), None)).unwrap();

        assert_eq!(roots.app_data_dir, PathBuf::from("/home/rider/.local/share/com.ovrley.desktop"));
        assert_eq!(roots.app_cache_dir, PathBuf::from("/home/rider/.cache/com.ovrley.desktop"));
        assert_eq!(roots.resource_root, source_repo_root());
    }

    #[test]
    fn platform_roots_match_the_dirs_crate() {
        let roots = RuntimeRoots::from_platform().unwrap();

        assert_eq!(roots.app_data_dir, dirs::data_dir().unwrap().join(APP_IDENTIFIER));
        assert_eq!(roots.app_cache_dir, dirs::cache_dir().unwrap().join(APP_IDENTIFIER));
    }
}
