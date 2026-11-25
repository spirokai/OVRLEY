//! Where the app runs from: the bundle identifier, the four runtime roots every path derives from,
//! where each package keeps its resources, whether this is an installed or a portable copy, and
//! the AppImage's menu entry.

use crate::maps::MapStyleDirs;
use ovrley_core::paths::AppPaths;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The bundle identifier, under which app data and caches are kept.
pub const APP_IDENTIFIER: &str = "com.ovrley.desktop";

/// The marker file beside the executable of a portable copy.
const PORTABLE_MARKER: &str = ".ovrley-portable";

/// The repository root: `CARGO_MANIFEST_DIR` is `crates/ovrley_services`, two levels below it.
#[must_use]
pub fn source_repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root
}

/// How this copy of the app was distributed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Installed,
    /// Unpacked from an archive, with the portable marker beside the executable. It never updates
    /// itself.
    Portable,
}

impl Distribution {
    /// Detects this copy's distribution from the marker beside the executable.
    ///
    /// # Errors
    ///
    /// When the executable's place is unknown, or the marker exists but is not a readable file.
    pub fn detect() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let directory = executable
            .parent()
            .ok_or_else(|| format!("Executable has no parent directory: {}", executable.display()))?;
        let marker = directory.join(PORTABLE_MARKER);
        match std::fs::metadata(&marker) {
            Ok(metadata) if metadata.is_file() => Ok(Self::Portable),
            Ok(_) => Err(format!("Portable marker is not a file: {}", marker.display())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::Installed),
            Err(error) => Err(format!("Could not inspect portable marker {}: {error}", marker.display())),
        }
    }
}

/// The platform directories. Absent entries are the platform's documented gaps.
struct PlatformDirs {
    /// The user's documents folder. Optional on Linux (`~/.config/user-dirs.dirs` may not define it).
    document_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    /// The per-user application data root (macOS `Application Support`, Windows Roaming
    /// `AppData`, Linux `$XDG_DATA_HOME`).
    data_dir: Option<PathBuf>,
    /// The per-user cache root (macOS `Caches`, Windows Local `AppData`, Linux `$XDG_CACHE_HOME`).
    cache_dir: Option<PathBuf>,
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
    /// `<cache dir>/com.ovrley.desktop`, where the map tile and filmstrip caches live.
    pub app_cache_dir: PathBuf,
}

impl RuntimeRoots {
    /// Resolves the roots from the platform's directories and the executable's place.
    ///
    /// # Errors
    ///
    /// When the platform has no home, data or cache directory, or the executable's place is unknown.
    pub fn from_platform() -> Result<Self, String> {
        Self::resolve(PlatformDirs {
            document_dir: dirs::document_dir(),
            home_dir: dirs::home_dir(),
            data_dir: dirs::data_dir(),
            cache_dir: dirs::cache_dir(),
        })
    }

    fn resolve(dirs: PlatformDirs) -> Result<Self, String> {
        let documents_dir = match dirs.document_dir {
            Some(dir) => dir,
            None => dirs.home_dir.ok_or("Could not resolve the home directory")?.join("Documents"),
        };
        Ok(Self {
            resource_root: resource_root()?,
            user_data_root: documents_dir.join("OVRLEY"),
            app_data_dir: dirs.data_dir.ok_or("Could not resolve the application data directory")?.join(APP_IDENTIFIER),
            app_cache_dir: dirs.cache_dir.ok_or("Could not resolve the application cache directory")?.join(APP_IDENTIFIER),
        })
    }

    /// The core's path configuration, with its directories created: resources from the checkout in
    /// development and from the resource root otherwise; templates in `templates/`, render outputs
    /// at the root and runtime scratch data in `.runtime/` of the user data root.
    ///
    /// # Errors
    ///
    /// When a directory cannot be created.
    pub fn app_paths(&self) -> Result<AppPaths, String> {
        let paths = AppPaths::from_runtime_roots(source_repo_root(), self.resource_root.clone(), self.user_data_root.clone());
        paths.ensure_dirs().map_err(|error| error.to_string())?;
        Ok(paths)
    }

    /// The installed map styles' folders: `map-styles` in the build, which a development build
    /// reads from `assets/`, and `map-styles` in the user's OVRLEY folder.
    #[must_use]
    pub fn map_style_dirs(&self) -> MapStyleDirs {
        let bundled = if cfg!(debug_assertions) {
            source_repo_root().join("assets").join("map-styles")
        } else {
            self.resource_root.join("map-styles")
        };
        MapStyleDirs { bundled, user: self.user_data_root.join("map-styles") }
    }

    /// The bundled map fonts' folder.
    #[must_use]
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

/// Adds the running AppImage to the user's application menu. An AppImage carries its desktop entry
/// and icons inside the image, where launchers cannot see them, so on every start they are copied
/// into the user's XDG data directory. The entry is named after [`APP_IDENTIFIER`], because Wayland
/// compositors match a window's app id to `<app id>.desktop` to find its icon.
#[cfg(target_os = "linux")]
mod appimage {
    use super::APP_IDENTIFIER;
    use std::path::{Path, PathBuf};

    /// Registers the AppImage when the app runs from one. Failure only costs the menu entry, so it is
    /// logged rather than stopping startup.
    pub fn register_appimage() {
        let (Some(appimage), Some(appdir)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
        else {
            return;
        };
        if let Err(error) = install(Path::new(&appimage), Path::new(&appdir)) {
            log::warn!("Could not add the AppImage to the application menu: {error}");
        }
    }

    fn install(appimage: &Path, appdir: &Path) -> Result<(), String> {
        let data_dir = dirs::data_dir().ok_or("the user's data directory is unknown")?;
        let entry_name = format!("{APP_IDENTIFIER}.desktop");
        let bundled_entry = appdir.join("usr/share/applications").join(&entry_name);
        let entry = launch_from(
            &std::fs::read_to_string(&bundled_entry)
                .map_err(|error| format!("{}: {error}", bundled_entry.display()))?,
            appimage,
        );
        let target = data_dir.join("applications").join(&entry_name);

        // An AppImage manager such as Shelly names its entry after the bundled one. When that entry
        // already launches this file, the manager owns it and its icons, so neither is touched.
        let path = appimage.to_string_lossy();
        if std::fs::read_to_string(&target).is_ok_and(|current| {
            current != entry
                && current
                    .lines()
                    .any(|line| line.starts_with("Exec=") && line.contains(&*path))
        }) {
            return Ok(());
        }

        // Icons go first so the entry never appears without one.
        let icon_sizes = appdir.join("usr/share/icons/hicolor");
        for size in read_dir(&icon_sizes)? {
            let icon = size.join("apps/ovrley.png");
            if icon.is_file() {
                let size = size.file_name().ok_or("icon size directory has no name")?;
                let target = data_dir
                    .join("icons/hicolor")
                    .join(size)
                    .join("apps/ovrley.png");
                write_if_changed(
                    &target,
                    &std::fs::read(&icon).map_err(|error| error.to_string())?,
                )?;
            }
        }

        write_if_changed(&target, entry.as_bytes())
    }

    /// Points the bundled entry, whose `Exec` names the binary inside the image, at the AppImage file.
    /// `TryExec` makes launchers hide the entry once that file is deleted.
    fn launch_from(entry: &str, appimage: &Path) -> String {
        let path = appimage.to_string_lossy();
        // Desktop Entry spec: quote the argument, backslash-escape `"`, `` ` ``, `$` and `\` inside the
        // quotes, then escape those backslashes again as string-value escapes; `%` is written `%%`.
        let mut exec = String::from("Exec=\"");
        for character in path.chars() {
            match character {
                '"' | '`' | '$' => exec.extend(['\\', '\\', character]),
                '\\' => exec.push_str("\\\\\\\\"),
                '%' => exec.push_str("%%"),
                _ => exec.push(character),
            }
        }
        exec.push('"');

        entry
            .lines()
            .filter(|line| !line.starts_with("TryExec="))
            .map(|line| {
                if line.starts_with("Exec=") {
                    format!("TryExec={path}\n{exec}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect()
    }

    fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
        std::fs::read_dir(dir)
            .and_then(|entries| {
                entries
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect()
            })
            .map_err(|error| format!("{}: {error}", dir.display()))
    }

    fn write_if_changed(target: &Path, contents: &[u8]) -> Result<(), String> {
        if std::fs::read(target).is_ok_and(|current| current == contents) {
            return Ok(());
        }
        let parent = target.parent().ok_or("target path has no parent")?;
        std::fs::create_dir_all(parent)
            .and_then(|()| std::fs::write(target, contents))
            .map_err(|error| format!("{}: {error}", target.display()))
    }

    #[cfg(test)]
    mod tests {
        use super::launch_from;
        use std::path::Path;

        #[test]
        fn entry_launches_the_appimage_with_spec_escaping() {
            let entry = "[Desktop Entry]\nName=OVRLEY\nExec=OVRLEY\nIcon=ovrley\n";
            assert_eq!(
                launch_from(entry, Path::new("/home/a b/$OVRLEY 100%.AppImage")),
                "[Desktop Entry]\nName=OVRLEY\nTryExec=/home/a b/$OVRLEY 100%.AppImage\n\
                 Exec=\"/home/a b/\\\\$OVRLEY 100%%.AppImage\"\nIcon=ovrley\n"
            );
        }
    }
}

#[cfg(target_os = "linux")]
pub use appimage::register_appimage;

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
        assert!(RuntimeRoots::resolve(platform_dirs(None, None)).is_err());
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
