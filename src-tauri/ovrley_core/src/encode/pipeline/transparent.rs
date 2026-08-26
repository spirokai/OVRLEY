//! Transparent overlay frame-worker pipeline.
//!
//! Renders Skia frames and streams them to ffmpeg via stdin.
//! Produces alpha-preserving overlay video (ProRes, QTRLE, or Vulkan).
//!
//! Must not import from [`composite`].
//!
//! The pipeline prepares reusable Skia assets, renders frames into a bounded
//! pool of RGBA buffers, and streams those buffers to ffmpeg through stdin. A
//! separate monitor thread parses ffmpeg stderr for encoded-frame progress,
//! while the writer thread keeps expensive IO off the render loop.
//!
//! ## FFmpeg Process Lifecycle
//!
//! 1. **Spawn**: the shared FFmpeg process owner creates the child with piped stdin
//!    (raw RGBA video) and piped stderr (progress). The child inherits no stdin.
//! 2. **Stdin**: The writer thread takes `child.stdin.take()`, writes frames in a
//!    loop, then drops the handle (EOF) so ffmpeg finalizes output.
//! 3. **Stderr**: The monitor thread takes `child.stderr.take()`, parses
//!    `frame=N` lines, and updates a shared `Arc<AtomicU32>` counter.
//! 4. **Wait**: Writer drain and FFmpeg finalization use bounded polling.
//! 5. **Cancel**: On cancellation, FFmpeg is terminated before the writer is
//!    joined so a blocked pipe write cannot stall teardown.
//! 6. **Error**: If ffmpeg exits non-zero or the writer panics, the partial
//!    output file is removed and `CoreError::Ffmpeg` or `CoreError::Encode` is
//!    returned. A frame-count mismatch after success is also treated as a failure.

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::encode::debug::video::{
    create_debug_dir, render_sample_frames_enabled, sample_frame_indices, write_prepare_summary,
    write_sample_frame, write_timing_summary,
};
use crate::encode::ffmpeg::binary::{resolve_ffmpeg_binary, spawn_ffmpeg};
use crate::encode::ffmpeg::settings::build_ffmpeg_settings;
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use crate::encode::pipeline::frame_pool::{
    diagnose_frame_worker_count, ParallelFrameChannels, ParallelFramePoolPlan,
    ParallelFrameProgress,
};
use crate::encode::pipeline::frames::render_frames_parallel;
use crate::encode::pipeline::lifecycle::{
    finalize_pipeline, FfmpegChildGuard, PartialOutputGuard, PipelineFailurePolicy, PipelineKind,
    PipelineShutdown,
};
use crate::encode::pipeline::queue::{merge_timing_maps, writer_worker, FrameBuffer, WriterMode};
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::{prepare_preview_assets, FrameSize, VideoFrameRenderer};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

struct TransparentFailurePolicy;

impl PipelineFailurePolicy for TransparentFailurePolicy {
    fn writer_failure(
        &self,
        error: CoreError,
        _status: Option<std::process::ExitStatus>,
    ) -> CoreError {
        error
    }

    fn ffmpeg_failure(&self, status: std::process::ExitStatus) -> CoreError {
        CoreError::Encode(format!("ffmpeg encoding failed ({status})"))
    }
}

/// Renders one transparent-overlay video by streaming Skia frames to ffmpeg.
///
/// The pipeline diagnoses a profile-specific worker count, prepares reusable
/// Skia assets, and sends ordered worker output through one FFmpeg process.
///
/// # Arguments
///
/// * `paths` — Central path configuration (fonts, templates, debug/output dirs).
/// * `config` — Validated render configuration with scene/widget/ffmpeg settings.
/// * `activity` — Parsed (but untrimmed) source activity for asset preparation.
/// * `dense_activity` — Frame-aligned dense report used for per-frame telemetry.
/// * `controller` — Shared render state; cloned to observe progress/cancellation.
///
/// # Returns
///
/// On success, returns the output filename (relative to the downloads directory).
/// Debug timing summaries are written to `paths.debug_render_dir/phase_6/`.
///
/// # Errors
///
/// Returns [`CoreError::Cancelled`] when the user cancels (output is cleaned up).
/// Returns [`CoreError::Ffmpeg`] if ffmpeg exits non-zero.
/// Returns [`CoreError::Encode`] on thread panic, pipe failure, or frame-count mismatch.
/// Returns [`CoreError::Render`] if any frame fails to render.
/// Returns [`CoreError::Io`] on filesystem errors.
///
/// # Thread Safety
///
/// Spawns two threads whose handles are stored and joined before returning:
/// a monitor thread (ffmpeg stderr → AtomicU32 counter) and a writer thread
/// (bounded channel → ffmpeg stdin, with buffer return to free pool). The
/// render loop runs on the calling thread.
///
/// # Cancellation
///
/// Checks `controller.cancel_flag` between every frame and at buffer-acquire
/// time. On cancellation, FFmpeg is terminated before threads are joined, the
/// partial output is removed, and `CoreError::Cancelled` is returned.
///
/// # Performance
///
/// This is a render hot path. Frame rendering and ffmpeg stdin writing overlap
/// via a bounded channel and a pooled buffer ring. Parallel rendering sizes the
/// pool from resolution and a fixed memory ceiling. Avoid per-frame allocations
/// inside the loop — buffers are reused.
pub fn render_video(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    output_target: &RenderOutputTarget,
) -> CoreResult<String> {
    // ── PHASE 1: SETUP — derive dimensions, frame counts, paths, and ffmpeg args ──
    let scene = &config.scene;
    let ffmpeg_settings = build_ffmpeg_settings(&scene.ffmpeg)?;
    if !scene.width.is_multiple_of(2) || !scene.height.is_multiple_of(2) {
        return Err(CoreError::Config(format!(
            "Transparent video dimensions must be even; received {}x{}",
            scene.width, scene.height
        )));
    }
    let frame_size = FrameSize {
        width: scene.width,
        height: scene.height,
    };
    let layout_total_frames = u32::try_from(dense_activity.frame_count)
        .map_err(|_| CoreError::Encode("Transparent layout frame count exceeds u32".to_string()))?;
    let update_rate = scene.update_rate;
    // `rendered_frame_count` applies frame decimation: when update_rate > 1,
    // we render fewer frames than the dense report has, skipping layout frames
    // that would not change the visible overlay at the configured rate.
    let total_frames = u32::try_from(rendered_frame_count(
        dense_activity.frame_count,
        update_rate,
    )?)
    .map_err(|_| CoreError::Encode("Transparent output frame count exceeds u32".to_string()))?;
    let container_fps = scene.fps / f64::from(scene.update_rate.get());
    let workers = diagnose_frame_worker_count(
        total_frames as usize,
        transparent_profile(ffmpeg_settings.codec_id).cpu_cores_per_frame_worker,
    )?;
    let channels = ParallelFramePoolPlan::for_frame_size(frame_size, workers)?.create_channels()?;
    let debug_dir = create_debug_dir(paths)?;
    // ── PHASE 2: BUILD SKIA ASSETS — pre-render maps, fonts, and label cache ──
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    let renderer = VideoFrameRenderer::new(
        paths,
        dense_activity,
        &prepared_preview_assets,
        frame_size,
        0,
    )?;
    write_prepare_summary(
        &debug_dir,
        prepare_total_ms,
        prepare_timings,
        label_cache_status,
    )?;

    let output_path = output_target.path();
    let mut output_guard = PartialOutputGuard::new(&output_path);
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let input_pix_fmt = ffmpeg_input_pix_fmt()?;
    let encoded_frames = Arc::new(AtomicU32::new(0));
    let shutdown = PipelineShutdown::shared(controller.cancel_flag());
    let render_started = Instant::now();

    // ── PHASE 3: CREATE BUFFER POOL (N+1 buffers for N-slot bounded channel) ──
    let ParallelFrameChannels {
        frame_sender,
        frame_receiver,
        free_sender,
        free_receiver,
    } = channels;
    // ── PHASE 4: SPAWN FFMPEG & WORKER THREADS (writer + monitor) ──
    // ffmpeg is spawned before the render loop starts. The writer owns stdin
    // and drains the bounded frame queue; the monitor parses stderr for progress.
    let mut child = FfmpegChildGuard::new(
        spawn_ffmpeg(
            &ffmpeg_bin,
            &ffmpeg_settings.command_args(&output_path, frame_size, container_fps, &input_pix_fmt),
        )?,
        PipelineKind::Transparent,
    );

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CoreError::Encode("Failed to capture ffmpeg stderr".to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Encode("Failed to capture ffmpeg stdin".to_string()))?;
    let encoded_frames_for_monitor = encoded_frames.clone();
    let monitor_thread = thread::spawn(move || monitor_ffmpeg(stderr, encoded_frames_for_monitor));
    let shutdown_for_writer = Arc::clone(&shutdown);
    let writer_thread = thread::spawn(move || {
        writer_worker(
            stdin,
            frame_receiver,
            free_sender,
            shutdown_for_writer,
            WriterMode::Transparent,
        )
    });

    let sample_frames = if render_sample_frames_enabled()? {
        sample_frame_indices(total_frames as usize)
    } else {
        Vec::new()
    };
    let sample_output_frame_indices = sample_frames
        .iter()
        .copied()
        .map(|index| {
            u64::try_from(index).map_err(|_| {
                CoreError::Encode("Sample frame index exceeds u64 capacity".to_string())
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    // ── PHASE 5: HOT RENDER LOOP ──
    // ffmpeg is running, the writer is draining the channel, the monitor is
    // parsing stderr. We own the render thread and produce exactly total_frames.
    // The bounded channel provides backpressure: if the writer falls behind,
    // the next queue_frame call blocks, capping memory usage.
    let observe_ordered_frame =
        |output_frame_index: u64, dense_frame_index: usize, buffer: &FrameBuffer| {
            if sample_output_frame_indices
                .binary_search(&output_frame_index)
                .is_ok()
            {
                write_sample_frame(
                    &ffmpeg_bin,
                    &debug_dir,
                    frame_size,
                    buffer.pixels.as_slice(),
                    dense_frame_index,
                    &input_pix_fmt,
                )?;
            }
            Ok(())
        };
    let render_result = render_frames_parallel(
        renderer,
        total_frames as usize,
        update_rate,
        workers,
        ParallelFrameProgress::Transparent(&encoded_frames),
        PipelineKind::Transparent,
        controller,
        shutdown.as_ref(),
        &frame_sender,
        Some(&observe_ordered_frame),
        free_receiver,
        &mut child,
        render_started,
    );
    drop(frame_sender);
    // ── PHASE 6: THREAD JOIN & FFMPEG WAIT ──
    // Dropping the sender signals the writer to exit its recv() loop.
    // The writer flushes stdin and returns, which causes ffmpeg to see EOF
    // and finalize the output file. We join threads before waiting on ffmpeg
    // so pipe-write errors are collected before we check the exit status.
    let outcome = finalize_pipeline(
        &mut child,
        writer_thread,
        monitor_thread,
        render_result,
        shutdown.as_ref(),
        PipelineKind::Transparent,
        &TransparentFailurePolicy,
    )?;

    let rendered_frames = outcome.producer.rendered_frames;
    let producer_timings = outcome.producer.timings;
    let writer_result = outcome.writer;
    if writer_result.written_frames != u64::from(total_frames) {
        // Frame-count mismatch means ffmpeg accepted stdin but produced fewer
        // frames than expected — typically a pipe-write error partway through
        // that ffmpeg didn't report via exit status. Clean up and fail.
        return Err(CoreError::Encode(format!(
            "ffmpeg encode pipeline ended early: wrote {} of {} frames",
            writer_result.written_frames, total_frames
        )));
    }
    output_guard.preserve();

    let total_time_taken = render_started.elapsed().as_secs_f64();

    // ── PHASE 7: FINALIZATION — write debug summary, return public filename ──
    let merged_timings = merge_timing_maps(producer_timings, writer_result.timings);
    write_timing_summary(
        &debug_dir,
        prepared_preview_assets.scene(),
        &output_path,
        total_frames,
        layout_total_frames,
        rendered_frames,
        total_time_taken,
        sample_frames,
        merged_timings,
    )?;
    Ok(output_target.filename().to_owned())
}

/// Computes how many frames will be written after applying frame decimation.
pub fn rendered_frame_count(
    layout_frame_count: usize,
    update_rate: std::num::NonZeroU32,
) -> CoreResult<usize> {
    // Decimation keeps the first frame and then every `update_rate`th layout
    // frame. The +1 form avoids off-by-one loss for non-divisible lengths.
    if layout_frame_count == 0 {
        return Err(CoreError::Encode(
            "Layout frame count must be greater than zero".to_string(),
        ));
    }
    Ok(((layout_frame_count - 1) / update_rate.get() as usize) + 1)
}

// Monitors ffmpeg stderr and updates the encoded-frame counter.
fn monitor_ffmpeg(stderr: std::process::ChildStderr, encoded_frames: Arc<AtomicU32>) {
    // ffmpeg progress is emitted on stderr as human-readable status lines. We
    // parse frame counts opportunistically and ignore unrelated log messages.
    let reader = BufReader::new(stderr);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(frame_index) = parse_ffmpeg_frame(&line) {
            encoded_frames.store(frame_index, Ordering::SeqCst);
        }
    }
}

// Extracts a frame count from one ffmpeg status line.
fn parse_ffmpeg_frame(line: &str) -> Option<u32> {
    // Accept ffmpeg's padded `frame=  123` status format.
    let marker = "frame=";
    let start = line.find(marker)? + marker.len();
    let digits = line[start..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u32>().ok()
}

// Resolves the raw pixel format used for ffmpeg stdin.
fn ffmpeg_input_pix_fmt() -> CoreResult<String> {
    // Exposed for diagnosing platform-specific pixel-format issues without
    // recompiling the backend.
    match std::env::var("OVRLEY_INPUT_PIX_FMT") {
        Ok(value) if value.trim().is_empty() => Err(CoreError::Encode(
            "OVRLEY_INPUT_PIX_FMT must not be empty".to_string(),
        )),
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok("rgba".to_string()),
        Err(std::env::VarError::NotUnicode(_)) => Err(CoreError::Encode(
            "OVRLEY_INPUT_PIX_FMT must contain Unicode text".to_string(),
        )),
    }
}
