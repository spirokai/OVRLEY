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
