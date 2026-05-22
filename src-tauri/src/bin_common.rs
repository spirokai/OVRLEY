//! Shared CLI argument helpers for diagnostic binaries.
//!
//! These helpers use `Result<T, String>` so they work without
//! additional dependencies. Binaries that need `anyhow::Result`
//! can wrap with `.map_err(|e| anyhow::anyhow!("{e}"))?`.
//!
//! Each binary uses a different subset — dead code warnings are
//! expected and suppressed at the module level.

#![allow(dead_code)]

use std::path::PathBuf;

/// Resolves the repository root from `CARGO_MANIFEST_DIR`.
///
/// Walks one directory up from the Tauri crate directory to reach the
/// workspace root where fixtures, fonts, and templates live.
pub fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Failed to resolve repo root".to_string())
}

/// Reads a required `--flag value` pair from the argument list.
///
/// Returns an error if the flag is absent or has no following value.
pub fn read_arg(flag: &str, args: &[String]) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("Missing required argument: {flag}"))
}

/// Reads an optional `--flag value` pair. Returns `None` when absent.
pub fn read_optional_arg(flag: &str, args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

/// Reads a positional (non-flag) argument by index. Returns `None` if out of range.
pub fn read_positional(index: usize, args: &[String]) -> Option<String> {
    let non_flag = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .collect::<Vec<_>>();
    non_flag.get(index).map(|s| (*s).clone())
}

/// Resolves a path: tries absolute, then relative-to-cwd, then relative-to-root.
///
/// Returns the first path variant that exists on disk. Falls back to the
/// original input if none of the resolved variants are found.
pub fn resolve_path(input: &PathBuf, root: &PathBuf) -> PathBuf {
    if input.is_absolute() {
        input.clone()
    } else if input.exists() {
        input.clone()
    } else {
        let rooted = root.join(input);
        if rooted.exists() {
            rooted
        } else {
            input.clone()
        }
    }
}

/// Formats seconds as `MM:SS` for human-readable timing output.
pub fn format_mmss(secs: f64) -> String {
    let total = secs.round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes:02}:{seconds:02}")
}

/// Returns the current Unix timestamp in seconds as a string.
pub fn unix_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
