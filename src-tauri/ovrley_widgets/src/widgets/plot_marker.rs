//! The current-position marker shared by every plot family: one variant list, one footprint,
//! one drawing.
use crate::{
    error::{WidgetError, WidgetResult},
    render::text::parse_color,
};
use skia_safe::{paint::Style, Canvas, Paint, Path, PathBuilder, Point};

/// Ring strokes keep a hairline width at every scene scale.
const RING_STROKE: f32 = 1.5;
/// Halos fill the variant diameter at this share of the marker opacity.
const HALO_OPACITY: f32 = 0.35;
/// Markers never shrink below this radius, so a tiny marker stays visible.
const MIN_RADIUS: f32 = 2.0;
/// The caret's corners and how round each is, at radius 1, pointing up: tip, right, cutout, left.
/// Rounding pulls the outline in far from these corners, and all of it stays within radius 1.
const CARET_CORNERS: [((f32, f32), f32); 4] = [
    ((0.0, -1.55), 0.38),
    ((1.04, 1.12), 0.29),
    ((0.0, 0.56), 0.25),
    ((-1.04, 1.12), 0.29),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerVariant {
    Single,
    Ring,
    Halo,
    Caret,
}

fn caret_path(radius: f32) -> Path {
    let corner = |index: usize| {
        let ((x, y), _) = CARET_CORNERS[index % CARET_CORNERS.len()];
        Point::new(x * radius, y * radius)
    };
    let (last, first) = (corner(CARET_CORNERS.len() - 1), corner(0));
    let mut path = PathBuilder::new();
    path.move_to(Point::new((last.x + first.x) / 2.0, (last.y + first.y) / 2.0));
    for (index, (_, rounding)) in CARET_CORNERS.iter().enumerate() {
        path.arc_to_tangent(corner(index), corner(index + 1), rounding * radius);
    }
    path.close();
    path.detach()
}

impl MarkerVariant {
    /// The variants of a plot with no direction of travel.
    pub fn parse(value: &str, field: &str) -> WidgetResult<Self> {
        match value {
            "single" => Ok(Self::Single),
            "ring" => Ok(Self::Ring),
            "halo" => Ok(Self::Halo),
            _ => Err(WidgetError::Config(format!(
                "{field}: unsupported marker variant"
            ))),
        }
    }

    /// The variants of a family that draws a route, whose caret points along it.
    pub fn parse_on_route(value: &str, field: &str) -> WidgetResult<Self> {
        match value {
            "caret" => Ok(Self::Caret),
            _ => Self::parse(value, field),
        }
    }
}

/// A marker's scene-pixel radius from its unscaled config size.
pub fn plot_marker_radius(size: f32, scale: f32) -> f32 {
    (size * scale).max(MIN_RADIUS)
}

/// Prepared paints and scene-pixel radii of one marker style.
#[derive(Clone, Debug)]
pub struct PlotMarker {
    variant: MarkerVariant,
    radius: f32,
    variant_radius: f32,
    marker: Paint,
    surround: Paint,
}

impl PlotMarker {
    /// `size` and `variant_diameter` are unscaled config values; `opacity` is the final opacity.
    pub fn new(
        variant: MarkerVariant,
        size: f32,
        variant_diameter: f32,
        color: &str,
        opacity: f32,
        scale: f32,
    ) -> Self {
        let paint = |opacity| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(parse_color(color, opacity));
            paint
        };
        let marker = paint(opacity);
        let surround = match variant {
            MarkerVariant::Ring => {
                let mut ring = marker.clone();
                ring.set_style(Style::Stroke);
                ring.set_stroke_width(RING_STROKE);
                ring
            }
            MarkerVariant::Halo => paint(opacity * HALO_OPACITY),
            MarkerVariant::Single | MarkerVariant::Caret => marker.clone(),
        };
        Self {
            variant,
            radius: plot_marker_radius(size, scale),
            variant_radius: variant_diameter * scale * 0.5,
            marker,
            surround,
        }
    }

    /// Distance from the marker centre to the edge of everything it paints.
    pub fn extent(&self) -> f32 {
        match self.variant {
            MarkerVariant::Single | MarkerVariant::Caret => self.radius,
            MarkerVariant::Ring => self.radius.max(self.variant_radius + RING_STROKE * 0.5),
            MarkerVariant::Halo => self.radius.max(self.variant_radius),
        }
    }

    /// Circles need only their scene position; callers draw them outside any frame rotation.
    pub fn draw(&self, canvas: &Canvas, center: impl Into<Point>) {
        self.draw_heading(canvas, center, 0.0);
    }

    /// As [`Self::draw`], for a family that draws a route, whose caret points `heading_deg` clockwise
    /// from up. Its path is rebuilt here because paths aren't shared across export workers.
    pub fn draw_heading(&self, canvas: &Canvas, center: impl Into<Point>, heading_deg: f32) {
        let center = center.into();
        match self.variant {
            MarkerVariant::Caret => {
                canvas.save();
                canvas.translate(center);
                canvas.rotate(heading_deg, None);
                canvas.draw_path(&caret_path(self.radius), &self.marker);
                canvas.restore();
            }
            MarkerVariant::Single => {
                canvas.draw_circle(center, self.radius, &self.marker);
            }
            MarkerVariant::Ring | MarkerVariant::Halo => {
                canvas.draw_circle(center, self.variant_radius, &self.surround);
                canvas.draw_circle(center, self.radius, &self.marker);
            }
        }
    }
}

/// Marker scale for one frame resize. Edge handles reshape the plot and keep the marker;
/// corner handles scale it by the geometric mean of the axis ratios, so repeated gestures
/// are reversible. `fixed` is the anchored corner, where an edge handle has a 0.5 component.
pub(crate) fn frame_resize_marker_factor(
    previous: [f32; 2],
    next: [f32; 2],
    fixed: [f32; 2],
) -> f32 {
    if fixed.contains(&0.5) {
        return 1.0;
    }
    ((next[0] / previous[0]) * (next[1] / previous[1])).sqrt()
}
