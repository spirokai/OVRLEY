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
static NEXT_BATCH_PROBE: AtomicI64 = AtomicI64::new(0);

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

/// Plans mandatory batch names and validates the entire set before probing
/// destinations. Existing outputs may be overwritten; input aliases may not.
pub fn plan_batch_output_targets(
    directory: &Path,
    kind: RenderOutputKind,
    sources: &[PathBuf],
    calibration_source: Option<&Path>,
) -> CoreResult<Vec<RenderOutputTarget>> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(CoreError::OutputInvalid(
            "Choose an existing absolute output directory".into(),
        ));
    }
    let directory = fs::canonicalize(directory).map_err(|source| CoreError::OutputIo {
        path: directory.to_path_buf(),
        source,
    })?;
    let case_sensitive = directory_is_case_sensitive(&directory)?;
    let file_handle = |path: &Path| {
        same_file::Handle::from_path(path).map_err(|source| CoreError::OutputIo {
            path: path.to_path_buf(),
            source,
        })
    };
    let input_handles = sources
        .iter()
        .map(PathBuf::as_path)
        .chain(calibration_source)
        .map(file_handle)
        .collect::<CoreResult<Vec<_>>>()?;
    let mut output_handles = Vec::new();
    let mut destinations = Vec::with_capacity(sources.len());
    let mut keys = std::collections::HashSet::new();
    for source in sources {
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                CoreError::OutputInvalid(format!(
                    "Source has no Unicode filename stem: {}",
                    source.display()
                ))
            })?;
        let filename = format!("{stem}_{}.{}", kind.filename_prefix(), kind.extension());
        let target = directory.join(&filename);
        let key = if case_sensitive {
            filename
        } else {
            filename.to_lowercase()
        };
        let handle = target.exists().then(|| file_handle(&target)).transpose()?;
        let aliases_input = handle
            .as_ref()
            .is_some_and(|handle| input_handles.contains(handle));
        if aliases_input {
            return Err(CoreError::OutputInvalid(format!(
                "Batch output aliases an input: {}",
                target.display()
            )));
        }
        let aliases_output = handle
            .as_ref()
            .is_some_and(|handle| output_handles.contains(handle));
        if !keys.insert(key) || aliases_output {
            return Err(CoreError::OutputInvalid(format!(
                "Batch outputs have conflicting destinations: {}",
                target.display()
            )));
        }
        destinations.push(target);
        if let Some(handle) = handle {
            output_handles.push(handle);
        }
    }
    destinations
        .iter()
        .map(|path| {
            RenderOutputTarget::validate(
                path.to_str().expect("validated Unicode output"),
                kind,
                true,
            )
        })
        .collect()
}

/// Query the selected filesystem rather than assuming the host's default case
/// rules (case-sensitive directories and mounted volumes can differ).
fn directory_is_case_sensitive(directory: &Path) -> CoreResult<bool> {
    let id = NEXT_BATCH_PROBE.fetch_add(1, Ordering::Relaxed);
    let name = format!(".ovrley-case-probe-{}-{id}", std::process::id());
    let path = directory.join(&name);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| CoreError::OutputIo {
            path: path.clone(),
            source,
        })?;
    let _cleanup = ProbeCleanup::new(path, file);
    Ok(!directory.join(name.to_uppercase()).exists())
}

/// Submission must reuse the reviewed destinations, never repair a mismatch.
pub fn verify_batch_output_paths(
    targets: &[RenderOutputTarget],
    submitted: &[String],
) -> CoreResult<()> {
    if targets.len() != submitted.len()
        || targets
            .iter()
            .zip(submitted)
            .any(|(target, path)| target.path() != Path::new(path))
    {
        return Err(CoreError::OutputInvalid(
            "Batch destinations do not match the planned output directory and naming convention"
                .into(),
        ));
    }
    Ok(())
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
    fn batch_outputs_reject_input_and_calibration_aliases_before_overwrite() {
        let directory = temp_target("batch-aliases");
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("ride.mp4");
        let target = directory.join("ride_video.mp4");
        fs::write(&source, b"source video").unwrap();
        fs::write(&target, b"reference video").unwrap();
        for (inputs, reference) in [
            (vec![source.clone(), target.clone()], None),
            (vec![source.clone()], Some(target.as_path())),
        ] {
            let error = plan_batch_output_targets(
                &directory,
                RenderOutputKind::Composite,
                &inputs,
                reference,
            )
            .unwrap_err();
            assert!(error.to_string().contains("aliases an input"));
            assert_eq!(fs::read(&target).unwrap(), b"reference video");
        }
        fs::remove_file(&target).unwrap();
        fs::hard_link(&source, &target).unwrap();
        assert!(plan_batch_output_targets(
            &directory,
            RenderOutputKind::Composite,
            &[source.clone()],
            None
        )
        .is_err());
        assert_eq!(fs::read(&source).unwrap(), b"source video");
        fs::remove_file(&target).unwrap();
        let targets =
            plan_batch_output_targets(&directory, RenderOutputKind::Composite, &[source], None)
                .unwrap();
        verify_batch_output_paths(&targets, &[targets[0].path().to_str().unwrap().into()]).unwrap();
        assert!(verify_batch_output_paths(
            &targets,
            &[directory.join("wrong.mp4").to_str().unwrap().into()]
        )
        .is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn suggested_timestamps_are_compact_and_unique() {
        let first = suggested_timestamp().unwrap();
        let second = suggested_timestamp().unwrap();

        assert_eq!(first.len(), 13);
        assert_ne!(first, second);
    }
}
