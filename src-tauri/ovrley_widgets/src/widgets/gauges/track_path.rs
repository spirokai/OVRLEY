//! Shared path geometry for rounded gauge tracks.
//!
//! Arc and linear tracks express their start edge as the same local frame:
//! origin, forward tangent, and cross-track normal. That lets both renderers
//! share identical rounded fillets and the translated cap used for tiny fills.

use skia_safe::{Path, PathBuilder, PathFillType, Point};

pub(crate) const TRACK_PATH_EPSILON: f32 = 0.001;

const QUARTER_CIRCLE_KAPPA: f32 = 0.552_284_76;

#[derive(Clone, Copy, Debug)]
/// Local coordinate frame at a track endpoint.
pub(crate) struct TrackPathFrame {
    pub origin: Point,
    pub tangent: Point,
    pub normal: Point,
}

impl TrackPathFrame {
    fn point(self, tangent_offset: f32, normal_offset: f32) -> Point {
        Point::new(
            self.origin.x + self.tangent.x * tangent_offset + self.normal.x * normal_offset,
            self.origin.y + self.tangent.y * tangent_offset + self.normal.y * normal_offset,
        )
    }
}

#[derive(Clone, Copy, Debug)]
/// Minimum rounded cap shifted backward along a track's tangent.
pub(crate) struct TranslatedTrackCap {
    pub corner_radius: f32,
    pub cap_offset: f32,
}

/// Appends one rounded radial edge in the supplied endpoint frame.
pub(crate) fn append_track_fillet(
    path: &mut PathBuilder,
    frame: TrackPathFrame,
    half_thickness: f32,
    corner_radius: f32,
    start_normal_direction: f32,
) {
    if corner_radius <= TRACK_PATH_EPSILON {
        path.line_to(frame.point(0.0, -start_normal_direction * half_thickness));
        return;
    }

    let kappa = corner_radius * QUARTER_CIRCLE_KAPPA;
    let curved_inset = half_thickness - corner_radius;
    path.cubic_to(
        frame.point(kappa, start_normal_direction * half_thickness),
        frame.point(
            corner_radius,
            start_normal_direction * (curved_inset + kappa),
        ),
        frame.point(corner_radius, start_normal_direction * curved_inset),
    );
    path.line_to(frame.point(corner_radius, -start_normal_direction * curved_inset));
    path.cubic_to(
        frame.point(
            corner_radius,
            -start_normal_direction * (curved_inset + kappa),
        ),
        frame.point(kappa, -start_normal_direction * half_thickness),
        frame.point(0.0, -start_normal_direction * half_thickness),
    );
}

/// Returns the translated phase while the revealed length is shorter than the minimum cap.
pub(crate) fn translated_track_cap_reveal(
    revealed_length: f32,
    corner_radius: f32,
) -> Option<TranslatedTrackCap> {
    let cap_length = corner_radius * 2.0;
    if revealed_length <= 0.0
        || corner_radius <= TRACK_PATH_EPSILON
        || revealed_length >= cap_length
    {
        return None;
    }

    Some(TranslatedTrackCap {
        corner_radius,
        cap_offset: revealed_length - cap_length,
    })
}

/// Builds the minimum rounded rectangle translated from the frame origin along its tangent.
pub(crate) fn translated_track_cap_path(
    frame: TrackPathFrame,
    track_thickness: f32,
    cap: TranslatedTrackCap,
) -> Path {
    let translated_frame = TrackPathFrame {
        origin: Point::new(
            frame.origin.x + frame.tangent.x * cap.cap_offset,
            frame.origin.y + frame.tangent.y * cap.cap_offset,
        ),
        ..frame
    };
    let half_thickness = track_thickness * 0.5;
    let outer_edge = translated_frame.point(0.0, half_thickness);
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    path.move_to(outer_edge);
    append_track_fillet(
        &mut path,
        translated_frame,
        half_thickness,
        cap.corner_radius,
        1.0,
    );
    append_track_fillet(
        &mut path,
        TrackPathFrame {
            tangent: Point::new(-translated_frame.tangent.x, -translated_frame.tangent.y),
            ..translated_frame
        },
        half_thickness,
        cap.corner_radius,
        -1.0,
    );
    path.close();
    path.detach()
}
