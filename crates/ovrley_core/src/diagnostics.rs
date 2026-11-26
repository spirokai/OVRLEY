//! Render timing and debug artifacts: the named-bucket profiler, timing summaries, sample
//! frames and composite command summaries written to the runtime debug folder. Writing an
//! artifact is best-effort and never fails a render.

use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use crate::render::FrameSize;
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::encode::ffmpeg::composite::CompositeEncoding;

/// Aggregated timing statistics for a named operation.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TimingBucket {
    /// Number of recorded samples.
    pub count: u32,
    /// Total time across all samples in milliseconds.
    pub total_ms: f64,
    /// Average sample time in milliseconds.
    pub avg_ms: f64,
    /// Maximum observed sample time in milliseconds.
    pub max_ms: f64,
}

impl TimingBucket {
    /// Adds one duration sample and updates total, average, and max values.
    fn add_sample(&mut self, duration_ms: f64) {
        self.count += 1;
        self.total_ms += duration_ms;
        self.avg_ms = self.total_ms / f64::from(self.count);
        if duration_ms > self.max_ms {
            self.max_ms = duration_ms;
        }
    }
}

/// Lightweight named profiler used during render preparation and frames.
#[derive(Clone, Debug, Default)]
pub struct RenderProfiler {
    buckets: BTreeMap<String, TimingBucket>,
}

impl RenderProfiler {
    /// Records a duration in milliseconds under `name`.
    pub fn record_ms(&mut self, name: impl Into<String>, duration_ms: f64) {
        self.buckets
            .entry(name.into())
            .or_default()
            .add_sample(duration_ms);
    }

    /// Measures a synchronous callback and records its elapsed duration.
    pub fn measure<T>(&mut self, name: impl Into<String>, callback: impl FnOnce() -> T) -> T {
        let name = name.into();
        let started = Instant::now();
        let result = callback();
        self.record_ms(name, started.elapsed().as_secs_f64() * 1000.0);
        result
    }

    /// Returns a clone of the current timing buckets for serialization.
    pub fn summary(&self) -> BTreeMap<String, TimingBucket> {
        self.buckets.clone()
    }
}


fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

static LAST_TIMESTAMP_NANOS: AtomicU64 = AtomicU64::new(0);
const DEBUG_TIMING_RETENTION_LIMIT: usize = 20;
const TRANSPARENT_DEBUG_DIR: &str = "transparent";

#[derive(Serialize)]
/// Timing summary of one transparent render.
struct TimingSummary {
    export: &'static str,
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
}

/// Writes preparation timing data for one render.
pub(crate) fn write_prepare_summary(
    debug_dir: &Path,
    total_ms: f64,
    timings: BTreeMap<String, TimingBucket>,
) -> CoreResult<()> {
    let summary = PrepareTimingSummary { total_ms, timings };
    write_json(
        debug_dir.join("prepare_render_assets_timing.json"),
        &summary,
    )
}

/// Writes the timing summary of one transparent render.
#[expect(clippy::too_many_arguments, reason = "the summary records each figure of one render")]
pub(crate) fn write_timing_summary(
    debug_dir: &Path,
    scene: &crate::config::ValidatedSceneConfig,
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
        export: TRANSPARENT_DEBUG_DIR,
        timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        overlay_filename: output_path.to_string_lossy().to_string(),
        fps: scene.fps / f64::from(update_rate),
        layout_fps: scene.fps,
        update_rate,
        width: scene.presentation.width,
        height: scene.presentation.height,
        total_frames,
        layout_total_frames,
        rendered_frames,
        total_time_taken: round3(total_time_taken),
        sample_frame_indices,
        timings,
    };
    write_json(debug_dir.join("timing_summary.json"), &summary)?;
    let export_dir = debug_dir.parent().ok_or_else(|| {
        CoreError::Encode(format!(
            "Render debug directory has no parent: {}",
            debug_dir.display()
        ))
    })?;
    prune_completed_timing_directories(export_dir, DEBUG_TIMING_RETENTION_LIMIT);
    Ok(())
}

/// Creates a timestamped debug directory for a transparent render.
pub(crate) fn create_debug_dir(paths: &AppPaths) -> CoreResult<PathBuf> {
    let export_dir = paths.debug_render_dir.join(TRANSPARENT_DEBUG_DIR);
    let dir = export_dir.join(timestamp_nanos()?.to_string());
    fs::create_dir_all(&dir).map_err(|error| CoreError::Io {
        path: dir.clone(),
        source: error,
    })?;
    prune_completed_timing_directories(&export_dir, DEBUG_TIMING_RETENTION_LIMIT);
    Ok(dir)
}

/// Retains only the newest completed timing-summary directories.
///
/// Unknown entries and in-progress render directories are never removed.
fn prune_completed_timing_directories(parent: &Path, keep: usize) {
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
fn timestamp_nanos() -> CoreResult<u128> {
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

/// Writes the renderer's canonical RGBA pixels directly, without a subprocess.
pub(crate) fn write_sample_frame(
    debug_dir: &Path,
    frame_size: FrameSize,
    rgba: &[u8],
    frame_index: usize,
) -> CoreResult<()> {
    let path = debug_dir.join(format!("sample_{frame_index:04}.png"));
    image::save_buffer(
        &path,
        rgba,
        frame_size.width,
        frame_size.height,
        image::ColorType::Rgba8,
    )
    .map_err(|error| {
        CoreError::Encode(format!(
            "Could not write sample frame {}: {error}",
            path.display()
        ))
    })
}

// Serializes a payload as pretty JSON and writes it to disk.
fn write_json<T: Serialize>(path: PathBuf, payload: &T) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(payload)?;
    fs::write(&path, json).map_err(|error| CoreError::Io {
        path: path.clone(),
        source: error,
    })
}

const COMPOSITE_DEBUG_DIR: &str = "composite";

#[derive(Serialize)]
/// Top-level timing and command summary for one MP4 composite render.
struct CompositeTimingSummary<'a> {
    export: &'static str,
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
    #[serde(flatten)]
    quality: crate::encode::settings::EncodingQuality,
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
#[expect(clippy::too_many_arguments, reason = "the summary records each figure of one render")]
pub fn write_composite_timing_summary(
    paths: &AppPaths,
    plan: &CompositeEncoding,
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
        .join(COMPOSITE_DEBUG_DIR)
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
        export: COMPOSITE_DEBUG_DIR,
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
            quality: plan.render.quality,
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
        &paths.debug_render_dir.join(COMPOSITE_DEBUG_DIR),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_png_preserves_rgba_pixels_and_alpha() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-png-{}", timestamp_nanos().unwrap()));
        fs::create_dir(&directory).unwrap();
        let rgba = [255, 0, 0, 0, 0, 127, 255, 128];
        write_sample_frame(
            &directory,
            FrameSize {
                width: 2,
                height: 1,
            },
            &rgba,
            7,
        )
        .unwrap();
        let path = directory.join("sample_0007.png");
        assert_eq!(image::open(&path).unwrap().to_rgba8().as_raw(), &rgba);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
