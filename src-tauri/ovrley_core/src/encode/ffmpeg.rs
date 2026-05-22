//! ffmpeg discovery and process helpers.
//!
//! Resolves the ffmpeg binary and provides platform-specific process-launch
//! utilities. Codec settings construction has moved to
//! [`ffmpeg_settings`](crate::encode::ffmpeg_settings).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{CoreError, CoreResult};

/// Resolves the ffmpeg executable used for previews, encoding, and stitching.
///
/// Search order is: explicit environment override, known app-local vendor
/// locations, then `PATH`. Returning a concrete path lets health checks and
/// render failures show actionable messages.
pub fn resolve_ffmpeg_binary(repo_root: &Path) -> CoreResult<PathBuf> {
    let mut candidate_paths = Vec::new();

    if let Some(env_override) =
        env::var_os("OVRLEY_FFMPEG").or_else(|| env::var_os("FFMPEG_BINARY"))
    {
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
    candidate_paths.push(repo_root.join("ffmpeg").join("bin").join(local_name));
    candidate_paths.push(repo_root.join(".ffmpeg").join("bin").join(local_name));
    candidate_paths.push(repo_root.join(".ffmpeg").join(local_name));
    candidate_paths.push(repo_root.join("backend").join(local_name));

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

#[cfg(windows)]
/// Prevents spawned ffmpeg/explorer processes from opening console windows.
pub fn suppress_child_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
/// No-op console suppression on platforms without Windows creation flags.
pub fn suppress_child_console(_command: &mut Command) {}

// Searches the process PATH for a binary with the requested platform filename.
fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary_name))
        .find(|candidate| candidate.is_file())
}
