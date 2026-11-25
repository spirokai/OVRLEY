//! Gauge track geometry: bar validation, track paths and caps, arc segments, and the limits
//! and suggestions the gauge editors derive from them, so the inspector bounds its sliders by
//! what the renderer draws.
use crate::model::catalog::TrackFillStyle;
use crate::widgets::gauges::arc::MAX_ARC_ANGLE_DEGREES;
use crate::widgets::gauges::arc::{arc_point, ArcGaugeGeometry};
use crate::{WidgetError, WidgetResult};
use skia_safe::{Canvas, Paint, Path, PathBuilder, PathFillType, Point};

const MIN_BAR_PX: f32 = 2.0;

pub fn arc_track_radius(frame_size: f32, track_thickness: f32, border_thickness: f32) -> f32 {
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
#[expect(
    clippy::too_many_arguments,
    reason = "each parameter is one validated track field, shared by ingress and resize"
)]
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

/// The track radius and sweep the editor bounds its controls with.
pub fn arc_track_dimensions(
    frame_size: f32,
    thickness: f32,
    corner_radius: f32,
    border: f32,
    arc_angle: f32,
    corner: bool,
) -> (f32, f32) {
    if corner {
        (
            corner_track_radius(frame_size, thickness, corner_radius, border),
            90.0,
        )
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

/// About `target_bars` bars (at least three where they fit), 4px apart.
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

/// A bar count proportional to the sweep.
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

pub(crate) const TRACK_PATH_EPSILON: f32 = 0.001;

const QUARTER_CIRCLE_KAPPA: f32 = 0.552_284_8;

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

/// The unit tangent of a circle at `angle` degrees, in the direction of increasing angle.
pub(crate) fn path_tangent(angle: f32) -> Point {
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
        return crate::widgets::gauges::track::ArcTrackSpec::full(geometry, track_thickness, 0.0)
            .filled_path();
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
    use super::{
        rounded_segment_geometry, rounded_segment_path, segment_geometries, ArcTrackSpec,
        RevealClip,
    };
    use super::{ResolvedBarGeometry, TRACK_PATH_EPSILON, *};
    use crate::widgets::gauges::arc::{arc_gauge_geometry, ArcGaugeGeometry};

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
