//! Batch ingress, output review and per-video preparation.
//! Owns the source-local clock and coverage for both export modes; pipelines
//! consume the resulting fixed plans without reinterpreting batch input.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use super::contracts::{BatchEncodingSettings, BatchExportMode, BatchJobTiming, BatchTemplate};
use super::inspection::{
    InspectionRejection, InspectionSourceSelection, InspectionValidation, VideoInspectionService,
};
use crate::activity::{
    build_dense_activity_report_for_timeline,
    schema::{DenseActivityReport, ParsedActivity},
};
use crate::encode::composite::CompositeRenderPlan;
use crate::encode::ffmpeg::catalog::{CodecSelection, CompositeFilterStackKind};
use crate::encode::fps::Fps;
use crate::encode::pipeline::transparent::{rendered_frame_count, TransparentRenderPlan};
use crate::encode::quality::{validate_quality, EncodingQuality};
use crate::encode::video_timing::ActivityCoverage;
use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::InspectedVideoSource;
use crate::normalize::{
    raw::RenderConfig, validate_render_config_with_resources, ValidatedRenderConfig,
};
use crate::output::{plan_batch_output_targets, RenderOutputKind};
use crate::raster::RasterResourceResolver;

/// Validated once at configuration/acceptance ingress; private fields keep
/// consumers from substituting malformed settings after validation.
pub struct ValidatedBatchEncoding {
    layout_fps: Fps,
    update_rate: NonZeroU32,
    codec: CodecSelection,
    quality: EncodingQuality,
    qsv_full_init_args: Vec<String>,
}

pub fn validate_batch_encoding(raw: &BatchEncodingSettings) -> CoreResult<ValidatedBatchEncoding> {
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
            let layout = usize::try_from(layout_fps.frame_count_for_duration(duration)?)
                .map_err(|_| CoreError::Encode("Batch layout exceeds usize".into()))?;
            (
                rendered_frame_count(layout, update_rate)? as u64,
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

/// Immutable shared validated presentation; raster resources are retained by
/// the existing normalization/resource seam. An internal unit clock lets that
/// seam validate presentation; already validated encoding is applied afterward.
pub struct ValidatedBatchTemplate {
    config: ValidatedRenderConfig,
    layout_fps: Fps,
}

pub fn validate_batch_template(
    template: BatchTemplate,
    encoding: ValidatedBatchEncoding,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<ValidatedBatchTemplate> {
    let mut scene = template.scene.presentation;
    if scene.keys().any(|key| {
        matches!(
            key.as_str(),
            "width"
                | "height"
                | "start"
                | "end"
                | "fps"
                | "updateRate"
                | "update_rate"
                | "ffmpeg"
                | "qualityType"
                | "qualityValue"
                | "custom_export_range_active"
        ) || key.starts_with("composite_")
    }) {
        return Err(CoreError::Config(
            "Batch template must contain only shared presentation fields".into(),
        ));
    }
    scene.insert("width".into(), json!(template.scene.width));
    scene.insert("height".into(), json!(template.scene.height));
    scene.insert("start".into(), json!(0));
    scene.insert("end".into(), json!(1));
    scene.insert("fps".into(), json!(1));
    scene.insert("update_rate".into(), json!(1));
    let raw = RenderConfig {
        scene: serde_json::from_value(Value::Object(scene.into_iter().collect()))?,
        backdrops: template.backdrops,
        rasters: template.rasters,
        labels: template.labels,
        values: template.values,
        plots: Value::Array(template.plots),
        extra: Default::default(),
    };
    let mut config = validate_render_config_with_resources(raw, resources)?;
    config.scene.fps = encoding.layout_fps.as_f64();
    config.scene.update_rate = encoding.update_rate;
    config.scene.ffmpeg.codec = encoding.codec;
    config.scene.ffmpeg.qsv_full_init_args = encoding.qsv_full_init_args;
    config.scene.quality = Some(encoding.quality);
    Ok(ValidatedBatchTemplate {
        config,
        layout_fps: encoding.layout_fps,
    })
}

pub enum VideoRenderModePlan {
    Composite(CompositeRenderPlan),
    Transparent(TransparentRenderPlan),
}

/// No setters: each job owns its effective configuration and fixed frame grid.
pub struct PlannedVideoRender {
    pub(crate) config: ValidatedRenderConfig,
    pub(crate) mode: VideoRenderModePlan,
    pub(crate) source: InspectedVideoSource,
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
            VideoRenderModePlan::Composite(plan) => plan.output_frame_count,
            VideoRenderModePlan::Transparent(plan) => plan.output_frame_count,
        }
    }
    pub fn prepare_activity(&self, activity: &ParsedActivity) -> CoreResult<DenseActivityReport> {
        let coverage = match &self.mode {
            VideoRenderModePlan::Composite(plan) => &plan.coverage,
            VideoRenderModePlan::Transparent(plan) => &plan.coverage,
        };
        build_dense_activity_report_for_timeline(
            activity,
            &self.config,
            coverage.frame_timeline(self.sampling_fps, self.activity_offset),
        )
    }
}

/// Finalizes activity-dependent coverage at the owning job's preparation seam.
/// Sources must come from the inspection service's owned validation result.
pub fn plan_video_render(
    template: &ValidatedBatchTemplate,
    source: &InspectedVideoSource,
    timing: &BatchJobTiming,
    skip_overlay: bool,
    activity: &ParsedActivity,
) -> CoreResult<PlannedVideoRender> {
    let offset = match timing {
        BatchJobTiming::EmbeddedActivity { offset_seconds } if *offset_seconds == 0.0 => 0.0,
        BatchJobTiming::EmbeddedActivity { .. } => {
            return Err(CoreError::Config(
                "Embedded batch offset must be zero".into(),
            ))
        }
        BatchJobTiming::ExternalActivity {
            offset_seconds,
            automatic_offset_seconds,
        } => {
            if !offset_seconds.is_finite() || !automatic_offset_seconds.is_finite() {
                return Err(CoreError::Config("Batch offsets must be finite".into()));
            }
            *offset_seconds
        }
    };
    let activity_end = activity.trim_end_seconds.max(
        activity
            .sample_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default(),
    );
    if !activity_end.is_finite() || activity_end <= 0.0 || activity.sample_elapsed_seconds.len() < 2
    {
        return Err(CoreError::Activity(
            "Batch rendering requires a positive activity timeline".into(),
        ));
    }
    let duration = source.metadata.duration.expect("inspected source duration");
    let mut config = template.config.clone();
    config.scene.width = source.display_resolution.width as u32;
    config.scene.height = source.display_resolution.height as u32;
    config.scene.custom_export_range_active = Some(true);
    if skip_overlay {
        config.values.clear();
        config.course_plots.clear();
        config.elevation_plots.clear();
    }
    let scene = &mut config.scene;
    let update_rate = scene.update_rate;
    let (output_frame_count, container_fps) =
        video_output_work(source, scene.ffmpeg.codec, template.layout_fps, update_rate)?;
    let sampling_fps = match scene.ffmpeg.codec {
        CodecSelection::Composite(_) => container_fps.divided_by(update_rate)?,
        CodecSelection::Transparent(_) => template.layout_fps,
    };
    let layout_frame_count = sampling_fps.frame_count_for_duration(duration)?;
    let coverage = ActivityCoverage::for_video(
        duration,
        offset,
        activity_end,
        sampling_fps,
        layout_frame_count,
    )?;
    scene.start = coverage.start;
    scene.end = coverage.end;
    scene.fps = sampling_fps.as_f64();
    let mode = match scene.ffmpeg.codec {
        CodecSelection::Composite(requested_codec_id) => {
            scene.composite_video_path = Some(source.metadata.path.clone());
            scene.composite_sync_offset = Some(offset);
            scene.update_rate = NonZeroU32::MIN;
            VideoRenderModePlan::Composite(CompositeRenderPlan {
                video_path: PathBuf::from(&source.metadata.path),
                quality: scene.quality.expect("validated batch quality"),
                sync_offset: offset,
                trim_start: 0.0,
                render_duration: duration,
                update_rate,
                source_fps: container_fps,
                overlay_pipe_fps: sampling_fps,
                overlay_frame_count: layout_frame_count,
                output_frame_count,
                coverage,
                requested_codec_id,
                qsv_full_init_args: scene.ffmpeg.qsv_full_init_args.clone(),
            })
        }
        CodecSelection::Transparent(_) => {
            if !scene.width.is_multiple_of(2) || !scene.height.is_multiple_of(2) {
                return Err(CoreError::Config(
                    "Transparent batch video dimensions must be even".into(),
                ));
            }
            VideoRenderModePlan::Transparent(TransparentRenderPlan {
                layout_frame_count,
                output_frame_count,
                update_rate,
                container_fps: container_fps.ffmpeg_arg(),
                coverage,
            })
        }
    };
    Ok(PlannedVideoRender {
        config,
        mode,
        source: source.clone(),
        sampling_fps,
        activity_offset: offset,
    })
}
