//! Solid or gradient fill shared by track gauges and the lean-angle sector.
//!
//! The gradient is anchored to the track, not to the current fill: offset 0 is
//! the track start and offset 1 its end, so the fill reveals a static gradient.

use crate::error::{WidgetError, WidgetResult};
use crate::model::validation::{require_bool, require_hex_color};
use crate::render::text::parse_color;
use crate::widgets::value::preview::{surface, SHAPE_COLOR_TYPE};
use serde::{Deserialize, Serialize};
use skia_safe::{
    gradient, paint::Style, Canvas, ClipOp, Color, Color4f, Image, Matrix, Paint, Path, Point,
    RRect, Rect, Shader, TileMode,
};

pub const MIN_GRADIENT_STOPS: usize = 2;
pub const MAX_GRADIENT_STOPS: usize = 5;

/// One configured gradient stop; `offset` is in 0..=1 along the track.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct GradientStop {
    pub offset: f32,
    pub color: String,
}

/// Validated fill color. Gradient stops are sorted by offset.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackFillColor {
    Solid(String),
    Gradient(Vec<GradientStop>),
}

/// Validates the solid color, gradient switch and stops; all three are required.
pub fn validate_track_fill_color(
    color: Option<&str>,
    gradient: Option<bool>,
    stops: Option<Vec<GradientStop>>,
    p: impl Fn(&str) -> String,
) -> WidgetResult<TrackFillColor> {
    let color = require_hex_color(color, &p("track_filled_color"))?;
    let gradient = require_bool(gradient, &p("track_filled_gradient"))?;
    let field = p("track_filled_gradient_stops");
    let mut stops = stops.ok_or_else(|| WidgetError::Config(format!("{field}: required")))?;
    if !(MIN_GRADIENT_STOPS..=MAX_GRADIENT_STOPS).contains(&stops.len()) {
        return Err(WidgetError::Config(format!(
            "{field}: expected {MIN_GRADIENT_STOPS}..={MAX_GRADIENT_STOPS} stops, got {}",
            stops.len()
        )));
    }
    for (index, stop) in stops.iter_mut().enumerate() {
        if !(0.0..=1.0).contains(&stop.offset) {
            return Err(WidgetError::Config(format!(
                "{field}[{index}].offset: must be 0.0..1.0, got {}",
                stop.offset
            )));
        }
        stop.color = require_hex_color(Some(&stop.color), &format!("{field}[{index}].color"))?;
    }
    if !gradient {
        return Ok(TrackFillColor::Solid(color));
    }
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    Ok(TrackFillColor::Gradient(stops))
}

impl TrackFillColor {
    /// The color at the track start, used by decorations that need one color.
    pub fn leading_color(&self) -> &str {
        match self {
            Self::Solid(color) => color,
            Self::Gradient(stops) => &stops[0].color,
        }
    }

    /// Fill paint whose gradient runs along a straight track from `start` to `end`.
    pub fn linear_paint(&self, opacity: f32, start: Point, end: Point) -> WidgetResult<Paint> {
        self.paint(opacity, |gradient| {
            gradient::shaders::linear_gradient((start, end), gradient, None)
        })
    }

    /// Fill paint whose gradient follows a circular track from `start_angle` along the
    /// signed `sweep_angle` (Skia screen-space degrees). Skia sweeps measure 0..360°
    /// from the local +x axis, so the local frame starts the track at half the open
    /// gap: either cap overhang then clamps to its own end color instead of wrapping.
    pub fn sweep_paint(
        &self,
        opacity: f32,
        center: Point,
        start_angle: f32,
        sweep_angle: f32,
    ) -> WidgetResult<Paint> {
        let sweep = sweep_angle.abs().min(360.0);
        let half_gap = (360.0 - sweep) * 0.5;
        let mut local = Matrix::translate(center);
        local
            .pre_rotate(start_angle, None)
            .pre_scale((1.0, sweep_angle.signum()), None)
            .pre_rotate(-half_gap, None);
        self.paint(opacity, |gradient| {
            gradient::shaders::sweep_gradient(
                Point::new(0.0, 0.0),
                (half_gap, half_gap + sweep),
                gradient,
                &local,
            )
        })
    }

    /// Fill paint mirrored about `center_angle`: offset 0 sits at the center and
    /// offset 1 at `half_sweep` degrees on either side.
    pub fn mirrored_sweep_paint(
        &self,
        opacity: f32,
        center: Point,
        center_angle: f32,
        half_sweep: f32,
    ) -> WidgetResult<Paint> {
        let mirrored = match self {
            Self::Solid(_) => self.clone(),
            Self::Gradient(stops) => Self::Gradient(
                stops
                    .iter()
                    .rev()
                    .map(|stop| (0.5 - stop.offset * 0.5, stop))
                    .chain(stops.iter().map(|stop| (0.5 + stop.offset * 0.5, stop)))
                    .map(|(offset, stop)| GradientStop {
                        offset,
                        color: stop.color.clone(),
                    })
                    .collect(),
            ),
        };
        mirrored.sweep_paint(opacity, center, center_angle - half_sweep, half_sweep * 2.0)
    }

    fn paint(
        &self,
        opacity: f32,
        shader: impl FnOnce(&gradient::Gradient) -> Option<Shader>,
    ) -> WidgetResult<Paint> {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Fill);
        match self {
            Self::Solid(color) => {
                paint.set_color(parse_color(color, opacity));
            }
            Self::Gradient(stops) => {
                let (colors, positions): (Vec<Color4f>, Vec<f32>) = stops
                    .iter()
                    .map(|stop| (Color4f::from(parse_color(&stop.color, 1.0)), stop.offset))
                    .unzip();
                let gradient = gradient::Gradient::new(
                    gradient::Colors::new(&colors, Some(&positions), TileMode::Clamp, None),
                    gradient::Interpolation::default(),
                );
                let shader = shader(&gradient).ok_or_else(|| {
                    WidgetError::Render("unable to prepare track fill gradient".into())
                })?;
                paint.set_shader(shader);
                paint.set_alpha_f(opacity);
            }
        }
        Ok(paint)
    }
}

/// A prepared track fill. Shading a gradient every frame is slow in the WASM build,
/// so a gradient fill is rasterized once at its full extent and frames only clip it.
#[derive(Clone, Debug)]
pub enum TrackFill {
    Paint(Paint),
    Image { image: Image, x: f32, y: f32 },
}

impl TrackFill {
    /// Solid colors keep `paint`. Gradients draw the full fill with `paint` into a
    /// raster covering `bounds`, placed at `origin` like the gauge's static raster.
    pub fn prepare(
        color: &TrackFillColor,
        paint: Paint,
        bounds: Rect,
        origin: (f32, f32),
        draw_full_fill: impl FnOnce(&Canvas, &Paint),
    ) -> WidgetResult<Self> {
        if let TrackFillColor::Solid(_) = color {
            return Ok(Self::Paint(paint));
        }
        let mut surface = surface(bounds.width() as u32, bounds.height() as u32, SHAPE_COLOR_TYPE)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-bounds.left, -bounds.top));
        draw_full_fill(canvas, &paint);
        Ok(Self::Image {
            image: surface.image_snapshot(),
            x: origin.0 + bounds.left,
            y: origin.1 + bounds.top,
        })
    }

    /// Solid fills run `paint_fill`; gradient fills reveal the raster within `reveal`,
    /// since the raster already carries the full fill's own outline.
    pub fn draw(&self, canvas: &Canvas, reveal: &FillShape, paint_fill: impl FnOnce(&Paint)) {
        match self {
            Self::Paint(paint) => paint_fill(paint),
            Self::Image { image, x, y } => {
                canvas.save();
                reveal.clip(canvas);
                canvas.draw_image(image, (*x, *y), None);
                canvas.restore();
            }
        }
    }
}

/// A fill outline, painted directly or used as an anti-aliased clip.
pub enum FillShape {
    Rect(Rect),
    RRect(RRect),
    Path(Path),
}

impl FillShape {
    pub fn draw(&self, canvas: &Canvas, paint: &Paint) {
        match self {
            Self::Rect(rect) => canvas.draw_rect(rect, paint),
            Self::RRect(rrect) => canvas.draw_rrect(rrect, paint),
            Self::Path(path) => canvas.draw_path(path, paint),
        };
    }

    pub fn clip(&self, canvas: &Canvas) {
        match self {
            Self::Rect(rect) => canvas.clip_rect(rect, ClipOp::Intersect, true),
            Self::RRect(rrect) => canvas.clip_rrect(rrect, ClipOp::Intersect, true),
            Self::Path(path) => canvas.clip_path(path, ClipOp::Intersect, true),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skia_safe::surfaces;

    fn stop(offset: f32, color: &str) -> GradientStop {
        GradientStop { offset, color: color.into() }
    }

    fn p(field: &str) -> String {
        format!("values[0].{field}")
    }

    fn pixel(paint: &Paint, point: (i32, i32)) -> Color {
        let mut surface = surfaces::raster_n32_premul((200, 200)).unwrap();
        surface.canvas().draw_paint(paint);
        surface.image_snapshot().peek_pixels().unwrap().get_color(point)
    }

    #[test]
    fn validation_sorts_stops_and_rejects_malformed_input() {
        let stops = || vec![stop(1.0, "#0000ff"), stop(0.0, "#ff0000")];
        let fill = validate_track_fill_color(Some("#ffffff"), Some(true), Some(stops()), p).unwrap();
        assert_eq!(fill, TrackFillColor::Gradient(vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")]));
        let fill = validate_track_fill_color(Some("#ffffff"), Some(false), Some(stops()), p).unwrap();
        assert_eq!(fill, TrackFillColor::Solid("#ffffff".into()));

        let validate = |stops| validate_track_fill_color(Some("#fff000"), Some(true), Some(stops), p);
        assert!(validate(vec![stop(0.0, "#ff0000")]).is_err());
        assert!(validate(vec![stop(0.0, "#ff0000"), stop(1.5, "#0000ff")]).is_err());
        assert!(validate(vec![stop(0.0, "blue"), stop(1.0, "#0000ff")]).is_err());
        assert!(validate_track_fill_color(Some("#fff000"), None, Some(stops()), p).is_err());
        assert!(validate_track_fill_color(Some("#fff000"), Some(false), None, p).is_err());
    }

    #[test]
    fn sweep_runs_along_a_negative_sweep_and_clamps_cap_overhang() {
        let fill = TrackFillColor::Gradient(vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")]);
        // Bottom-left corner: 0° sweeping counter-clockwise to -90°.
        let paint = fill.sweep_paint(1.0, Point::new(100.0, 100.0), 0.0, -90.0).unwrap();
        let (start, end, start_overhang) =
            (pixel(&paint, (190, 99)), pixel(&paint, (101, 10)), pixel(&paint, (190, 110)));
        assert!(start.r() > 240 && start.b() < 15, "{start:?}");
        assert!(end.b() > 240 && end.r() < 15, "{end:?}");
        assert!(start_overhang.r() > 240, "{start_overhang:?}");
    }

    #[test]
    fn mirrored_sweep_is_symmetric_about_the_center() {
        let fill = TrackFillColor::Gradient(vec![stop(0.0, "#ff0000"), stop(1.0, "#0000ff")]);
        let paint = fill.mirrored_sweep_paint(1.0, Point::new(100.0, 100.0), 270.0, 60.0).unwrap();
        let center = pixel(&paint, (100, 10));
        // Pixel centers 39.5 and 160.5 mirror about x = 100.
        let (left, right) = (pixel(&paint, (39, 60)), pixel(&paint, (160, 60)));
        assert!(center.r() > 240 && center.b() < 15, "{center:?}");
        assert!(left.r().abs_diff(right.r()) <= 1 && left.b().abs_diff(right.b()) <= 1, "{left:?} {right:?}");
        assert!(left.b() > left.r(), "{left:?}");
    }
}
