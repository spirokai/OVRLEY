//! Composite render and FFmpeg plan derivation.

use std::path::{Path, PathBuf};

use crate::encode::composite::CompositeRenderPlan;
use crate::encode::ffmpeg::catalog::CodecSelection;
use crate::encode::ffmpeg::composite::{build_composite_ffmpeg_settings, CompositeFfmpegSettings};
use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::output::RenderOutputTarget;
use crate::paths::AppPaths;
use crate::render::FrameSize;

const COMPOSITE_ACTIVITY_DURATION_SLACK_SECONDS: f64 = 0.25;

/// Validates composite render fields and derives timing/FPS values.
///
/// Required fields fail before dense activity is built, while optional fields
/// receive standard defaults.
pub fn derive_composite_render_plan(
    scene: &mut crate::normalize::ValidatedSceneConfig,
    activity_end: Option<f64>,
) -> CoreResult<CompositeRenderPlan> {
    let video_path = scene
        .composite_video_path
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            CoreError::Config("scene.composite_video_path required for composite render".into())
        })?;
    let bitrate = scene
        .composite_bitrate
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            CoreError::Config("scene.composite_bitrate required for composite render".into())
        })?;
    let fps_num = scene.composite_video_fps_num.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_num required for composite render".into())
    })?;
    let fps_den = scene.composite_video_fps_den.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_den required for composite render".into())
    })?;
    let source_fps = Fps::new(fps_num, fps_den)?;
    let video_duration = scene.composite_video_duration.ok_or_else(|| {
        CoreError::Config("scene.composite_video_duration required for composite render".into())
    })?;
    if !video_duration.is_finite() || video_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_video_duration must be greater than zero: {video_duration}"
        )));
    }

    let sync_offset = scene.composite_sync_offset.ok_or_else(|| {
        CoreError::Config("scene.composite_sync_offset required for composite render".into())
    })?;
    if !sync_offset.is_finite() {
        return Err(CoreError::Config(format!(
            "scene.composite_sync_offset must be finite: {sync_offset}"
        )));
    }
    if sync_offset <= -video_duration {
        return Err(CoreError::Config(format!(
            "scene.composite_sync_offset ({sync_offset}) must leave a positive overlap with scene.composite_video_duration ({video_duration})"
        )));
    }
    let trim_start = scene.composite_video_trim_start.ok_or_else(|| {
        CoreError::Config("scene.composite_video_trim_start required for composite render".into())
    })?;
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

    let update_rate = scene.composite_widget_update_rate.ok_or_else(|| {
        CoreError::Config("scene.composite_widget_update_rate required for composite render".into())
    })?;
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let mut render_duration = scene
        .composite_render_duration
        .unwrap_or(video_duration - trim_start);
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_render_duration must be greater than zero: {render_duration}"
        )));
    }
    let mut activity_overlap_duration = render_duration;
    if let Some(activity_end) = activity_end {
        if !activity_end.is_finite() || activity_end < 0.0 {
            return Err(CoreError::Config(format!(
                "Composite activity end must be finite and zero or greater: {activity_end}"
            )));
        }
        let video_end = sync_offset + render_duration;
        if sync_offset >= activity_end || video_end <= 0.0 {
            return Err(CoreError::Config(format!(
                "Composite video range [{sync_offset}, {video_end}] does not overlap activity range [0, {activity_end}]"
            )));
        }
        let max_render_duration = activity_end - sync_offset;
        let overrun = render_duration - max_render_duration;
        if sync_offset >= 0.0 {
            let ends_just_after_activity =
                overrun > 0.0 && overrun <= COMPOSITE_ACTIVITY_DURATION_SLACK_SECONDS;
            if ends_just_after_activity {
                render_duration = max_render_duration;
            }
        }
        let overlap_start = sync_offset.max(0.0);
        let overlap_end = activity_end.min(sync_offset + render_duration);
        activity_overlap_duration = overlap_end - overlap_start;

        scene.start = overlap_start;
        scene.end = overlap_end;
    } else {
        scene.start = sync_offset.max(0.0);
        scene.end = scene.start + render_duration;
    }
    let requested_codec_id = match scene.ffmpeg.codec {
        CodecSelection::Composite(codec_id) => codec_id,
        CodecSelection::Transparent(codec_id) => {
            return Err(CoreError::Config(format!(
                "Transparent codec '{}' cannot be used for a composite render",
                codec_id.metadata().profile_name
            )))
        }
    };

    scene.fps = overlay_pipe_fps.as_f64();
    scene.update_rate = std::num::NonZeroU32::MIN;
    let overlay_frame_count = overlay_pipe_fps.frame_count_for_duration(render_duration)?;
    let blank_leading_frame_count =
        blank_leading_frame_count(overlay_pipe_fps, sync_offset, overlay_frame_count)?;
    let output_frame_count = u32::try_from(source_fps.frame_count_for_duration(render_duration)?)
        .map_err(|_| {
        CoreError::Encode("Composite output frame count exceeds u32".to_string())
    })?;

    Ok(CompositeRenderPlan {
        video_path: PathBuf::from(video_path),
        bitrate,
        sync_offset,
        trim_start,
        render_duration,
        update_rate,
        source_fps,
        overlay_pipe_fps,
        overlay_frame_count,
        output_frame_count,
        activity_overlap_duration,
        blank_leading_frame_count,
        requested_codec_id,
        qsv_full_init_args: scene.ffmpeg.qsv_full_init_args.clone(),
    })
}

/// Counts output overlay frames whose canonical timeline timestamp precedes
/// activity time zero, using the canonical rational-FPS duration conversion.
fn blank_leading_frame_count(fps: Fps, sync_offset: f64, frame_count: u64) -> CoreResult<u64> {
    if sync_offset >= 0.0 {
        return Ok(0);
    }
    Ok(fps.frame_count_for_duration(-sync_offset)?.min(frame_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_negative_lead_in_frames_at_fractional_offsets() {
        let fps = Fps::new(30, 1).expect("valid fps");

        assert_eq!(blank_leading_frame_count(fps, -0.01, 30).unwrap(), 1);
        assert_eq!(blank_leading_frame_count(fps, -5.0, 900).unwrap(), 150);
        assert_eq!(blank_leading_frame_count(fps, -5.01, 900).unwrap(), 151);
    }
}

/// Process resources derived from the canonical composite render plan.
///
/// Keeping this as a small data object makes timing math easy to test and
/// keeps process-only state separate from source-owned timeline state.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositePipelinePlan {
    pub render: CompositeRenderPlan,
    pub frame_size: FrameSize,
    pub ffmpeg_settings: CompositeFfmpegSettings,
    pub output_path: PathBuf,
}

impl CompositePipelinePlan {
    pub fn output_progress(&self, written_overlay_frames: u64) -> u32 {
        assert!(
            written_overlay_frames > 0,
            "written overlay frames must be non-zero"
        );
        assert!(
            written_overlay_frames <= self.render.overlay_frame_count,
            "written overlay frames must not exceed the source-video timeline plan"
        );
        let video_local_time = self
            .render
            .overlay_pipe_fps
            .seconds_at_frame(written_overlay_frames - 1);
        let output_progress = (video_local_time * self.render.source_fps.as_f64()).round() as u64;
        assert!(
            output_progress <= u64::from(self.render.output_frame_count),
            "composite output progress must not exceed the source-video frame count"
        );
        u32::try_from(output_progress).expect("validated composite output progress fits u32")
    }
}

/// Derives composite timing and FFmpeg settings.
///
/// Frame counts are already fixed by `derive_composite_render_plan`; this phase
/// adds only process resources that require application paths or source probing.
pub fn derive_composite_pipeline_plan(
    _paths: &AppPaths,
    scene: &crate::normalize::ValidatedSceneConfig,
    render: CompositeRenderPlan,
    include_audio: bool,
    source_rotation_degrees: Option<i32>,
    output_target: &RenderOutputTarget,
) -> CoreResult<CompositePipelinePlan> {
    // —— PHASE 1: VALIDATE & DERIVE TIMING VALUES ——
    let frame_size = FrameSize {
        width: scene.width,
        height: scene.height,
    };
    // —— PHASE 2: BUILD COMPOSITE FFMPEG SETTINGS ——
    let ffmpeg_settings = build_composite_ffmpeg_settings(
        &render,
        frame_size,
        include_audio,
        source_rotation_degrees,
    )?;
    // —— PHASE 3: GENERATE OUTPUT FILENAME ——
    let output_path = output_target.path().to_path_buf();

    Ok(CompositePipelinePlan {
        render,
        frame_size,
        ffmpeg_settings,
        output_path,
    })
}

pub(crate) fn verify_composite_source_resolution(
    paths: &AppPaths,
    composite_video_path: &Path,
    scene_width: u32,
    scene_height: u32,
) -> CoreResult<(Option<i32>, bool)> {
    if !composite_video_path.is_file() {
        return Err(CoreError::Config(format!(
            "Composite video does not exist: {}",
            composite_video_path.display()
        )));
    }

    let video_path = composite_video_path.to_str().ok_or_else(|| {
        CoreError::Config(format!(
            "Composite video path is not valid Unicode: {}",
            composite_video_path.display()
        ))
    })?;
    let metadata = crate::media::video_probe::probe_video(&paths.repo_root, video_path)?;
    let resolution = metadata.resolution.ok_or_else(|| {
        CoreError::Config(format!(
            "Could not read composite video resolution for {}",
            composite_video_path.display()
        ))
    })?;

    let rotation = metadata
        .rotation_degrees
        .map(|degrees| degrees.rem_euclid(360));
    let (display_width, display_height) = if matches!(rotation, Some(90 | 270)) {
        (resolution.height, resolution.width)
    } else {
        (resolution.width, resolution.height)
    };

    if u64::from(scene_width) != display_width || u64::from(scene_height) != display_height {
        return Err(CoreError::Config(format!(
            "scene resolution {scene_width}x{scene_height} must match display-oriented composite video resolution {display_width}x{display_height} (coded {}x{}, rotation {})",
            resolution.width,
            resolution.height,
            metadata
                .rotation_degrees
                .map(|degrees| degrees.to_string())
                .unwrap_or_else(|| "none".to_string())
        )));
    }

    Ok((metadata.rotation_degrees, metadata.has_audio))
}
