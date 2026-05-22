//! Render lifecycle orchestration.
//!
//! This module coordinates long-running video renders, progress reporting,
//! cancellation, optional qtrle segmentation, and final segment stitching. The
//! actual single-pass frame production and ffmpeg streaming are implemented in
//! `video_pipeline`.

use crate::activity::build_dense_activity_report;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::fps::Fps;
use crate::encode::video_composite_pipeline::render_composite_video_single;
use crate::encode::video_debug::{concat_video_segments, timestamp_nanos, write_stitch_summary};
use crate::encode::video_pipeline::render_video_single;
pub use crate::encode::video_pipeline::rendered_frame_count;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub use crate::encode::progress::RenderController;

/// Renders multiple configs concurrently and stitches their outputs.
///
/// This is primarily a diagnostic/benchmark helper. Each segment receives an
/// independent controller, then ffmpeg concatenates the produced files.
pub fn run_parallel_renders(
    paths: &AppPaths,
    configs: Vec<RenderConfig>,
    activity: &ParsedActivity,
    reports: Vec<DenseActivityReport>,
) -> CoreResult<Duration> {
    if configs.len() != reports.len() {
        return Err(CoreError::Encode(
            "Configs and reports vectors must have the same length".to_string(),
        ));
    }

    let start_time = Instant::now();
    let total_jobs = configs.len();
    let worker_count = estimate_parallel_render_worker_count(total_jobs);
    let work_queue = Arc::new(Mutex::new(
        configs
            .into_iter()
            .zip(reports)
            .enumerate()
            .collect::<VecDeque<_>>(),
    ));
    let (result_tx, result_rx) = mpsc::channel::<(usize, CoreResult<String>)>();
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let paths_clone = paths.clone();
        let activity_clone = activity.clone();
        let work_queue_clone = work_queue.clone();
        let result_tx_clone = result_tx.clone();
        let handle = thread::spawn(move || loop {
            let next_job = {
                let mut queue = match work_queue_clone.lock() {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
                queue.pop_front()
            };
            let Some((index, (config, report))) = next_job else {
                return;
            };

            let controller = RenderController::default();
            let start_result = controller.try_start(
                rendered_frame_count(report.frame_count, config.widget_update_rate() as usize)
                    as u32,
                &format!("Parallel Render {}", index + 1),
            );
            let result = match start_result {
                Ok(_) => render_video(&paths_clone, &config, &activity_clone, &report, &controller),
                Err(error) => Err(error),
            };
            let _ = result_tx_clone.send((index, result));
        });
        handles.push(handle);
    }
    drop(result_tx);

    let mut filenames = vec![None; total_jobs];
    for _ in 0..filenames.len() {
        let (index, result) = result_rx.recv().map_err(|_| {
            CoreError::Encode("Parallel render worker channel disconnected".to_string())
        })?;
        filenames[index] = Some(result?);
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| CoreError::Encode("Parallel render thread panicked".to_string()))?;
    }

    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let output_filename = format!("parallel_stitch_{}.mov", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&output_filename);
    let filenames = filenames
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            CoreError::Encode(
                "Parallel render finished without producing all output filenames".to_string(),
            )
        })?;
    concat_video_segments(paths, &ffmpeg_bin, &filenames, &output_path)?;

    Ok(start_time.elapsed())
}

// Estimates a conservative number of parallel render workers for the machine.
fn estimate_parallel_render_worker_count(total_jobs: usize) -> usize {
    // Rendering is CPU and memory heavy, so use a conservative fraction of
    // available logical cores instead of saturating the machine.
    let logical_cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    let worker_count = (logical_cores / 4).max(1).min(4);
    worker_count.min(total_jobs.max(1))
}

fn estimate_composite_segment_count(total_jobs: usize, codec: &str) -> usize {
    let worker_count = estimate_parallel_render_worker_count(total_jobs);
    if matches!(codec, "h264_amf" | "hevc_amf") {
        worker_count.min(2)
    } else {
        worker_count
    }
}

/// Renders a video, using segmentation when the selected codec benefits from it.
pub fn render_video(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> CoreResult<String> {
    if should_parallelize_segmented(config, dense_activity) {
        return render_video_segmented(paths, config, activity, dense_activity, controller);
    }
    render_video_single(paths, config, activity, dense_activity, controller)
}

/// Bundled parameters for composite MP4 rendering.
///
/// Consolidates what was previously 14 separate parameters shared between
/// `render_composite_video`, `render_composite_video_segmented`, and
/// `render_composite_video_single`.
pub struct CompositeRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub activity: &'a ParsedActivity,
    pub dense_activity: &'a DenseActivityReport,
    pub controller: &'a RenderController,
    pub composite_video_path: &'a str,
    pub composite_bitrate: &'a str,
    pub composite_sync_offset: f64,
    pub composite_video_fps_num: u32,
    pub composite_video_fps_den: u32,
    pub composite_video_duration: f64,
    pub composite_render_duration: Option<f64>,
    pub composite_video_trim_start: Option<f64>,
    pub composite_widget_update_rate: Option<u32>,
}

/// Renders an imported video with the Skia overlay composited into an MP4 output.
///
/// Longer renders are automatically split into parallel segments for better CPU
/// utilization and then stitched with FFmpeg stream copy.
pub fn render_composite_video(request: &CompositeRenderRequest<'_>) -> CoreResult<String> {
    let render_duration = request
        .composite_render_duration
        .unwrap_or(request.composite_video_duration - request.composite_video_trim_start.unwrap_or(0.0));
    let trim_start = request.composite_video_trim_start.unwrap_or(0.0);
    let update_rate = request.composite_widget_update_rate.unwrap_or(1).max(1);

    let codec = request.config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("libx264");
    if should_parallelize_composite(render_duration, request.composite_video_fps_num, update_rate, codec) {
        return render_composite_video_segmented(
            request.paths,
            request.config,
            request.activity,
            request.dense_activity,
            request.controller,
            request.composite_video_path,
            request.composite_bitrate,
            request.composite_sync_offset,
            request.composite_video_fps_num,
            request.composite_video_fps_den,
            request.composite_video_duration,
            render_duration,
            trim_start,
            update_rate,
        );
    }

    render_composite_video_single(
        request.paths,
        request.config,
        request.activity,
        request.dense_activity,
        request.controller,
        request.composite_video_path,
        request.composite_bitrate,
        request.composite_sync_offset,
        request.composite_video_fps_num,
        request.composite_video_fps_den,
        request.composite_video_duration,
        Some(render_duration),
        Some(trim_start),
        Some(update_rate),
    )
}

/// Returns whether composite rendering should be split into parallel segments.
///
/// Software CPU encoders (libx264, libx265) run single-pass because parallel
/// segments would fight for the same cores. Hardware encoders benefit more
/// from splitting when there are enough frames to justify the overhead.
fn should_parallelize_composite(
    render_duration: f64,
    fps_num: u32,
    update_rate: u32,
    codec: &str,
) -> bool {
    if matches!(codec, "libx264" | "libx265") {
        return false;
    }
    let overlay_fps = fps_num as f64 / update_rate.max(1) as f64;
    let total_frames = (render_duration * overlay_fps).ceil() as u32;
    total_frames >= 120 && estimate_composite_segment_count(total_frames as usize, codec) >= 2
}

/// Renders a composite MP4 as parallel time-window segments stitched together.
///
/// When a composite render exceeds a per-codec segment threshold, this function
/// divides the render timeline into roughly equal contiguous windows and spawns
/// one thread per window. Each thread runs an independent `render_composite_video_single`
/// with its own ffmpeg process and buffer pool. The segment outputs are then
/// concatenated via ffmpeg stream copy (no re-encode).
///
/// If the computed segment count is 1 (render too short for the codec), it falls
/// through to a single-pass `render_composite_video_single`.
///
/// # Progress & Cancellation
/// Child controllers share the parent's cancel flag. On first segment error, the
/// cancel flag is set to stop in-flight segments. Aggregate progress is the sum
/// of child progress values polled at 200ms intervals.
///
/// # Errors
/// Returns the first segment error or stitch failure. All segment outputs are
/// cleaned up on any error path (including cancellation).
// Private function passing through the CompositeRenderRequest fields to
// render_composite_video_single — deferred from request-struct refactor.
#[allow(clippy::too_many_arguments)]
fn render_composite_video_segmented(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    composite_video_path: &str,
    composite_bitrate: &str,
    composite_sync_offset: f64,
    composite_video_fps_num: u32,
    composite_video_fps_den: u32,
    composite_video_duration: f64,
    render_duration: f64,
    trim_start: f64,
    update_rate: u32,
) -> CoreResult<String> {
    let codec = config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("libx264");
    let source_fps = Fps::new(composite_video_fps_num, composite_video_fps_den)?;
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let total_overlay_frames = (render_duration * overlay_pipe_fps.as_f64()).ceil() as usize;
    let segment_count = estimate_composite_segment_count(total_overlay_frames.max(1), codec);
    if segment_count < 2 {
        // Segment count below threshold — fall through to single-pass.
        // We pass the computed render_duration/trim_start/update_rate so the
        // single-pass path receives the same timing parameters the segments would.
        return render_composite_video_single(
            paths,
            config,
            activity,
            dense_activity,
            controller,
            composite_video_path,
            composite_bitrate,
            composite_sync_offset,
            composite_video_fps_num,
            composite_video_fps_den,
            composite_video_duration,
            Some(render_duration),
            Some(trim_start),
            Some(update_rate),
        );
    }

    let total_output_frames = (render_duration * source_fps.as_f64()).ceil().max(0.0) as u32;
    let segments = composite_output_frame_windows(
        total_output_frames,
        render_duration,
        source_fps,
        segment_count,
    );
    if segments.len() < 2 {
        return render_composite_video_single(
            paths,
            config,
            activity,
            dense_activity,
            controller,
            composite_video_path,
            composite_bitrate,
            composite_sync_offset,
            composite_video_fps_num,
            composite_video_fps_den,
            composite_video_duration,
            Some(render_duration),
            Some(trim_start),
            Some(update_rate),
        );
    }

    let segment_output_frames = segments
        .iter()
        .map(|segment| segment.output_end_frame - segment.output_start_frame)
        .collect::<Vec<_>>();

    let actual_segment_count = segments.len();
    if actual_segment_count < 2 {
        return render_composite_video_single(
            paths,
            config,
            activity,
            dense_activity,
            controller,
            composite_video_path,
            composite_bitrate,
            composite_sync_offset,
            composite_video_fps_num,
            composite_video_fps_den,
            composite_video_duration,
            Some(render_duration),
            Some(trim_start),
            Some(update_rate),
        );
    }

    let combined_frames = segment_output_frames.iter().copied().sum::<u32>();

    let child_cancel_flag = controller.cancel_flag();
    // Create one child controller per segment. Each child mirrors the parent's
    // cancel flag so a cancel (user-triggered or first-error propagation) stops
    // all in-flight segments. Progress is tracked independently per segment.
    let segment_controllers: Vec<RenderController> = segment_output_frames
        .iter()
        .map(|frames| child_render_controller(*frames, &child_cancel_flag))
        .collect();

    enum SegmentEvent {
        Completed(usize, CoreResult<String>),
    }

    let (tx, rx) = mpsc::channel::<SegmentEvent>();
    let mut handles = Vec::with_capacity(actual_segment_count);
    for index in 0..actual_segment_count {
        let tx = tx.clone();
        let segment_controller = segment_controllers[index].clone();
        let segment_paths = paths.clone();
        let segment_config = config.clone();
        let segment_activity = activity.clone();
        let segment_dense = dense_activity.clone();
        let segment_video_path = composite_video_path.to_string();
        let segment_bitrate = composite_bitrate.to_string();
        let segment = segments[index];
        let segment_trim_start = trim_start + segment.video_start_seconds;
        let segment_render_duration = segment.render_duration_seconds;
        let segment_sync_offset = composite_sync_offset + segment.video_start_seconds;

        let handle = thread::spawn(move || {
            let result = render_composite_video_single(
                &segment_paths,
                &segment_config,
                &segment_activity,
                &segment_dense,
                &segment_controller,
                &segment_video_path,
                &segment_bitrate,
                segment_sync_offset,
                composite_video_fps_num,
                composite_video_fps_den,
                composite_video_duration,
                Some(segment_render_duration),
                Some(segment_trim_start),
                Some(update_rate),
            );
            let _ = tx.send(SegmentEvent::Completed(index, result));
        });
        handles.push(handle);
    }
    // ── PROGRESS AGGREGATION ──
    // Drop sender so the rx loop below will see Disconnected when all segment
    // threads have completed (rather than the sender clones keeping the channel open).
    drop(tx);

    let mut results = vec![None; actual_segment_count];
    let mut completed = 0usize;
    let mut first_error: Option<CoreError> = None;

    while completed < actual_segment_count {
        let progress_snapshots = segment_controllers
            .iter()
            .map(RenderController::progress)
            .collect::<Vec<_>>();
        let current = progress_snapshots.iter().map(|p| p.current).sum::<u32>();
        let encoded = progress_snapshots.iter().map(|p| p.encoded).sum::<u32>();
        let estimate = progress_snapshots
            .iter()
            .filter_map(|p| p.estimated_seconds_remaining)
            .max();
        let rendering_fps = progress_snapshots
            .iter()
            .filter_map(|p| p.rendering_fps)
            .sum::<f64>();
        let rendering_fps = (rendering_fps > 0.0).then_some(rendering_fps);
        controller.set_frame_progress(current, combined_frames, encoded, estimate, rendering_fps);

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(SegmentEvent::Completed(index, Ok(filename))) => {
                results[index] = Some(filename);
                completed += 1;
            }
            Ok(SegmentEvent::Completed(_, Err(error))) => {
                if first_error.is_none() {
                    first_error = Some(error);
                    controller.cancel_flag().store(true, Ordering::SeqCst);
                }
                completed += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    for handle in handles {
        // Join every segment thread, even if we already have an error.
        // Skipping a join would leak the thread and its ffmpeg process.
        handle.join().map_err(|_| {
            CoreError::Encode("Composite segment render thread panicked".to_string())
        })?;
    }

    if let Some(error) = first_error {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }

    if controller.cancel_flag().load(Ordering::SeqCst) {
        cleanup_segment_outputs(paths, &results);
        return Err(CoreError::Cancelled);
    }

    let segment_filenames: Option<Vec<String>> = results.iter().cloned().collect();
    let segment_filenames = match segment_filenames {
        Some(filenames) => filenames,
        None => {
            cleanup_segment_outputs(paths, &results);
            return Err(CoreError::Encode(
                "Composite segment render did not produce all output files".to_string(),
            ));
        }
    };

    controller.set_frame_progress(
        combined_frames,
        combined_frames,
        combined_frames,
        Some(0),
        None,
    );

    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let public_filename = format!("video_composited_{}.mp4", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&public_filename);
    if let Err(error) = concat_video_segments(paths, &ffmpeg_bin, &segment_filenames, &output_path)
    {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }
    cleanup_segment_outputs(paths, &results);
    Ok(public_filename)
}

/// Output-frame window range for one parallel composite segment.
///
/// Parallel composite rendering splits the total output frame range into
/// roughly equal non-overlapping windows. Each segment runs an independent
/// ffmpeg process responsible for exactly the frames in `[output_start_frame,
/// output_end_frame)`. The video-time equivalents are derived from the
/// source FPS so each ffmpeg invocation can seek and trim correctly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositeSegmentWindow {
    /// First frame index this segment must produce (inclusive).
    pub output_start_frame: u32,
    /// Frame index one past the last frame (exclusive).
    pub output_end_frame: u32,
    /// Source-video seek position in seconds for this segment.
    pub video_start_seconds: f64,
    /// Duration in seconds this segment's ffmpeg should process.
    pub render_duration_seconds: f64,
}

/// Divides total output frames into roughly equal windows for segmented rendering.
///
/// When the segment count exceeds the frame count, it is clamped so each segment
/// produces at least one frame (extra "remainder" segments are dropped). The
/// remainder from integer division is distributed one frame at a time across the
/// first N segments so early segments may be one frame longer than later ones.
///
/// Returns an empty `Vec` when `total_output_frames` is zero.
pub fn composite_output_frame_windows(
    // test seam
    total_output_frames: u32,
    render_duration: f64,
    source_fps: Fps,
    segment_count: usize,
) -> Vec<CompositeSegmentWindow> {
    if total_output_frames == 0 {
        return Vec::new();
    }

    let actual_segment_count = segment_count.min(total_output_frames as usize).max(1);
    let base_frames = total_output_frames / actual_segment_count as u32;
    let extra_segments = total_output_frames % actual_segment_count as u32;
    let frame_seconds = source_fps.den as f64 / source_fps.num as f64;
    let mut output_start_frame = 0u32;
    let mut windows = Vec::with_capacity(actual_segment_count);

    for index in 0..actual_segment_count {
        let segment_frames = base_frames + u32::from((index as u32) < extra_segments);
        let output_end_frame = output_start_frame + segment_frames;
        let video_start_seconds = output_start_frame as f64 * frame_seconds;
        let segment_end_seconds = if index == actual_segment_count - 1 {
            render_duration
        } else {
            output_end_frame as f64 * frame_seconds
        };

        windows.push(CompositeSegmentWindow {
            output_start_frame,
            output_end_frame,
            video_start_seconds,
            render_duration_seconds: (segment_end_seconds - video_start_seconds).max(0.0),
        });
        output_start_frame = output_end_frame;
    }

    windows
}

// Returns whether rendering should be split into stitched segments.
fn should_parallelize_segmented(
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
) -> bool {
    // qtrle and prores_ks_vulkan encoding are comparatively slow and
    // stitch-friendly for integer second windows.
    config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .map(|codec| codec == "qtrle" || codec == "prores_ks_vulkan")
        .unwrap_or(false)
        && integer_second_duration(config).unwrap_or(0) >= 2
        && dense_activity.frame_count >= 2
}

// Renders output as multiple second-aligned segments and stitches them.
fn render_video_segmented(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> CoreResult<String> {
    // Segmented transparent rendering: split the scene into integer-second
    // windows, render each on its own thread, then stitch via stream copy.
    // Only qtrle and prores_ks_vulkan codecs are segmented — other codecs
    // produce I-frame-only output that doesn't benefit from parallelism.
    let Some(total_seconds) = integer_second_duration(config) else {
        return render_video_single(paths, config, activity, dense_activity, controller);
    };
    let segment_count = estimate_parallel_render_worker_count(total_seconds as usize);
    if segment_count < 2 {
        return render_video_single(paths, config, activity, dense_activity, controller);
    }

    let mut segment_configs = Vec::with_capacity(segment_count);
    let mut segment_reports = Vec::with_capacity(segment_count);
    for (segment_start, segment_end) in integer_second_windows(config, total_seconds, segment_count)
    {
        let mut segment_config = config.clone();
        segment_config.scene.start = segment_start;
        segment_config.scene.end = segment_end;
        let segment_dense = build_dense_activity_report(activity, &segment_config)?;
        if segment_dense.frame_count == 0 {
            continue;
        }
        segment_configs.push(segment_config);
        segment_reports.push(segment_dense);
    }

    let actual_segment_count = segment_configs.len();
    if actual_segment_count < 2 {
        return render_video_single(paths, config, activity, dense_activity, controller);
    }
    let combined_frames = segment_reports
        .iter()
        .map(|report| {
            rendered_frame_count(report.frame_count, config.widget_update_rate() as usize) as u32
        })
        .sum::<u32>();

    let child_cancel_flag = controller.cancel_flag();
    let segment_controllers = segment_reports
        .iter()
        .map(|report| {
            child_render_controller(
                rendered_frame_count(report.frame_count, config.widget_update_rate() as usize)
                    as u32,
                &child_cancel_flag,
            )
        })
        .collect::<Vec<_>>();

    enum SegmentEvent {
        Completed(usize, CoreResult<String>),
    }

    let (tx, rx) = mpsc::channel::<SegmentEvent>();
    let mut handles = Vec::with_capacity(actual_segment_count);
    for index in 0..actual_segment_count {
        let tx = tx.clone();
        let segment_controller = segment_controllers[index].clone();
        let segment_paths = paths.clone();
        let segment_config = segment_configs[index].clone();
        let segment_activity = activity.clone();
        let segment_dense = segment_reports[index].clone();
        let handle = thread::spawn(move || {
            let result = render_video_single(
                &segment_paths,
                &segment_config,
                &segment_activity,
                &segment_dense,
                &segment_controller,
            );
            let _ = tx.send(SegmentEvent::Completed(index, result));
        });
        handles.push(handle);
    }
    // ── PROGRESS AGGREGATION ──
    // Same aggregation pattern as the composite segmented variant: poll child
    // controllers, sum progress, collect results via mpsc with 200ms timeout.
    // First error sets the parent cancel flag to stop remaining segments.
    drop(tx);

    let mut results = vec![None; actual_segment_count];
    let mut completed = 0usize;
    let mut first_error: Option<CoreError> = None;

    while completed < actual_segment_count {
        let progress_snapshots = segment_controllers
            .iter()
            .map(RenderController::progress)
            .collect::<Vec<_>>();
        let current = progress_snapshots
            .iter()
            .map(|progress| progress.current)
            .sum::<u32>();
        let encoded = progress_snapshots
            .iter()
            .map(|progress| progress.encoded)
            .sum::<u32>();
        let estimate = progress_snapshots
            .iter()
            .filter_map(|progress| progress.estimated_seconds_remaining)
            .max();
        let rendering_fps = progress_snapshots
            .iter()
            .filter_map(|progress| progress.rendering_fps)
            .sum::<f64>();
        let rendering_fps = (rendering_fps > 0.0).then_some(rendering_fps);
        controller.set_frame_progress(current, combined_frames, encoded, estimate, rendering_fps);

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(SegmentEvent::Completed(index, Ok(filename))) => {
                results[index] = Some(filename);
                completed += 1;
            }
            Ok(SegmentEvent::Completed(_, Err(error))) => {
                if first_error.is_none() {
                    first_error = Some(error);
                    controller.cancel_flag().store(true, Ordering::SeqCst);
                }
                completed += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| CoreError::Encode("Segmented render thread panicked".to_string()))?;
    }

    if let Some(error) = first_error {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }

    if controller.cancel_flag().load(Ordering::SeqCst) {
        cleanup_segment_outputs(paths, &results);
        return Err(CoreError::Cancelled);
    }

    let segment_filenames = results
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            cleanup_segment_outputs(paths, &results);
            CoreError::Encode("Segmented render did not produce all output files".to_string())
        })?;

    controller.set_frame_progress(
        combined_frames,
        combined_frames,
        combined_frames,
        Some(0),
        None,
    );

    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let public_filename = format!("video_{}.mov", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&public_filename);
    let concat_started = Instant::now();
    if let Err(error) = concat_video_segments(paths, &ffmpeg_bin, &segment_filenames, &output_path)
    {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }
    let concat_duration_ms = concat_started.elapsed().as_secs_f64() * 1000.0;
    if let Err(error) = write_stitch_summary(
        paths,
        config,
        &public_filename,
        concat_duration_ms,
        &segment_configs,
        &segment_reports,
        &segment_filenames,
    ) {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }
    cleanup_segment_outputs(paths, &results);
    Ok(public_filename)
}

// Returns the scene duration when start and end are exact integer seconds.
fn integer_second_duration(config: &RenderConfig) -> Option<u32> {
    // Stitching expects clean second boundaries so duplicated or missing frames
    // are not introduced at segment joins.
    let start = config.scene.start.round();
    let end = config.scene.end.round();
    if (config.scene.start - start).abs() > 1e-9 || (config.scene.end - end).abs() > 1e-9 {
        return None;
    }
    if end <= start {
        return None;
    }
    Some((end - start) as u32)
}

// Splits an integer-second scene into balanced contiguous render windows.
fn integer_second_windows(
    config: &RenderConfig,
    total_seconds: u32,
    segment_count: usize,
) -> Vec<(f64, f64)> {
    let actual_segment_count = segment_count.min(total_seconds as usize).max(1);
    let base_seconds = total_seconds / actual_segment_count as u32;
    let extra_segments = total_seconds % actual_segment_count as u32;
    let mut cursor = config.scene.start.round();
    let mut windows = Vec::with_capacity(actual_segment_count);

    for index in 0..actual_segment_count {
        let segment_seconds = base_seconds + u32::from((index as u32) < extra_segments);
        let next_cursor = cursor + f64::from(segment_seconds);
        windows.push((cursor, next_cursor));
        cursor = next_cursor;
    }

    windows
}

// Creates a segment-local controller that shares the parent cancel flag.
fn child_render_controller(total_frames: u32, cancel_flag: &Arc<AtomicBool>) -> RenderController {
    // Child controllers reuse the parent cancel flag so any segment failure or
    // user cancellation can stop all in-flight workers promptly.
    let controller = RenderController {
        progress: Arc::new(Mutex::new(RenderProgress::default())),
        cancel_flag: cancel_flag.clone(),
        running: Arc::new(AtomicBool::new(false)),
        next_render_id: Arc::new(AtomicU32::new(0)),
    };
    let _ = controller.try_start(total_frames, "Segment render");
    controller
}

// Removes temporary segment output files after stitching or failure.
fn cleanup_segment_outputs(paths: &AppPaths, results: &[Option<String>]) {
    // Segment files are implementation details; callers only receive the final
    // stitched movie. Best-effort cleanup keeps failed renders from piling up.
    for filename in results.iter().flatten() {
        let _ = std::fs::remove_file(paths.downloads_dir.join(filename));
    }
}
