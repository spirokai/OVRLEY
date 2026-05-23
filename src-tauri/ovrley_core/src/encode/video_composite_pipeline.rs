//! Multi-pass composite MP4 render pipeline.
//!
//! Renders Skia frames, composites with source video segments,
//! and produces final H.264/H.265 MP4 output.
//!
//! Must not import from [`video_pipeline`].
//!
//! The composite path renders transparent Skia overlay frames at the derived
//! overlay FPS and streams them to FFmpeg, which composites them over input
//! video frames and writes the final MP4 output.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::debug::RenderProfiler;
use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use crate::encode::ffmpeg_composite::{
    build_composite_ffmpeg_settings, CompositeFfmpegBuildRequest, CompositeFfmpegSettings,
    HwAccelInfo,
};
use crate::encode::fps::Fps;
use crate::encode::pipeline_shared::{
    acquire_frame_buffer, merge_timing_maps, queue_frame, writer_worker, FrameBuffer,
    WriterCancellation, WriterWorkerConfig,
};
use crate::encode::progress::ProgressEstimator;
use crate::encode::video::RenderController;
use crate::encode::video_composite_debug::{
    write_composite_timing_summary, CompositeTimingSummaryInput,
};
use crate::encode::video_composite_support::{
    format_pipe_write_failure, is_pipe_write_error, output_progress_for_overlay_time, stderr_tail,
    verify_successful_composite_output,
};
use crate::encode::video_debug::timestamp_nanos;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;
use crate::render::{prepare_preview_assets, render_frame_rgba, FrameRenderRequest, RenderTarget};

/// Composite render values derived from render-time scene fields.
///
/// These values drive dense-report timing and are passed to the composite
/// FFmpeg pipeline without reinterpreting sync offset as seek.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeRenderPlan {
    // test seam
    pub video_path: String,
    pub bitrate: String,
    pub sync_offset: f64,
    pub trim_start: f64,
    pub video_duration: f64,
    pub render_duration: f64,
    pub update_rate: u32,
    pub source_fps: Fps,
    pub overlay_pipe_fps: Fps,
}

/// Validates composite render fields and derives timing/FPS values.
///
/// Required fields fail before dense activity is built, while optional fields
/// receive standard defaults.
pub fn derive_composite_render_plan(config: &RenderConfig) -> CoreResult<CompositeRenderPlan> {
    // test seam
    let video_path = config
        .scene
        .composite_video_path
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            CoreError::Config("scene.composite_video_path required for composite render".into())
        })?;
    let bitrate = config
        .scene
        .composite_bitrate
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            CoreError::Config("scene.composite_bitrate required for composite render".into())
        })?;
    let fps_num = config.scene.composite_video_fps_num.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_num required for composite render".into())
    })?;
    let fps_den = config.scene.composite_video_fps_den.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_den required for composite render".into())
    })?;
    let source_fps = Fps::new(fps_num, fps_den)?;
    let video_duration = config.scene.composite_video_duration.ok_or_else(|| {
        CoreError::Config("scene.composite_video_duration required for composite render".into())
    })?;
    if !video_duration.is_finite() || video_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_video_duration must be greater than zero: {video_duration}"
        )));
    }

    let sync_offset = config.scene.composite_sync_offset.unwrap_or(0.0);
    if !sync_offset.is_finite() || sync_offset < 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_sync_offset must be zero or greater: {sync_offset}"
        )));
    }
    let trim_start = config.scene.composite_video_trim_start.unwrap_or(0.0);
    if !trim_start.is_finite() || trim_start < 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_video_trim_start must be zero or greater: {trim_start}"
        )));
    }
    if trim_start >= video_duration {
        return Err(CoreError::Config(format!(
            "scene.composite_video_trim_start ({trim_start}) must be less than scene.composite_video_duration ({video_duration})"
        )));
    }

    let update_rate = config
        .scene
        .composite_widget_update_rate
        .unwrap_or(1)
        .max(1);
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let render_duration = config
        .scene
        .composite_render_duration
        .unwrap_or(video_duration - trim_start);
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_render_duration must be greater than zero: {render_duration}"
        )));
    }

    Ok(CompositeRenderPlan {
        video_path,
        bitrate,
        sync_offset,
        trim_start,
        video_duration,
        render_duration,
        update_rate,
        source_fps,
        overlay_pipe_fps,
    })
}

/// Applies composite timing to a local render config before densification.
///
/// This keeps persisted template timing untouched while aligning dense frames
/// with the lower-FPS overlay stream used by compositing mode.
pub fn apply_composite_scene_timing(config: &mut RenderConfig, plan: &CompositeRenderPlan) {
    // test seam
    config.scene.start = plan.sync_offset;
    config.scene.end = plan.sync_offset + plan.render_duration;
    config.scene.fps = plan.overlay_pipe_fps.as_f64();
    config.scene.update_rate = Some(1);
}

/// Timing and command values derived by the composite pipeline shell.
///
/// Keeping this as a small data object makes timing math easy to test and
/// gives the render loop one place to read its exact frame counts.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositePipelinePlan {
    // test seam
    pub source_fps: Fps,
    pub output_fps: Fps,
    pub overlay_pipe_fps: Fps,
    pub render_duration: f64,
    pub overlay_frame_count: u64,
    pub output_frame_count: u64,
    pub first_overrun_overlay_index: u64,
    pub widget_update_rate: u32,
    pub trim_start: f64,
    pub codec_name: String,
    pub bitrate: String,
    pub ffmpeg_settings: CompositeFfmpegSettings,
    pub output_filename: String,
    pub output_path: PathBuf,
}

/// Runs the software composite render pipeline.
///
/// This renders only overlay-frame timestamps, writes raw RGBA frames to
/// FFmpeg stdin, and lets FFmpeg repeat overlay frames between updates.
///
/// # Phases
/// 1. Derive pipeline plan (timing, FPS, FFmpeg args, output path)
/// 2. Prepare Skia assets
/// 3. Spawn ffmpeg, monitor thread, and writer thread
/// 4. Hot render loop: produce overlay frames into bounded queue, track progress
/// 5. Drain writer, wait for ffmpeg, join monitor
/// 6. Verify output, write debug summary
// Called from multiple sites across video.rs, tests, and benchmarks;
// request-struct refactor deferred to avoid destabilising test seams.
#[allow(clippy::too_many_arguments)]
pub fn render_composite_video_single(
    // test seam
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
) -> CoreResult<String> {
    if controller.cancel_flag().load(Ordering::SeqCst) {
        return Err(CoreError::Cancelled);
    }

    // ── PHASE 1: DERIVE PIPELINE PLAN (timing, FPS, FFmpeg args, output path) ──
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

    std::fs::create_dir_all(&paths.downloads_dir).map_err(|error| CoreError::Io {
        path: paths.downloads_dir.clone(),
        source: error,
    })?;
    controller.set_frame_progress(
        0,
        plan.output_frame_count.min(u64::from(u32::MAX)) as u32,
        0,
        None,
        None,
    );

    // ── PHASE 2: PREPARE SKIA ASSETS ──
    let (prepared_preview_assets, _, _, _) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;

    // ── PHASE 3: SPAWN FFMPEG & WORKER THREADS ──
    let mut child = spawn_composite_ffmpeg_process(&ffmpeg_bin, &plan)?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Encode("Failed to capture composite ffmpeg stdin".to_string()))?;
    let stderr = child.stderr.take().ok_or_else(|| {
        CoreError::Encode("Failed to capture composite ffmpeg stderr".to_string())
    })?;
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
            .map_err(|_| {
                CoreError::Encode("Failed to initialize composite frame buffer pool".to_string())
            })?;
    }

    let writer_thread = thread::spawn(move || {
        writer_worker(
            stdin,
            receiver,
            free_sender,
            WriterWorkerConfig {
                cancellation: WriterCancellation::DrainUntilQueueCloses,
                write_error_context: "Failed writing composite overlay frame",
                queue_wait_metric: None,
                release_wait_metric: None,
                release_error_message: None,
                flush_error_is_fatal: false,
            },
        )
    });

    // ── PHASE 4: HOT RENDER LOOP — produce overlay frames into bounded queue ──
    // The bounded channel (capacity 4 for composite) provides backpressure; the
    // writer drains it and feeds ffmpeg stdin. Overlay frames are rendered at
    // the pipe FPS; ffmpeg repeats them across output frames internally.
    let render_result = (|| -> CoreResult<()> {
        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|error| CoreError::Encode(format!("ffmpeg process error: {error}")))?
            {
                return Err(CoreError::Encode(format!(
                    "composite ffmpeg exited unexpectedly with status {status}"
                )));
            }

            let video_local_time = overlay_frame_index as f64 / plan.overlay_pipe_fps.as_f64();
            if video_local_time >= plan.render_duration {
                break;
            }
            let frame_started = Instant::now();
            let activity_time = composite_sync_offset + video_local_time;
            let frame_result = (|| -> CoreResult<()> {
                let dense_frame_index =
                    dense_frame_index_for_overlay(config, dense_activity, &plan, activity_time)?;

                let mut frame_buffer =
                    acquire_frame_buffer(&free_receiver, cancel_flag.as_ref(), &mut profiler)?;
                render_frame_rgba(FrameRenderRequest {
                    paths,
                    config,
                    dense_activity,
                    prepared_assets: &prepared_preview_assets.prepared_assets,
                    frame_index: dense_frame_index,
                    scale,
                    labels_image: None,
                    target: RenderTarget {
                        width,
                        height,
                        pixels: frame_buffer.pixels.as_mut_slice(),
                    },
                    frame_profiler: &mut profiler,
                })?;
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

    // ── PHASE 5: DRAIN WRITER, FINALIZE FFMPEG, JOIN MONITOR ──
    drop(sender);
    let writer_result = writer_thread
        .join()
        .map_err(|_| CoreError::Encode("Composite encoder writer thread panicked".to_string()))?;
    let was_cancelled = cancel_flag.load(Ordering::SeqCst);
    let ffmpeg_finalize_started = Instant::now();
    let status = if was_cancelled {
        terminate_composite_ffmpeg_after_cancel(&mut child)?
    } else {
        child
            .wait()
            .map_err(|error| CoreError::Encode(error.to_string()))?
    };
    let ffmpeg_finalize_wait_ms = ffmpeg_finalize_started.elapsed().as_secs_f64() * 1000.0;
    monitor_thread
        .join()
        .map_err(|_| CoreError::Encode("Composite ffmpeg monitor thread panicked".to_string()))?;

    let writer = match writer_result {
        Ok(w) => w,
        Err(error) => {
            let _ = std::fs::remove_file(&plan.output_path);
            let stderr = stderr_snapshot(&stderr_lines);
            let error_str = error.to_string();
            if is_pipe_write_error(&error_str) {
                return Err(CoreError::Encode(format_pipe_write_failure(
                    error_str, status, &stderr, &plan,
                )));
            }
            if stderr.is_empty() {
                return Err(error);
            }
            return Err(CoreError::Encode(format!(
                "{error}. FFmpeg stderr:\n{}",
                stderr_tail(&stderr)
            )));
        }
    };

    if let Err(error) = render_result {
        let _ = std::fs::remove_file(&plan.output_path);
        let stderr = stderr_snapshot(&stderr_lines);
        let error_str = error.to_string();
        if is_pipe_write_error(&error_str) {
            return Err(CoreError::Encode(format_pipe_write_failure(
                error_str, status, &stderr, &plan,
            )));
        }
        if stderr.is_empty() {
            return Err(error);
        }
        return Err(CoreError::Encode(format!(
            "{error}. FFmpeg stderr:\n{}",
            stderr_tail(&stderr)
        )));
    }
    if was_cancelled {
        let _ = std::fs::remove_file(&plan.output_path);
        return Err(CoreError::Cancelled);
    }
    if !status.success() {
        let _ = std::fs::remove_file(&plan.output_path);
        let stderr = stderr_snapshot(&stderr_lines);
        return Err(CoreError::Ffmpeg {
            status,
            stderr: stderr_tail(&stderr),
        });
    }
    if writer.written_frames != expected_guarded_overlay_frame_count(&plan) {
        let _ = std::fs::remove_file(&plan.output_path);
        return Err(CoreError::Encode(format!(
            "Composite overlay writer ended early: wrote {} of {} frames",
            writer.written_frames,
            expected_guarded_overlay_frame_count(&plan)
        )));
    }
    verify_successful_composite_output(&plan.output_path)?;

    // ── PHASE 6: WRITE DEBUG SUMMARY ──
    let total_ms = render_started.elapsed().as_secs_f64() * 1000.0;
    let merged_timings = merge_timing_maps(profiler.summary(), writer.timings);
    write_composite_timing_summary(CompositeTimingSummaryInput {
        debug_render_dir: &paths.debug_render_dir,
        ffmpeg_settings: &plan.ffmpeg_settings,
        output_path: &plan.output_path,
        source_fps: plan.source_fps,
        overlay_pipe_fps: plan.overlay_pipe_fps,
        widget_update_rate: plan.widget_update_rate,
        render_duration: plan.render_duration,
        overlay_frame_count: writer.written_frames,
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

/// Terminates FFmpeg after a user cancellation request.
///
/// Closing stdin gives FFmpeg a short chance to exit cleanly; if it keeps
/// running, the process is killed and waited so no encoder process is orphaned.
fn terminate_composite_ffmpeg_after_cancel(
    child: &mut std::process::Child,
) -> CoreResult<std::process::ExitStatus> {
    for _ in 0..10 {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| CoreError::Encode(error.to_string()))?
        {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(50));
    }

    child.kill().map_err(|error| {
        CoreError::Encode(format!(
            "Failed to terminate composite ffmpeg after cancellation: {error}"
        ))
    })?;
    child
        .wait()
        .map_err(|error| CoreError::Encode(error.to_string()))
}

/// Derives composite timing and FFmpeg settings.
///
/// This helper mirrors the render loop's timing math, including the
/// fractional-frame overrun guard, without producing any overlay frames.
///
/// # Phases
/// 1. Validate required fields and derive FPS / durations
/// 2. Compute frame counts and overrun guard index
/// 3. Build FFmpeg settings from the composite profile catalog
/// 4. Generate output filename and assemble the plan
#[allow(clippy::too_many_arguments)]
pub fn derive_composite_pipeline_plan(
    // test seam
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
) -> CoreResult<CompositePipelinePlan> {
    // ── PHASE 1: VALIDATE & DERIVE TIMING VALUES ──
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
        return Err(CoreError::Encode(format!(
            "Composite video duration must be greater than zero: {composite_video_duration}"
        )));
    }
    if !trim_start.is_finite() || trim_start < 0.0 {
        return Err(CoreError::Encode(format!(
            "Composite video trim start must be zero or greater: {trim_start}"
        )));
    }
    if trim_start >= composite_video_duration {
        return Err(CoreError::Encode(format!(
            "Composite video trim start ({trim_start}) must be less than video duration ({composite_video_duration})"
        )));
    }
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(CoreError::Encode(format!(
            "Composite render duration must be greater than zero: {render_duration}"
        )));
    }

    // ── PHASE 2: COMPUTE FRAME COUNTS & OVERRUN GUARD ──
    let overlay_frame_count = (render_duration * overlay_pipe_fps.as_f64())
        .ceil()
        .max(0.0) as u64;
    let output_frame_count = (render_duration * output_fps.as_f64()).ceil().max(0.0) as u64;
    let first_overrun_overlay_index =
        first_fractional_overrun_overlay_index(render_duration, overlay_pipe_fps);
    // ── PHASE 3: BUILD COMPOSITE FFMPEG SETTINGS ──
    let mut hwaccel_info = HwAccelInfo::trust_selected_profile();
    hwaccel_info.available_codecs.qsv_full_init_args = composite_qsv_full_init_args(config);
    let ffmpeg_settings = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: &codec_name,
        bitrate: composite_bitrate,
        video_path: Path::new(composite_video_path),
        video_trim_start: trim_start,
        render_duration,
        width,
        height,
        source_fps,
        overlay_pipe_fps,
        hwaccel_available: &hwaccel_info,
    })?;
    // ── PHASE 4: GENERATE OUTPUT FILENAME ──
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
) -> CoreResult<std::process::Child> {
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
        .map_err(|error| CoreError::Encode(format!("Could not start composite ffmpeg: {error}")))
}

/// Maps one overlay timestamp to a dense activity frame index.
///
/// Composite-adjusted dense reports use direct `overlay j -> dense j` mapping;
/// otherwise this falls back to scene-start-relative time mapping.
pub fn dense_frame_index_for_overlay(
    // test seam
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    plan: &CompositePipelinePlan,
    activity_time: f64,
) -> CoreResult<usize> {
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
                return Err(CoreError::Encode(format!(
                    "Composite overlay frame is before dense activity range: activity_time={activity_time}, scene.start={}",
                    config.scene.start
                )));
            }
            idx as usize
        }
    };

    if dense_frame_index >= dense_activity.frame_count {
        return Err(CoreError::Encode(format!(
            "Composite dense frame index {dense_frame_index} is outside dense activity range 0..{}",
            dense_activity.frame_count
        )));
    }
    Ok(dense_frame_index)
}

/// Returns whether the dense report was rebuilt for the composite window.
///
/// This checks the composite timing contract with a small floating-point
/// tolerance so the hot render loop can use direct frame-index mapping
/// when valid.
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
pub fn expected_guarded_overlay_frame_count(plan: &CompositePipelinePlan) -> u64 {
    // test seam
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
pub fn first_fractional_overrun_overlay_index(render_duration: f64, overlay_pipe_fps: Fps) -> u64 {
    // test seam
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
