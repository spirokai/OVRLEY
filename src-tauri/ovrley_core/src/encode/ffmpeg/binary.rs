//! ffmpeg discovery and process helpers.
//!
//! Resolves the ffmpeg binary and provides platform-specific process
//! configuration. Running tools is owned by
//! [`process`](crate::encode::ffmpeg::process); codec settings construction by
//! [`settings`](crate::encode::ffmpeg::settings).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{CoreError, CoreResult};

/// Resolves the ffmpeg executable used for previews and encoding.
///
/// Search order is: explicit environment override, the bundled vendor path,
/// then `PATH`. Returning a concrete path lets health checks and
/// render failures show actionable messages.
pub fn resolve_ffmpeg_binary(repo_root: &Path) -> CoreResult<PathBuf> {
    let mut candidate_paths = Vec::new();

    if let Some(env_override) = env::var_os("OVRLEY_FFMPEG") {
        candidate_paths.push(PathBuf::from(env_override));
    }

    let local_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    candidate_paths.push(
        repo_root
            .join("vendor")
            .join("ffmpeg")
            .join("bin")
            .join(local_name),
    );
    for candidate in candidate_paths {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(path) = find_in_path(local_name) {
        return Ok(path);
    }

    Err(CoreError::FfmpegNotFound(
        "ffmpeg executable not found. Run pnpm install, install ffmpeg on PATH, or set OVRLEY_FFMPEG."
            .to_string(),
    ))
}

/// Applies platform-specific process configuration for FFmpeg tools.
pub fn configure_ffmpeg_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    let _ = command;
}

// Searches the process PATH for a binary with the requested platform filename.
fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary_name))
        .find(|candidate| candidate.is_file())
}
