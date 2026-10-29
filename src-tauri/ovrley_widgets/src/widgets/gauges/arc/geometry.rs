//! Pure layout and polar geometry for arc-shaped gauges.
//!
//! This module resolves validated widget dimensions into one canonical center,
//! radius, and directed sweep. Both continuous tracks and segmented wedges use
//! that geometry, preventing their static and dynamic layers from drifting.

use crate::widgets::gauges::{
    bar_geometry::{arc_track_radius, corner_track_cap_padding, corner_track_radius},
    validation::{ValidatedCornerGaugeOrientation, MAX_ARC_ANGLE_DEGREES, MIN_ARC_ANGLE_DEGREES},
};
use skia_safe::Point;

const CORNER_GAUGE_DEFAULT_FRAME_SIZE: f32 = 110.0;
const CORNER_GAUGE_INNER_INSET: f32 = 22.0;

/// Arc geometry shared by static and dynamic drawing. Angles use Skia's
/// screen-space convention: 0° is right and increasing angles move clockwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcGaugeGeometry {
    /// Track-circle center on the widget-local x axis.
    pub center_x: f32,
    /// Track-circle center on the widget-local y axis.
    pub center_y: f32,
    /// Horizontal anchor for the numeric value/unit stack.
    pub inner_widget_center_x: f32,
    /// Vertical anchor for the numeric value/unit stack.
    pub inner_widget_center_y: f32,
    /// Radius measured to the track centerline.
    pub radius: f32,
    /// Start angle in Skia screen-space degrees.
    pub start_angle: f32,
    /// Signed clockwise sweep in Skia screen-space degrees.
    pub sweep_angle: f32,
}

/// Centers a validated arc sweep around the top of its frame.
pub fn arc_start_end_angles(arc_angle: f32) -> (f32, f32) {
    let angle = arc_angle.clamp(MIN_ARC_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES);
    (270.0 - angle * 0.5, 270.0 + angle * 0.5)
}

/// GT gauges open their gap on the right instead of the bottom.
pub const GT_GAUGE_TRACK_ROTATION_DEGREES: f32 = -90.0;

/// Centers a validated GT sweep around the left of its frame.
pub fn gt_start_end_angles(arc_angle: f32) -> (f32, f32) {
    let (start, end) = arc_start_end_angles(arc_angle);
    (
        start + GT_GAUGE_TRACK_ROTATION_DEGREES,
        end + GT_GAUGE_TRACK_ROTATION_DEGREES,
    )
}

/// Returns the directed quarter-circle sweep for a corner gauge.
pub fn corner_start_end_angles(orientation: ValidatedCornerGaugeOrientation) -> (f32, f32) {
    match orientation {
        ValidatedCornerGaugeOrientation::BottomLeft => (0.0, -90.0),
        ValidatedCornerGaugeOrientation::BottomRight => (180.0, 270.0),
    }
}

/// Fits the track centerline radius inside the smaller frame dimension.
pub fn arc_radius(width: f32, height: f32, track_thickness: f32, border_thickness: f32) -> f32 {
    arc_track_radius(width.min(height), track_thickness, border_thickness)
}

/// Projects a polar track coordinate into widget-local Cartesian space.
pub fn arc_point(center_x: f32, center_y: f32, radius: f32, angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(
        center_x + radius * radians.cos(),
        center_y + radius * radians.sin(),
    )
}

/// Resolves centered arc-gauge geometry from validated dimensions and style.
pub fn arc_gauge_geometry(
    width: f32,
    height: f32,
    arc_angle: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, end_angle) = arc_start_end_angles(arc_angle);
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    ArcGaugeGeometry {
        center_x,
        center_y,
        inner_widget_center_x: center_x,
        inner_widget_center_y: center_y,
        radius: arc_radius(width, height, track_thickness, border_thickness),
        start_angle,
        sweep_angle: end_angle - start_angle,
    }
}

/// Resolves arc geometry rotated into the GT gauge's fixed track orientation.
pub fn gt_gauge_geometry(
    width: f32,
    height: f32,
    arc_angle: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, _) = gt_start_end_angles(arc_angle);
    ArcGaugeGeometry {
        start_angle,
        ..arc_gauge_geometry(width, height, arc_angle, track_thickness, border_thickness)
    }
}

/// Resolves quarter-arc geometry and its inset value anchor for a corner gauge.
pub fn corner_gauge_geometry(
    width: f32,
    height: f32,
    orientation: ValidatedCornerGaugeOrientation,
    track_thickness: f32,
    track_corner_radius: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, end_angle) = corner_start_end_angles(orientation);
    let frame_size = width.min(height);
    let cap_padding = corner_track_cap_padding(
        frame_size,
        track_thickness,
        track_corner_radius,
        border_thickness,
    );
    let inner_inset = frame_size * CORNER_GAUGE_INNER_INSET / CORNER_GAUGE_DEFAULT_FRAME_SIZE;
    let is_bottom_right = orientation == ValidatedCornerGaugeOrientation::BottomRight;
    let center_x = if is_bottom_right {
        width - cap_padding
    } else {
        cap_padding
    };

    ArcGaugeGeometry {
        center_x,
        center_y: height - cap_padding,
        inner_widget_center_x: if is_bottom_right {
            width - inner_inset
        } else {
            inner_inset
        },
        inner_widget_center_y: height - inner_inset,
        radius: corner_track_radius(
            frame_size,
            track_thickness,
            track_corner_radius,
            border_thickness,
        ),
        start_angle,
        sweep_angle: end_angle - start_angle,
    }
}
