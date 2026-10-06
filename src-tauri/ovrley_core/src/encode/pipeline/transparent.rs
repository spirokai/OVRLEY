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
use crate::encode::ffmpeg::binary::resolve_ffmpeg_binary;
use crate::encode::ffmpeg::settings::FfmpegSettings;
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use crate::encode::pipeline::frame_pool::ParallelFrameProgress;
use crate::encode::pipeline::frames::FrameProductionPlan;
use crate::encode::pipeline::lifecycle::{PipelineFailurePolicy, PipelineKind};
use crate::encode::pipeline::queue::FrameBuffer;
use crate::encode::pipeline::run::{run_frame_pipeline, FramePipelinePlan, PipelineDiagnostics};
use crate::encode::progress::RenderController;
use crate::encode::video_timing::ActivityCoverage;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::{prepare_preview_assets, FrameSize, VideoFrameRenderer};
use std::io::{BufRead, BufReader};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

/// Fixed input to the transparent pipeline; timing is supplied by the job owner.
#[derive(Clone, Debug)]
pub struct TransparentRenderPlan {
    pub(crate) frames: FrameProductionPlan,
    pub(crate) ffmpeg: FfmpegSettings,
    pub layout_frame_count: u32,
    pub output_frame_count: u32,
    pub update_rate: NonZeroU32,
    pub container_fps: String,
    pub coverage: ActivityCoverage,
}

struct TransparentDiagnostics {
    encoded_frames: Arc<AtomicU32>,
}

impl PipelineFailurePolicy for TransparentDiagnostics {
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

impl PipelineDiagnostics for TransparentDiagnostics {
    fn start_monitor(&self, stderr: std::process::ChildStderr) -> thread::JoinHandle<()> {
        let encoded_frames = Arc::clone(&self.encoded_frames);
        thread::spawn(move || monitor_ffmpeg(stderr, encoded_frames))
    }
    fn progress(&self) -> ParallelFrameProgress<'_> {
        ParallelFrameProgress::Transparent(&self.encoded_frames)
    }
    fn verify_output(&self, _path: &std::path::Path) -> CoreResult<()> {
        Ok(())
    }
}

/// Executes a fixed video-local plan through the same transparent pipeline.
pub(crate) fn render_transparent_video(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    output_target: &RenderOutputTarget,
    plan: &TransparentRenderPlan,
) -> CoreResult<String> {
    controller.check_cancelled()?;
    // ── PHASE 1: SETUP — derive dimensions, frame counts, paths, and ffmpeg args ──
    let scene = &config.scene;
    let ffmpeg_settings = &plan.ffmpeg;
    let frame_size = FrameSize {
        width: scene.presentation.width,
        height: scene.presentation.height,
    };
    let layout_total_frames = plan.layout_frame_count;
    let total_frames = plan.output_frame_count;
    let debug_dir = create_debug_dir(paths)?;
    controller.check_cancelled()?;
    // ── PHASE 2: BUILD SKIA ASSETS — pre-render maps, fonts, and label cache ──
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    controller.check_cancelled()?;
    let renderer = VideoFrameRenderer::new(
        paths,
        dense_activity,
        &prepared_preview_assets,
        frame_size,
        plan.coverage.blank_leading_frame_count,
    )?;
    write_prepare_summary(
        &debug_dir,
        prepare_total_ms,
        prepare_timings,
        label_cache_status,
    )?;

    let output_path = output_target.path();
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let input_pix_fmt = ffmpeg_input_pix_fmt()?;
    let sample_frames = if render_sample_frames_enabled()? {
        sample_frame_indices(total_frames as usize)
    } else {
        Vec::new()
    };
    let observe_ordered_frame =
        |output_frame_index: u64, dense_frame_index: usize, buffer: &FrameBuffer| {
            if sample_frames
                .binary_search(&(output_frame_index as usize))
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
    let outcome = run_frame_pipeline(
        paths,
        renderer,
        FramePipelinePlan {
            kind: PipelineKind::Transparent,
            frames: plan.frames,
            frame_size,
            output_frame_count: total_frames,
            cpu_cores_per_frame_worker: transparent_profile(ffmpeg_settings.codec_id)
                .cpu_cores_per_frame_worker,
            ffmpeg_args: ffmpeg_settings.command_args(
                &output_path,
                frame_size,
                &plan.container_fps,
                &input_pix_fmt,
            ),
        },
        controller,
        output_target,
        &TransparentDiagnostics {
            encoded_frames: Arc::new(AtomicU32::new(0)),
        },
        Some(&observe_ordered_frame),
    )?;
    write_timing_summary(
        &debug_dir,
        prepared_preview_assets.scene(),
        &output_path,
        total_frames,
        layout_total_frames,
        outcome.rendered_frames,
        outcome.total_seconds,
        sample_frames,
        outcome.timings,
    )?;
    Ok(output_target.filename().to_owned())
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
