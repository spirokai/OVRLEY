//! Segmented render orchestration for transparent and composite exports.
//!
//! This module owns the segmented render path: heuristics, child-controller
//! plumbing, progress aggregation, final segment stitching, and best-effort
//! cleanup of temporary segment outputs. The public facade in `encode::video`
//! decides when to call into these helpers.

use crate::activity::build_dense_activity_report;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::fps::Fps;
use crate::encode::progress::RenderController;
use crate::encode::video::CompositeRenderRequest;
use crate::encode::video_composite_pipeline::render_composite_video_single;
use crate::encode::video_debug::{concat_video_segments, timestamp_nanos, write_stitch_summary};
use crate::encode::video_parallel::{
    estimate_composite_segment_count, estimate_parallel_render_worker_count,
};
use crate::encode::video_pipeline::{render_video_single, rendered_frame_count};
use crate::encode::video_windows::{
    composite_output_frame_windows, integer_second_duration, integer_second_windows,
};
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Returns whether transparent rendering should be split into stitched segments.
pub(crate) fn should_parallelize_segmented(
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
) -> bool {
    // qtrle and prores_ks_vulkan encoding are comparatively slow and
    // stitch-friendly for integer-second windows.
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

/// Returns whether composite rendering should be split into parallel segments.
pub(crate) fn should_parallelize_composite(
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

/// Renders output as multiple second-aligned transparent segments and stitches them.
pub(crate) fn render_video_segmented(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> CoreResult<String> {
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

    // Layer 1 aggregates child progress into the parent controller.
    // Layer 2 collects filenames and propagates the first failure through the
    // shared cancel flag so the remaining segments stop promptly.
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

/// Renders a composite MP4 as parallel time-window segments stitched together.
pub(crate) fn render_composite_video_segmented(
    request: &CompositeRenderRequest<'_>,
    render_duration: f64,
    trim_start: f64,
    update_rate: u32,
) -> CoreResult<String> {
    let codec = request
        .config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("libx264");
    let source_fps = Fps::new(
        request.composite_video_fps_num,
        request.composite_video_fps_den,
    )?;
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let total_overlay_frames = (render_duration * overlay_pipe_fps.as_f64()).ceil() as usize;
    let segment_count = estimate_composite_segment_count(total_overlay_frames.max(1), codec);
    if segment_count < 2 {
        return render_composite_single_pass(request, render_duration, trim_start, update_rate);
    }

    let total_output_frames = (render_duration * source_fps.as_f64()).ceil().max(0.0) as u32;
    let segments = composite_output_frame_windows(
        total_output_frames,
        render_duration,
        source_fps,
        segment_count,
    );
    if segments.len() < 2 {
        return render_composite_single_pass(request, render_duration, trim_start, update_rate);
    }

    let segment_output_frames = segments
        .iter()
        .map(|segment| segment.output_end_frame - segment.output_start_frame)
        .collect::<Vec<_>>();
    let actual_segment_count = segments.len();
    if actual_segment_count < 2 {
        return render_composite_single_pass(request, render_duration, trim_start, update_rate);
    }

    let combined_frames = segment_output_frames.iter().copied().sum::<u32>();

    let child_cancel_flag = request.controller.cancel_flag();
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
        let segment_paths = request.paths.clone();
        let segment_config = request.config.clone();
        let segment_activity = request.activity.clone();
        let segment_dense = request.dense_activity.clone();
        let segment_video_path = request.composite_video_path.to_string();
        let segment_bitrate = request.composite_bitrate.to_string();
        let segment = segments[index];
        let segment_trim_start = trim_start + segment.video_start_seconds;
        let segment_render_duration = segment.render_duration_seconds;
        let segment_sync_offset = request.composite_sync_offset + segment.video_start_seconds;
        let composite_video_fps_num = request.composite_video_fps_num;
        let composite_video_fps_den = request.composite_video_fps_den;
        let composite_video_duration = request.composite_video_duration;

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

    // Layer 1 aggregates child progress into the parent controller.
    // Layer 2 collects filenames, cancels sibling segments on first failure,
    // then stitches the successful outputs.
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
        request.controller.set_frame_progress(
            current,
            combined_frames,
            encoded,
            estimate,
            rendering_fps,
        );

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(SegmentEvent::Completed(index, Ok(filename))) => {
                results[index] = Some(filename);
                completed += 1;
            }
            Ok(SegmentEvent::Completed(_, Err(error))) => {
                if first_error.is_none() {
                    first_error = Some(error);
                    request
                        .controller
                        .cancel_flag()
                        .store(true, Ordering::SeqCst);
                }
                completed += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    for handle in handles {
        handle.join().map_err(|_| {
            CoreError::Encode("Composite segment render thread panicked".to_string())
        })?;
    }

    if let Some(error) = first_error {
        cleanup_segment_outputs(request.paths, &results);
        return Err(error);
    }

    if request.controller.cancel_flag().load(Ordering::SeqCst) {
        cleanup_segment_outputs(request.paths, &results);
        return Err(CoreError::Cancelled);
    }

    let segment_filenames: Option<Vec<String>> = results.iter().cloned().collect();
    let segment_filenames = match segment_filenames {
        Some(filenames) => filenames,
        None => {
            cleanup_segment_outputs(request.paths, &results);
            return Err(CoreError::Encode(
                "Composite segment render did not produce all output files".to_string(),
            ));
        }
    };

    request.controller.set_frame_progress(
        combined_frames,
        combined_frames,
        combined_frames,
        Some(0),
        None,
    );

    let ffmpeg_bin = resolve_ffmpeg_binary(&request.paths.repo_root)?;
    let public_filename = format!("video_composited_{}.mp4", timestamp_nanos()?);
    let output_path = request.paths.downloads_dir.join(&public_filename);
    if let Err(error) =
        concat_video_segments(request.paths, &ffmpeg_bin, &segment_filenames, &output_path)
    {
        cleanup_segment_outputs(request.paths, &results);
        return Err(error);
    }
    cleanup_segment_outputs(request.paths, &results);
    Ok(public_filename)
}

/// Runs the composite single-pass renderer with normalized timing parameters.
fn render_composite_single_pass(
    request: &CompositeRenderRequest<'_>,
    render_duration: f64,
    trim_start: f64,
    update_rate: u32,
) -> CoreResult<String> {
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

/// Creates a segment-local controller that shares the parent cancel flag.
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

/// Removes temporary segment output files after stitching or failure.
fn cleanup_segment_outputs(paths: &AppPaths, results: &[Option<String>]) {
    // Segment files are implementation details; callers only receive the final
    // stitched movie. Best-effort cleanup keeps failed renders from piling up.
    for filename in results.iter().flatten() {
        let _ = std::fs::remove_file(paths.downloads_dir.join(filename));
    }
}
