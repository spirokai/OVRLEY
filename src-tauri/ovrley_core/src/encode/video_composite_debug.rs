//! Composite-only debug artifacts.
//!
//! MP4 compositing writes diagnostics into a fixed phase directory so the new
//! pipeline can be inspected without changing the transparent render debug path.

use crate::debug::TimingBucket;
use crate::encode::ffmpeg_composite::CompositeFfmpegSettings;
use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
/// Top-level timing and command summary for one MP4 composite render.
struct CompositeTimingSummary {
    phase: String,
    mode: String,
    overlay_filename: String,
    fps: f64,
    layout_fps: f64,
    update_rate: u32,
    width: u32,
    height: u32,
    total_frames: u64,
    layout_total_frames: u64,
    rendered_frames: u64,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    performance: CompositePerformanceSummary,
    timings: BTreeMap<String, TimingBucket>,
    diagnostics: CompositeDiagnostics,
}

#[derive(Serialize)]
/// Per-frame performance values derived from measured timing buckets.
struct CompositePerformanceSummary {
    overlay_render_exact: Option<PerFrameTiming>,
    overlay_frame_total_exact: Option<PerFrameTiming>,
    ffmpeg_write_backpressure: Option<PerFrameTiming>,
    ffmpeg_finalize_wait_estimate: PerFrameTiming,
    ffmpeg_decode_filter_encode_note: String,
}

#[derive(Serialize)]
/// Total, average, and maximum timing values for one measured operation.
struct PerFrameTiming {
    total_ms: f64,
    avg_ms_per_frame: f64,
    max_ms_per_frame: Option<f64>,
    frame_basis: String,
}

#[derive(Serialize)]
/// FFmpeg and render-plan diagnostics included with the timing summary.
struct CompositeDiagnostics {
    codec: String,
    bitrate: String,
    source_fps: String,
    overlay_pipe_fps: String,
    render_duration: f64,
    render_loop_ms: f64,
    ffmpeg_finalize_wait_ms: f64,
    filter_complex: String,
    input_width: u32,
    input_height: u32,
    trim_start: f64,
    sync_offset: f64,
    ffmpeg_args: Vec<String>,
    ffmpeg_timing_note: String,
}

/// Immutable inputs needed to write a Phase 7 composite timing summary.
pub struct CompositeTimingSummaryInput<'a> {
    // test seam
    pub debug_render_dir: &'a Path,
    pub ffmpeg_settings: &'a CompositeFfmpegSettings,
    pub output_path: &'a Path,
    pub source_fps: Fps,
    pub overlay_pipe_fps: Fps,
    pub widget_update_rate: u32,
    pub render_duration: f64,
    pub overlay_frame_count: u64,
    pub output_frame_count: u64,
    pub total_ms: f64,
    pub render_loop_ms: f64,
    pub ffmpeg_finalize_wait_ms: f64,
    pub timings: BTreeMap<String, TimingBucket>,
    pub codec: &'a str,
    pub bitrate: &'a str,
    pub input_width: u32,
    pub input_height: u32,
    pub trim_start: f64,
    pub sync_offset: f64,
}

/// Writes the Phase 7 composite timing summary JSON.
///
/// The summary is stored under `debug/timings/phase_7/<video-id>/timing_summary.json`
/// and includes rational FPS values, frame counts, timings, and FFmpeg diagnostics.
pub fn write_composite_timing_summary(
    // test seam
    input: CompositeTimingSummaryInput<'_>,
) -> CoreResult<PathBuf> {
    let debug_dir = input
        .debug_render_dir
        .join("phase_7")
        .join(composite_debug_id(input.output_path));
    fs::create_dir_all(&debug_dir).map_err(|error| CoreError::Io {
        path: debug_dir.clone(),
        source: error,
    })?;

    let performance = composite_performance_summary(
        &input.timings,
        input.overlay_frame_count,
        input.output_frame_count,
        input.ffmpeg_finalize_wait_ms,
    );
    let summary = CompositeTimingSummary {
        phase: "phase_7".to_string(),
        mode: "mp4_composite".to_string(),
        overlay_filename: input.output_path.to_string_lossy().to_string(),
        fps: round3(input.source_fps.as_f64()),
        layout_fps: round3(input.overlay_pipe_fps.as_f64()),
        update_rate: input.widget_update_rate,
        width: input.input_width,
        height: input.input_height,
        total_frames: input.output_frame_count,
        layout_total_frames: input.overlay_frame_count,
        rendered_frames: input.overlay_frame_count,
        total_time_taken: round3(input.total_ms / 1000.0),
        sample_frame_indices: Vec::new(),
        performance,
        timings: input.timings,
        diagnostics: CompositeDiagnostics {
            codec: input.codec.to_string(),
            bitrate: input.bitrate.to_string(),
            source_fps: input.source_fps.ffmpeg_arg(),
            overlay_pipe_fps: input.overlay_pipe_fps.ffmpeg_arg(),
            render_duration: round3(input.render_duration),
            render_loop_ms: round3(input.render_loop_ms),
            ffmpeg_finalize_wait_ms: round3(input.ffmpeg_finalize_wait_ms),
            filter_complex: input.ffmpeg_settings.filter_complex.clone(),
            input_width: input.input_width,
            input_height: input.input_height,
            trim_start: round3(input.trim_start),
            sync_offset: round3(input.sync_offset),
            ffmpeg_args: composite_ffmpeg_args(input.ffmpeg_settings, input.output_path),
            ffmpeg_timing_note:
                "FFmpeg decode/filter/encode timings are not isolated; ffmpeg.write measures stdin write/backpressure time."
                    .to_string(),
        },
    };

    let summary_path = debug_dir.join("timing_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&summary_path, json).map_err(|error| CoreError::Io {
        path: summary_path.clone(),
        source: error,
    })?;
    Ok(summary_path)
}

/// Builds exact and estimated per-frame performance summaries.
///
/// Rust-side rendering and pipe writes are measured directly; FFmpeg internals
/// are exposed only as backpressure/finalize estimates unless benchmark logs
/// are parsed in a future profiling mode.
fn composite_performance_summary(
    timings: &BTreeMap<String, TimingBucket>,
    overlay_frame_count: u64,
    output_frame_count: u64,
    ffmpeg_finalize_wait_ms: f64,
) -> CompositePerformanceSummary {
    CompositePerformanceSummary {
        overlay_render_exact: timings
            .get("frame.draw")
            .map(|bucket| per_frame_bucket(bucket, overlay_frame_count, "overlay_frame")),
        overlay_frame_total_exact: timings
            .get("frame.total")
            .map(|bucket| per_frame_bucket(bucket, overlay_frame_count, "overlay_frame")),
        ffmpeg_write_backpressure: timings
            .get("ffmpeg.write")
            .map(|bucket| per_frame_bucket(bucket, overlay_frame_count, "overlay_frame")),
        ffmpeg_finalize_wait_estimate: PerFrameTiming {
            total_ms: round3(ffmpeg_finalize_wait_ms),
            avg_ms_per_frame: avg_per_frame(ffmpeg_finalize_wait_ms, output_frame_count),
            max_ms_per_frame: None,
            frame_basis: "output_frame".to_string(),
        },
        ffmpeg_decode_filter_encode_note:
            "Decode, filter/overlay, encode, and mux timings cannot be exactly separated from this process output. ffmpeg.write is a stdin backpressure proxy; ffmpeg_finalize_wait_ms is the final drain after overlay stdin closes."
                .to_string(),
    }
}

/// Converts one profiler bucket into rounded per-frame timing values.
fn per_frame_bucket(bucket: &TimingBucket, frame_count: u64, frame_basis: &str) -> PerFrameTiming {
    PerFrameTiming {
        total_ms: round3(bucket.total_ms),
        avg_ms_per_frame: avg_per_frame(bucket.total_ms, frame_count),
        max_ms_per_frame: Some(round3(bucket.max_ms)),
        frame_basis: frame_basis.to_string(),
    }
}

/// Returns a rounded average per frame, guarding against empty frame counts.
fn avg_per_frame(total_ms: f64, frame_count: u64) -> f64 {
    if frame_count == 0 {
        0.0
    } else {
        round3(total_ms / frame_count as f64)
    }
}

/// Reconstructs the FFmpeg argument list for diagnostics.
///
/// The list mirrors the spawned composite command after the executable name and
/// keeps the final output path visible for local debugging.
fn composite_ffmpeg_args(settings: &CompositeFfmpegSettings, output_path: &Path) -> Vec<String> {
    let mut args = vec!["-loglevel".to_string(), "info".to_string()];
    args.extend(settings.hw_init_args.clone());
    args.extend(settings.input_0_args.clone());
    args.extend(settings.input_1_args.clone());
    args.push("-filter_complex".to_string());
    args.push(settings.filter_complex.clone());
    args.extend(settings.output_args.clone());
    args.push(output_path.to_string_lossy().to_string());
    args
}

/// Rounds summary floating-point values to three decimal places.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Derives the Phase 7 debug directory name from the output video filename.
///
/// Composite outputs use `video_composited_<id>.mp4`, so this keeps diagnostics
/// tied to the user-visible render artifact instead of a separate write time.
fn composite_debug_id(output_path: &Path) -> String {
    let stem = output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown");
    stem.strip_prefix("video_composited_")
        .unwrap_or(stem)
        .to_string()
}
