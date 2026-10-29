//! Rounded annular-sector paths for segmented arc gauges.
//!
//! Each bar remains an annular wedge: its long edges follow concentric arcs,
//! while four circular fillets round the radial ends. The requested radius is
//! fitted to both the track thickness and the available angular span so narrow
//! segments cannot self-intersect.

use super::super::track_path::TRACK_PATH_EPSILON;
use super::geometry::{arc_point, ArcGaugeGeometry};
use super::path::append_circular_arc;
use crate::widgets::gauges::{
    bar_geometry::ResolvedBarGeometry, validation::MAX_ARC_ANGLE_DEGREES,
};
use skia_safe::{Canvas, Paint, Path, PathBuilder, PathFillType, Point};

#[derive(Clone, Copy, Debug)]
/// Circular sub-arc used to assemble one rounded wedge contour.
pub(crate) struct CircularArc {
    pub(crate) center_x: f32,
    pub(crate) center_y: f32,
    pub(crate) radius: f32,
    pub(crate) start_angle: f32,
    pub(crate) sweep_angle: f32,
}

#[derive(Clone, Copy, Debug)]
/// Complete contour recipe for a rounded annular segment.
pub(crate) struct RoundedSegmentPathGeometry {
    pub(crate) outer_arc: CircularArc,
    pub(crate) end_outer_fillet: CircularArc,
    pub(crate) end_inner_side: Point,
    pub(crate) end_inner_fillet: CircularArc,
    pub(crate) inner_arc: CircularArc,
    pub(crate) start_inner_fillet: CircularArc,
    pub(crate) start_outer_side: Point,
    pub(crate) start_outer_fillet: CircularArc,
}

/// Resolves the configured bar extent and gap into individual arc geometries.
pub(crate) fn segment_geometries(
    track: ArcGaugeGeometry,
    bars: ResolvedBarGeometry,
) -> Vec<ArcGaugeGeometry> {
    let direction = track.sweep_angle.signum();
    let mut segments = Vec::with_capacity(bars.count as usize);
    for index in 0..bars.count {
        let segment_start = index as f32 * (bars.extent + bars.gap);
        segments.push(ArcGaugeGeometry {
            start_angle: track.start_angle
                + direction * (segment_start / track.radius).to_degrees(),
            sweep_angle: direction * (bars.extent / track.radius).to_degrees(),
            ..track
        });
    }
    segments
}

/// Paints one rounded wedge when its geometry has a visible sweep.
pub(crate) fn draw_segment(
    canvas: &Canvas,
    geometry: ArcGaugeGeometry,
    thickness: f32,
    corner_radius: f32,
    paint: &Paint,
) {
    if let Some(path) = rounded_segment_path(geometry, thickness, corner_radius) {
        canvas.draw_path(&path, paint);
    }
}

/// Returns a geometry inset by `border_thickness` on all sides for
/// inward-border drawing. Angular boundaries are shifted at the
/// centerline radius, and radial thickness is handled separately.
pub(crate) fn inner_segment_geometry(
    geometry: ArcGaugeGeometry,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let direction = geometry.sweep_angle.signum();
    let max_angle_offset_deg = geometry.sweep_angle.abs() / 2.0;
    let angle_offset_deg = if border_thickness > 0.0 && geometry.radius > 0.0 {
        ((border_thickness / geometry.radius).to_degrees()).min(max_angle_offset_deg)
    } else {
        0.0
    };
    ArcGaugeGeometry {
        start_angle: geometry.start_angle + direction * angle_offset_deg,
        sweep_angle: geometry.sweep_angle - direction * angle_offset_deg * 2.0,
        ..geometry
    }
}

fn circular_arc(
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
    sweep_angle: f32,
) -> CircularArc {
    CircularArc {
        center_x,
        center_y,
        radius,
        start_angle,
        sweep_angle,
    }
}

fn append_arc(path: &mut PathBuilder, arc: CircularArc) {
    append_circular_arc(
        path,
        arc.center_x,
        arc.center_y,
        arc.radius,
        arc.start_angle,
        arc.sweep_angle,
    );
}

fn fitted_corner_radius(
    outer_radius: f32,
    inner_radius: f32,
    sweep_magnitude: f32,
    requested_radius: f32,
) -> f32 {
    let half_sweep_sine = (sweep_magnitude.min(180.0) * 0.5).to_radians().sin();
    let outer_limit = outer_radius * half_sweep_sine / (1.0 + half_sweep_sine);
    let inner_limit = if half_sweep_sine == 1.0 {
        f32::INFINITY
    } else {
        inner_radius * half_sweep_sine / (1.0 - half_sweep_sine)
    };
    requested_radius
        .min((outer_radius - inner_radius) * 0.5)
        .min(outer_limit)
        .min(inner_limit)
}

/// Constructs the concentric arcs, radial sides, and four corner fillets for a bar.
pub(crate) fn rounded_segment_geometry(
    geometry: ArcGaugeGeometry,
    track_thickness: f32,
    corner_radius: f32,
) -> RoundedSegmentPathGeometry {
    let sweep_magnitude = geometry.sweep_angle.abs();
    let direction = geometry.sweep_angle.signum();
    let half_thickness = track_thickness * 0.5;
    let outer_radius = geometry.radius + half_thickness;
    let inner_radius = geometry.radius - half_thickness;
    let fillet_radius =
        fitted_corner_radius(outer_radius, inner_radius, sweep_magnitude, corner_radius);
    let end_angle = geometry.start_angle + geometry.sweep_angle;
    let outer_fillet_center_radius = outer_radius - fillet_radius;
    let inner_fillet_center_radius = inner_radius + fillet_radius;
    let outer_inset_angle = (fillet_radius / outer_fillet_center_radius)
        .asin()
        .to_degrees();
    let inner_inset_angle = (fillet_radius / inner_fillet_center_radius)
        .asin()
        .to_degrees();
    let outer_side_radius = (outer_fillet_center_radius.powi(2) - fillet_radius.powi(2)).sqrt();
    let inner_side_radius = (inner_fillet_center_radius.powi(2) - fillet_radius.powi(2)).sqrt();
    let outer_start_angle = geometry.start_angle + direction * outer_inset_angle;
    let outer_end_angle = end_angle - direction * outer_inset_angle;
    let inner_end_angle = end_angle - direction * inner_inset_angle;
    let inner_start_angle = geometry.start_angle + direction * inner_inset_angle;
    let end_outer_center = arc_point(
        geometry.center_x,
        geometry.center_y,
        outer_fillet_center_radius,
        outer_end_angle,
    );
    let end_inner_center = arc_point(
        geometry.center_x,
        geometry.center_y,
        inner_fillet_center_radius,
        inner_end_angle,
    );
    let start_inner_center = arc_point(
        geometry.center_x,
        geometry.center_y,
        inner_fillet_center_radius,
        inner_start_angle,
    );
    let start_outer_center = arc_point(
        geometry.center_x,
        geometry.center_y,
        outer_fillet_center_radius,
        outer_start_angle,
    );

    RoundedSegmentPathGeometry {
        outer_arc: circular_arc(
            geometry.center_x,
            geometry.center_y,
            outer_radius,
            outer_start_angle,
            direction * (sweep_magnitude - outer_inset_angle * 2.0),
        ),
        end_outer_fillet: circular_arc(
            end_outer_center.x,
            end_outer_center.y,
            fillet_radius,
            outer_end_angle,
            direction * (90.0 + outer_inset_angle),
        ),
        end_inner_side: arc_point(
            geometry.center_x,
            geometry.center_y,
            inner_side_radius,
            end_angle,
        ),
        end_inner_fillet: circular_arc(
            end_inner_center.x,
            end_inner_center.y,
            fillet_radius,
            end_angle + direction * 90.0,
            direction * (90.0 - inner_inset_angle),
        ),
        inner_arc: circular_arc(
            geometry.center_x,
            geometry.center_y,
            inner_radius,
            inner_end_angle,
            -direction * (sweep_magnitude - inner_inset_angle * 2.0),
        ),
        start_inner_fillet: circular_arc(
            start_inner_center.x,
            start_inner_center.y,
            fillet_radius,
            inner_start_angle + direction * 180.0,
            direction * (90.0 - inner_inset_angle),
        ),
        start_outer_side: arc_point(
            geometry.center_x,
            geometry.center_y,
            outer_side_radius,
            geometry.start_angle,
        ),
        start_outer_fillet: circular_arc(
            start_outer_center.x,
            start_outer_center.y,
            fillet_radius,
            geometry.start_angle - direction * 90.0,
            direction * (90.0 + outer_inset_angle),
        ),
    }
}

/// Builds a closed rounded wedge, falling back to the ordinary annular path
/// when rounding is absent or the segment is a complete circle.
pub(crate) fn rounded_segment_path(
    geometry: ArcGaugeGeometry,
    track_thickness: f32,
    corner_radius: f32,
) -> Option<Path> {
    let sweep_magnitude = geometry.sweep_angle.abs();
    if sweep_magnitude == 0.0 {
        return None;
    }
    if corner_radius <= TRACK_PATH_EPSILON
        || sweep_magnitude >= MAX_ARC_ANGLE_DEGREES - TRACK_PATH_EPSILON
    {
        return super::path::ArcTrackSpec::full(geometry, track_thickness, 0.0).filled_path();
    }

    let contour = rounded_segment_geometry(geometry, track_thickness, corner_radius);
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    path.move_to(arc_point(
        contour.outer_arc.center_x,
        contour.outer_arc.center_y,
        contour.outer_arc.radius,
        contour.outer_arc.start_angle,
    ));
    append_arc(&mut path, contour.outer_arc);
    append_arc(&mut path, contour.end_outer_fillet);
    path.line_to(contour.end_inner_side);
    append_arc(&mut path, contour.end_inner_fillet);
    append_arc(&mut path, contour.inner_arc);
    append_arc(&mut path, contour.start_inner_fillet);
    path.line_to(contour.start_outer_side);
    append_arc(&mut path, contour.start_outer_fillet);
    path.close();
    Some(path.detach())
}

#[cfg(test)]
mod tests {
    use crate::widgets::gauges::arc::{
        rounded_segment_geometry, rounded_segment_path, segment_geometries, ArcGaugeGeometry,
    };
    use crate::widgets::gauges::bar_geometry::ResolvedBarGeometry;
    use crate::widgets::gauges::track_path::TRACK_PATH_EPSILON;

    fn test_geometry() -> ArcGaugeGeometry {
        ArcGaugeGeometry {
            center_x: 80.0,
            center_y: 80.0,
            inner_widget_center_x: 80.0,
            inner_widget_center_y: 80.0,
            radius: 64.0,
            start_angle: 180.0,
            sweep_angle: 12.0,
        }
    }

    #[test]
    fn rounded_segment_retains_annular_outer_and_inner_arcs() {
        let geometry = rounded_segment_geometry(test_geometry(), 12.0, 6.0);

        assert!(geometry.outer_arc.sweep_angle > 1.0);
        assert!(geometry.inner_arc.sweep_angle < -1.0);
        assert!((geometry.outer_arc.radius - 70.0).abs() < TRACK_PATH_EPSILON);
        assert!((geometry.inner_arc.radius - 58.0).abs() < TRACK_PATH_EPSILON);
        assert!(rounded_segment_path(test_geometry(), 12.0, 6.0).is_some());
    }

    #[test]
    fn segments_use_the_full_resolved_bar_extent() {
        let track = ArcGaugeGeometry {
            sweep_angle: 180.0,
            radius: 50.0,
            ..test_geometry()
        };
        let bars = ResolvedBarGeometry {
            count: 4,
            gap: 3.0,
            extent: 10.0,
        };
        let segments = segment_geometries(track, bars);

        assert_eq!(segments.len(), 4);
        assert!((segments[0].start_angle - track.start_angle).abs() < f32::EPSILON);
        assert!(
            (segments[0].sweep_angle - (bars.extent / track.radius).to_degrees()).abs()
                < f32::EPSILON
        );
    }
}
