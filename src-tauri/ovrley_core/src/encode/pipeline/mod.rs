//! Shared video preparation and encoding, with one supervised native pipeline.

mod buffers;
#[doc(hidden)]
pub mod diagnostics;
mod frames;
mod process;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Instant;

use self::buffers::{
    diagnose_frame_worker_count, merge_timing_maps, FrameBuffer, ParallelFrameChannels,
    ParallelFramePoolPlan,
};
use self::diagnostics::{
    format_pipe_write_failure, is_pipe_write_error, stderr_tail, EncoderMonitor,
};
use self::frames::{render_frames_parallel, ParallelFrameProgress};
use self::process::{PipelineKind, PipelineProcesses, PipelineShutdown};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::debug::TimingBucket;
use crate::encode::debug::composite::write_composite_timing_summary;
use crate::encode::debug::video::{
    create_debug_dir, render_sample_frames_enabled, sample_frame_indices, write_prepare_summary,
    write_sample_frame, write_timing_summary,
};
use crate::encode::ffmpeg::binary::resolve_ffmpeg_binary;
use crate::encode::ffmpeg::composite::CompositeEncoding;
use crate::encode::ffmpeg::composite_profiles::composite_profile;
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use crate::encode::plan::{FrameProductionPlan, TransparentRenderPlan};
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::{prepare_preview_assets, FrameSize, VideoFrameRenderer};

/// Process-ready settings. Source probing and activity timing belong to jobs.
pub(crate) enum VideoEncoding {
    Composite(CompositeEncoding),
    Transparent(TransparentRenderPlan),
}

impl VideoEncoding {
    fn frame_plan(&self, size: FrameSize, output: &Path, input_format: &str) -> FramePipelinePlan {
        match self {
            Self::Composite(plan) => FramePipelinePlan {
                kind: PipelineKind::Composite,
                frames: plan.render.frames,
                frame_size: size,
                output_frame_count: plan.render.output_frame_count,
                cpu_cores_per_frame_worker: composite_profile(plan.ffmpeg_settings.codec_id)
                    .cpu_cores_per_frame_worker,
                ffmpeg_args: plan.ffmpeg_settings.command_args(output),
            },
            Self::Transparent(plan) => FramePipelinePlan {
                kind: PipelineKind::Transparent,
                frames: plan.frames,
                frame_size: size,
                output_frame_count: plan.output_frame_count,
                cpu_cores_per_frame_worker: transparent_profile(plan.ffmpeg.codec_id)
                    .cpu_cores_per_frame_worker,
                ffmpeg_args: plan.ffmpeg.command_args(
                    output,
                    size,
                    &plan.container_fps,
                    input_format,
                ),
            },
        }
    }

    pub(crate) fn progress<'a>(&'a self, monitor: &'a EncoderMonitor) -> ParallelFrameProgress<'a> {
        match self {
            Self::Composite(plan) => ParallelFrameProgress::Composite(&plan.render),
            Self::Transparent(_) => ParallelFrameProgress::Transparent(monitor),
        }
    }

    pub(crate) fn writer_failure(
        &self,
        error: CoreError,
        status: ExitStatus,
        stderr: &str,
    ) -> CoreError {
        if let Self::Composite(plan) = self {
            let message = error.to_string();
            if is_pipe_write_error(&message) {
                return CoreError::Encode(format_pipe_write_failure(message, status, stderr, plan));
            }
        }
        if stderr.is_empty() {
            error
        } else {
            CoreError::Encode(format!("{error}. FFmpeg stderr:\n{}", stderr_tail(stderr)))
        }
    }
}

pub(crate) fn encode_video(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    controller: &RenderController,
    target: &RenderOutputTarget,
    encoding: VideoEncoding,
) -> CoreResult<String> {
    controller.check_cancelled()?;
    let size = FrameSize {
        width: config.scene.presentation.width,
        height: config.scene.presentation.height,
    };
    let debug_dir = match &encoding {
        VideoEncoding::Transparent(_) => Some(create_debug_dir(paths)?),
        VideoEncoding::Composite(_) => None,
    };
    let input_format = match &encoding {
        VideoEncoding::Transparent(_) => input_pixel_format()?,
        VideoEncoding::Composite(_) => "rgba".into(),
    };
    let frame_plan = encoding.frame_plan(size, target.path(), &input_format);
    let samples = if debug_dir.is_some() && render_sample_frames_enabled()? {
        sample_frame_indices(frame_plan.output_frame_count as usize)
    } else {
        Vec::new()
    };
    let binary = resolve_ffmpeg_binary(&paths.repo_root)?;
    let (assets, labels, prepare_timings, prepare_ms) =
        prepare_preview_assets(paths, config, activity, dense)?;
    controller.check_cancelled()?;
    if let Some(directory) = &debug_dir {
        write_prepare_summary(directory, prepare_ms, prepare_timings, labels)?;
    }
    let coverage = match &encoding {
        VideoEncoding::Composite(plan) => &plan.render.coverage,
        VideoEncoding::Transparent(plan) => &plan.coverage,
    };
    let renderer = VideoFrameRenderer::new(
        paths,
        dense,
        &assets,
        size,
        coverage.blank_leading_frame_count,
    )?;
    let observe = |index: u64, dense_index: usize, buffer: &FrameBuffer| {
        if samples.binary_search(&(index as usize)).is_ok() {
            write_sample_frame(
                debug_dir
                    .as_ref()
                    .expect("samples require a debug directory"),
                size,
                &buffer.pixels,
                dense_index,
            )?;
        }
        Ok(())
    };
    let outcome = run_frame_pipeline(
        &binary,
        renderer,
        frame_plan,
        controller,
        target,
        &encoding,
        Arc::new(EncoderMonitor::default()),
        Some(&observe),
    )?;
    // Encoding has committed the output. Diagnostic IO must not turn that
    // completed file into a failed job with a retained, unreported output.
    let diagnostics = match &encoding {
        VideoEncoding::Composite(plan) => write_composite_timing_summary(
            paths,
            plan,
            outcome.total_seconds * 1000.0,
            outcome.render_loop_ms,
            outcome.finalize_ms,
            outcome.timings,
            outcome.workers,
            outcome.rendered_frames,
        )
        .map(|_| ()),
        VideoEncoding::Transparent(plan) => write_timing_summary(
            debug_dir
                .as_ref()
                .expect("transparent diagnostics directory"),
            assets.scene(),
            target.path(),
            plan.output_frame_count,
            plan.layout_frame_count,
            outcome.rendered_frames,
            outcome.total_seconds,
            samples,
            outcome.timings,
        ),
    };
    if let Err(error) = diagnostics {
        log::warn!("Could not write completed render diagnostics: {error}");
    }
    Ok(target.filename().to_owned())
}

fn input_pixel_format() -> CoreResult<String> {
    match std::env::var("OVRLEY_INPUT_PIX_FMT") {
        Ok(value) if value.trim().is_empty() => Err(CoreError::Encode(
            "OVRLEY_INPUT_PIX_FMT must not be empty".into(),
        )),
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok("rgba".into()),
        Err(std::env::VarError::NotUnicode(_)) => Err(CoreError::Encode(
            "OVRLEY_INPUT_PIX_FMT must contain Unicode text".into(),
        )),
    }
}

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

pub(crate) struct FramePipelineOutcome {
    pub rendered_frames: u32,
    pub timings: BTreeMap<String, TimingBucket>,
    pub total_seconds: f64,
    pub render_loop_ms: f64,
    pub finalize_ms: f64,
    pub workers: usize,
}

pub(crate) fn run_frame_pipeline(
    binary: &Path,
    renderer: VideoFrameRenderer<'_>,
    plan: FramePipelinePlan,
    controller: &RenderController,
    target: &RenderOutputTarget,
    encoding: &VideoEncoding,
    monitor: Arc<EncoderMonitor>,
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
        buffers,
    } = ParallelFramePoolPlan::for_frame_size(plan.frame_size, workers)?.create_channels();
    let shutdown = PipelineShutdown::shared(controller.cancel_flag());
    controller.set_frame_progress(0, plan.output_frame_count, 0, 0, None, None);
    controller.start_encoding()?;
    let mut processes = PipelineProcesses::start(
        binary,
        &plan,
        target.path(),
        frame_receiver,
        Arc::clone(&buffers),
        Arc::clone(&shutdown),
        Arc::clone(&monitor),
    )?;
    let started = Instant::now();
    let producer = render_frames_parallel(
        renderer,
        plan.frames,
        workers,
        encoding.progress(&monitor),
        plan.kind,
        controller,
        &shutdown,
        &frame_sender,
        observer,
        &buffers,
        &mut processes.child,
        started,
    );
    let render_loop_ms = started.elapsed().as_secs_f64() * 1000.0;
    drop(frame_sender);
    let finalize_started = Instant::now();
    let outcome = processes.finish(producer, encoding, &monitor)?;
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    if outcome.writer.written_frames != u64::from(plan.frames.count()) {
        return Err(CoreError::Encode(format!(
            "{} encoder writer ended early: wrote {} of {} frames",
            plan.kind,
            outcome.writer.written_frames,
            plan.frames.count()
        )));
    }
    diagnostics::verify_successful_output(target.path())?;
    controller.set_frame_progress(
        plan.output_frame_count,
        plan.output_frame_count,
        outcome.producer.rendered_frames,
        plan.output_frame_count,
        Some(0),
        None,
    );
    processes.preserve_output();
    Ok(FramePipelineOutcome {
        rendered_frames: outcome.producer.rendered_frames,
        timings: merge_timing_maps(outcome.producer.timings, outcome.writer.timings),
        total_seconds: started.elapsed().as_secs_f64(),
        render_loop_ms,
        finalize_ms,
        workers: workers.get(),
    })
}
