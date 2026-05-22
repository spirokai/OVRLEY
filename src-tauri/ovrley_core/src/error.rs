//! Structured error types and result alias.
//!
//! Owns: `CoreError` (the single error enum for the entire core crate) and the
//!       `CoreResult<T>` type alias. Every fallible function in the crate returns
//!       `CoreResult<T>` instead of `Result<T, String>`.
//! Does not own: individual domain error types (a flat enum is used instead of
//!       sub-error enums per the refactor plan — split only when a domain grows
//!       large enough to warrant it).
//!
//! Allowed dependencies: `std`, `thiserror`, `serde_json`.
//! Forbidden dependencies: all other crate modules (this is a leaf dependency).
//!
//! Related modules: consumed by every module in the crate via `use crate::error::CoreResult`.
//!
//! ## Display Contract
//! Every variant's `#[error("...")]` message is user-visible at the Tauri boundary
//! (via `.to_string()`). Messages should be readable by end users, not Rust developers.
//! Avoid leaking internal implementation details (paths may be an exception when
//! they help the user diagnose file-system issues).
//!
//! ## Thread Safety
//! `CoreError` is `Send + Sync` (all contained types satisfy those bounds).
//! No shared mutable state.

use std::path::PathBuf;
use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

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
