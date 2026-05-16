//! Composite MP4 render pipeline.
//!
//! The composite path renders transparent Skia overlay frames at the derived
//! overlay FPS and streams them to FFmpeg, which composites them over input
//! video frames and writes the final MP4 output.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use crate::encode::ffmpeg_composite::{
    build_composite_ffmpeg_settings, CompositeFfmpegSettings, HwAccelInfo,
};
use crate::encode::fps::Fps;
use crate::encode::progress::ProgressEstimator;
use crate::encode::video::RenderController;
use crate::encode::video_composite_debug::{
    write_composite_timing_summary, CompositeTimingSummaryInput,
};
use crate::encode::video_debug::timestamp_nanos;
use crate::render::{prepare_preview_assets, render_frame_rgba, RenderTarget};

/// Reusable raw RGBA frame buffer.
struct FrameBuffer {
    pixels: Vec<u8>,
}

/// Result returned by the ffmpeg stdin writer thread.
struct WriterResult {
    written_frames: u64,
    timings: BTreeMap<String, TimingBucket>,
}

/// Timing and command values derived by the composite pipeline shell.
///
/// Keeping this as a small data object makes Phase 4 behavior easy to test and
/// gives the Phase 5 render loop one place to read its exact frame counts.
#[derive(Debug, Clone, PartialEq)]
struct CompositePipelinePlan {
    source_fps: Fps,
    output_fps: Fps,
    overlay_pipe_fps: Fps,
    render_duration: f64,
    overlay_frame_count: u64,
    output_frame_count: u64,
    first_overrun_overlay_index: u64,
    widget_update_rate: u32,
    trim_start: f64,
    codec_name: String,
    bitrate: String,
    ffmpeg_settings: CompositeFfmpegSettings,
    output_filename: String,
    output_path: PathBuf,
}

/// Runs the software composite render pipeline.
///
/// This renders only overlay-frame timestamps, writes raw RGBA frames to
/// FFmpeg stdin, and lets FFmpeg repeat overlay frames between updates.
pub(crate) fn render_composite_video_single(
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
    composite_render_duration: Option<f64>,
    composite_video_trim_start: Option<f64>,
    composite_widget_update_rate: Option<u32>,
) -> Result<String, String> {
    if controller.cancel_flag().load(Ordering::SeqCst) {
        return Err("Rendering cancelled".to_string());
    }

    let plan = derive_composite_pipeline_plan(
        paths,
        config,
        composite_video_path,
        composite_bitrate,
        composite_video_fps_num,
        composite_video_fps_den,
        composite_video_duration,
        composite_render_duration,
        composite_video_trim_start,
        composite_widget_update_rate,
    )?;

    std::fs::create_dir_all(&paths.downloads_dir).map_err(|error| {
        format!(
            "Failed to create output directory {}: {error}",
            paths.downloads_dir.display()
        )
    })?;
    controller.set_frame_progress(
        0,
        plan.output_frame_count.min(u64::from(u32::MAX)) as u32,
        0,
        None,
        None,
    );

    let (prepared_preview_assets, _, _, _) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let mut child = spawn_composite_ffmpeg_process(&ffmpeg_bin, &plan)?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to capture composite ffmpeg stdin".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture composite ffmpeg stderr".to_string())?;
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));
    let monitor_lines = stderr_lines.clone();
    let monitor_thread = thread::spawn(move || monitor_composite_ffmpeg(stderr, monitor_lines));

    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let frame_byte_len = (width as usize) * (height as usize) * 4;
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let total_progress = plan.output_frame_count.min(u64::from(u32::MAX)) as u32;
    let cancel_flag = controller.cancel_flag();
    let mut profiler = RenderProfiler::default();
    let mut overlay_frame_index = 0u64;
    let render_started = Instant::now();
    let render_loop_started = Instant::now();
    let output_frame_equivalent_multiplier =
        plan.output_fps.as_f64() / plan.overlay_pipe_fps.as_f64();
    let mut estimator = ProgressEstimator::default();

    let frame_queue_size = 4usize;
    let (sender, receiver) = mpsc::sync_channel::<FrameBuffer>(frame_queue_size);
    let (free_sender, free_receiver) = mpsc::sync_channel::<FrameBuffer>(frame_queue_size + 1);
    for _ in 0..(frame_queue_size + 1) {
        free_sender
            .send(FrameBuffer {
                pixels: vec![0u8; frame_byte_len],
            })
            .map_err(|_| "Failed to initialize composite frame buffer pool".to_string())?;
    }

    let writer_thread = thread::spawn(move || {
        writer_worker(stdin, receiver, free_sender)
    });

    let render_result = (|| -> Result<(), String> {
        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "composite ffmpeg exited unexpectedly with status {status}"
                ));
            }

            let video_local_time = overlay_frame_index as f64 / plan.overlay_pipe_fps.as_f64();
            if video_local_time >= plan.render_duration {
                break;
            }
            let frame_started = Instant::now();
            let activity_time = composite_sync_offset + video_local_time;
            let frame_result = (|| -> Result<(), String> {
                let dense_frame_index =
                    dense_frame_index_for_overlay(config, dense_activity, &plan, activity_time)?;

                let mut frame_buffer =
                    acquire_frame_buffer(&free_receiver, cancel_flag.as_ref(), &mut profiler)?;
                render_frame_rgba(
                    paths,
                    config,
                    dense_activity,
                    &prepared_preview_assets.prepared_assets,
                    dense_frame_index,
                    scale,
                    None,
                    RenderTarget {
                        width,
                        height,
                        pixels: frame_buffer.pixels.as_mut_slice(),
                    },
                    &mut profiler,
                )?;
                queue_frame(&sender, frame_buffer, cancel_flag.as_ref(), &mut profiler)?;
                Ok(())
            })();
            profiler.record_ms(
                "frame.total",
                frame_started.elapsed().as_secs_f64() * 1000.0,
            );
            frame_result?;

            overlay_frame_index += 1;
            let estimated_output_progress =
                output_progress_for_overlay_time(video_local_time, &plan);
            let current_progress = estimated_output_progress.min(total_progress);
            let output_equivalent_frame_seconds =
                frame_started.elapsed().as_secs_f64() / output_frame_equivalent_multiplier;
            let (estimate, rendering_fps) = estimator.record(
                current_progress,
                total_progress,
                output_equivalent_frame_seconds,
                render_started.elapsed().as_secs_f64(),
            );
            controller.set_frame_progress(
                current_progress,
                total_progress,
                current_progress,
                estimate,
                rendering_fps,
            );
        }
        Ok(())
    })();
    let render_loop_ms = render_loop_started.elapsed().as_secs_f64() * 1000.0;

    drop(sender);
    let writer_result = writer_thread
        .join()
        .map_err(|_| "Composite encoder writer thread panicked".to_string())?
        .map_err(|error| error)?;
    let was_cancelled = cancel_flag.load(Ordering::SeqCst);
    let ffmpeg_finalize_started = Instant::now();
    let status = if was_cancelled {
        terminate_composite_ffmpeg_after_cancel(&mut child)?
    } else {
        child.wait().map_err(|error| error.to_string())?
    };
    let ffmpeg_finalize_wait_ms = ffmpeg_finalize_started.elapsed().as_secs_f64() * 1000.0;
    monitor_thread
        .join()
        .map_err(|_| "Composite ffmpeg monitor thread panicked".to_string())?;

    if let Err(error) = render_result {
        let _ = std::fs::remove_file(&plan.output_path);
        let stderr = stderr_snapshot(&stderr_lines);
        if is_pipe_write_error(&error) {
            return Err(format_pipe_write_failure(error, status, &stderr, &plan));
        }
        if stderr.is_empty() {
            return Err(error);
        }
        return Err(format!("{error}. FFmpeg stderr:\n{}", stderr_tail(&stderr)));
    }
    if was_cancelled {
        let _ = std::fs::remove_file(&plan.output_path);
        return Err("Rendering cancelled".to_string());
    }
    if !status.success() {
        let _ = std::fs::remove_file(&plan.output_path);
        let stderr = stderr_snapshot(&stderr_lines);
        return Err(format_composite_ffmpeg_failure(&plan, status, &stderr));
    }
    if writer_result.written_frames != expected_guarded_overlay_frame_count(&plan) {
        let _ = std::fs::remove_file(&plan.output_path);
        return Err(format!(
            "Composite overlay writer ended early: wrote {} of {} frames",
            writer_result.written_frames,
            expected_guarded_overlay_frame_count(&plan)
        ));
    }
    verify_successful_composite_output(&plan.output_path)?;

    let total_ms = render_started.elapsed().as_secs_f64() * 1000.0;
    let merged_timings = merge_timing_maps(profiler.summary(), writer_result.timings);
    write_composite_timing_summary(CompositeTimingSummaryInput {
        debug_render_dir: &paths.debug_render_dir,
        ffmpeg_settings: &plan.ffmpeg_settings,
        output_path: &plan.output_path,
        source_fps: plan.source_fps,
        overlay_pipe_fps: plan.overlay_pipe_fps,
        widget_update_rate: plan.widget_update_rate,
        render_duration: plan.render_duration,
        overlay_frame_count: writer_result.written_frames,
        output_frame_count: plan.output_frame_count,
        total_ms,
        render_loop_ms,
        ffmpeg_finalize_wait_ms,
        timings: merged_timings,
        codec: &plan.codec_name,
        bitrate: &plan.bitrate,
        input_width: width,
        input_height: height,
        trim_start: plan.trim_start,
        sync_offset: composite_sync_offset,
    })?;
    controller.set_frame_progress(
        total_progress,
        total_progress,
        total_progress,
        Some(0),
        None,
    );
    Ok(plan.output_filename)
}

/// Converts one overlay timestamp into user-facing output-frame progress.
///
/// Composite renders may write fewer overlay frames than final video frames, so
/// progress is based on the source/output FPS rather than the overlay pipe FPS.
fn output_progress_for_overlay_time(video_local_time: f64, plan: &CompositePipelinePlan) -> u32 {
    (video_local_time * plan.output_fps.as_f64())
        .round()
        .max(0.0)
        .min(plan.output_frame_count as f64) as u32
}

/// Terminates FFmpeg after a user cancellation request.
///
/// Closing stdin gives FFmpeg a short chance to exit cleanly; if it keeps
/// running, the process is killed and waited so no encoder process is orphaned.
fn terminate_composite_ffmpeg_after_cancel(
    child: &mut std::process::Child,
) -> Result<std::process::ExitStatus, String> {
    for _ in 0..10 {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(50));
    }

    child.kill().map_err(|error| {
        format!("Failed to terminate composite ffmpeg after cancellation: {error}")
    })?;
    child.wait().map_err(|error| error.to_string())
}

/// Confirms that FFmpeg finalized a usable output file on success.
///
/// A successful process exit without a non-empty MP4 is treated as a render
/// failure because callers need a playable artifact, not just a clean status.
fn verify_successful_composite_output(output_path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(output_path).map_err(|error| {
        format!(
            "Composite render finished but output file is missing ({}): {error}",
            output_path.display()
        )
    })?;
    if metadata.len() == 0 {
        return Err(format!(
            "Composite render finished but output file is empty: {}",
            output_path.display()
        ));
    }
    Ok(())
}

/// Returns whether an overlay write error indicates FFmpeg closed the pipe.
///
/// Broken-pipe wording varies by platform, so this uses the common error text
/// and OS error fragment instead of matching a single exact message.
fn is_pipe_write_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("failed writing composite overlay frame")
        && (lower.contains("broken pipe")
            || lower.contains("pipe is being closed")
            || lower.contains("os error 32")
            || lower.contains("os error 109")
            || lower.contains("os error 232"))
}

/// Formats a pipe-write failure with FFmpeg status and stderr diagnostics.
///
/// This makes early FFmpeg exits distinguishable from renderer bugs while still
/// preserving the underlying write error and recent encoder output.
fn format_pipe_write_failure(
    error: String,
    status: std::process::ExitStatus,
    stderr: &str,
    plan: &CompositePipelinePlan,
) -> String {
    let mut message = format!(
        "{error}. FFmpeg terminated before all overlay frames were written (status {status}) for profile {}.",
        plan.ffmpeg_settings.selected_profile_name
    );
    if let Some(fallback) = &plan.ffmpeg_settings.fallback_profile_name {
        message.push_str(&format!(
            "\nSafe fallback profile available: {fallback}. This explicit experimental render was not silently retried."
        ));
    }
    message.push_str("\nFilter graph:\n");
    message.push_str(&plan.ffmpeg_settings.filter_complex);
    if !stderr.trim().is_empty() {
        message.push_str("\nFFmpeg stderr:\n");
        message.push_str(&stderr_tail(stderr));
    }
    message
}

/// Formats a failed FFmpeg completion with full-GPU profile diagnostics.
///
/// Experimental CUDA/QSV profiles include the selected profile, filter graph,
/// and safe fallback profile name so callers can decide whether to retry.
fn format_composite_ffmpeg_failure(
    plan: &CompositePipelinePlan,
    status: std::process::ExitStatus,
    stderr: &str,
) -> String {
    let mut message = format!(
        "Composite ffmpeg failed ({status}) for profile {}.",
        plan.ffmpeg_settings.selected_profile_name
    );
    if let Some(fallback) = &plan.ffmpeg_settings.fallback_profile_name {
        message.push_str(&format!(
            "\nSafe fallback profile available: {fallback}. This explicit experimental render was not silently retried."
        ));
    }
    message.push_str("\nFilter graph:\n");
    message.push_str(&plan.ffmpeg_settings.filter_complex);
    message.push_str("\nFFmpeg stderr:\n");
    message.push_str(&stderr_tail(stderr));
    message
}

/// Derives Phase 4 composite timing and FFmpeg settings.
///
/// This helper mirrors the future render loop's timing math, including the
/// fractional-frame overrun guard, without producing any overlay frames.
fn derive_composite_pipeline_plan(
    paths: &AppPaths,
    config: &RenderConfig,
    composite_video_path: &str,
    composite_bitrate: &str,
    composite_video_fps_num: u32,
    composite_video_fps_den: u32,
    composite_video_duration: f64,
    composite_render_duration: Option<f64>,
    composite_video_trim_start: Option<f64>,
    composite_widget_update_rate: Option<u32>,
) -> Result<CompositePipelinePlan, String> {
    let source_fps = Fps::new(composite_video_fps_num, composite_video_fps_den)?;
    let output_fps = source_fps;
    let update_rate = composite_widget_update_rate.unwrap_or(1).max(1);
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let trim_start = composite_video_trim_start.unwrap_or(0.0);
    let render_duration =
        composite_render_duration.unwrap_or(composite_video_duration - trim_start);
    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let codec_name = composite_codec_name(config);

    if !composite_video_duration.is_finite() || composite_video_duration <= 0.0 {
        return Err(format!(
            "Composite video duration must be greater than zero: {composite_video_duration}"
        ));
    }
    if !trim_start.is_finite() || trim_start < 0.0 {
        return Err(format!(
            "Composite video trim start must be zero or greater: {trim_start}"
        ));
    }
    if trim_start >= composite_video_duration {
        return Err(format!(
            "Composite video trim start ({trim_start}) must be less than video duration ({composite_video_duration})"
        ));
    }
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(format!(
            "Composite render duration must be greater than zero: {render_duration}"
        ));
    }

    let overlay_frame_count = (render_duration * overlay_pipe_fps.as_f64())
        .ceil()
        .max(0.0) as u64;
    let output_frame_count = (render_duration * output_fps.as_f64()).ceil().max(0.0) as u64;
    let first_overrun_overlay_index =
        first_fractional_overrun_overlay_index(render_duration, overlay_pipe_fps);
    let mut hwaccel_info = HwAccelInfo::trust_selected_profile();
    hwaccel_info.qsv_full_init_args = composite_qsv_full_init_args(config);
    let ffmpeg_settings = build_composite_ffmpeg_settings(
        &codec_name,
        composite_bitrate,
        Path::new(composite_video_path),
        trim_start,
        render_duration,
        width,
        height,
        source_fps,
        overlay_pipe_fps,
        &hwaccel_info,
    )?;
    let output_filename = format!("video_composited_{}.mp4", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&output_filename);

    Ok(CompositePipelinePlan {
        source_fps,
        output_fps,
        overlay_pipe_fps,
        render_duration,
        overlay_frame_count,
        output_frame_count,
        first_overrun_overlay_index,
        widget_update_rate: update_rate,
        trim_start,
        codec_name,
        bitrate: composite_bitrate.to_string(),
        ffmpeg_settings,
        output_filename,
        output_path,
    })
}

/// Spawns FFmpeg for a three-input composite render.
///
/// Input 0 is the unseeked source video used for filter-side video trimming,
/// input 1 is raw RGBA overlay frames streamed through stdin as `pipe:0`, and
/// input 2 is a separately trimmed source-media input used for audio copy.
fn spawn_composite_ffmpeg_process(
    ffmpeg_bin: &Path,
    plan: &CompositePipelinePlan,
) -> Result<std::process::Child, String> {
    let mut command = Command::new(ffmpeg_bin);
    suppress_child_console(&mut command);
    command.arg("-loglevel").arg("info");
    command.args(&plan.ffmpeg_settings.hw_init_args);
    command.args(&plan.ffmpeg_settings.input_0_args);
    command.args(&plan.ffmpeg_settings.input_1_args);
    command.args(&plan.ffmpeg_settings.input_2_args);
    command
        .arg("-filter_complex")
        .arg(&plan.ffmpeg_settings.filter_complex)
        .args(&plan.ffmpeg_settings.output_args)
        .arg(&plan.output_path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null());

    command
        .spawn()
        .map_err(|error| format!("Could not start composite ffmpeg: {error}"))
}

/// Maps one overlay timestamp to a dense activity frame index.
///
/// Composite-adjusted dense reports use direct `overlay j -> dense j` mapping;
/// otherwise this falls back to scene-start-relative time mapping.
fn dense_frame_index_for_overlay(
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    plan: &CompositePipelinePlan,
    activity_time: f64,
) -> Result<usize, String> {
    let direct_index = if dense_report_matches_composite_window(config, plan) {
        let video_local_time = activity_time - config.scene.start;
        Some((video_local_time * plan.overlay_pipe_fps.as_f64()).round() as usize)
    } else {
        None
    };
    let dense_frame_index = match direct_index {
        Some(index) => index,
        None => {
            let idx = ((activity_time - config.scene.start) * config.scene.fps).floor();
            if idx < 0.0 {
                return Err(format!(
                    "Composite overlay frame is before dense activity range: activity_time={activity_time}, scene.start={}",
                    config.scene.start
                ));
            }
            idx as usize
        }
    };

    if dense_frame_index >= dense_activity.frame_count {
        return Err(format!(
            "Composite dense frame index {dense_frame_index} is outside dense activity range 0..{}",
            dense_activity.frame_count
        ));
    }
    Ok(dense_frame_index)
}

/// Returns whether the dense report was rebuilt for the composite window.
///
/// This checks the Phase 3 timing contract with a small floating-point tolerance
/// so the hot render loop can use direct frame-index mapping when valid.
fn dense_report_matches_composite_window(
    config: &RenderConfig,
    plan: &CompositePipelinePlan,
) -> bool {
    let expected_end = config.scene.start + plan.render_duration;
    (config.scene.end - expected_end).abs() <= 1e-6
        && (config.scene.fps - plan.overlay_pipe_fps.as_f64()).abs() <= 1e-9
        && dense_report_frame_count_matches(config, plan)
}

/// Checks whether scene timing implies the same guarded overlay frame count.
fn dense_report_frame_count_matches(config: &RenderConfig, plan: &CompositePipelinePlan) -> bool {
    let scene_frames = ((config.scene.end - config.scene.start) * config.scene.fps)
        .ceil()
        .max(0.0) as u64;
    scene_frames == expected_guarded_overlay_frame_count(plan)
}

/// Counts overlay frames whose timestamps are strictly inside render duration.
fn expected_guarded_overlay_frame_count(plan: &CompositePipelinePlan) -> u64 {
    plan.first_overrun_overlay_index
}

/// Reads FFmpeg stderr without blocking the encoder process.
fn monitor_composite_ffmpeg(stderr: std::process::ChildStderr, lines: Arc<Mutex<Vec<String>>>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(mut locked) = lines.lock() {
            locked.push(line);
        }
    }
}

/// Returns a best-effort snapshot of collected FFmpeg stderr lines.
fn stderr_snapshot(lines: &Arc<Mutex<Vec<String>>>) -> String {
    lines
        .lock()
        .map(|lines| lines.join("\n"))
        .unwrap_or_default()
}

/// Returns the final part of FFmpeg stderr for concise error messages.
fn stderr_tail(stderr: &str) -> String {
    let lines = stderr.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(30);
    lines[start..].join("\n")
}

/// Returns the composite video codec requested by `scene.ffmpeg`.
///
/// MP4 compositing defaults to software H.264 because the transparent-export
/// defaults are alpha codecs that are not suitable for final MP4 output.
fn composite_codec_name(config: &RenderConfig) -> String {
    config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("libx264")
        .to_string()
}

/// Reads detected QSV full-overlay initialization args from `scene.ffmpeg`.
///
/// The frontend injects these render-time args after codec detection so the
/// backend can reuse the exact hardware-device candidate that passed probing.
fn composite_qsv_full_init_args(config: &RenderConfig) -> Vec<String> {
    config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("qsv_full_init_args"))
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Finds the first overlay frame index that the render loop must reject.
///
/// The render loop uses the equivalent guard
/// `video_local_time >= render_duration` so fractional durations never emit an
/// extra tail frame.
fn first_fractional_overrun_overlay_index(render_duration: f64, overlay_pipe_fps: Fps) -> u64 {
    let mut index = (render_duration * overlay_pipe_fps.as_f64())
        .floor()
        .max(0.0) as u64;
    loop {
        let video_local_time = index as f64 / overlay_pipe_fps.as_f64();
        if video_local_time >= render_duration {
            return index;
        }
        index += 1;
    }
}

/// Writes queued frame buffers into ffmpeg stdin and returns buffers to the pool.
fn writer_worker(
    mut stdin: std::process::ChildStdin,
    receiver: Receiver<FrameBuffer>,
    free_sender: SyncSender<FrameBuffer>,
) -> Result<WriterResult, String> {
    let mut profiler = crate::debug::RenderProfiler::default();
    let mut written_frames = 0u64;
    loop {
        let frame = match receiver.recv() {
            Ok(frame) => frame,
            Err(_) => break,
        };
        let write_started = Instant::now();
        stdin
            .write_all(frame.pixels.as_slice())
            .map_err(|error| format!("Failed writing composite overlay frame: {error}"))?;
        profiler.record_ms(
            "ffmpeg.write",
            write_started.elapsed().as_secs_f64() * 1000.0,
        );
        written_frames += 1;
        let _ = free_sender.send(frame);
    }
    let _ = stdin.flush();
    Ok(WriterResult {
        written_frames,
        timings: profiler.summary(),
    })
}

/// Waits for a reusable frame buffer from the free-buffer pool.
fn acquire_frame_buffer(
    receiver: &Receiver<FrameBuffer>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    profiler: &mut RenderProfiler,
) -> Result<FrameBuffer, String> {
    let started = Instant::now();
    loop {
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Rendering cancelled".to_string());
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(buffer) => {
                profiler.record_ms(
                    "buffer.acquire_wait",
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                return Ok(buffer);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Frame buffer pool disconnected".to_string());
            }
        }
    }
}

/// Sends a completed frame to the writer thread while respecting cancellation.
fn queue_frame(
    sender: &SyncSender<FrameBuffer>,
    frame_buffer: FrameBuffer,
    cancel_flag: &std::sync::atomic::AtomicBool,
    profiler: &mut RenderProfiler,
) -> Result<(), String> {
    let started = Instant::now();
    let mut payload = frame_buffer;
    loop {
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Rendering cancelled".to_string());
        }
        match sender.try_send(payload) {
            Ok(()) => {
                profiler.record_ms(
                    "queue.put_wait",
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                return Ok(());
            }
            Err(TrySendError::Full(returned_payload)) => {
                payload = returned_payload;
                thread::sleep(Duration::from_millis(10));
            }
            Err(TrySendError::Disconnected(_)) => {
                return Err("Encoder queue disconnected".to_string());
            }
        }
    }
}

/// Merges timing buckets recorded on separate render/writer threads.
fn merge_timing_maps(
    mut left: BTreeMap<String, TimingBucket>,
    right: BTreeMap<String, TimingBucket>,
) -> BTreeMap<String, TimingBucket> {
    for (name, bucket) in right {
        let entry = left.entry(name).or_default();
        entry.count += bucket.count;
        entry.total_ms += bucket.total_ms;
        entry.avg_ms = if entry.count == 0 {
            0.0
        } else {
            entry.total_ms / f64::from(entry.count)
        };
        entry.max_ms = entry.max_ms.max(bucket.max_ms);
    }
    left
}

#[cfg(test)]
#[path = "tests/video_composite_pipeline_tests.rs"]
mod tests;
