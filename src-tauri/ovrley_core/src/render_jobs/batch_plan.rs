//! Batch ingress, output review and per-video preparation.
//! Owns the source-local clock and coverage for both export modes; pipelines
//! consume the resulting fixed plans without reinterpreting batch input.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::contracts::{BatchEncodingSettings, BatchExportMode};
use super::inspection::{
    InspectionRejection, InspectionSourceSelection, InspectionValidation, VideoInspectionService,
};
use crate::activity::{
    build_dense_activity_report_for_timeline,
    schema::{DenseActivityReport, ParsedActivity},
};
use crate::encode::composite::CompositeRenderPlan;
use crate::encode::ffmpeg::catalog::{
    CodecSelection, CompositeFilterStackKind, TransparentCodecId,
};
use crate::encode::ffmpeg::settings::build_ffmpeg_settings;
use crate::encode::fps::Fps;
use crate::encode::pipeline::frames::FrameProductionPlan;
use crate::encode::pipeline::transparent::TransparentRenderPlan;
use crate::encode::quality::{validate_quality, EncodingQuality};
use crate::encode::video_timing::ActivityCoverage;
use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::InspectedVideoSource;
use crate::normalize::{
    raw::{RenderConfig, ScenePresentationConfig},
    validate_ffmpeg_config, validate_render_config_with_resources, validate_render_presentation,
    ValidatedFfmpegConfig, ValidatedRenderConfig, ValidatedSceneConfig, ValidatedScenePresentation,
};
use crate::output::{plan_batch_output_targets, RenderOutputKind};
use crate::raster::RasterResourceResolver;

/// Validated once at configuration/acceptance ingress; private fields keep
/// consumers from substituting malformed settings after validation.
pub(crate) struct ValidatedBatchEncoding {
    layout_fps: Fps,
    update_rate: NonZeroU32,
    codec: CodecSelection,
    quality: EncodingQuality,
    qsv_full_init_args: Vec<String>,
}

pub(crate) fn validate_batch_encoding(
    raw: &BatchEncodingSettings,
) -> CoreResult<ValidatedBatchEncoding> {
    let layout_fps = Fps::new(raw.fps, 1)?;
    let update_rate = NonZeroU32::new(raw.update_rate)
        .ok_or_else(|| CoreError::Config("Batch updateRate must be positive".into()))?;
    if raw.update_rate > raw.fps || raw.fps % raw.update_rate != 0 {
        return Err(CoreError::Config(
            "Batch updateRate must divide the selected FPS".into(),
        ));
    }
    let codec = CodecSelection::from_external_name(&raw.export_codec).ok_or_else(|| {
        CoreError::Config(format!("Unsupported batch codec: {}", raw.export_codec))
    })?;
    if !matches!(
        (raw.export_mode, codec),
        (BatchExportMode::Composite, CodecSelection::Composite(_))
            | (BatchExportMode::Transparent, CodecSelection::Transparent(_))
    ) {
        return Err(CoreError::Config(
            "Batch codec must match the selected export container".into(),
        ));
    }
    let quality = validate_quality(raw.quality_type, raw.quality_value)?;
    let qsv_full_init_args = raw.qsv_full_init_args.clone().unwrap_or_default();
    let needs_qsv = matches!(codec, CodecSelection::Composite(id)
        if id.metadata().filter_stack_kind == CompositeFilterStackKind::QsvFullOverlay);
    if needs_qsv
        && (qsv_full_init_args.is_empty()
            || qsv_full_init_args.iter().any(|arg| arg.trim().is_empty()))
    {
        return Err(CoreError::Config(
            "QSV full batch encoding requires detected hardware init args".into(),
        ));
    }
    if !needs_qsv && raw.qsv_full_init_args.is_some() {
        return Err(CoreError::Config(
            "QSV init args are only valid for QSV full encoding".into(),
        ));
    }
    Ok(ValidatedBatchEncoding {
        layout_fps,
        update_rate,
        codec,
        quality,
        qsv_full_init_args,
    })
}

/// Output review and job preparation use the same exact frame-count rules.
fn video_output_work(
    source: &InspectedVideoSource,
    codec: CodecSelection,
    layout_fps: Fps,
    update_rate: NonZeroU32,
) -> CoreResult<(u32, Fps)> {
    let duration = source.metadata.duration.expect("inspected source duration");
    let (frames, fps) = match codec {
        CodecSelection::Composite(_) => {
            let fps = Fps::new(
                source.metadata.fps_num.expect("inspected FPS numerator"),
                source.metadata.fps_den.expect("inspected FPS denominator"),
            )?;
            (fps.frame_count_for_duration(duration)?, fps)
        }
        CodecSelection::Transparent(_) => {
            let layout = layout_fps.frame_count_for_duration(duration)?;
            (
                u64::from(FrameProductionPlan::decimated(layout, update_rate)?.count()),
                layout_fps.divided_by(update_rate)?,
            )
        }
    };
    let frames = u32::try_from(frames)
        .map_err(|_| CoreError::Encode("Batch output frame count exceeds u32".into()))?;
    Ok((frames, fps))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchVideoOutputPlan {
    pub source_id: String,
    pub output_path: PathBuf,
    pub output_duration_seconds: f64,
    pub planned_frames: u64,
    pub container_fps_num: u32,
    pub container_fps_den: u32,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum BatchPlanningResponse {
    Planned { plans: Vec<BatchVideoOutputPlan> },
    Rejected(InspectionRejection),
}

/// Public configuration seam: plans only fresh, session-owned descriptors.
pub fn plan_batch_configuration(
    inspection: &VideoInspectionService,
    selection: &InspectionSourceSelection,
    encoding: &BatchEncodingSettings,
    output_directory: &Path,
) -> CoreResult<BatchPlanningResponse> {
    let encoding = validate_batch_encoding(encoding)?;
    let sources = match inspection.validate_sources(selection) {
        InspectionValidation::Valid(sources) => sources,
        InspectionValidation::Rejected(rejection) => {
            return Ok(BatchPlanningResponse::Rejected(rejection))
        }
    };
    let paths = sources
        .sources()
        .iter()
        .map(|source| PathBuf::from(&source.metadata.path))
        .collect::<Vec<_>>();
    let targets = plan_batch_output_targets(
        output_directory,
        match encoding.codec {
            CodecSelection::Composite(_) => RenderOutputKind::Composite,
            CodecSelection::Transparent(_) => RenderOutputKind::Transparent,
        },
        &paths,
        sources
            .calibration_source()
            .map(|source| Path::new(&source.metadata.path)),
    )?;
    let plans = sources
        .sources()
        .iter()
        .zip(targets)
        .map(|(source, target)| {
            let (planned_frames, fps) = video_output_work(
                source,
                encoding.codec,
                encoding.layout_fps,
                encoding.update_rate,
            )?;
            let (container_fps_num, container_fps_den) = fps.components();
            Ok(BatchVideoOutputPlan {
                source_id: source.source_id.clone(),
                output_path: target.path().to_path_buf(),
                output_duration_seconds: source.metadata.duration.expect("inspected duration"),
                planned_frames: u64::from(planned_frames),
                container_fps_num,
                container_fps_den,
            })
        })
        .collect::<CoreResult<_>>()?;
    Ok(BatchPlanningResponse::Planned { plans })
}

/// Submission-owned presentation and encoding, independent of any render clock.
pub(crate) struct ValidatedBatchTemplate {
    config: ValidatedRenderConfig<ValidatedScenePresentation>,
    encoding: ValidatedBatchEncoding,
}

impl ValidatedBatchTemplate {
    pub(crate) fn output_work(&self, source: &InspectedVideoSource) -> CoreResult<(u32, Fps)> {
        video_output_work(
            source,
            self.encoding.codec,
            self.encoding.layout_fps,
            self.encoding.update_rate,
        )
    }
}

pub(crate) fn validate_batch_template(
    template: RenderConfig<ScenePresentationConfig>,
    encoding: ValidatedBatchEncoding,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<ValidatedBatchTemplate> {
    if !template.extra.is_empty() || !template.scene.extra.is_empty() {
        return Err(CoreError::Config(
            "Batch template must contain only shared presentation fields".into(),
        ));
    }
    Ok(ValidatedBatchTemplate {
        config: validate_render_presentation(template, resources)?,
        encoding,
    })
}

pub enum VideoRenderModePlan {
    Composite {
        render: CompositeRenderPlan,
        /// Absent for a single render until its source is probed on the worker.
        source_metadata: Option<(bool, Option<i32>)>,
    },
    Transparent(TransparentRenderPlan),
}

/// No setters: each job owns its effective configuration and fixed frame grid.
pub struct PlannedVideoRender {
    pub(crate) config: ValidatedRenderConfig,
    pub(crate) mode: VideoRenderModePlan,
    sampling_fps: Fps,
    activity_offset: f64,
}

impl PlannedVideoRender {
    pub fn config(&self) -> &ValidatedRenderConfig {
        &self.config
    }
    pub fn mode(&self) -> &VideoRenderModePlan {
        &self.mode
    }
    pub fn planned_frames(&self) -> u32 {
        match &self.mode {
            VideoRenderModePlan::Composite { render: plan, .. } => plan.output_frame_count,
            VideoRenderModePlan::Transparent(plan) => plan.output_frame_count,
        }
    }
    pub fn prepare_activity(&self, activity: &ParsedActivity) -> CoreResult<DenseActivityReport> {
        let coverage = match &self.mode {
            VideoRenderModePlan::Composite { render: plan, .. } => &plan.coverage,
            VideoRenderModePlan::Transparent(plan) => &plan.coverage,
        };
        build_dense_activity_report_for_timeline(
            activity,
            &self.config,
            coverage.frame_timeline(self.sampling_fps, self.activity_offset),
        )
    }
}

/// Derives a single custom-range plan from validated submission inputs.
pub fn plan_single_render(
    raw: RenderConfig,
    activity_end: f64,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<PlannedVideoRender> {
    let raw_scene = raw.scene.clone();
    let mut config = validate_render_config_with_resources(raw, resources)?;
    let (mode, sampling_fps, activity_offset) = if raw_scene.composite_video_path.is_some() {
        let render = crate::encode::pipeline::composite_plan::derive_composite_render_plan(
            &raw_scene,
            &mut config.scene,
            Some(activity_end),
        )?;
        let fps = render.overlay_pipe_fps;
        let offset = render.sync_offset;
        (
            VideoRenderModePlan::Composite {
                render,
                source_metadata: None,
            },
            fps,
            offset,
        )
    } else {
        let scene = &config.scene;
        let fps = Fps::new(scene.fps as u32, 1)?;
        let count = fps.frame_count_for_duration(scene.end - scene.start)?;
        (
            plan_transparent_render(
                scene,
                validate_ffmpeg_config(
                    raw_scene.ffmpeg,
                    CodecSelection::Transparent(TransparentCodecId::ProresKs),
                )?,
                count,
                fps.divided_by(scene.update_rate)?,
                ActivityCoverage {
                    start: scene.start,
                    end: scene.end,
                    blank_leading_frame_count: 0,
                    frame_count: count,
                },
            )?,
            fps,
            scene.start,
        )
    };
    Ok(PlannedVideoRender {
        config,
        mode,
        sampling_fps,
        activity_offset,
    })
}

/// Consumes timing and activity bounds validated by acceptance/job preparation.
pub(crate) fn plan_batch_item(
    template: &ValidatedBatchTemplate,
    source: &InspectedVideoSource,
    offset: f64,
    skip_overlay: bool,
    activity_end: f64,
    output_frame_count: u32,
    container_fps: Fps,
) -> CoreResult<PlannedVideoRender> {
    let duration = source.metadata.duration.expect("inspected source duration");
    let encoding = &template.encoding;
    let update_rate = encoding.update_rate;
    let sampling_fps = match encoding.codec {
        CodecSelection::Composite(_) => container_fps.divided_by(update_rate)?,
        CodecSelection::Transparent(_) => encoding.layout_fps,
    };
    let layout_frame_count = sampling_fps.frame_count_for_duration(duration)?;
    let coverage = ActivityCoverage::for_video(
        duration,
        offset,
        activity_end,
        sampling_fps,
        layout_frame_count,
    )?;
    let mut presentation = template.config.scene.clone();
    presentation.width = source.display_resolution.width as u32;
    presentation.height = source.display_resolution.height as u32;
    let scene = ValidatedSceneConfig {
        presentation,
        start: coverage.start,
        end: coverage.end,
        fps: sampling_fps.as_f64(),
        update_rate: if matches!(encoding.codec, CodecSelection::Composite(_)) {
            NonZeroU32::MIN
        } else {
            update_rate
        },
        export_start_seconds: offset,
        custom_export_range_active: Some(true),
    };
    let mut config = template.config.clone().with_scene(scene);
    if skip_overlay {
        config.values.clear();
        config.course_plots.clear();
        config.elevation_plots.clear();
    }
    let mode = match encoding.codec {
        CodecSelection::Composite(requested_codec_id) => VideoRenderModePlan::Composite {
            render: CompositeRenderPlan {
                video_path: PathBuf::from(&source.metadata.path),
                quality: encoding.quality,
                sync_offset: offset,
                trim_start: 0.0,
                render_duration: duration,
                update_rate,
                source_fps: container_fps,
                overlay_pipe_fps: sampling_fps,
                frames: FrameProductionPlan::new(layout_frame_count, NonZeroU32::MIN)?,
                overlay_frame_count: layout_frame_count,
                output_frame_count,
                coverage,
                requested_codec_id,
                qsv_full_init_args: encoding.qsv_full_init_args.clone(),
            },
            source_metadata: Some((source.metadata.has_audio, source.metadata.rotation_degrees)),
        },
        CodecSelection::Transparent(_) => plan_transparent_render(
            &config.scene,
            ValidatedFfmpegConfig {
                codec: encoding.codec,
                ..ValidatedFfmpegConfig::default()
            },
            layout_frame_count,
            container_fps,
            coverage,
        )?,
    };
    Ok(PlannedVideoRender {
        config,
        mode,
        sampling_fps,
        activity_offset: offset,
    })
}

fn plan_transparent_render(
    scene: &ValidatedSceneConfig,
    ffmpeg: ValidatedFfmpegConfig,
    layout_frame_count: u64,
    container_fps: Fps,
    coverage: ActivityCoverage,
) -> CoreResult<VideoRenderModePlan> {
    if !scene.presentation.width.is_multiple_of(2) || !scene.presentation.height.is_multiple_of(2) {
        return Err(CoreError::Config(
            "Transparent video dimensions must be even".into(),
        ));
    }
    let frames = FrameProductionPlan::decimated(layout_frame_count, scene.update_rate)?;
    Ok(VideoRenderModePlan::Transparent(TransparentRenderPlan {
        frames,
        ffmpeg: build_ffmpeg_settings(&ffmpeg)?,
        layout_frame_count: layout_frame_count as u32,
        output_frame_count: frames.count(),
        update_rate: scene.update_rate,
        container_fps: container_fps.ffmpeg_arg(),
        coverage,
    }))
}
