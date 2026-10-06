//! One native sequence for both output modes, from pipe capture through cleanup.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ChildStderr;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::frame_pool::{
    diagnose_frame_worker_count, ParallelFrameChannels, ParallelFramePoolPlan,
    ParallelFrameProgress,
};
use super::frames::{render_frames_parallel, FrameProductionPlan};
use super::lifecycle::{
    finalize_pipeline, PartialOutputGuard, PipelineFailurePolicy, PipelineKind, PipelineProcesses,
    PipelineShutdown,
};
use super::queue::{merge_timing_maps, writer_worker, FrameBuffer, WriterMode};
use crate::debug::TimingBucket;
use crate::encode::ffmpeg::binary::{resolve_ffmpeg_binary, spawn_ffmpeg};
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::{FrameSize, VideoFrameRenderer};

pub(crate) type OrderedFrameObserver<'a> = dyn Fn(u64, usize, &FrameBuffer) -> CoreResult<()> + 'a;

/// Immutable process/production settings; mode-specific codec construction stays outside.
pub(crate) struct FramePipelinePlan {
    pub kind: PipelineKind,
    pub frames: FrameProductionPlan,
    pub frame_size: FrameSize,
    pub output_frame_count: u32,
    pub cpu_cores_per_frame_worker: usize,
    pub ffmpeg_args: Vec<String>,
}

/// Mode-specific stderr interpretation, error messages and output verification.
pub(crate) trait PipelineDiagnostics: PipelineFailurePolicy {
    fn start_monitor(&self, stderr: ChildStderr) -> JoinHandle<()>;
    fn progress(&self) -> ParallelFrameProgress<'_>;
    fn verify_output(&self, path: &Path) -> CoreResult<()>;
}

pub(crate) struct FramePipelineOutcome {
    pub rendered_frames: u32,
    pub timings: BTreeMap<String, TimingBucket>,
    pub total_seconds: f64,
    pub render_loop_ms: f64,
    pub finalize_ms: f64,
    pub workers: usize,
}

pub(crate) fn run_frame_pipeline<D: PipelineDiagnostics>(
    paths: &AppPaths,
    renderer: VideoFrameRenderer<'_>,
    plan: FramePipelinePlan,
    controller: &RenderController,
    target: &RenderOutputTarget,
    diagnostics: &D,
    observer: Option<&OrderedFrameObserver<'_>>,
) -> CoreResult<FramePipelineOutcome> {
    controller.check_cancelled()?;
    let workers = diagnose_frame_worker_count(
        plan.frames.count() as usize,
        plan.cpu_cores_per_frame_worker,
    )?;
    let ParallelFrameChannels {
        frame_sender,
        frame_receiver,
        free_sender,
        free_receiver,
    } = ParallelFramePoolPlan::for_frame_size(plan.frame_size, workers)?.create_channels()?;
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let shutdown = PipelineShutdown::shared(controller.cancel_flag());
    controller.set_frame_progress(0, plan.output_frame_count, 0, 0, None, None);
    controller.start_encoding()?;
    // Declared before processes so unwinding terminates/reaps/joins before deletion.
    let mut output_guard = PartialOutputGuard::new(target.path());
    let mut processes = PipelineProcesses::new(
        spawn_ffmpeg(&ffmpeg_bin, &plan.ffmpeg_args)?,
        plan.kind,
        Arc::clone(&shutdown),
    );
    let stdin = processes.child.stdin.take().ok_or_else(|| {
        CoreError::Encode(format!("Failed to capture {} ffmpeg stdin", plan.kind))
    })?;
    let stderr = processes.child.stderr.take().ok_or_else(|| {
        CoreError::Encode(format!("Failed to capture {} ffmpeg stderr", plan.kind))
    })?;
    processes.monitor = Some(diagnostics.start_monitor(stderr));
    let writer_mode = match plan.kind {
        PipelineKind::Transparent => WriterMode::Transparent,
        PipelineKind::Composite => WriterMode::Composite,
    };
    let writer_shutdown = Arc::clone(&shutdown);
    processes.writer = Some(thread::spawn(move || {
        writer_worker(
            stdin,
            frame_receiver,
            free_sender,
            writer_shutdown,
            writer_mode,
        )
    }));
    let started = Instant::now();
    let producer = render_frames_parallel(
        renderer,
        plan.frames,
        workers,
        diagnostics.progress(),
        plan.kind,
        controller,
        &shutdown,
        &frame_sender,
        observer,
        free_receiver,
        &mut processes.child,
        started,
    );
    let render_loop_ms = started.elapsed().as_secs_f64() * 1000.0;
    drop(frame_sender);
    let finalize_started = Instant::now();
    let outcome = finalize_pipeline(&mut processes, producer, &shutdown, plan.kind, diagnostics)?;
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    drop(outcome.producer.free_receiver);
    if outcome.writer.written_frames != u64::from(plan.frames.count()) {
        return Err(CoreError::Encode(format!(
            "{} encoder writer ended early: wrote {} of {} frames",
            plan.kind,
            outcome.writer.written_frames,
            plan.frames.count()
        )));
    }
    diagnostics.verify_output(target.path())?;
    output_guard.preserve();
    controller.set_frame_progress(
        plan.output_frame_count,
        plan.output_frame_count,
        outcome.producer.rendered_frames,
        plan.output_frame_count,
        Some(0),
        None,
    );
    Ok(FramePipelineOutcome {
        rendered_frames: outcome.producer.rendered_frames,
        timings: merge_timing_maps(outcome.producer.timings, outcome.writer.timings),
        total_seconds: started.elapsed().as_secs_f64(),
        render_loop_ms,
        finalize_ms,
        workers: workers.get(),
    })
}
