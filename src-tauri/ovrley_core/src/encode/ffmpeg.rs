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

/// Applies platform-specific process configuration for bundled FFmpeg tools.
///
/// Windows release builds hide the child console window. Linux portable builds
/// ship FFmpeg as a shared build, so subprocesses need the sibling `lib`
/// directory on `LD_LIBRARY_PATH` even when the app was not started through the
/// portable launcher.
pub fn configure_ffmpeg_command(command: &mut Command, binary_path: &Path) {
    suppress_child_console(command);
    apply_bundled_ffmpeg_library_path(command, binary_path);
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

#[cfg(target_os = "linux")]
fn apply_bundled_ffmpeg_library_path(command: &mut Command, binary_path: &Path) {
    if !is_bundled_ffmpeg_tool(binary_path) {
        return;
    }

    let Some(bin_dir) = binary_path.parent() else {
        return;
    };
    let Some(ffmpeg_dir) = bin_dir.parent() else {
        return;
    };
    let lib_dir = ffmpeg_dir.join("lib");
    if !lib_dir.is_dir() {
        return;
    }

    let mut value = lib_dir.to_string_lossy().to_string();
    if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
        let existing = existing.to_string_lossy();
        if !existing.is_empty() {
            value.push(':');
            value.push_str(&existing);
        }
    }
    command.env("LD_LIBRARY_PATH", value);
}

#[cfg(not(target_os = "linux"))]
fn apply_bundled_ffmpeg_library_path(_command: &mut Command, _binary_path: &Path) {}

#[cfg(target_os = "linux")]
fn is_bundled_ffmpeg_tool(binary_path: &Path) -> bool {
    let Some(bin_dir) = binary_path.parent() else {
        return false;
    };
    if bin_dir.file_name().and_then(|name| name.to_str()) != Some("bin") {
        return false;
    }

    let Some(ffmpeg_dir) = bin_dir.parent() else {
        return false;
    };
    if ffmpeg_dir.file_name().and_then(|name| name.to_str()) != Some("ffmpeg") {
        return false;
    }

    ffmpeg_dir
        .parent()
        .and_then(|name| name.file_name())
        .and_then(|name| name.to_str())
        == Some("vendor")
}

// Searches the process PATH for a binary with the requested platform filename.
fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary_name))
        .find(|candidate| candidate.is_file())
}
