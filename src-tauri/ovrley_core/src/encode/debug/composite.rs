//! Composite-only debug artifacts.
//!
//! MP4 compositing writes diagnostic timing/command summaries into a fixed
//! debug directory so the composite pipeline can be inspected without changing
//! the transparent render debug path.
//!
//! Write failures are best-effort — the composite render continues even if
//! debug artifact creation fails. Completed timing artifacts have bounded
//! retention.

use crate::debug::TimingBucket;
use crate::encode::debug::round3;
use crate::encode::debug::video::{
    prune_completed_timing_directories, timestamp_nanos, DEBUG_TIMING_RETENTION_LIMIT,
};
use crate::encode::pipeline::composite_plan::CompositePipelinePlan;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const COMPOSITE_DEBUG_PHASE: &str = "composite";

#[derive(Serialize)]
/// Top-level timing and command summary for one MP4 composite render.
struct CompositeTimingSummary<'a> {
    phase: &'static str,
    mode: &'static str,
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
    diagnostics: CompositeDiagnostics<'a>,
}

#[derive(Serialize)]
/// Per-frame performance values derived from measured timing buckets.
struct CompositePerformanceSummary {
    overlay_render_exact: Option<PerFrameTiming>,
    overlay_frame_total_exact: Option<PerFrameTiming>,
    ffmpeg_write_backpressure: Option<PerFrameTiming>,
    ffmpeg_finalize_wait_estimate: PerFrameTiming,
    ffmpeg_decode_filter_encode_note: &'static str,
}

#[derive(Serialize)]
/// Total, average, and maximum timing values for one measured operation.
struct PerFrameTiming {
    total_ms: f64,
    avg_ms_per_frame: f64,
    max_ms_per_frame: Option<f64>,
    frame_basis: &'static str,
}

#[derive(Serialize)]
/// FFmpeg and render-plan diagnostics included with the timing summary.
struct CompositeDiagnostics<'a> {
    frame_render_mode: &'static str,
    frame_render_workers: usize,
    codec: &'static str,
    bitrate: &'a str,
    source_fps: String,
    overlay_pipe_fps: String,
    render_duration: f64,
    render_loop_ms: f64,
    ffmpeg_finalize_wait_ms: f64,
    filter_complex: &'a str,
    input_width: u32,
    input_height: u32,
    trim_start: f64,
    sync_offset: f64,
    ffmpeg_args: Vec<String>,
    ffmpeg_timing_note: &'static str,
}

/// Writes the composite timing summary JSON.
///
/// The summary is stored under `debug_render/composite/<video-id>/timing_summary.json`
/// and includes rational FPS values, frame counts, timings, and FFmpeg diagnostics.
#[allow(clippy::too_many_arguments)]
pub fn write_composite_timing_summary(
    paths: &AppPaths,
    plan: &CompositePipelinePlan,
    total_ms: f64,
    render_loop_ms: f64,
    ffmpeg_finalize_wait_ms: f64,
    timings: BTreeMap<String, TimingBucket>,
    frame_render_workers: usize,
    rendered_frames: u32,
) -> CoreResult<PathBuf> {
    let debug_id = timestamp_nanos()?.to_string();
    let debug_dir = paths
        .debug_render_dir
        .join(COMPOSITE_DEBUG_PHASE)
        .join(debug_id);
    fs::create_dir_all(&debug_dir).map_err(|error| CoreError::Io {
        path: debug_dir.clone(),
        source: error,
    })?;

    let performance = composite_performance_summary(
        &timings,
        plan.render.overlay_frame_count,
        u64::from(plan.render.output_frame_count),
        ffmpeg_finalize_wait_ms,
    );
    let summary = CompositeTimingSummary {
        phase: COMPOSITE_DEBUG_PHASE,
        mode: "mp4_composite",
        overlay_filename: plan.output_path.to_string_lossy().to_string(),
        fps: round3(plan.render.source_fps.as_f64()),
        layout_fps: round3(plan.render.overlay_pipe_fps.as_f64()),
        update_rate: plan.render.update_rate.get(),
        width: plan.frame_size.width,
        height: plan.frame_size.height,
        total_frames: u64::from(plan.render.output_frame_count),
        layout_total_frames: plan.render.overlay_frame_count,
        rendered_frames: u64::from(rendered_frames),
        total_time_taken: round3(total_ms / 1000.0),
        sample_frame_indices: Vec::new(),
        performance,
        timings,
        diagnostics: CompositeDiagnostics {
            frame_render_mode: "frame_workers",
            frame_render_workers,
            codec: plan
                .ffmpeg_settings
                .codec_id
                .metadata()
                .profile_name,
            bitrate: &plan.render.bitrate,
            source_fps: plan.render.source_fps.ffmpeg_arg(),
            overlay_pipe_fps: plan.render.overlay_pipe_fps.ffmpeg_arg(),
            render_duration: round3(plan.render.render_duration),
            render_loop_ms: round3(render_loop_ms),
            ffmpeg_finalize_wait_ms: round3(ffmpeg_finalize_wait_ms),
            filter_complex: &plan.ffmpeg_settings.filter_complex,
            input_width: plan.frame_size.width,
            input_height: plan.frame_size.height,
            trim_start: round3(plan.render.trim_start),
            sync_offset: round3(plan.render.sync_offset),
            ffmpeg_args: plan.ffmpeg_settings.command_args(&plan.output_path),
            ffmpeg_timing_note:
                "FFmpeg decode/filter/encode timings are not isolated; ffmpeg.write measures stdin write/backpressure time.",
        },
    };

    let summary_path = debug_dir.join("timing_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&summary_path, json).map_err(|error| CoreError::Io {
        path: summary_path.clone(),
        source: error,
    })?;
    prune_completed_timing_directories(
        &paths.debug_render_dir.join(COMPOSITE_DEBUG_PHASE),
        DEBUG_TIMING_RETENTION_LIMIT,
    );
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
            frame_basis: "output_frame",
        },
        ffmpeg_decode_filter_encode_note:
            "Decode, filter/overlay, encode, and mux timings cannot be exactly separated from this process output. ffmpeg.write is a stdin backpressure proxy; ffmpeg_finalize_wait_ms is the final drain after overlay stdin closes.",
    }
}

/// Converts one profiler bucket into rounded per-frame timing values.
fn per_frame_bucket(
    bucket: &TimingBucket,
    frame_count: u64,
    frame_basis: &'static str,
) -> PerFrameTiming {
    PerFrameTiming {
        total_ms: round3(bucket.total_ms),
        avg_ms_per_frame: avg_per_frame(bucket.total_ms, frame_count),
        max_ms_per_frame: Some(round3(bucket.max_ms)),
        frame_basis,
    }
}

/// Returns a rounded average per frame, guarding against empty frame counts.
fn avg_per_frame(total_ms: f64, frame_count: u64) -> f64 {
    assert!(frame_count > 0, "diagnostic frame count must be non-zero");
    round3(total_ms / frame_count as f64)
}
