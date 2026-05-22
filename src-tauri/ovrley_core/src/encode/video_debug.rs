//! Debug artifact helpers for video rendering.
//!
//! Render diagnostics are written as JSON summaries and optional sample PNGs in
//! timestamped directories. The files are designed for performance comparison
//! across phases without coupling the main render loop to reporting details.

use crate::activity::schema::DenseActivityReport;
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::TimingBucket;
use crate::encode::ffmpeg::suppress_child_console;
use crate::error::{CoreError, CoreResult};
use crate::render::LabelCacheStatus;
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
/// Timing summary for one video render phase.
struct TimingSummary {
    phase: String,
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
    label_cache_status: String,
}

#[derive(Serialize)]
/// Summary for a segmented render stitch operation.
struct StitchSummary {
    timestamp: String,
    codec: String,
    total_frames: u32,
    layout_total_frames: u32,
    segments: Vec<SegmentSummary>,
    concat_output: String,
    concat_duration_ms: f64,
}

#[derive(Serialize)]
/// Per-segment metadata included in a stitch summary.
struct SegmentSummary {
    index: usize,
    start_seconds: f64,
    end_seconds: f64,
    frames: u32,
    layout_frames: u32,
    filename: String,
}

/// Concatenates already-encoded video segments into one output file.
pub(crate) fn concat_video_segments(
    paths: &AppPaths,
    ffmpeg_bin: &Path,
    filenames: &[String],
    output_path: &Path,
) -> CoreResult<()> {
    // Use ffmpeg's concat demuxer with stream copy because each segment is
    // encoded with the same settings and should not be recompressed.
    let list_path = paths
        .temp_dir
        .join(format!("concat_list_{}.txt", timestamp_nanos()?));
    let mut list_content = String::new();
    for filename in filenames {
        list_content.push_str(&format!(
            "file '{}'\n",
            paths
                .downloads_dir
                .join(filename)
                .display()
                .to_string()
                .replace('\\', "/")
        ));
    }
    fs::write(&list_path, list_content).map_err(|e| CoreError::Io {
        path: list_path.clone(),
        source: e,
    })?;

    let mut command = Command::new(ffmpeg_bin);
    suppress_child_console(&mut command);
    let status = command
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_path)
        .arg("-c")
        .arg("copy")
        .arg("-y")
        .arg(output_path)
        .status()
        .map_err(|e| CoreError::Encode(format!("Failed to run ffmpeg concat: {e}")))?;

    let _ = fs::remove_file(&list_path);

    if status.success() {
        Ok(())
    } else {
        Err(CoreError::Encode(format!(
            "FFmpeg concat failed with status {status}"
        )))
    }
}

/// Writes preparation timing data for one render.
pub(crate) fn write_prepare_summary(
    debug_dir: &Path,
    total_ms: f64,
    timings: &BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
) -> CoreResult<()> {
    let summary = PrepareTimingSummary {
        total_ms,
        timings: timings.clone(),
        label_cache_status: match label_cache_status {
            LabelCacheStatus::None => "none".to_string(),
            LabelCacheStatus::Hit => "hit".to_string(),
            LabelCacheStatus::Miss => "miss".to_string(),
        },
    };
    write_json(
        debug_dir.join("prepare_render_assets_timing.json"),
        &summary,
    )
}

/// Writes aggregate timing data for one render phase.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_timing_summary_with_phase(
    debug_dir: &Path,
    config: &RenderConfig,
    output_path: &Path,
    phase: &str,
    total_frames: u32,
    layout_total_frames: u32,
    rendered_frames: u32,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    timings: BTreeMap<String, TimingBucket>,
) -> CoreResult<()> {
    let summary = TimingSummary {
        phase: phase.to_string(),
        timestamp: iso_timestamp_now(),
        overlay_filename: output_path.to_string_lossy().to_string(),
        fps: config.container_fps(),
        layout_fps: config.scene.fps,
        update_rate: config.widget_update_rate(),
        width: config.scene.width.unwrap_or(1920),
        height: config.scene.height.unwrap_or(1080),
        total_frames,
        layout_total_frames,
        rendered_frames,
        total_time_taken: round3(total_time_taken),
        sample_frame_indices,
        timings,
    };
    write_json(debug_dir.join("timing_summary.json"), &summary)
}

/// Writes timing and segment metadata for a stitched render.
pub(crate) fn write_stitch_summary(
    paths: &AppPaths,
    config: &RenderConfig,
    public_filename: &str,
    concat_duration_ms: f64,
    segment_configs: &[RenderConfig],
    segment_reports: &[DenseActivityReport],
    filenames: &[String],
) -> CoreResult<()> {
    let debug_dir = create_debug_dir(paths, "phase_6_stitch")?;
    let codec = config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let segment_summaries = segment_configs
        .iter()
        .enumerate()
        .map(|(index, segment_config)| SegmentSummary {
            index,
            start_seconds: round3(segment_config.scene.start),
            end_seconds: round3(segment_config.scene.end),
            frames: rendered_frame_count_for_summary(
                segment_reports[index].frame_count,
                config.widget_update_rate() as usize,
            ),
            layout_frames: segment_reports[index].frame_count as u32,
            filename: filenames[index].clone(),
        })
        .collect::<Vec<_>>();
    let total_frames = segment_summaries.iter().map(|segment| segment.frames).sum();
    let layout_total_frames = segment_summaries
        .iter()
        .map(|segment| segment.layout_frames)
        .sum();
    let summary = StitchSummary {
        timestamp: iso_timestamp_now(),
        codec,
        total_frames,
        layout_total_frames,
        segments: segment_summaries,
        concat_output: public_filename.to_string(),
        concat_duration_ms: round3(concat_duration_ms),
    };
    write_json(debug_dir.join("stitch_summary.json"), &summary)
}

/// Creates a timestamped debug directory for a render phase.
pub(crate) fn create_debug_dir(paths: &AppPaths, phase: &str) -> CoreResult<PathBuf> {
    let dir = paths.debug_render_dir.join(phase).join(timestamp_slug()?);
    fs::create_dir_all(&dir).map_err(|error| CoreError::Io {
        path: dir.clone(),
        source: error,
    })?;
    Ok(dir)
}

/// Returns the current Unix timestamp in nanoseconds.
pub(crate) fn timestamp_nanos() -> CoreResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|error| CoreError::Encode(error.to_string()))
}

/// Selects representative frame indexes for optional sample PNG export.
pub(crate) fn sample_frame_indices(total_frames: usize) -> Vec<usize> {
    // Capture coarse milestones so visual debugging is useful without producing
    // many large image files during long renders.
    if total_frames == 0 {
        return Vec::new();
    }
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

/// Returns whether sample-frame PNG debug output is enabled by environment.
pub(crate) fn render_sample_frames_enabled() -> bool {
    matches!(
        std::env::var("OVRLEY_SAMPLE_FRAMES").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

/// Writes one raw RGBA frame to a PNG sample file through ffmpeg.
pub(crate) fn write_sample_frame(
    ffmpeg_bin: &Path,
    debug_dir: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    frame_index: usize,
    input_pix_fmt: &str,
) -> CoreResult<()> {
    // Reuse ffmpeg for raw RGBA to PNG conversion so sample images exercise the
    // same input pixel format as the real encoder path.
    let png_path = debug_dir.join(format!("sample_{frame_index:04}.png"));
    let mut command = Command::new(ffmpeg_bin);
    suppress_child_console(&mut command);
    command
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg(input_pix_fmt)
        .arg("-s")
        .arg(format!("{width}x{height}"))
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
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(rgba)
            .map_err(|error| CoreError::Encode(error.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|error| CoreError::Encode(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(CoreError::Encode(format!(
            "Failed to write sample frame {}",
            png_path.display()
        )))
    }
}

// Formats the current local timestamp for debug summaries.
fn iso_timestamp_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

// Rounds a floating-point value to three decimal places.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

// Computes the encoded frame count reported in stitch summaries.
fn rendered_frame_count_for_summary(layout_frame_count: usize, update_rate: usize) -> u32 {
    if layout_frame_count == 0 {
        return 0;
    }
    let safe_update_rate = update_rate.max(1);
    (((layout_frame_count - 1) / safe_update_rate) + 1) as u32
}

// Serializes a payload as pretty JSON and writes it to disk.
fn write_json<T: Serialize>(path: PathBuf, payload: &T) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(payload)?;
    fs::write(&path, json).map_err(|error| CoreError::Io {
        path: path.clone(),
        source: error,
    })
}

// Builds a filesystem-safe timestamp slug for debug directories/files.
fn timestamp_slug() -> CoreResult<String> {
    Ok(timestamp_nanos()?.to_string())
}
