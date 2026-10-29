//! The current-position marker shared by every plot family: one variant list, one footprint,
//! one drawing.
use crate::{
    error::{WidgetError, WidgetResult},
    render::text::parse_color,
};
use skia_safe::{paint::Style, Canvas, Paint, Point};

/// Ring strokes keep a hairline width at every scene scale.
const RING_STROKE: f32 = 1.5;
/// Halos fill the variant diameter at this share of the marker opacity.
const HALO_OPACITY: f32 = 0.35;
/// Markers never shrink below this radius, so a tiny marker stays visible.
const MIN_RADIUS: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerVariant {
    Single,
    Ring,
    Halo,
}

impl MarkerVariant {
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
            MarkerVariant::Single => marker.clone(),
        };
        Self {
            variant,
            radius: (size * scale).max(MIN_RADIUS),
            variant_radius: variant_diameter * scale * 0.5,
            marker,
            surround,
        }
    }

    /// Distance from the marker centre to the edge of everything it paints.
    pub fn extent(&self) -> f32 {
        match self.variant {
            MarkerVariant::Single => self.radius,
            MarkerVariant::Ring => self.radius.max(self.variant_radius + RING_STROKE * 0.5),
            MarkerVariant::Halo => self.radius.max(self.variant_radius),
        }
    }

    /// Circles need only their scene position; callers draw them outside any frame rotation.
    pub fn draw(&self, canvas: &Canvas, center: impl Into<Point>) {
        let center = center.into();
        if self.variant != MarkerVariant::Single {
            canvas.draw_circle(center, self.variant_radius, &self.surround);
        }
        canvas.draw_circle(center, self.radius, &self.marker);
    }
}
