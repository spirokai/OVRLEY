//! Multi-pass composite MP4 render pipeline.
//!
//! Renders Skia frames, composites them with source video,
//! and produces final H.264/H.265 MP4 output.
//!
//! Must not import from [`transparent`].
//!
//! The composite path renders transparent Skia overlay frames at the derived
//! overlay FPS and streams them to FFmpeg, which composites them over input
//! video frames and writes the final MP4 output.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::ChildStderr;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::encode::composite::CompositeRenderPlan;
use crate::encode::debug::composite::write_composite_timing_summary;
use crate::encode::ffmpeg::composite_profiles::composite_profile;
use crate::encode::pipeline::composite_plan::{
    derive_composite_pipeline_plan, CompositePipelinePlan,
};
use crate::encode::pipeline::composite_support::{
    format_pipe_write_failure, is_pipe_write_error, stderr_tail, verify_successful_composite_output,
};
use crate::encode::pipeline::frame_pool::ParallelFrameProgress;
use crate::encode::pipeline::lifecycle::{PipelineFailurePolicy, PipelineKind};
use crate::encode::pipeline::run::{run_frame_pipeline, FramePipelinePlan, PipelineDiagnostics};
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::{prepare_preview_assets, VideoFrameRenderer};

const FFMPEG_STDERR_LINE_LIMIT: usize = 200;

// ── Failure policy ────────────────────────────────────────────────────────

struct CompositeFailurePolicy<'a> {
    stderr_lines: &'a Arc<Mutex<VecDeque<String>>>,
    plan: &'a CompositePipelinePlan,
}

impl PipelineFailurePolicy for CompositeFailurePolicy<'_> {
    fn writer_failure(
        &self,
        error: CoreError,
        status: Option<std::process::ExitStatus>,
    ) -> CoreError {
        let stderr = stderr_snapshot(self.stderr_lines);
        let error_text = error.to_string();
        if let Some(status) = status {
            if is_pipe_write_error(&error_text) {
                return CoreError::Encode(format_pipe_write_failure(
                    error_text, status, &stderr, self.plan,
                ));
            }
        }
        if stderr.is_empty() {
            error
        } else {
            CoreError::Encode(format!("{error}. FFmpeg stderr:\n{}", stderr_tail(&stderr)))
        }
    }

    fn ffmpeg_failure(&self, status: std::process::ExitStatus) -> CoreError {
        CoreError::Ffmpeg {
            status,
            stderr: stderr_tail(&stderr_snapshot(self.stderr_lines)),
        }
    }
}

impl PipelineDiagnostics for CompositeFailurePolicy<'_> {
    fn start_monitor(&self, stderr: ChildStderr) -> thread::JoinHandle<()> {
        let lines = Arc::clone(self.stderr_lines);
        thread::spawn(move || monitor_composite_ffmpeg(stderr, lines))
    }
    fn progress(&self) -> ParallelFrameProgress<'_> {
        ParallelFrameProgress::Composite(self.plan)
    }
    fn verify_output(&self, path: &Path) -> CoreResult<()> {
        verify_successful_composite_output(path)
    }
}

/// Executes a fixed plan with source geometry and rotation owned by preparation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_composite_video(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    render_plan: CompositeRenderPlan,
    include_audio: bool,
    source_rotation_degrees: Option<i32>,
    output_target: &RenderOutputTarget,
) -> CoreResult<String> {
    controller.check_cancelled()?;
    let scene = &config.scene;
    let plan = derive_composite_pipeline_plan(
        paths,
        scene,
        render_plan,
        include_audio,
        source_rotation_degrees,
        output_target,
    )?;
    // ── PHASE 2: PREPARE SKIA ASSETS ──
    let (prepared_preview_assets, _, _, _) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    controller.check_cancelled()?;
    let renderer = VideoFrameRenderer::new(
        paths,
        dense_activity,
        &prepared_preview_assets,
        plan.frame_size,
        plan.render.coverage.blank_leading_frame_count,
    )?;
    let stderr_lines = Arc::new(Mutex::new(VecDeque::with_capacity(
        FFMPEG_STDERR_LINE_LIMIT,
    )));
    let outcome = run_frame_pipeline(
        paths,
        renderer,
        FramePipelinePlan {
            kind: PipelineKind::Composite,
            frames: plan.render.frames,
            frame_size: plan.frame_size,
            output_frame_count: plan.render.output_frame_count,
            cpu_cores_per_frame_worker: composite_profile(plan.ffmpeg_settings.codec_id)
                .cpu_cores_per_frame_worker,
            ffmpeg_args: plan.ffmpeg_settings.command_args(&plan.output_path),
        },
        controller,
        output_target,
        &CompositeFailurePolicy {
            stderr_lines: &stderr_lines,
            plan: &plan,
        },
        None,
    )?;
    write_composite_timing_summary(
        paths,
        &plan,
        outcome.total_seconds * 1000.0,
        outcome.render_loop_ms,
        outcome.finalize_ms,
        outcome.timings,
        outcome.workers,
        outcome.rendered_frames,
    )?;
    Ok(output_target.filename().to_owned())
}

// ── FFmpeg stderr helpers ─────────────────────────────────────────────────

/// Reads FFmpeg stderr without blocking the encoder process.
fn monitor_composite_ffmpeg(stderr: ChildStderr, lines: Arc<Mutex<VecDeque<String>>>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines().map_while(Result::ok) {
        let mut locked = lines.lock().expect("FFmpeg stderr mutex poisoned");
        if locked.len() == FFMPEG_STDERR_LINE_LIMIT {
            locked.pop_front();
        }
        locked.push_back(line);
    }
}

/// Returns a snapshot of collected FFmpeg stderr lines.
fn stderr_snapshot(lines: &Arc<Mutex<VecDeque<String>>>) -> String {
    lines
        .lock()
        .expect("FFmpeg stderr mutex poisoned")
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}
