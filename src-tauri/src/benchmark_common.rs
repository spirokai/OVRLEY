//! Shared benchmark infrastructure for the diagnostic benchmark binaries.
//!
//! Owns: run-metric helpers, averaging calculations, sleep-prevention utilities,
//!       cooldown helpers, file-size normalization, and codec-availability
//!       adapters backed by the shared codec catalog in `ovrley_core`.
//! Does not own: binary-specific output schemas, CLI argument parsing, template
//!       config wiring, or render-dispatch logic.
//!
//! Each benchmark binary keeps its own `BenchmarkOutput`, `RunResult`, and
//! codec-entry structs where their output shapes legitimately differ. Only
//! truly shared scoring, sleep, and catalogue-lookup utilities are extracted.
//!
//! This module is pulled in by each benchmark binary via `#[path]` import
//! (same pattern as `bin_common.rs`). Each binary uses a different subset —
//! dead-code warnings are expected and suppressed at the module level.

#![allow(dead_code)]

use ovrley_core::encode::codec_catalog::{CompositeCodecId, TransparentCodecId};
use ovrley_core::encode::codec_detect::AvailableCodecs;
use std::fs;
use std::path::Path;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Windows sleep-prevention helpers (benchmark runs can be long)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Run-metric types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Averaging
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Cooldown / sleep
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// File-size normalization
// ---------------------------------------------------------------------------

/// Returns the size of the file at `path` in megabytes, or `0.0` if the file
/// does not exist or metadata cannot be read.
pub fn file_size_mb(path: &Path) -> f64 {
    fs::metadata(path)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Codec availability adapters (catalog-backed)
// ---------------------------------------------------------------------------

/// Checks whether a composite codec profile is available, using the shared
/// codec catalog for alias resolution rather than repeating field-name matches.
pub fn is_composite_codec_available(codecs: &AvailableCodecs, display_name: &str) -> bool {
    CompositeCodecId::from_alias(display_name).is_some_and(|id| codecs.has_composite_codec(id))
}

/// Checks whether a transparent codec is available, using the shared codec
/// catalog for alias resolution.
pub fn is_transparent_codec_available(codecs: &AvailableCodecs, name: &str) -> bool {
    TransparentCodecId::from_alias(name).is_some_and(|id| codecs.has_transparent_codec(id))
}

// ---------------------------------------------------------------------------
// Trivial local format helper (avoids importing bin_common from a shared module)
// ---------------------------------------------------------------------------

/// Formats seconds as `MM:SS` for human-readable timing output.
fn format_mmss(secs: f64) -> String {
    let total = secs.round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes:02}:{seconds:02}")
}
