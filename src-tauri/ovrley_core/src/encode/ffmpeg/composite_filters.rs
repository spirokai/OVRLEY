//! Composite filter-graph and input-validation helpers.

use crate::encode::ffmpeg::catalog::{CompositeCodecId, CompositeFilterStackKind};
use crate::error::{CoreError, CoreResult};

use super::composite::CompositeProfile;

const CUDA_FRAME_ALIGNMENT: u32 = 32;

/// Converts probed display rotation into the canonical quarter-turn contract
/// consumed by every command-building decision.
///
/// Missing and zero-degree metadata require no transform. Other full turns are
/// reduced to 90, 180, or 270 degrees; a present non-quarter angle is malformed
/// for the supported filter graphs and must fail before any arguments are built.
pub(super) fn normalize_source_rotation(rotation_degrees: Option<i32>) -> CoreResult<Option<i32>> {
    match rotation_degrees.map(|degrees| degrees.rem_euclid(360)) {
        None | Some(0) => Ok(None),
        Some(degrees @ (90 | 180 | 270)) => Ok(Some(degrees)),
        Some(degrees) => Err(CoreError::Encode(format!(
            "Unsupported source rotation {degrees} degrees; expected a multiple of 90 degrees"
        ))),
    }
}

/// Returns codec-specific metadata that hides CUDA's aligned frame overhang.
///
/// Composite dimensions are display-oriented before this point. CUDA and CPU
/// profiles physically orient the main video before scaling, while the full
/// QSV profile keeps the main surface in coded orientation and preserves its
/// display matrix.
pub(super) fn cuda_display_metadata_filter(
    codec_id: CompositeCodecId,
    display_width: u32,
    display_height: u32,
) -> Option<String> {
    match codec_id {
        CompositeCodecId::NnvgpuH264 => {
            let crop_right = cuda_frame_overhang(display_width);
            let crop_bottom = cuda_frame_overhang(display_height);
            (crop_right != 0 || crop_bottom != 0)
                .then(|| format!("h264_metadata=crop_right={crop_right}:crop_bottom={crop_bottom}"))
        }
        CompositeCodecId::NnvgpuHevc => Some(format!(
            "hevc_metadata=width={display_width}:height={display_height}"
        )),
        _ => None,
    }
}

fn cuda_frame_overhang(dimension: u32) -> u32 {
    (CUDA_FRAME_ALIGNMENT - dimension % CUDA_FRAME_ALIGNMENT) % CUDA_FRAME_ALIGNMENT
}

/// Builds the selected profile's composite filter graph.
///
/// Profile templates use `{base_video_filters}`, `{width}`, `{height}`,
/// `{main_width}`, `{main_height}`, and `{qsv_overlay_cpu_rotation_filter}`
/// placeholders. The QSV main dimensions are derived here: they match the
/// display dimensions except for a 90°/270° QSV source, where coded video has
/// the inverse width/height pair. The base-video filter chain owns exact video
/// trimming so the decoded frame boundary matches the render plan more closely
/// than input-side seek alone.
pub(super) fn composite_filter_complex(
    width: u32,
    height: u32,
    video_trim_start: f64,
    render_duration: f64,
    profile: &CompositeProfile,
    source_rotation_degrees: Option<i32>,
    source_rotation_filter: Option<&'static str>,
    qsv_overlay_cpu_rotation_filter: Option<&'static str>,
) -> CoreResult<String> {
    let template = profile
        .filter_complex
        .as_deref()
        .expect("composite profile catalog defines a filter graph");
    let mut base_video_filters = format!(
        "trim=start={}:end={},setpts=PTS-STARTPTS,",
        format_seconds_arg(video_trim_start),
        format_seconds_arg(video_trim_start + render_duration),
    );
    if let Some(rotation_filter) = source_rotation_filter {
        base_video_filters.push_str(rotation_filter);
        base_video_filters.push_str("sidedata=mode=delete:type=DISPLAYMATRIX,");
    }
    let (main_width, main_height) = if matches!(
        profile.codec_id.metadata().filter_stack_kind,
        CompositeFilterStackKind::QsvFullOverlay
    ) && matches!(source_rotation_degrees, Some(90 | 270))
    {
        // The RGBA pipe is rendered in display dimensions. With QSV main
        // frames left coded, the main target dimensions are the inverse pair.
        (height, width)
    } else {
        (width, height)
    };
    Ok(template
        .replace("{base_video_filters}", &base_video_filters)
        .replace("{width}", &width.to_string())
        .replace("{height}", &height.to_string())
        .replace("{main_width}", &main_width.to_string())
        .replace("{main_height}", &main_height.to_string())
        .replace(
            "{qsv_overlay_cpu_rotation_filter}",
            qsv_overlay_cpu_rotation_filter.unwrap_or_default(),
        ))
}

pub(super) fn source_rotation_filter(
    rotation_degrees: Option<i32>,
    filter_stack_kind: CompositeFilterStackKind,
) -> Option<&'static str> {
    // QSV VPP can accept transpose syntax while silently skipping the
    // requested rotation on some DXVA2/oneVPL combinations. Keep the main QSV
    // surface coded and rotate only the RGBA overlay instead; its display
    // matrix is preserved for the output container.
    if matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay) {
        return None;
    }

    match rotation_degrees {
        Some(90) => {
            if matches!(filter_stack_kind, CompositeFilterStackKind::CudaOverlay) {
                Some("transpose_cuda=2,")
            } else if matches!(filter_stack_kind, CompositeFilterStackKind::VaapiOverlay) {
                Some("transpose_vaapi=2,")
            } else {
                Some("transpose=2,")
            }
        }
        Some(180) => {
            if matches!(filter_stack_kind, CompositeFilterStackKind::CudaOverlay) {
                Some("transpose_cuda=4,")
            } else if matches!(filter_stack_kind, CompositeFilterStackKind::VaapiOverlay) {
                Some("transpose_vaapi=4,")
            } else {
                Some("hflip,vflip,")
            }
        }
        Some(270) => {
            if matches!(filter_stack_kind, CompositeFilterStackKind::CudaOverlay) {
                Some("transpose_cuda=1,")
            } else if matches!(filter_stack_kind, CompositeFilterStackKind::VaapiOverlay) {
                Some("transpose_vaapi=1,")
            } else {
                Some("transpose=1,")
            }
        }
        _ => None,
    }
}

/// Returns the CPU filter that maps a display-oriented RGBA overlay into the
/// coded orientation retained by the full QSV video leg.
///
/// A source display rotation describes how players rotate coded video. The
/// overlay therefore needs the inverse transform before `hwupload` so
/// `overlay_qsv` composites matching coded pixels. This intentionally uses
/// ordinary software filters: unlike the main QSV surface, the RGBA overlay is
/// already system-memory data, and QSV VPP transpose has been observed to
/// skip or misapply several modes on DXVA2-backed runtimes.
pub(super) fn qsv_overlay_cpu_rotation_filter(
    rotation_degrees: Option<i32>,
    filter_stack_kind: CompositeFilterStackKind,
) -> Option<&'static str> {
    if !matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay) {
        return None;
    }

    match rotation_degrees {
        Some(90) => Some("transpose=1,"),
        Some(180) => Some("hflip,vflip,"),
        Some(270) => Some("transpose=2,"),
        _ => None,
    }
}

/// Formats seconds for FFmpeg while trimming insignificant decimal zeros.
///
/// This keeps integer trim values readable as `10` while preserving fractional
/// durations when callers pass non-integer values.
pub(super) fn format_seconds_arg(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        return format!("{}", value.trunc() as i64);
    }
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
