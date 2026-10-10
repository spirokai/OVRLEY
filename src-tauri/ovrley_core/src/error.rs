//! Structured error types and result alias.
//!
//! Owns: `CoreError` (the single error enum for the entire core crate) and the
//!       `CoreResult<T>` type alias. Every fallible function in the crate returns
//!       `CoreResult<T>` instead of `Result<T, String>`.
//! Does not own: individual domain error types (a flat enum is used instead of
//!       sub-error enums per the refactor plan — split only when a domain grows
//!       large enough to warrant it).
//!
//! Allowed dependencies: `std`, `thiserror`, `serde`, `serde_json`.
//! Forbidden dependencies: all other crate modules (this is a leaf dependency).
//!
//! Related modules: consumed by every module in the crate via `use crate::error::CoreResult`.
//!
//! ## Display Contract
//! Render path errors cross the Tauri boundary as structured reasons and parameters;
//! the frontend owns their localized text. Other variants use `.to_string()` there.
//! Display messages should be readable by end users, not Rust developers.
//! Avoid leaking internal implementation details (paths may be an exception when
//! they help the user diagnose file-system issues).
//!
//! ## Thread Safety
//! `CoreError` is `Send + Sync` (all contained types satisfy those bounds).
//! No shared mutable state.

use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

/// Stable render path reasons for frontend localization. Display text is for native diagnostics.
#[derive(Error, Debug, Serialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum RenderPathError {
    #[error("Choose an output file")]
    OutputFileRequired,
    #[error("Choose a complete output path, including its folder: {path}")]
    OutputPathIncomplete { path: String },
    #[error("The output path must include a file name: {path}")]
    OutputFilenameRequired { path: String },
    #[error("The output file must use .{extension}: {path}")]
    OutputExtensionInvalid {
        path: String,
        extension: &'static str,
    },
    #[error("The selected output is not a file: {path}")]
    OutputNotFile { path: String },
    #[error("Choose an existing output directory")]
    OutputDirectoryInvalid,
    #[error("Source has no Unicode filename stem: {path}")]
    SourceFilenameInvalid { path: String },
    #[error("Batch output aliases an input: {path}")]
    OutputAliasesInput { path: String },
    #[error("Batch outputs have conflicting destinations: {path}")]
    OutputDestinationsConflict { path: String },
    #[error("The output directory does not exist: {path}")]
    OutputDirectoryMissing { path: String },
    #[error("You do not have permission to write the output file: {path}")]
    OutputPermissionDenied { path: String },
    #[error("The output file name or path is not valid: {path}")]
    OutputPathInvalid { path: String },
    #[error("Could not create or write the output file at {path}: {detail}")]
    OutputWriteFailed { path: String, detail: String },
    #[error("Choose a complete video directory path: {path}")]
    VideoDirectoryInvalid { path: String },
    #[error("The video directory does not exist: {path}")]
    VideoDirectoryMissing { path: String },
    #[error("You do not have permission to read the video directory: {path}")]
    VideoDirectoryPermissionDenied { path: String },
    #[error("Could not read the video directory at {path}: {detail}")]
    VideoDirectoryReadFailed { path: String, detail: String },
}

impl RenderPathError {
    pub fn output_io(path: PathBuf, source: std::io::Error) -> Self {
        match source.kind() {
            std::io::ErrorKind::NotFound => Self::OutputDirectoryMissing {
                path: path.parent().unwrap_or(&path).display().to_string(),
            },
            std::io::ErrorKind::PermissionDenied => Self::OutputPermissionDenied {
                path: path.display().to_string(),
            },
            std::io::ErrorKind::InvalidInput => Self::OutputPathInvalid {
                path: path.display().to_string(),
            },
            _ => Self::OutputWriteFailed {
                path: path.display().to_string(),
                detail: source.to_string(),
            },
        }
    }

    pub fn video_directory_io(path: &std::path::Path, source: std::io::Error) -> Self {
        match source.kind() {
            std::io::ErrorKind::NotFound => Self::VideoDirectoryMissing {
                path: path.display().to_string(),
            },
            std::io::ErrorKind::PermissionDenied => Self::VideoDirectoryPermissionDenied {
                path: path.display().to_string(),
            },
            _ => Self::VideoDirectoryReadFailed {
                path: path.display().to_string(),
                detail: source.to_string(),
            },
        }
    }
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Activity parse error: {0}")]
    Activity(String),

    #[error("Render error: {0}")]
    Render(String),

    #[error("Encoding error: {0}")]
    Encode(String),

    #[error("Output already exists: {0}")]
    OutputExists(String),

    #[error("Invalid output: {0}")]
    OutputInvalid(RenderPathError),

    #[error("IO error at {path}: {source}")]
    OutputIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("IO error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("FFmpeg error (exit {status}): {stderr}")]
    Ffmpeg {
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("FFmpeg not found: {0}")]
    FfmpegNotFound(String),

    #[error("Render cancelled")]
    Cancelled,

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
