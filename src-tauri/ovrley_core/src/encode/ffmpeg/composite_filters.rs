//! Composite filter-graph and input-validation helpers.

use crate::encode::ffmpeg::catalog::{CompositeCodecId, CompositeFilterStackKind};
use crate::error::{CoreError, CoreResult};
use crate::render::{AtlasPlan, AtlasSlot, FrameSize};
use skia_safe::IRect;

use super::composite_profiles::CompositeProfile;

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
/// `{main_width}`, `{main_height}`, `{atlas_slot_crops}` and
/// `{atlas_slot_overlays}` placeholders. The overlay input is the `atlas`
/// frame: every graph crops each of its slots and overlays it at the slot's
/// position on the main video. The QSV main dimensions are derived here: they
/// match the display dimensions except for a 90°/270° QSV source, where coded
/// video has the inverse width/height pair. The base-video filter chain owns
/// exact video trimming so the decoded frame boundary matches the render plan
/// more closely than input-side seek alone. `main_colorspace_tag` labels an
/// untagged main video (metadata only), so the overlay blends in, and the
/// output carries, the colour space players assume for it.
#[allow(clippy::too_many_arguments)]
pub(super) fn composite_filter_complex(
    width: u32,
    height: u32,
    atlas: &AtlasPlan,
    video_trim_start: f64,
    render_duration: f64,
    profile: &CompositeProfile,
    source_rotation_degrees: Option<i32>,
    source_rotation_filter: Option<&'static str>,
    qsv_overlay_cpu_rotation_filter: Option<&'static str>,
    main_colorspace_tag: Option<&'static str>,
) -> String {
    let template = profile
        .filter_complex
        .as_deref()
        .expect("composite profile catalog defines a filter graph");
    let filter_stack_kind = profile.codec_id.metadata().filter_stack_kind;
    let (atlas_slot_crops, atlas_slot_overlays) = atlas_slot_filters(
        atlas,
        FrameSize { width, height },
        filter_stack_kind,
        source_rotation_degrees,
        qsv_overlay_cpu_rotation_filter,
    );
    let mut base_video_filters = main_colorspace_tag
        .map(|tag| format!("setparams=colorspace={tag},"))
        .unwrap_or_default();
    base_video_filters.push_str(&format!(
        "trim=start={}:end={},setpts=PTS-STARTPTS,",
        format_seconds_arg(video_trim_start),
        format_seconds_arg(video_trim_start + render_duration),
    ));
    if let Some(rotation_filter) = source_rotation_filter {
        base_video_filters.push_str(rotation_filter);
        base_video_filters.push_str("sidedata=mode=delete:type=DISPLAYMATRIX,");
    }
    let (main_width, main_height) = if matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay)
        && matches!(source_rotation_degrees, Some(90 | 270))
    {
        // The RGBA pipe is rendered in display dimensions. With QSV main
        // frames left coded, the main target dimensions are the inverse pair.
        (height, width)
    } else {
        (width, height)
    };
    template
        .replace("{base_video_filters}", &base_video_filters)
        .replace("{width}", &width.to_string())
        .replace("{height}", &height.to_string())
        .replace("{main_width}", &main_width.to_string())
        .replace("{main_height}", &main_height.to_string())
        .replace("{atlas_slot_crops}", &atlas_slot_crops)
        .replace("{atlas_slot_overlays}", &atlas_slot_overlays)
}

/// Splits the atlas into one crop per slot, labelled `[slotN]`, and chains one
/// overlay per slot onto `[base]`. Hardware overlay stacks crop on the CPU and
/// upload each crop to the GPU before overlaying. QSV full overlay first turns
/// each crop into the coded orientation of its main video with
/// `qsv_overlay_cpu_rotation_filter`. The last overlay is left unlabelled for
/// the profile's output filters. A plan without slots overlays its transparent
/// atlas whole, so the atlas stream still ends the graph as it does with slots.
fn atlas_slot_filters(
    atlas: &AtlasPlan,
    frame: FrameSize,
    filter_stack_kind: CompositeFilterStackKind,
    source_rotation_degrees: Option<i32>,
    qsv_overlay_cpu_rotation_filter: Option<&'static str>,
) -> (String, String) {
    let slot_upload = match filter_stack_kind {
        CompositeFilterStackKind::SoftwareOverlay | CompositeFilterStackKind::AmfD3d11Overlay => String::new(),
        CompositeFilterStackKind::CudaOverlay | CompositeFilterStackKind::VaapiOverlay => ",hwupload".to_string(),
        CompositeFilterStackKind::QsvFullOverlay => format!(
            ",{}hwupload=extra_hw_frames=16",
            qsv_overlay_cpu_rotation_filter.unwrap_or_default()
        ),
    };
    let slots = atlas.slots();
    if slots.is_empty() {
        return (
            format!("null{slot_upload}[slot0]"),
            format!("[base][slot0]{}", slot_overlay_filter(filter_stack_kind, 0, 0)),
        );
    }
    let mut crops = String::new();
    if slots.len() > 1 {
        crops.push_str(&format!("split={}", slots.len()));
        for index in 0..slots.len() {
            crops.push_str(&format!("[atlas{index}]"));
        }
    }
    let mut overlays = String::new();
    for (index, slot) in slots.iter().enumerate() {
        let (rect, (x, y)) = slot_placement(slot, frame, filter_stack_kind, source_rotation_degrees);
        if slots.len() > 1 {
            crops.push_str(&format!(";[atlas{index}]"));
        }
        crops.push_str(&format!(
            "crop={}:{}:{}:{}{slot_upload}[slot{index}]",
            rect.width(),
            rect.height(),
            rect.left,
            rect.top
        ));
        if index > 0 {
            overlays.push_str(&format!("[main{index}];[main{index}]"));
        } else {
            overlays.push_str("[base]");
        }
        overlays.push_str(&format!("[slot{index}]{}", slot_overlay_filter(filter_stack_kind, x, y)));
    }
    (crops, overlays)
}

/// Atlas crop of `slot` and its overlay position on the main video.
///
/// Every stack but QSV full overlay orients the main video for display, so the
/// slot lands at its scene position. QSV full overlay keeps the main video
/// coded: the slot is clipped to the frame (a planned slot can extend one
/// column or row past an odd frame edge), and its position is turned by the
/// same quarter turn `qsv_overlay_cpu_rotation_filter` applies to its tile.
fn slot_placement(
    slot: &AtlasSlot,
    frame: FrameSize,
    filter_stack_kind: CompositeFilterStackKind,
    source_rotation_degrees: Option<i32>,
) -> (IRect, (i32, i32)) {
    let rect = AtlasPlan::atlas_rect(slot);
    if !matches!(filter_stack_kind, CompositeFilterStackKind::QsvFullOverlay) {
        return (rect, (slot.scene.left, slot.scene.top));
    }
    let (frame_width, frame_height) = (frame.width as i32, frame.height as i32);
    let scene = IRect::new(
        slot.scene.left,
        slot.scene.top,
        slot.scene.right.min(frame_width),
        slot.scene.bottom.min(frame_height),
    );
    let crop = IRect::from_xywh(rect.left, rect.top, scene.width(), scene.height());
    let position = match source_rotation_degrees {
        None => (scene.left, scene.top),
        // `transpose=1` turns the display frame clockwise into coded orientation.
        Some(90) => (frame_height - scene.bottom, scene.left),
        Some(180) => (frame_width - scene.right, frame_height - scene.bottom),
        // `transpose=2` turns the display frame counter-clockwise.
        Some(270) => (scene.top, frame_width - scene.right),
        Some(degrees) => unreachable!("source rotation {degrees} is normalized to a quarter turn"),
    };
    (crop, position)
}

/// The stack's overlay filter placing one slot at `x`, `y` on the main video.
fn slot_overlay_filter(filter_stack_kind: CompositeFilterStackKind, x: i32, y: i32) -> String {
    const OVERLAY_OPTIONS: &str = "eof_action=repeat:shortest=1";
    match filter_stack_kind {
        CompositeFilterStackKind::SoftwareOverlay | CompositeFilterStackKind::AmfD3d11Overlay => {
            format!("overlay={x}:{y}:{OVERLAY_OPTIONS}")
        }
        CompositeFilterStackKind::CudaOverlay => format!("overlay_cuda={x}:{y}:{OVERLAY_OPTIONS}"),
        CompositeFilterStackKind::VaapiOverlay => format!("overlay_vaapi=x={x}:y={y}:{OVERLAY_OPTIONS}"),
        CompositeFilterStackKind::QsvFullOverlay => format!("overlay_qsv=x={x}:y={y}"),
    }
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
