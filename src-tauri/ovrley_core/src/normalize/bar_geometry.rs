//! Shared segmented-track geometry validation.

use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::types::TrackFillStyle;

const MIN_BAR_PX: f32 = 2.0;

pub(crate) fn arc_track_radius(
    frame_size: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> f32 {
    (frame_size * 0.5 - track_thickness * 0.5 - border_thickness).max(0.0)
}

pub(crate) fn corner_track_cap_padding(
    frame_size: f32,
    track_thickness: f32,
    corner_radius: f32,
    border_thickness: f32,
) -> f32 {
    (corner_radius.clamp(0.0, track_thickness * 0.5) + border_thickness).min(frame_size)
}

pub(crate) fn corner_track_radius(
    frame_size: f32,
    track_thickness: f32,
    corner_radius: f32,
    border_thickness: f32,
) -> f32 {
    let cap_padding =
        corner_track_cap_padding(frame_size, track_thickness, corner_radius, border_thickness);
    (frame_size - cap_padding - track_thickness * 0.5 - border_thickness).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedBarGeometry {
    pub count: u32,
    pub gap: f32,
    pub extent: f32,
}

pub(crate) fn track_corner_radius_max(
    cross_extent: f32,
    track_span: f32,
    bar_geometry: Option<&ResolvedBarGeometry>,
) -> f32 {
    let along_extent = bar_geometry.map_or(track_span, |geometry| geometry.extent);
    (cross_extent.min(along_extent) * 0.5).floor()
}

pub(crate) fn scale_bar_geometry(
    geometry: Option<ResolvedBarGeometry>,
    scale: f32,
) -> Option<ResolvedBarGeometry> {
    match geometry {
        Some(geometry) => Some(ResolvedBarGeometry {
            count: geometry.count,
            gap: geometry.gap * scale,
            extent: geometry.extent * scale,
        }),
        None => None,
    }
}

fn resolve_bar_geometry(
    span: f32,
    count: u32,
    requested_gap: f32,
    field_path: &str,
) -> CoreResult<ResolvedBarGeometry> {
    if span < MIN_BAR_PX {
        return Err(CoreError::Config(format!(
            "{field_path}: track span must be at least {MIN_BAR_PX}px, got {span}"
        )));
    }
    if count == 0 {
        return Err(CoreError::Config(format!(
            "{field_path}.bar_count: must be >= 1"
        )));
    }
    if requested_gap < 0.0 {
        return Err(CoreError::Config(format!(
            "{field_path}.bar_gap: must be >= 0"
        )));
    }
    let max_count = (span / MIN_BAR_PX).floor() as u32;
    if count > max_count {
        return Err(CoreError::Config(format!(
            "{field_path}.bar_count: {count} bars do not fit in {span}px"
        )));
    }

    if count == 1 {
        return Ok(ResolvedBarGeometry {
            count,
            gap: 0.0,
            extent: span,
        });
    }

    let max_gap = (span - count as f32 * MIN_BAR_PX) / (count - 1) as f32;
    let gap = requested_gap.clamp(0.0, max_gap);
    let extent = (span - (count - 1) as f32 * gap) / count as f32;
    Ok(ResolvedBarGeometry { count, gap, extent })
}

pub(crate) fn resolve_bar_style_geometry(
    style: TrackFillStyle,
    span: f32,
    count_override: Option<u32>,
    gap_override: Option<f32>,
    field_path: &str,
) -> CoreResult<Option<ResolvedBarGeometry>> {
    match style {
        TrackFillStyle::Fill => Ok(None),
        TrackFillStyle::Bars => {
            let count = count_override.ok_or_else(|| {
                CoreError::Config(format!("{field_path}.bar_count: required in bars mode"))
            })?;
            let gap = gap_override.ok_or_else(|| {
                CoreError::Config(format!("{field_path}.bar_gap: required in bars mode"))
            })?;
            Ok(Some(resolve_bar_geometry(span, count, gap, field_path)?))
        }
    }
}
