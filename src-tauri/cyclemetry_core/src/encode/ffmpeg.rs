use std::env;
use std::path::{Path, PathBuf};

pub fn resolve_ffmpeg_binary(repo_root: &Path) -> Result<PathBuf, String> {
    let mut candidate_paths = Vec::new();

    if let Some(env_override) = env::var_os("CYCLEMETRY_FFMPEG").or_else(|| env::var_os("FFMPEG_BINARY")) {
        candidate_paths.push(PathBuf::from(env_override));
    }

    let local_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    candidate_paths.push(repo_root.join("backend").join(local_name));

    for candidate in candidate_paths {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(path) = find_in_path(local_name) {
        return Ok(path);
    }

    Err(
        "ffmpeg executable not found. Install ffmpeg and add it to PATH or set CYCLEMETRY_FFMPEG."
            .to_string(),
    )
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary_name))
        .find(|candidate| candidate.is_file())
}
