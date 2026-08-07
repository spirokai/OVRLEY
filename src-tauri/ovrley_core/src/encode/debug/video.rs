//! Debug artifact helpers for video rendering.
//!
//! Render diagnostics are written as JSON summaries and optional sample PNGs in
//! timestamped directories. The files are designed for performance comparison
//! across phases without coupling the main render loop to reporting details.

use crate::debug::TimingBucket;
use crate::encode::debug::round3;
use crate::encode::ffmpeg::binary::configure_ffmpeg_command;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use crate::render::{FrameSize, LabelCacheStatus};
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_TIMESTAMP_NANOS: AtomicU64 = AtomicU64::new(0);
pub(crate) const DEBUG_TIMING_RETENTION_LIMIT: usize = 20;
const TRANSPARENT_DEBUG_PHASE: &str = "phase_6";

#[derive(Serialize)]
/// Timing summary for one video render phase.
struct TimingSummary {
    phase: &'static str,
    timestamp: String,
    overlay_filename: String,
    fps: f64,
    layout_fps: f64,
    update_rate: u32,
    width: u32,
    height: u32,
    total_frames: u32,
    layout_total_frames: u32,
    rendered_frames: u32,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    timings: BTreeMap<String, TimingBucket>,
}

#[derive(Serialize)]
/// Timing summary for the asset-preparation step.
struct PrepareTimingSummary {
    total_ms: f64,
    timings: BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
}

/// Writes preparation timing data for one render.
pub(crate) fn write_prepare_summary(
    debug_dir: &Path,
    total_ms: f64,
    timings: BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
) -> CoreResult<()> {
    let summary = PrepareTimingSummary {
        total_ms,
        timings,
        label_cache_status,
    };
    write_json(
        debug_dir.join("prepare_render_assets_timing.json"),
        &summary,
    )
}

/// Writes aggregate timing data for one render phase.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_timing_summary(
    debug_dir: &Path,
    scene: &crate::normalize::ValidatedSceneConfig,
    output_path: &Path,
    total_frames: u32,
    layout_total_frames: u32,
    rendered_frames: u32,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    timings: BTreeMap<String, TimingBucket>,
) -> CoreResult<()> {
    let update_rate = scene.update_rate.get();
    let summary = TimingSummary {
        phase: TRANSPARENT_DEBUG_PHASE,
        timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        overlay_filename: output_path.to_string_lossy().to_string(),
        fps: scene.fps / f64::from(update_rate),
        layout_fps: scene.fps,
        update_rate,
        width: scene.width,
        height: scene.height,
        total_frames,
        layout_total_frames,
        rendered_frames,
        total_time_taken: round3(total_time_taken),
        sample_frame_indices,
        timings,
    };
    write_json(debug_dir.join("timing_summary.json"), &summary)?;
    let phase_dir = debug_dir.parent().ok_or_else(|| {
        CoreError::Encode(format!(
            "Render debug directory has no phase parent: {}",
            debug_dir.display()
        ))
    })?;
    prune_completed_timing_directories(phase_dir, DEBUG_TIMING_RETENTION_LIMIT);
    Ok(())
}

/// Creates a timestamped debug directory for a render phase.
pub(crate) fn create_debug_dir(paths: &AppPaths) -> CoreResult<PathBuf> {
    let phase_dir = paths.debug_render_dir.join(TRANSPARENT_DEBUG_PHASE);
    let dir = phase_dir.join(timestamp_nanos()?.to_string());
    fs::create_dir_all(&dir).map_err(|error| CoreError::Io {
        path: dir.clone(),
        source: error,
    })?;
    prune_completed_timing_directories(&phase_dir, DEBUG_TIMING_RETENTION_LIMIT);
    Ok(dir)
}

/// Retains only the newest completed timing-summary directories.
///
/// Unknown entries and in-progress render directories are never removed.
pub(crate) fn prune_completed_timing_directories(parent: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let mut completed = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let id = entry.file_name().to_str()?.parse::<u128>().ok()?;
            (path.is_dir() && path.join("timing_summary.json").is_file()).then_some((id, path))
        })
        .collect::<Vec<_>>();
    completed.sort_unstable_by_key(|(id, _)| *id);
    let remove_count = completed.len().saturating_sub(keep);
    for (_, path) in completed.into_iter().take(remove_count) {
        if let Err(error) = fs::remove_dir_all(&path) {
            log::warn!(
                "Could not remove expired render diagnostics {}: {error}",
                path.display()
            );
        }
    }
}

/// Returns a process-unique, monotonic Unix-nanosecond identifier.
///
/// The wall clock alone is not a uniqueness guarantee when parallel render
/// workers create output files at nearly the same time. Preserve the timestamp
/// shape for readable artifact names, but advance the value when the clock has
/// not advanced far enough.
pub(crate) fn timestamp_nanos() -> CoreResult<u128> {
    let clock_value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Encode(error.to_string()))?
        .as_nanos();
    let clock_value = u64::try_from(clock_value)
        .map_err(|_| CoreError::Encode("Timestamp exceeds supported identifier range".into()))?;

    let mut previous = LAST_TIMESTAMP_NANOS.load(Ordering::Relaxed);
    loop {
        let next = clock_value.max(
            previous
                .checked_add(1)
                .ok_or_else(|| CoreError::Encode("Timestamp identifier space exhausted".into()))?,
        );
        match LAST_TIMESTAMP_NANOS.compare_exchange_weak(
            previous,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Ok(u128::from(next)),
            Err(observed) => previous = observed,
        }
    }
}

/// Selects representative frame indexes for optional sample PNG export.
pub(crate) fn sample_frame_indices(total_frames: usize) -> Vec<usize> {
    assert!(total_frames > 0, "sample frame count must be non-zero");
    let mut indices = vec![
        0,
        total_frames / 4,
        total_frames / 2,
        (total_frames * 3) / 4,
        total_frames.saturating_sub(1),
    ];
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Parses the optional sample-frame PNG debug switch at the environment boundary.
pub(crate) fn render_sample_frames_enabled() -> CoreResult<bool> {
    match std::env::var("OVRLEY_SAMPLE_FRAMES") {
        Ok(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(CoreError::Encode(format!(
                "OVRLEY_SAMPLE_FRAMES must be a boolean value; received {value:?}"
            ))),
        },
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(std::env::VarError::NotUnicode(_)) => Err(CoreError::Encode(
            "OVRLEY_SAMPLE_FRAMES must contain Unicode text".to_string(),
        )),
    }
}

/// Writes one raw RGBA frame to a PNG sample file through ffmpeg.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_sample_frame(
    ffmpeg_bin: &Path,
    debug_dir: &Path,
    frame_size: FrameSize,
    rgba: &[u8],
    frame_index: usize,
    input_pix_fmt: &str,
) -> CoreResult<()> {
    let png_path = debug_dir.join(format!("sample_{frame_index:04}.png"));
    let mut command = Command::new(ffmpeg_bin);
    configure_ffmpeg_command(&mut command);
    command
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg(input_pix_fmt)
        .arg("-s")
        .arg(format!("{}x{}", frame_size.width, frame_size.height))
        .arg("-i")
        .arg("-")
        .arg("-frames:v")
        .arg("1")
        .arg("-y")
        .arg(&png_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        CoreError::Encode(format!("Failed to spawn ffmpeg for sample frame: {error}"))
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Encode("Failed to capture sample-frame ffmpeg stdin".into()))?;
    stdin
        .write_all(rgba)
        .map_err(|error| CoreError::Encode(error.to_string()))?;
    drop(stdin);
    let status = child
        .wait()
        .map_err(|error| CoreError::Encode(error.to_string()))?;
    if !status.success() {
        return Err(CoreError::Encode(format!(
            "Failed to write sample frame {}",
            png_path.display()
        )));
    }
    Ok(())
}

// Serializes a payload as pretty JSON and writes it to disk.
fn write_json<T: Serialize>(path: PathBuf, payload: &T) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(payload)?;
    fs::write(&path, json).map_err(|error| CoreError::Io {
        path: path.clone(),
        source: error,
    })
}
