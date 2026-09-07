//! FFmpeg argument builder for MP4 compositing mode.
//!
//! This module is intentionally separate from the transparent-overlay FFmpeg
//! builder so composite rendering can evolve as a parallel backend path.
//!
//! Owns: `CompositeProfile` (per-codec encoding profile), `CompositeFfmpegSettings`
//!       (grouped FFmpeg arguments for 3-input composite encodes), and
//!       `build_composite_ffmpeg_settings`
//!       (the main argument construction function).
//! Does not own: encoder profile templates (see
//!       [`crate::encode::ffmpeg::composite_profiles`]), codec detection (see
//!       [`crate::encode::ffmpeg::detect`]), actual ffmpeg process spawning (see
//!       [`crate::encode::pipeline::composite`]).
//!
//! Allowed dependencies: `crate::encode::ffmpeg::detect`, `crate::encode::ffmpeg::composite_profiles`,
//!       `crate::encode::fps`, `crate::error`.
//! Forbidden dependencies: `crate::commands`, `crate::render`,
//!       `crate::encode::pipeline::transparent`, `crate::encode::pipeline::composite`.
//!
//! ## Thread Safety
//! All types are plain data (no shared mutable state). Callers construct
//! `CompositeFfmpegSettings` on the render thread before spawning ffmpeg.

use std::path::Path;

use crate::encode::composite::CompositeRenderPlan;
use crate::encode::ffmpeg::catalog::{CompositeCodecId, CompositeFilterStackKind};
use crate::error::{CoreError, CoreResult};
use crate::render::FrameSize;

use super::composite_filters::{
    composite_filter_complex, composite_overlay_thread_queue_size, cuda_display_metadata_filter,
    format_seconds_arg, normalize_source_rotation, qsv_overlay_cpu_rotation_filter,
    source_rotation_filter,
};
use super::composite_profiles::composite_profile;

/// Profile-specific FFmpeg settings for composite encoding.
///
/// Later phases can use this to describe hardware decoder, filter, and encoder
/// variations without changing the software default builder surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeProfile {
    pub codec_id: CompositeCodecId,
    pub cpu_cores_per_frame_worker: usize,
    pub input_args: &'static [&'static str],
    pub filter_complex: Option<&'static str>,
    pub output_args: &'static [&'static str],
}

/// Grouped FFmpeg arguments needed to spawn a composite render.
///
/// The caller is expected to concatenate these groups in order and append the
/// output path if its process-spawning architecture owns destination handling.
///
/// Composite mode currently uses three inputs:
/// - input 0: coarsely seeked source video for frame-accurate filter-side video trim
/// - input 1: raw RGBA overlay frames from stdin (`pipe:0`)
/// - input 2: coarsely seeked source media for filtered audio encoding
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeFfmpegSettings {
    pub codec_id: CompositeCodecId,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub input_2_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}

/// Keeps a small source-local preroll after the input seek so the filter graph
/// remains responsible for the exact export boundary.
///
/// The input seek is intentionally an implementation detail derived from the
/// already validated `trim_start`; it is not part of the render config.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompositeInputWindow {
    seek_start: f64,
    filter_start: f64,
}

const COMPOSITE_INPUT_SEEK_PREROLL_SECONDS: f64 = 5.0;

fn derive_composite_input_window(trim_start: f64) -> CompositeInputWindow {
    let seek_start = (trim_start - COMPOSITE_INPUT_SEEK_PREROLL_SECONDS).max(0.0);
    CompositeInputWindow {
        seek_start,
        filter_start: trim_start - seek_start,
    }
}

impl CompositeFfmpegSettings {
    /// Returns the canonical FFmpeg argv sequence for this composite encode.
    pub fn command_args(&self, output_path: &Path) -> Vec<String> {
        let mut args = vec!["-loglevel".to_string(), "info".to_string()];
        args.extend(self.input_0_args.iter().cloned());
        args.extend(self.input_1_args.iter().cloned());
        args.extend(self.input_2_args.iter().cloned());
        args.extend(["-filter_complex".to_string(), self.filter_complex.clone()]);
        args.extend(self.output_args.iter().cloned());
        args.push(output_path.to_string_lossy().into_owned());
        args
    }
}

/// Builds FFmpeg argument groups from the canonical composite render plan.
///
/// Input 0 uses a coarse input seek followed by frame-accurate filter-side
/// video trimming. Input 1 is the raw RGBA overlay stream on `pipe:0`, and
/// input 2 uses the same derived source window for filtered audio encoding.
///
/// The function consumes the validated render plan, selects and configures an encoder profile,
/// then assembles four argument groups that callers concatenate in order: HW
/// init args, input 0 (seeked and filter-trimmed video), input 1 (overlay pipe),
/// input 2 (seeked and filter-trimmed audio source), filter_complex, and output
/// args.
pub fn build_composite_ffmpeg_settings(
    render: &CompositeRenderPlan,
    frame_size: FrameSize,
    include_audio: bool,
    source_rotation_degrees: Option<i32>,
) -> CoreResult<CompositeFfmpegSettings> {
    let FrameSize { width, height } = frame_size;
    // Establish one canonical rotation value before profile selection and reuse
    // it for filters, dimensions, autorotation, and output metadata.
    let source_rotation_degrees = normalize_source_rotation(source_rotation_degrees)?;
    // ── PHASE 1: VALIDATE INPUTS & REDUCE FPS ──
    let video_path = render.video_path.to_string_lossy().into_owned();

    // ── PHASE 2: SELECT & CONFIGURE PROFILE ──
    let mut selected_profile = composite_profile(render.requested_codec_id);
    let source_rotation_filter = source_rotation_filter(
        source_rotation_degrees,
        selected_profile.codec_id.metadata().filter_stack_kind,
    );
    if source_rotation_filter.is_some() {
        let has_gpu_rotation = matches!(
            selected_profile.codec_id.metadata().filter_stack_kind,
            CompositeFilterStackKind::CudaOverlay
                | CompositeFilterStackKind::QsvFullOverlay
                | CompositeFilterStackKind::VaapiOverlay
        );
        if !has_gpu_rotation {
            if let Some(fallback_profile) = selected_profile.codec_id.metadata().fallback_profile {
                log::info!(
                    "Composite source rotation requires CPU orientation filters; using {} instead of {}",
                    fallback_profile.metadata().profile_name,
                    selected_profile.codec_id.metadata().profile_name
                );
                selected_profile = composite_profile(fallback_profile);
            }
        }
    }
    if matches!(
        selected_profile.codec_id.metadata().filter_stack_kind,
        CompositeFilterStackKind::QsvFullOverlay
    ) {
        if render.qsv_full_init_args.is_empty() {
            return Err(CoreError::Encode(format!(
                "Requested experimental QSV overlay profile {} is unavailable; codec detection did not provide working QSV hardware-device init args.",
                selected_profile.codec_id.metadata().profile_name
            )));
        }
    }

    let filter_stack_kind = selected_profile.codec_id.metadata().filter_stack_kind;
    let qsv_full_overlay = matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay);
    let qsv_overlay_cpu_rotation_filter =
        qsv_overlay_cpu_rotation_filter(source_rotation_degrees, filter_stack_kind);
    let input_window = derive_composite_input_window(render.trim_start);

    // ── PHASE 3: BUILD INPUT 0 ARGS (coarse seek plus filter-side trim) ──
    let mut input_0_args = if matches!(
        selected_profile.codec_id.metadata().filter_stack_kind,
        CompositeFilterStackKind::QsvFullOverlay
    ) {
        render.qsv_full_init_args.clone()
    } else {
        selected_profile
            .input_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect()
    };
    if input_window.seek_start > 0.0 {
        input_0_args.extend([
            "-ss".to_string(),
            format_seconds_arg(input_window.seek_start),
        ]);
    }
    // QSV-full must retain coded main-video surfaces for zero-copy compositing.
    // Other profiles keep their existing behavior: disable FFmpeg autorotation
    // only when the selected graph physically applies a rotation filter.
    let source_autorotate_arg = if qsv_full_overlay || source_rotation_filter.is_some() {
        "-noautorotate"
    } else {
        "-autorotate"
    };
    input_0_args.extend([
        source_autorotate_arg.to_string(),
        "-i".to_string(),
        video_path.clone(),
    ]);

    // ── PHASE 4: BUILD INPUT 1 ARGS (raw RGBA overlay via stdin pipe) ──
    let overlay_thread_queue_size = composite_overlay_thread_queue_size(width, height).to_string();
    let input_1_args = vec![
        "-thread_queue_size".to_string(),
        overlay_thread_queue_size,
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{width}x{height}", width = width, height = height),
        "-r".to_string(),
        render.overlay_pipe_fps.ffmpeg_arg(),
        "-i".to_string(),
        "pipe:0".to_string(),
    ];

    // ── PHASE 5: BUILD INPUT 2 ARGS (coarse seek plus filter-side audio trim) ──
    let mut input_2_args = Vec::new();
    if include_audio {
        if input_window.seek_start > 0.0 {
            input_2_args.extend([
                "-ss".to_string(),
                format_seconds_arg(input_window.seek_start),
            ]);
        }
        input_2_args.extend(["-i".to_string(), video_path]);
    }

    // ── PHASE 6: BUILD FILTER COMPLEX (video trim + scale + overlay + format) ──
    let mut filter_complex = composite_filter_complex(
        width,
        height,
        input_window.filter_start,
        render.render_duration,
        selected_profile,
        source_rotation_degrees,
        source_rotation_filter,
        qsv_overlay_cpu_rotation_filter,
    )?;
    if include_audio {
        filter_complex.push_str(&format!(
            ";[2:a]atrim=start={}:duration={},asetpts=N/SR/TB[aout]",
            format_seconds_arg(input_window.filter_start),
            format_seconds_arg(render.render_duration),
        ));
    }

    // ── PHASE 7: BUILD OUTPUT ARGS (map, codecs, bitrate, audio encode, mux flags) ──
    let mut output_args = vec!["-map".to_string(), "[out]".to_string()];
    if include_audio {
        output_args.extend(["-map".to_string(), "[aout]".to_string()]);
    }
    output_args.extend(["-r".to_string(), render.source_fps.ffmpeg_arg()]);
    output_args.extend([
        "-c:v".to_string(),
        selected_profile
            .codec_id
            .metadata()
            .encoder_id
            .metadata()
            .ffmpeg_name
            .to_string(),
    ]);
    output_args.extend(
        selected_profile
            .output_args
            .iter()
            .map(|arg| (*arg).to_string()),
    );
    if let Some(metadata_filter) =
        cuda_display_metadata_filter(selected_profile.codec_id, width, height)
    {
        output_args.extend(["-bsf:v".to_string(), metadata_filter]);
    }
    output_args.extend(["-b:v".to_string(), render.bitrate.clone()]);
    if include_audio {
        output_args.extend([
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
        ]);
    }
    output_args.extend(["-movflags".to_string(), "faststart".to_string()]);
    // Physical-rotation profiles clear the now-stale display matrix. QSV-full
    // leaves it untouched so players rotate the completed coded composite.
    if !qsv_full_overlay {
        output_args.extend(["-metadata:s:v:0".to_string(), "rotate=0".to_string()]);
    }
    output_args.push("-y".to_string());

    Ok(CompositeFfmpegSettings {
        codec_id: selected_profile.codec_id,
        input_0_args,
        input_1_args,
        input_2_args,
        filter_complex,
        output_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::fps::Fps;

    fn assert_argument_pair(args: &[String], flag: &str, value: &str) {
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == flag && pair[1] == value),
            "missing argument pair {flag} {value}: {args:?}"
        );
    }

    #[test]
    fn rotated_cuda_profiles_use_post_orientation_dimensions_for_metadata() {
        for (codec_id, expected_metadata) in [
            (
                CompositeCodecId::NnvgpuH264,
                "h264_metadata=crop_right=8:crop_bottom=0",
            ),
            (
                CompositeCodecId::NnvgpuHevc,
                "hevc_metadata=width=1080:height=1920",
            ),
        ] {
            let render = CompositeRenderPlan {
                video_path: "rotated-landscape.mp4".into(),
                bitrate: "60M".to_string(),
                sync_offset: 0.0,
                trim_start: 0.0,
                render_duration: 1.0,
                update_rate: std::num::NonZeroU32::MIN,
                source_fps: Fps::new(30, 1).unwrap(),
                overlay_pipe_fps: Fps::new(30, 1).unwrap(),
                overlay_frame_count: 30,
                output_frame_count: 30,
                activity_overlap_duration: 1.0,
                blank_leading_frame_count: 0,
                requested_codec_id: codec_id,
                qsv_full_init_args: Vec::new(),
            };
            let settings = build_composite_ffmpeg_settings(
                &render,
                FrameSize {
                    width: 1080,
                    height: 1920,
                },
                true,
                Some(90),
            )
            .unwrap();

            assert!(settings
                .filter_complex
                .contains("transpose_cuda=2,sidedata=mode=delete:type=DISPLAYMATRIX,scale_cuda"));
            assert_argument_pair(&settings.output_args, "-bsf:v", expected_metadata);
        }
    }
}
