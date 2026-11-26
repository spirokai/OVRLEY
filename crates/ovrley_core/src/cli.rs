//! Helpers shared by the diagnostic binaries in `src/bin`: argument reading, path resolution,
//! and benchmark scoring, cooldowns and codec lookups.

use std::path::{Path, PathBuf};
use crate::encode::ffmpeg::catalog::{CompositeCodecId, TransparentCodecId};
use crate::encode::ffmpeg::detect::AvailableCodecs;
use std::fs;
use std::time::Duration;



/// Resolves the repository root from `CARGO_MANIFEST_DIR`.
///
/// Walks two directories up from the crate to reach the workspace root,
/// where fonts and templates live.
pub fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
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
pub fn resolve_path(input: &Path, root: &Path) -> PathBuf {
    if input.is_absolute() || input.exists() {
        input.to_path_buf()
    } else {
        let rooted = root.join(input);
        if rooted.exists() {
            rooted
        } else {
            input.to_path_buf()
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



#[cfg(windows)]
extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

#[cfg(windows)]
const ES_CONTINUOUS: u32 = 0x8000_0000;
#[cfg(windows)]
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
#[cfg(windows)]
const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;

/// Prevents the system from sleeping, hibernating, or turning off the display
/// during long-running benchmark groups. No-op on non-Windows platforms.
pub fn prevent_sleep() {
    #[cfg(windows)]
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
    }
}

/// Shared metric for one successful benchmark run.
#[derive(Debug, Clone, Copy)]
pub struct CommonRunMetrics {
    /// Wall-clock elapsed time in seconds.
    pub job_time_seconds: f64,
    /// Output file size in megabytes.
    pub file_size_mb: f64,
}

/// Averaged metrics computed from a batch of successful runs.
#[derive(Debug, Clone)]
pub struct AverageRunMetrics {
    /// Human-readable averaged time (MM:SS).
    pub job_time: String,
    /// Averaged wall-clock time in seconds.
    pub job_time_seconds: f64,
    /// Averaged output file size in megabytes.
    pub file_size_mb: f64,
}

/// Computes average time and file size from a batch of successful-run metrics.
///
/// Returns `None` when the batch is empty (no successful runs).
pub fn average_successful_runs(successful_runs: &[CommonRunMetrics]) -> Option<AverageRunMetrics> {
    if successful_runs.is_empty() {
        return None;
    }
    let count = successful_runs.len() as f64;
    let avg_time = successful_runs
        .iter()
        .map(|r| r.job_time_seconds)
        .sum::<f64>()
        / count;
    let avg_size = successful_runs.iter().map(|r| r.file_size_mb).sum::<f64>() / count;
    Some(AverageRunMetrics {
        job_time: format_mmss(avg_time),
        job_time_seconds: avg_time,
        file_size_mb: avg_size,
    })
}

/// Returns the count of successful and failed runs.
///
/// Successful runs are taken from the `successful_runs` slice; total run count
/// must be provided separately because binary-specific `RunResult` structs
/// have different shapes.
pub fn summarize_run_outcome(successful: usize, total_runs: usize) -> (u32, u32) {
    let successful_count = successful as u32;
    let failed_count = total_runs as u32 - successful_count;
    (successful_count, failed_count)
}

/// Sleeps for 60 seconds between benchmark iterations within a codec or update-rate group.
pub fn sleep_between_benchmark_runs() {
    println!("      Cooldown 60s...");
    std::thread::sleep(Duration::from_secs(60));
}

/// Sleeps for 60 seconds between benchmark groups (codec or update rate).
///
/// Useful to let thermal throttling recover before the next group.
pub fn sleep_between_benchmark_groups(group_label: &str) {
    println!("      {group_label} done, cooldown 60s before next group...");
    std::thread::sleep(Duration::from_secs(60));
}

/// Returns the size of the file at `path` in megabytes, or `0.0` if the file
/// does not exist or metadata cannot be read.
pub fn file_size_mb(path: &Path) -> f64 {
    fs::metadata(path)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0)
}

/// Checks whether a composite codec profile is available, using the shared
/// codec catalog for alias resolution rather than repeating field-name matches.
pub fn is_composite_codec_available(codecs: &AvailableCodecs, external_name: &str) -> bool {
    CompositeCodecId::from_external_name(external_name)
        .is_some_and(|id| codecs.has_composite_codec(id))
}

/// Checks whether a transparent codec is available, using the shared codec
/// catalog for alias resolution.
pub fn is_transparent_codec_available(codecs: &AvailableCodecs, name: &str) -> bool {
    TransparentCodecId::from_external_name(name).is_some_and(|id| codecs.has_transparent_codec(id))
}

