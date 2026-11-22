//! Shared segmented-track geometry validation, and the limits and suggestions the gauge editors
//! derive from it (plan D2), so the inspector bounds its sliders by the geometry the renderer draws.

use crate::error::{WidgetError, WidgetResult};
use crate::model::types::TrackFillStyle;

const MIN_BAR_PX: f32 = 2.0;

pub fn arc_track_radius(
    frame_size: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> f32 {
    (frame_size * 0.5 - track_thickness * 0.5 - border_thickness).max(0.0)
}

pub fn corner_track_cap_padding(
    frame_size: f32,
    track_thickness: f32,
    corner_radius: f32,
    border_thickness: f32,
) -> f32 {
    (corner_radius.clamp(0.0, track_thickness * 0.5) + border_thickness).min(frame_size)
}

pub fn corner_track_radius(
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

pub fn track_corner_radius_max(
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
    geometry.map(|geometry| ResolvedBarGeometry {
        count: geometry.count,
        gap: geometry.gap * scale,
        extent: geometry.extent * scale,
    })
}

/// Resolve the same cap limits and bar span at ingress and after intrinsic resizing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_arc_track_geometry(
    frame_size: f32,
    thickness: f32,
    requested_corner: f32,
    border: f32,
    angle: f32,
    corner: bool,
    style: TrackFillStyle,
    count: Option<u32>,
    gap: Option<f32>,
    field: &str,
) -> WidgetResult<(f32, Option<ResolvedBarGeometry>)> {
    let initial_corner = requested_corner.min(thickness * 0.5);
    let radius = if corner {
        corner_track_radius(frame_size, thickness, initial_corner, border)
    } else {
        arc_track_radius(frame_size, thickness, border)
    };
    let bars =
        resolve_bar_style_geometry(style, angle.to_radians().abs() * radius, count, gap, field)?;
    let corner_radius =
        requested_corner.min(track_corner_radius_max(thickness, thickness, bars.as_ref()));
    let bars = if corner && corner_radius < initial_corner {
        let radius = corner_track_radius(frame_size, thickness, corner_radius, border);
        resolve_bar_style_geometry(style, angle.to_radians().abs() * radius, count, gap, field)?
    } else {
        bars
    };
    Ok((corner_radius, bars))
}

pub fn resolve_bar_geometry(
    span: f32,
    count: u32,
    requested_gap: f32,
    field_path: &str,
) -> WidgetResult<ResolvedBarGeometry> {
    if span < MIN_BAR_PX {
        return Err(WidgetError::Config(format!(
            "{field_path}: track span must be at least {MIN_BAR_PX}px, got {span}"
        )));
    }
    if count == 0 {
        return Err(WidgetError::Config(format!(
            "{field_path}.bar_count: must be >= 1"
        )));
    }
    if !requested_gap.is_finite() || requested_gap < 0.0 {
        return Err(WidgetError::Config(format!(
            "{field_path}.bar_gap: must be >= 0"
        )));
    }
    let max_count = (span / MIN_BAR_PX).floor() as u32;
    if count > max_count {
        return Err(WidgetError::Config(format!(
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
) -> WidgetResult<Option<ResolvedBarGeometry>> {
    match style {
        TrackFillStyle::Fill => Ok(None),
        TrackFillStyle::Bars => {
            let count = count_override.ok_or_else(|| {
                WidgetError::Config(format!("{field_path}.bar_count: required in bars mode"))
            })?;
            let gap = gap_override.ok_or_else(|| {
                WidgetError::Config(format!("{field_path}.bar_gap: required in bars mode"))
            })?;
            Ok(Some(resolve_bar_geometry(span, count, gap, field_path)?))
        }
    }
}

/// The arc length a gauge track spans (the bars' span).
pub fn arc_span(radius: f32, sweep_degrees: f32) -> f32 {
    sweep_degrees.to_radians().abs() * radius
}

/// `getArcTrackDimensions`: the track radius and sweep the editor bounds its controls with.
pub fn arc_track_dimensions(
    frame_size: f32,
    thickness: f32,
    corner_radius: f32,
    border: f32,
    arc_angle: f32,
    corner: bool,
) -> (f32, f32) {
    if corner {
        (corner_track_radius(frame_size, thickness, corner_radius, border), 90.0)
    } else {
        (arc_track_radius(frame_size, thickness, border), arc_angle)
    }
}

/// The widest whole-pixel gap `count` bars can leave in `span` (Bar Gap's maximum).
pub fn bar_gap_max(span: f32, count: u32) -> WidgetResult<f32> {
    let geometry = resolve_bar_geometry(span, count, 0.0, "bar gap")?;
    Ok(if geometry.count == 1 {
        0.0
    } else {
        ((span - count as f32 * MIN_BAR_PX) / (count - 1) as f32).floor()
    })
}

const SUGGESTED_BAR_GAP_PX: f32 = 4.0;
/// The bar count suggested for a linear track switching to bars.
pub const SUGGESTED_LINEAR_BAR_COUNT: u32 = 20;
/// The bar count suggested for a full circle of arc track; a corner track takes half as many again.
const SUGGESTED_ARC_BAR_COUNT_FULL: f32 = 60.0;

/// Bars suggested when a track switches to bars, and the corner radius they allow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuggestedBars {
    pub count: u32,
    pub gap: f32,
    pub corner_radius_max: f32,
}

/// `getSuggestedBarGeometry`: about `target_bars` bars (at least three where they fit), 4px apart.
pub fn suggested_bar_geometry(
    span: f32,
    cross_extent: f32,
    target_bars: u32,
) -> WidgetResult<SuggestedBars> {
    let max_count = (span / MIN_BAR_PX).floor() as u32;
    let count = max_count.min(target_bars.max(if max_count >= 3 { 3 } else { 1 }));
    let gap = SUGGESTED_BAR_GAP_PX.min(bar_gap_max(span, count)?);
    let geometry = resolve_bar_geometry(span, count, gap, "suggested bars")?;
    Ok(SuggestedBars {
        count,
        gap,
        corner_radius_max: track_corner_radius_max(cross_extent, span, Some(&geometry)),
    })
}

/// `getSuggestedArcBarGeometry`: a bar count proportional to the sweep.
pub fn suggested_arc_bar_geometry(
    radius: f32,
    sweep_degrees: f32,
    thickness: f32,
    corner: bool,
) -> WidgetResult<SuggestedBars> {
    let full = if corner {
        SUGGESTED_ARC_BAR_COUNT_FULL * 1.5
    } else {
        SUGGESTED_ARC_BAR_COUNT_FULL
    };
    let target = (sweep_degrees.abs() * (full / 360.0)).round() as u32;
    suggested_bar_geometry(arc_span(radius, sweep_degrees), thickness, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_limits_follow_the_drawn_geometry() {
        // 10 bars in 100px leave at most (100 - 20) / 9 = 8.88px, 8 whole pixels.
        assert_eq!(bar_gap_max(100.0, 10).unwrap(), 8.0);
        assert_eq!(bar_gap_max(100.0, 1).unwrap(), 0.0);
        assert!(bar_gap_max(10.0, 6).is_err(), "six bars do not fit in 10px");
        let suggested = suggested_bar_geometry(300.0, 20.0, SUGGESTED_LINEAR_BAR_COUNT).unwrap();
        assert_eq!((suggested.count, suggested.gap), (20, 4.0));
        // 20 bars of (300 - 19 * 4) / 20 = 11.2px; the radius is half the shorter side, floored.
        assert_eq!(suggested.corner_radius_max, 5.0);
        let tiny = suggested_bar_geometry(5.0, 20.0, 20).unwrap();
        assert_eq!(tiny.count, 2, "only as many bars as fit");
        let arc = suggested_arc_bar_geometry(100.0, 270.0, 10.0, false).unwrap();
        assert_eq!(arc.count, 45);
        let (radius, sweep) = arc_track_dimensions(200.0, 20.0, 4.0, 2.0, 270.0, false);
        assert_eq!((radius, sweep), (88.0, 270.0));
        let (radius, sweep) = arc_track_dimensions(200.0, 20.0, 4.0, 2.0, 270.0, true);
        assert_eq!((radius, sweep), (200.0 - 6.0 - 10.0 - 2.0, 90.0));
    }
}
