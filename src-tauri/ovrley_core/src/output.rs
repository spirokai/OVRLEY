//! Request-owned production render output contracts.

use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

static LAST_SUGGESTED_SECONDS: AtomicI64 = AtomicI64::new(0);

/// The two production output containers supported by OVRLEY.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RenderOutputKind {
    Transparent,
    Composite,
}

impl RenderOutputKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Transparent => "mov",
            Self::Composite => "mp4",
        }
    }

    pub fn filename_prefix(self) -> &'static str {
        match self {
            Self::Transparent => "overlay",
            Self::Composite => "video",
        }
    }
}

/// A validated, request-specific production output destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOutputTarget {
    path: PathBuf,
}

impl RenderOutputTarget {
    /// Validates and probes one exact output path.
    pub fn validate(raw_path: &str, kind: RenderOutputKind, overwrite: bool) -> CoreResult<Self> {
        if raw_path.trim().is_empty() {
            return Err(CoreError::OutputInvalid("Choose an output file".into()));
        }

        let path = PathBuf::from(raw_path);
        if !path.is_absolute() {
            return Err(CoreError::OutputInvalid(format!(
                "Choose a complete output path, including its folder: {}",
                path.display()
            )));
        }

        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                CoreError::OutputInvalid(format!(
                    "The output path must include a file name: {}",
                    path.display()
                ))
            })?;
        if filename.is_empty() || filename == "." || filename == ".." {
            return Err(CoreError::OutputInvalid(format!(
                "The output path must include a file name: {}",
                path.display()
            )));
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                CoreError::OutputInvalid(format!(
                    "The output file must use .{}: {}",
                    kind.extension(),
                    path.display()
                ))
            })?;
        if !extension.eq_ignore_ascii_case(kind.extension()) {
            return Err(CoreError::OutputInvalid(format!(
                "The output file must use .{}: {}",
                kind.extension(),
                path.display()
            )));
        }

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                let _cleanup = ProbeCleanup::new(path.clone(), file);
                Ok(Self { path })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !overwrite {
                    return Err(CoreError::OutputExists(path.display().to_string()));
                }

                let metadata = fs::metadata(&path).map_err(|source| CoreError::OutputIo {
                    path: path.clone(),
                    source,
                })?;
                if !metadata.is_file() {
                    return Err(CoreError::OutputInvalid(format!(
                        "The selected output is not a file: {}",
                        path.display()
                    )));
                }

                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|source| CoreError::OutputIo {
                        path: path.clone(),
                        source,
                    })?;
                Ok(Self { path })
            }
            Err(source) => Err(CoreError::OutputIo { path, source }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn filename(&self) -> &str {
        self.path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("RenderOutputTarget always has a Unicode filename")
    }
}

struct ProbeCleanup {
    path: PathBuf,
    file: Option<File>,
}

impl ProbeCleanup {
    fn new(path: PathBuf, file: File) -> Self {
        Self {
            path,
            file: Some(file),
        }
    }
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        self.file.take();
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != io::ErrorKind::NotFound {
                log::warn!(
                    "Could not remove output probe {}: {error}",
                    self.path.display()
                );
            }
        }
    }
}

/// Returns a fresh suggested absolute production output path.
pub fn suggest_output_path(
    paths: &AppPaths,
    kind: RenderOutputKind,
    remembered_directory: Option<&Path>,
) -> CoreResult<PathBuf> {
    let directory = remembered_directory.unwrap_or(&paths.downloads_dir);
    if !directory.is_absolute() {
        return Err(CoreError::Config(format!(
            "Remembered render output directory must be absolute: {}",
            directory.display()
        )));
    }

    let timestamp = suggested_timestamp()?;
    Ok(directory.join(format!(
        "{}_{}.{}",
        kind.filename_prefix(),
        timestamp,
        kind.extension()
    )))
}

fn suggested_timestamp() -> CoreResult<String> {
    let now_seconds = Utc::now().timestamp();
    let mut previous_seconds = LAST_SUGGESTED_SECONDS.load(Ordering::Relaxed);
    let unique_seconds = loop {
        let candidate = now_seconds.max(previous_seconds.saturating_add(1));
        match LAST_SUGGESTED_SECONDS.compare_exchange_weak(
            previous_seconds,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break candidate,
            Err(observed) => previous_seconds = observed,
        }
    };
    let timestamp = DateTime::<Utc>::from_timestamp(unique_seconds, 0)
        .ok_or_else(|| CoreError::Encode("Failed to format render output timestamp".into()))?
        .with_timezone(&Local);

    Ok(format!(
        "{:02}{:02}{:02}_{:02}{:02}{:02}",
        timestamp.year().rem_euclid(100),
        timestamp.month(),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_target(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ovrley-output-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn validates_new_existing_and_authorized_existing_targets() {
        let directory = temp_target("branches");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("custom.mov");

        let target = RenderOutputTarget::validate(
            path.to_str().unwrap(),
            RenderOutputKind::Transparent,
            false,
        )
        .unwrap();
        assert_eq!(target.path(), path);
        assert!(!path.exists());

        let missing_parent = directory.join("missing-parent").join("nested.mov");
        assert!(matches!(
            RenderOutputTarget::validate(
                missing_parent.to_str().unwrap(),
                RenderOutputKind::Transparent,
                false
            ),
            Err(CoreError::OutputIo { .. })
        ));

        fs::write(&path, b"existing").unwrap();
        assert!(matches!(
            RenderOutputTarget::validate(
                path.to_str().unwrap(),
                RenderOutputKind::Transparent,
                false
            ),
            Err(CoreError::OutputExists(_))
        ));
        let target = RenderOutputTarget::validate(
            path.to_str().unwrap(),
            RenderOutputKind::Transparent,
            true,
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"existing");
        assert_eq!(target.filename(), "custom.mov");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn suggested_timestamps_are_compact_and_unique() {
        let first = suggested_timestamp().unwrap();
        let second = suggested_timestamp().unwrap();

        assert_eq!(first.len(), 13);
        assert_ne!(first, second);
    }
}
