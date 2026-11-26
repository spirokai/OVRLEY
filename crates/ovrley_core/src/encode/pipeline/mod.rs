//! Shared video preparation and encoding, with one supervised native pipeline.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Instant;
use self::frames::{
    diagnose_frame_worker_count, merge_timing_maps, FrameBuffer, ParallelFrameChannels,
    ParallelFramePoolPlan,
};
use self::process::{
    format_pipe_write_failure, is_pipe_write_error, stderr_tail, EncoderMonitor,
};
use self::frames::{render_frames_parallel, ParallelFrameProgress, PipedFrameFormat};
use self::process::{PipelineKind, PipelineProcesses, PipelineShutdown};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::diagnostics::TimingBucket;
use crate::diagnostics::write_composite_timing_summary;
use crate::diagnostics::{
    create_debug_dir, render_sample_frames_enabled, sample_frame_indices, write_prepare_summary,
    write_sample_frame, write_timing_summary,
};
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::ffmpeg::composite::CompositeEncoding;
use crate::encode::ffmpeg::profiles::composite_profile;
use crate::encode::ffmpeg::profiles::transparent_profile;
use crate::encode::overlay::OverlayPixelFormat;
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::config::ValidatedRenderConfig;
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::diagnostics::RenderProfiler;
use crate::render::{ExportMapJob, ExportWidgets, FrameSize, PreparedAtlas, SlotPlacement};
use crate::jobs::inspection::CompositeSourceMetadata;
use std::num::NonZeroU32;
use std::path::PathBuf;
use crate::encode::ffmpeg::catalog::CompositeCodecId;
use crate::encode::ffmpeg::transparent::FfmpegSettings;
use crate::encode::timing::Fps;
use crate::encode::timing::ActivityCoverage;
use std::sync::atomic::AtomicBool;
use serde::Deserialize;
use self::process::PartialOutputGuard;
use crate::encode::ffmpeg::assemble::{
    concat_manifest, join_args, probe_output_args, remux_part_args, transition_segment_args,
    AssemblyKind, AssemblySegment, OutputAssembly,
};
use crate::encode::ffmpeg::run;

#[cfg(test)]
mod atlas_export_tests;
pub mod frames;
pub mod process;



/// Encoding inputs fixed by job planning. Source probing and activity timing
/// belong to jobs; a composite encoding is completed by the export's atlas plan.
pub(crate) enum VideoEncodingRequest {
    Composite {
        render: CompositeRenderPlan,
        source: CompositeSourceMetadata,
    },
    Transparent(TransparentRenderPlan),
}

/// Process-ready settings.
pub(crate) enum VideoEncoding {
    Composite(CompositeEncoding),
    Transparent(TransparentRenderPlan),
}

impl VideoEncoding {
    fn frame_plan(
        &self,
        atlas: PreparedAtlas,
        size: FrameSize,
        output: &Path,
        input_format: &str,
    ) -> FramePipelinePlan {
        match self {
            Self::Composite(plan) => FramePipelinePlan {
                kind: PipelineKind::Composite,
                frames: plan.render.frames,
                atlas,
                pixel_format: plan.ffmpeg_settings.overlay_pixel_format,
                output_frame_count: plan.render.output_frame_count,
                cpu_cores_per_frame_worker: composite_profile(plan.ffmpeg_settings.codec_id)
                    .cpu_cores_per_frame_worker,
                ffmpeg_args: plan.ffmpeg_settings.command_args(output),
            },
            Self::Transparent(plan) => FramePipelinePlan {
                kind: PipelineKind::Transparent,
                frames: plan.frames,
                ffmpeg_args: plan.ffmpeg.command_args(
                    output,
                    atlas.plan(),
                    size,
                    &plan.container_fps,
                    input_format,
                ),
                atlas,
                pixel_format: OverlayPixelFormat::Rgba,
                output_frame_count: plan.output_frame_count,
                cpu_cores_per_frame_worker: transparent_profile(plan.ffmpeg.codec_id)
                    .cpu_cores_per_frame_worker,
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
            let message = error.encode_message();
            if is_pipe_write_error(&message) {
                return CoreError::Encode(format_pipe_write_failure(message, status, stderr, plan));
            }
        }
        if stderr.is_empty() {
            error
        } else {
            CoreError::Encode(format!(
                "{}. FFmpeg stderr:\n{}",
                error.encode_message(),
                stderr_tail(stderr)
            ))
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "each argument is a separate encoding input")]
pub(crate) fn encode_video(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: Option<&ParsedActivity>,
    dense: &DenseActivityReport,
    controller: &RenderController,
    target: &RenderOutputTarget,
    request: VideoEncodingRequest,
    map_source: Option<&ovrley_map::gpu::MapSource>,
) -> CoreResult<String> {
    controller.check_cancelled()?;
    let size = FrameSize {
        width: config.scene.presentation.width,
        height: config.scene.presentation.height,
    };
    let debug_dir = match &request {
        VideoEncodingRequest::Transparent(_) => Some(create_debug_dir(paths)?),
        VideoEncodingRequest::Composite { .. } => None,
    };
    let input_format = match &request {
        VideoEncodingRequest::Transparent(_) => input_pixel_format()?,
        VideoEncodingRequest::Composite { .. } => "rgba".into(),
    };
    let binary = resolve_ffmpeg_binary(&paths.repo_root)?;
    let coverage = match &request {
        VideoEncodingRequest::Composite { render, .. } => &render.coverage,
        VideoEncodingRequest::Transparent(plan) => &plan.coverage,
    };
    let frame_indices = match &request {
        VideoEncodingRequest::Composite { render, .. } => render.frames.dense_frame_indices(),
        VideoEncodingRequest::Transparent(plan) => plan.frames.dense_frame_indices(),
    };
    let mut prepare_profiler = RenderProfiler::default();
    let prepare_started = Instant::now();
    // A video without telemetry samples no activity; its plan keeps rasters only.
    let no_activity = ParsedActivity::default();
    let export = ExportWidgets::prepare(
        &paths.font_dirs,
        config,
        activity.unwrap_or(&no_activity),
        dense,
        size,
        coverage.blank_leading_frame_count,
        ExportMapJob {
            source: map_source,
            frame_indices: &frame_indices,
            controller,
        },
        &mut prepare_profiler,
    )?;
    let (encoding, atlas) = match request {
        // Every composite graph overlays the planned atlas slot by slot, in the
        // placement its filter stack plans.
        VideoEncodingRequest::Composite { render, source } => {
            let atlas = export.plan_atlas(
                &frame_indices,
                render.requested_codec_id.metadata().filter_stack_kind.slot_placement(),
                &mut prepare_profiler,
            )?;
            let encoding = CompositeEncoding::new(
                size,
                render,
                &atlas,
                source.has_audio,
                source.audio_bit_rate,
                source.rotation_degrees,
                source.color,
                target,
            )?;
            (VideoEncoding::Composite(encoding), atlas)
        }
        // FFmpeg copies the planned atlas slots back onto the transparent full frame.
        VideoEncodingRequest::Transparent(plan) => {
            let atlas = export.plan_atlas(
                &frame_indices,
                SlotPlacement::Copy,
                &mut prepare_profiler,
            )?;
            (VideoEncoding::Transparent(plan), atlas)
        }
    };
    let atlas = export.prepare_atlas(atlas, &mut prepare_profiler)?;
    controller.check_cancelled()?;
    let frame_plan = encoding.frame_plan(atlas, size, target.path(), &input_format);
    let samples = if debug_dir.is_some() && render_sample_frames_enabled()? {
        sample_frame_indices(frame_plan.output_frame_count as usize)
    } else {
        Vec::new()
    };
    let prepare_timings = prepare_profiler.summary();
    if let Some(directory) = &debug_dir {
        write_prepare_summary(
            directory,
            prepare_started.elapsed().as_secs_f64() * 1000.0,
            prepare_timings.clone(),
        )?;
    }
    let sample_atlas = frame_plan.atlas.plan().clone();
    let observe = |index: u64, dense_index: usize, buffer: &FrameBuffer| {
        if samples.binary_search(&(index as usize)).is_ok() {
            write_sample_frame(
                debug_dir
                    .as_ref()
                    .expect("samples require a debug directory"),
                size,
                &sample_atlas.unpack_rgba(&buffer.pixels, size)?,
                dense_index,
            )?;
        }
        Ok(())
    };
    let outcome = run_frame_pipeline(
        &binary,
        &export,
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
            merge_timing_maps(prepare_timings, outcome.timings),
            outcome.workers,
            outcome.rendered_frames,
        )
        .map(|_| ()),
        VideoEncoding::Transparent(plan) => write_timing_summary(
            debug_dir
                .as_ref()
                .expect("transparent diagnostics directory"),
            &config.scene,
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
    /// Slots and static bases of the piped atlas frames; its size is the piped frame size.
    pub atlas: PreparedAtlas,
    /// Format of the frames written to FFmpeg; the renderer always draws RGBA.
    pub pixel_format: OverlayPixelFormat,
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

#[expect(clippy::too_many_arguments, reason = "each argument is a separate pipeline input")]
pub(crate) fn run_frame_pipeline(
    binary: &Path,
    export: &ExportWidgets<'_>,
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
    } = ParallelFramePoolPlan::for_frame_size(plan.atlas.plan().size(), plan.pixel_format, workers)?
        .create_channels();
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
        export,
        plan.frames,
        PipedFrameFormat {
            atlas: &plan.atlas,
            pixel_format: plan.pixel_format,
        },
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
    process::verify_successful_output(target.path())?;
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

/// Supported task count and dense-index stride, checked once during planning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameProductionPlan {
    count: u32,
    stride: NonZeroU32,
}

impl FrameProductionPlan {
    pub(crate) fn new(count: u64, stride: NonZeroU32) -> CoreResult<Self> {
        let count = u32::try_from(count)
            .ok()
            .filter(|count| *count > 0)
            .ok_or_else(|| CoreError::Encode("Frame count must be positive and fit u32".into()))?;
        let last_index = u64::from(count - 1) * u64::from(stride.get());
        usize::try_from(last_index.max(u64::from(count)))
            .map_err(|_| CoreError::Encode("Frame task indices exceed usize".into()))?;
        Ok(Self { count, stride })
    }

    pub(crate) fn decimated(layout_count: u64, stride: NonZeroU32) -> CoreResult<Self> {
        let layout = Self::new(layout_count, NonZeroU32::MIN)?;
        Ok(Self {
            count: (layout.count - 1) / stride.get() + 1,
            stride,
        })
    }

    pub(crate) fn count(self) -> u32 {
        self.count
    }

    pub(crate) fn stride(self) -> NonZeroU32 {
        self.stride
    }

    /// Dense frame index of every produced frame, in output order.
    pub(crate) fn dense_frame_indices(self) -> Vec<usize> {
        (0..self.count as usize)
            .map(|index| index * self.stride.get() as usize)
            .collect()
    }
}

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

/// Validated inputs and derived timing for one composite render.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeRenderPlan {
    pub(crate) frames: FrameProductionPlan,
    pub(crate) video_path: PathBuf,
    pub quality: crate::encode::settings::EncodingQuality,
    pub(crate) sync_offset: f64,
    pub(crate) trim_start: f64,
    pub(crate) render_duration: f64,
    pub(crate) update_rate: NonZeroU32,
    pub(crate) source_fps: Fps,
    pub(crate) overlay_pipe_fps: Fps,
    pub overlay_frame_count: u64,
    pub output_frame_count: u32,
    pub coverage: crate::encode::timing::ActivityCoverage,
    pub(crate) requested_codec_id: CompositeCodecId,
    pub(crate) qsv_full_init_args: Vec<String>,
}

impl CompositeRenderPlan {
    /// Maps a completed overlay tick onto the validated source frame grid.
    pub fn output_progress(&self, written_overlay_frames: u64) -> u32 {
        ((written_overlay_frames - 1) * u64::from(self.update_rate.get())) as u32
    }
}

/// Encodes composite transitions, remuxes every composite segment, joins them
/// into the target and verifies its frame count and audio track.
pub(crate) fn assemble_output(
    paths: &AppPaths,
    assembly: &OutputAssembly,
    controller: &RenderController,
) -> CoreResult<()> {
    let ffmpeg = resolve_ffmpeg_binary(&paths.repo_root)?;
    let cancel = controller.cancel_flag();
    if let AssemblyKind::Composite {
        hevc,
        transition_encoding,
        ..
    } = &assembly.kind
    {
        for segment in &assembly.segments {
            if matches!(segment, AssemblySegment::Transition { .. }) {
                run_join_step(
                    &ffmpeg,
                    &transition_segment_args(transition_encoding, segment),
                    &cancel,
                )?;
            }
        }
        for (part, remux) in assembly.remuxes() {
            run_join_step(&ffmpeg, &remux_part_args(&part, &remux, *hevc), &cancel)?;
        }
    }
    let manifest = assembly.manifest_path();
    std::fs::write(&manifest, concat_manifest(&assembly.joined_parts())).map_err(|source| {
        CoreError::Io {
            path: manifest.clone(),
            source,
        }
    })?;
    let mut output = PartialOutputGuard::new(&assembly.target);
    run_join_step(&ffmpeg, &join_args(assembly), &cancel)?;
    verify_joined_output(&ffmpeg, assembly, &cancel)?;
    output.preserve();
    Ok(())
}

fn run_join_step(ffmpeg: &Path, args: &[String], cancel: &AtomicBool) -> CoreResult<()> {
    let step = run(ffmpeg, args, false, cancel)?;
    if !step.status.success() {
        return Err(CoreError::Encode(format!(
            "ffmpeg failed while joining output parts ({}). FFmpeg stderr:\n{}",
            step.status,
            stderr_tail(&step.stderr)
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct ProbedOutput {
    streams: Vec<ProbedStream>,
}

#[derive(Deserialize)]
struct ProbedStream {
    codec_type: String,
    nb_read_packets: Option<String>,
}

/// The joined output must hold exactly the planned video frames, and an audio
/// track exactly when the assembly encodes one.
fn verify_joined_output(
    ffmpeg: &Path,
    assembly: &OutputAssembly,
    cancel: &AtomicBool,
) -> CoreResult<()> {
    let ffprobe = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let output = run(&ffprobe, &probe_output_args(&assembly.target), true, cancel)?;
    if !output.status.success() {
        return Err(CoreError::Encode(format!(
            "ffprobe could not read the joined output {}: {}",
            assembly.target.display(),
            output.stderr.trim()
        )));
    }
    let probed: ProbedOutput = serde_json::from_slice(&output.stdout).map_err(|error| {
        CoreError::Encode(format!("ffprobe returned unreadable stream data: {error}"))
    })?;
    let video_frames = probed
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "video")
        .map(|stream| stream.nb_read_packets.as_deref().map(str::parse::<u64>))
        .collect::<Vec<_>>();
    let frames = match video_frames.as_slice() {
        [Some(Ok(frames))] => *frames,
        _ => {
            return Err(CoreError::Encode(format!(
                "Joined output {} must contain one counted video stream",
                assembly.target.display()
            )))
        }
    };
    if frames != assembly.frames {
        return Err(CoreError::Encode(format!(
            "Joined output {} has {frames} video frames instead of {}",
            assembly.target.display(),
            assembly.frames
        )));
    }
    let expects_audio = matches!(
        assembly.kind,
        AssemblyKind::Composite { audio: Some(_), .. }
    );
    let audio_streams = probed
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "audio")
        .count();
    if audio_streams != usize::from(expects_audio) {
        return Err(CoreError::Encode(format!(
            "Joined output {} has {audio_streams} audio streams instead of {}",
            assembly.target.display(),
            usize::from(expects_audio)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg() -> std::path::PathBuf {
        resolve_ffmpeg_binary(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
    }

    #[test]
    fn failed_steps_report_ffmpeg_stderr() {
        let args = ["-nostdin", "-i", "/nonexistent/part.ts", "-f", "null", "-"].map(String::from);
        let error = run_join_step(&ffmpeg(), &args, &AtomicBool::new(false)).unwrap_err();
        assert!(
            error.to_string().contains("/nonexistent/part.ts"),
            "{error}"
        );
    }

    #[test]
    fn cancellation_stops_output_verification() {
        let assembly = OutputAssembly {
            target: "/nonexistent/joined.mov".into(),
            parts_directory: "/nonexistent".into(),
            segments: Vec::new(),
            frames: 1,
            kind: AssemblyKind::Transparent,
        };
        assert!(matches!(
            verify_joined_output(&ffmpeg(), &assembly, &AtomicBool::new(true)),
            Err(CoreError::Cancelled)
        ));
    }
}
