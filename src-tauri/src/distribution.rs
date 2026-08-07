use serde::Serialize;

#[cfg(windows)]
use std::path::PathBuf;

/// The two supported application distribution modes.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DistributionKind {
    Installed,
    Portable,
}

/// Detects the distribution mode once during Rust startup.
pub(crate) fn detect() -> Result<DistributionKind, String> {
    #[cfg(windows)]
    {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let executable_dir = executable.parent().ok_or_else(|| {
            format!(
                "Executable has no parent directory: {}",
                executable.display()
            )
        })?;
        let marker = PathBuf::from(executable_dir).join(".ovrley-portable");
        return match std::fs::metadata(&marker) {
            Ok(metadata) if metadata.is_file() => Ok(DistributionKind::Portable),
            Ok(_) => Err(format!(
                "Portable marker is not a file: {}",
                marker.display()
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(DistributionKind::Installed)
            }
            Err(error) => Err(format!(
                "Could not inspect portable marker {}: {error}",
                marker.display()
            )),
        };
    }

    #[cfg(not(windows))]
    {
        Ok(DistributionKind::Installed)
    }
}
