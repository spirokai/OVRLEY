//! Continuous-track path geometry shared by arc-shaped gauges.
//!
//! Arc bodies require circular offset curves and endpoint fillets. Their
//! translated low-fill cap is shared with linear gauges.

use super::super::track_path::{
    append_track_fillet, translated_track_cap_path, translated_track_cap_reveal, TrackPathFrame,
    TranslatedTrackCap, TRACK_PATH_EPSILON,
};

use super::geometry::{arc_point, ArcGaugeGeometry};
use crate::widgets::gauges::validation::MAX_ARC_ANGLE_DEGREES;
use skia_safe::{Path, PathBuilder, PathFillType, Point};

/// Geometry used to build the moving reveal clip for a continuous arc track.
/// The immutable source track is the rounded annular sector from `segment`;
/// angles here follow Skia's clockwise screen-space convention.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ArcTrackSpec {
    pub geometry: ArcGaugeGeometry,
    pub sweep_angle: f32,
    pub thickness: f32,
    pub start_corner_radius: f32,
    pub end_corner_radius: f32,
}

/// Reveal clip geometry produced by [`ArcTrackSpec::reveal_clip`]. Below the
/// low-fill threshold the clip is a translated rounded rectangle whose
/// intersection with the annular source track grows from its start edge
/// (Option B); above the threshold it is a normal partial-track shape. Keeping
/// the leading corners at the full cap radius at every fill level makes the
/// handoff between the two variants seamless.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RevealClip {
    /// Ordinary partial annular track used after the tiny-fill phase.
    Track(ArcTrackSpec),
    /// Minimum cap translated backward so intersection grows from zero.
    TranslatedCap(TranslatedTrackCap),
}

impl RevealClip {
    /// Builds the concrete clip path for this variant. `TranslatedCap` borrows
    /// the source geometry (center, radius, start angle, sweep, thickness).
    pub(crate) fn path(self, source: &ArcTrackSpec) -> Option<Path> {
        match self {
            RevealClip::Track(spec) => spec.filled_path(),
            RevealClip::TranslatedCap(cap) => Some(source.translated_cap_path(cap)),
        }
    }
}

impl ArcTrackSpec {
    /// Creates a reveal specification spanning the complete track.
    pub(crate) fn full(geometry: ArcGaugeGeometry, thickness: f32, corner_radius: f32) -> Self {
        Self {
            geometry,
            sweep_angle: geometry.sweep_angle,
            thickness,
            start_corner_radius: corner_radius,
            end_corner_radius: corner_radius,
        }
    }

    /// Overrides the cap at the advancing end of the sweep while retaining the
    /// configured start cap.
    pub(crate) fn with_end_corner_radius(self, end_corner_radius: f32) -> Self {
        Self {
            end_corner_radius,
            ..self
        }
    }

    /// Builds the reveal clip for this complete track. Below a
    /// threshold fill the clip is a translated rounded rectangle (Option B):
    /// it slides backward along the sweep tangent from the start so its
    /// intersection with the annular source grows monotonically. At or above
    /// the threshold the clip is a normal partial-track shape with an anchored
    /// start and a progressively revealed end cap.
    pub(crate) fn reveal_clip(self, fill01: f32) -> Option<RevealClip> {
        let fill = fill01.clamp(0.0, 1.0);
        let sweep = self
            .sweep_angle
            .clamp(-MAX_ARC_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES);
        let sweep_magnitude = sweep.abs();
        let direction = sweep.signum();
        let radius = self.geometry.radius;
        if fill <= 0.0 || sweep_magnitude <= 0.0 || radius <= TRACK_PATH_EPSILON {
            return None;
        }

        let full_circle = sweep_magnitude >= MAX_ARC_ANGLE_DEGREES - TRACK_PATH_EPSILON;
        let half_thickness = self.thickness * 0.5;
        let start_corner_radius = if full_circle {
            0.0
        } else {
            self.start_corner_radius.clamp(0.0, half_thickness)
        };
        let end_corner_radius = if full_circle {
            0.0
        } else {
            self.end_corner_radius.clamp(0.0, half_thickness)
        };
        let start_cap_angle = arc_cap_angle_degrees(radius, start_corner_radius);
        let end_cap_angle = arc_cap_angle_degrees(radius, end_corner_radius);
        let total_span = sweep_magnitude + start_cap_angle + end_cap_angle;
        let revealed_sweep = total_span * fill;
        let available_end_cap_length = radius * revealed_sweep.to_radians();
        let revealed_end_corner_radius = end_corner_radius.min(available_end_cap_length);
        let revealed_end_cap_angle = arc_cap_angle_degrees(radius, revealed_end_corner_radius);
        let body_sweep = (revealed_sweep - revealed_end_cap_angle).max(TRACK_PATH_EPSILON);

        if end_corner_radius > TRACK_PATH_EPSILON && !full_circle && total_span > TRACK_PATH_EPSILON
        {
            if let Some(cap) =
                translated_track_cap_reveal(available_end_cap_length, end_corner_radius)
            {
                return Some(RevealClip::TranslatedCap(cap));
            }
        }

        Some(RevealClip::Track(ArcTrackSpec {
            geometry: ArcGaugeGeometry {
                start_angle: self.geometry.start_angle - direction * start_cap_angle,
                sweep_angle: direction * body_sweep,
                ..self.geometry
            },
            sweep_angle: direction * body_sweep,
            thickness: self.thickness,
            start_corner_radius: 0.0,
            end_corner_radius: revealed_end_corner_radius,
        }))
    }

    /// Builds a closed annular clip with independently rounded end caps.
    pub(super) fn filled_path(self) -> Option<Path> {
        let sweep = self
            .sweep_angle
            .clamp(-MAX_ARC_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES);
        let sweep_magnitude = sweep.abs();
        let direction = sweep.signum();
        let half_thickness = self.thickness * 0.5;
        let outer_radius = self.geometry.radius + half_thickness;
        let inner_radius = self.geometry.radius - half_thickness;
        // A zero sweep is an empty fill, but every non-zero sweep represents a
        // real value. In particular, a short rounded fill is a cap rather than
        // an empty track, so do not discard it based on a geometric epsilon.
        if sweep_magnitude <= 0.0
            || half_thickness <= TRACK_PATH_EPSILON
            || inner_radius <= TRACK_PATH_EPSILON
        {
            return None;
        }

        let start = self.geometry.start_angle;
        let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
        path.move_to(arc_point(
            self.geometry.center_x,
            self.geometry.center_y,
            outer_radius,
            start,
        ));

        if sweep_magnitude >= MAX_ARC_ANGLE_DEGREES - TRACK_PATH_EPSILON {
            append_circular_arc(
                &mut path,
                self.geometry.center_x,
                self.geometry.center_y,
                outer_radius,
                start,
                direction * MAX_ARC_ANGLE_DEGREES,
            );
            path.close();
            path.move_to(arc_point(
                self.geometry.center_x,
                self.geometry.center_y,
                inner_radius,
                start,
            ));
            append_circular_arc(
                &mut path,
                self.geometry.center_x,
                self.geometry.center_y,
                inner_radius,
                start,
                -direction * MAX_ARC_ANGLE_DEGREES,
            );
            path.close();
            return Some(path.detach());
        }

        let end = start + sweep;
        let start_fillet_radius = self.start_corner_radius.clamp(0.0, half_thickness);
        let end_fillet_radius = self.end_corner_radius.clamp(0.0, half_thickness);
        append_circular_arc(
            &mut path,
            self.geometry.center_x,
            self.geometry.center_y,
            outer_radius,
            start,
            sweep,
        );
        append_track_fillet(
            &mut path,
            TrackPathFrame {
                origin: arc_point(
                    self.geometry.center_x,
                    self.geometry.center_y,
                    self.geometry.radius,
                    end,
                ),
                tangent: directed_path_tangent(end, direction),
                normal: path_normal(end),
            },
            half_thickness,
            end_fillet_radius,
            1.0,
        );
        append_circular_arc(
            &mut path,
            self.geometry.center_x,
            self.geometry.center_y,
            inner_radius,
            end,
            -sweep,
        );
        let start_tangent = directed_path_tangent(start, direction);
        append_track_fillet(
            &mut path,
            TrackPathFrame {
                origin: arc_point(
                    self.geometry.center_x,
                    self.geometry.center_y,
                    self.geometry.radius,
                    start,
                ),
                tangent: Point::new(-start_tangent.x, -start_tangent.y),
                normal: path_normal(start),
            },
            half_thickness,
            start_fillet_radius,
            -1.0,
        );
        path.close();
        Some(path.detach())
    }

    /// Converts arc geometry into the shared translated-cap coordinate frame.
    fn translated_cap_path(self, cap: TranslatedTrackCap) -> Path {
        let sweep = self
            .sweep_angle
            .clamp(-MAX_ARC_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES);
        let direction = sweep.signum();
        let start_center = arc_point(
            self.geometry.center_x,
            self.geometry.center_y,
            self.geometry.radius,
            self.geometry.start_angle,
        );
        let start_tangent = path_tangent(self.geometry.start_angle);
        let sweep_forward = Point::new(start_tangent.x * direction, start_tangent.y * direction);
        translated_track_cap_path(
            TrackPathFrame {
                origin: start_center,
                tangent: sweep_forward,
                normal: path_normal(self.geometry.start_angle),
            },
            self.thickness,
            cap,
        )
    }
}

fn arc_cap_angle_degrees(radius: f32, corner_radius: f32) -> f32 {
    if radius <= TRACK_PATH_EPSILON || corner_radius <= 0.0 {
        0.0
    } else {
        corner_radius.atan2(radius).to_degrees()
    }
}

pub(super) fn append_circular_arc(
    path: &mut PathBuilder,
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
    sweep_angle: f32,
) {
    let segment_count = ((sweep_angle.abs() / 90.0).ceil() as u32).max(1);
    let segment_sweep = sweep_angle / segment_count as f32;

    for index in 0..segment_count {
        let angle0 = start_angle + segment_sweep * index as f32;
        let angle1 = angle0 + segment_sweep;
        let control_distance = radius * (4.0 / 3.0) * ((angle1 - angle0).to_radians() * 0.25).tan();
        let start = arc_point(center_x, center_y, radius, angle0);
        let end = arc_point(center_x, center_y, radius, angle1);
        let start_tangent = path_tangent(angle0);
        let end_tangent = path_tangent(angle1);
        path.cubic_to(
            Point::new(
                start.x + start_tangent.x * control_distance,
                start.y + start_tangent.y * control_distance,
            ),
            Point::new(
                end.x - end_tangent.x * control_distance,
                end.y - end_tangent.y * control_distance,
            ),
            end,
        );
    }
}

fn path_tangent(angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(-radians.sin(), radians.cos())
}

fn directed_path_tangent(angle: f32, direction: f32) -> Point {
    let tangent = path_tangent(angle);
    Point::new(tangent.x * direction, tangent.y * direction)
}

fn path_normal(angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(radians.cos(), radians.sin())
}

#[cfg(test)]
mod tests {
    use crate::widgets::gauges::arc::{
        arc_gauge_geometry, ArcGaugeGeometry, ArcTrackSpec, RevealClip,
    };
    use crate::widgets::gauges::track_path::TRACK_PATH_EPSILON;

    #[test]
    fn reveal_clip_handles_zero_and_tiny_fills() {
        let geometry = arc_gauge_geometry(160.0, 160.0, 180.0, 12.0, 2.0);
        let rounded = ArcTrackSpec::full(geometry, 12.0, 6.0);
        let minimum_positive_fill = 0.000_001;

        assert!(rounded.reveal_clip(0.0).is_none());
        assert!(rounded
            .reveal_clip(minimum_positive_fill)
            .and_then(|clip| clip.path(&rounded))
            .is_some());
        assert!(rounded
            .with_end_corner_radius(0.0)
            .reveal_clip(minimum_positive_fill)
            .and_then(|clip| clip.path(&rounded))
            .is_some());
    }

    #[test]
    fn reveal_clip_translates_below_threshold_and_anchors_above() {
        let geometry = ArcGaugeGeometry {
            center_x: 80.0,
            center_y: 80.0,
            inner_widget_center_x: 80.0,
            inner_widget_center_y: 80.0,
            radius: 64.0,
            start_angle: 180.0,
            sweep_angle: 180.0,
        };
        let source = ArcTrackSpec::full(geometry, 12.0, 6.0);

        match source.reveal_clip(0.001).unwrap() {
            RevealClip::TranslatedCap(cap) => {
                assert!((cap.corner_radius - 6.0).abs() < TRACK_PATH_EPSILON);
                assert!(cap.cap_offset < 0.0 && cap.cap_offset > -12.0);
            }
            clip => panic!("expected translated cap below threshold, got {clip:?}"),
        }

        match source.reveal_clip(0.06).unwrap() {
            RevealClip::Track(spec) => {
                assert!((spec.geometry.start_angle - 174.644).abs() < 0.01);
                assert!((spec.end_corner_radius - 6.0).abs() < TRACK_PATH_EPSILON);
            }
            clip => panic!("expected track above threshold, got {clip:?}"),
        }

        match source.reveal_clip(0.5).unwrap() {
            RevealClip::Track(spec) => {
                assert!((spec.geometry.start_angle - 174.644).abs() < 0.01);
                assert!((spec.sweep_angle - 90.0).abs() < 0.001);
                assert_eq!(spec.end_corner_radius, 6.0);
            }
            clip => panic!("expected track at halfway, got {clip:?}"),
        }
    }
}
