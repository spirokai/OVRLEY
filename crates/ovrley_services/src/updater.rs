//! The app's updater, on cargo-packager-updater: the release channel's endpoint and public key, the
//! check, the verified download, the installation and the relaunch into the installed version.
//! When to check and what the user sees are the app's.

use cargo_packager_updater::{Config, Update, WindowsConfig, WindowsUpdateInstallMode};
use std::cell::Cell;

/// The manifest of the latest release, which every installed release reads.
const ENDPOINT: &str = "https://github.com/spirokai/OVRLEY/releases/latest/download/latest.json";

/// The minisign public key of every OVRLEY release.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDBENUE0NkEyQ0UwMzMyQ0IKUldUTE1nUE9va1phRFF1Q0hvSGZoQW5GOGRkYkwyOEFUblhtV3FmWnh4WFdyaXFNL0VWNjg1b2cK";

/// How much of an update has arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    /// `None` when the server sends no length.
    pub content_length: Option<u64>,
}

/// A release newer than the running app.
#[derive(Debug)]
pub struct ReleaseUpdate(Update);

/// The release newer than `current_version`, if there is one. Blocking.
///
/// # Errors
///
/// When `current_version` is not a version, or the manifest cannot be fetched or verified.
pub fn check(current_version: &str) -> Result<Option<ReleaseUpdate>, String> {
    let current_version = current_version
        .parse()
        .map_err(|error| format!("The app's version {current_version} is invalid: {error}"))?;
    let config = Config {
        endpoints: vec![ENDPOINT.parse().expect("the endpoint is a URL")],
        pubkey: PUBLIC_KEY.into(),
        windows: Some(WindowsConfig {
            install_mode: Some(WindowsUpdateInstallMode::Passive),
            // The installer then leaves the desktop shortcut as it is.
            installer_args: Some(vec!["/UPDATE".into()]),
        }),
        ..Config::default()
    };
    cargo_packager_updater::check_update(current_version, config)
        .map(|update| update.map(ReleaseUpdate))
        .map_err(|error| error.to_string())
}

impl ReleaseUpdate {
    #[must_use]
    pub fn version(&self) -> &str {
        &self.0.version
    }

    /// Downloads the package and verifies its signature and signed version. Blocking.
    ///
    /// # Errors
    ///
    /// When the download fails or the package's signature does not verify.
    pub fn download(&self, on_progress: &dyn Fn(DownloadProgress)) -> Result<Vec<u8>, String> {
        let downloaded = Cell::new(0);
        self.0
            .download_extended(
                |chunk, content_length| {
                    downloaded.set(downloaded.get() + chunk as u64);
                    on_progress(DownloadProgress { downloaded: downloaded.get(), content_length });
                },
                || {},
            )
            .map_err(|error| error.to_string())
    }

    /// Installs a downloaded package. Blocking. On Windows it starts the installer, which starts
    /// the app afterwards, and ends this process without running drops.
    ///
    /// # Errors
    ///
    /// When the package cannot be installed.
    pub fn install(&self, package: Vec<u8>) -> Result<(), String> {
        // The crate panics when it cannot start the Windows installer.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.0.install(package)))
            .map_err(|_| "The update's installer could not be started".to_string())?
            .map_err(|error| error.to_string())
    }
}

/// Starts the installed version with this process's arguments. The caller exits right afterwards.
///
/// # Errors
///
/// When the installed version cannot be started.
pub fn relaunch() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    // An AppImage starts from its own file. The running binary lies in the old file's mount.
    let appimage = if cfg!(target_os = "linux") { std::env::var_os("APPIMAGE") } else { None };
    let binary = match appimage {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::current_exe().map_err(|error| error.to_string())?,
    };
    let mut command = std::process::Command::new(&binary);
    command.args(&arguments);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::process::CommandExt;
        use std::process::Stdio;
        if let Some(bundle) = bundle_of(&binary) {
            // Through LaunchServices, so the new instance shares neither this one's output nor its
            // process group.
            match std::process::Command::new("/usr/bin/open").arg("-n").arg(bundle).arg("--args").args(&arguments).stdin(Stdio::null()).output() {
                Ok(output) if output.status.success() => return Ok(()),
                Ok(output) => log::error!("LaunchServices could not relaunch the app: {}", String::from_utf8_lossy(&output.stderr).trim()),
                Err(error) => log::error!("LaunchServices could not relaunch the app: {error}"),
            }
            command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
        }
    }
    command.spawn().map(drop).map_err(|error| format!("Could not start {}: {error}", binary.display()))
}

/// The bundle of `<bundle>/Contents/MacOS/<binary>`.
#[cfg(target_os = "macos")]
fn bundle_of(binary: &std::path::Path) -> Option<&std::path::Path> {
    let macos = binary.parent().filter(|path| path.ends_with("Contents/MacOS"))?;
    macos.parent()?.parent()
}

#[cfg(test)]
mod tests {
    use cargo_packager_updater::RemoteRelease;

    #[test]
    fn the_release_script_writes_a_manifest_the_updater_reads() {
        let staging = std::env::temp_dir().join(format!("ovrley-latest-json-{}", uuid::Uuid::new_v4()));
        let artifacts = staging.join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        for name in [
            "OVRLEY-win-9.8.7.exe",
            "OVRLEY-win-9.8.7.exe.sig",
            "OVRLEY-win-9.8.7-portable.zip",
            "OVRLEY-macos-9.8.7.dmg",
            "OVRLEY-macos-9.8.7.app.tar.gz",
            "OVRLEY-macos-9.8.7.app.tar.gz.sig",
            "OVRLEY-linux-9.8.7.AppImage",
            "OVRLEY-linux-9.8.7.AppImage.sig",
            "OVRLEY-linux-9.8.7-AppImage.zip",
        ] {
            // A signature is base64, which never holds an artifact's name.
            std::fs::write(artifacts.join(name), format!("content{}", name.len())).unwrap();
        }
        std::fs::write(staging.join("notes.txt"), "notes").unwrap();
        let written = std::process::Command::new("node")
            .arg(crate::runtime::source_repo_root().join("scripts/release/create-latest-json.mjs"))
            .args(["--owner", "spirokai", "--repo", "OVRLEY", "--tag", "9.8.7", "--version", "9.8.7"])
            .args(["--notes-file", "notes.txt", "--staging-dir", "artifacts", "--output", "latest.json"])
            .current_dir(&staging)
            .output()
            .expect("node runs the release script");
        assert!(written.status.success(), "{}", String::from_utf8_lossy(&written.stderr));

        let manifest = std::fs::read(staging.join("latest.json")).unwrap();
        let release: RemoteRelease = serde_json::from_slice(&manifest).unwrap();
        assert_eq!(release.version.to_string(), "9.8.7");
        for (target, format, file) in [
            ("windows-x86_64", "nsis", "OVRLEY-win-9.8.7.exe"),
            ("macos-aarch64", "app", "OVRLEY-macos-9.8.7.app.tar.gz"),
            // The key older installed releases read for the same file.
            ("darwin-aarch64", "app", "OVRLEY-macos-9.8.7.app.tar.gz"),
            ("linux-x86_64", "appimage", "OVRLEY-linux-9.8.7.AppImage"),
        ] {
            assert_eq!(release.format(target).unwrap().to_string(), format, "{target}");
            assert!(release.download_url(target).unwrap().as_str().ends_with(file), "{target}");
            assert_eq!(release.signature(target).unwrap(), &format!("content{}", file.len() + ".sig".len()), "{target}");
        }
        std::fs::remove_dir_all(staging).unwrap();
    }
}
