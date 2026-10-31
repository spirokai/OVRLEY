//! What the widget draws itself: the flat placeholder, the marker, the attribution and the border.
//! The map and the route arrive inside the frame.
//!
//! The renderer crops the map to the shape, so preview and export crop it the same way. This clips
//! what the widget draws over it to that shape.
use super::validation::{MapShape, ValidatedMapWidget};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
    render::{
        preview::PreviewRasterScale,
        text::{parse_color, resolve_font, FontResolver},
    },
    widgets::{
        plot_marker::PlotMarker,
        route::{geometry::PLACEHOLDER_ROUTE, layout::stroke_paint},
    },
};
use ovrley_map::{MapRouteStyle, MapStyle};
use skia_safe::{
    Canvas, Color, ContourMeasure, ContourMeasureIter, FilterMode, Font, Image, Paint, PaintJoin,
    PaintStyle, Path, PathBuilder, Point, RRect, RSXform, Rect, TextBlob,
};
use std::f32::consts::FRAC_1_SQRT_2;

/// OpenFreeMap's required credit, as `map_tile_service.rs` serves it to MapLibre.
const ATTRIBUTION: &str = "OpenFreeMap \u{a9} OpenMapTiles Data from OpenStreetMap";
const ATTRIBUTION_FONT_SIZE: f32 = 8.0;
const ATTRIBUTION_PADDING: f32 = 2.0;
const ATTRIBUTION_TEXT_OPACITY: f32 = 0.8;
const ATTRIBUTION_BACKING_OPACITY: f32 = 0.65;
/// The most of a curved edge the credit fills, so it never wraps up the sides.
const ATTRIBUTION_MAX_EDGE_SHARE: f32 = 0.9;

/// Each style's own `background-color`, shown until its frame arrives.
fn placeholder_color(style: MapStyle) -> Color {
    match style {
        MapStyle::Positron => Color::from_rgb(242, 243, 240),
        MapStyle::Bright | MapStyle::Liberty => Color::from_rgb(0xf8, 0xf4, 0xf0),
        MapStyle::Dark => Color::from_rgb(12, 12, 12),
        MapStyle::Fiord => Color::from_rgb(0x45, 0x51, 0x6e),
    }
}

fn color_floats(color: &str, opacity: f32) -> [f32; 4] {
    let color = parse_color(color, opacity);
    [color.r(), color.g(), color.b(), color.a()].map(|channel| f32::from(channel) / 255.0)
}

/// The middle of a band along the bottom of `area`: up the lower left corner's arc, along the bottom
/// edge, and up the lower right corner's. A circle is both arcs with no edge between them.
fn bottom_edge(area: Rect, corner_radius: f32, inset: f32) -> WidgetResult<ContourMeasure> {
    let radius = (corner_radius - inset).max(0.0);
    let (left, right, bottom) = (area.left + inset, area.right - inset, area.bottom - inset);
    let mut line = PathBuilder::new();
    line.move_to((left, bottom - radius));
    line.conic_to((left, bottom), (left + radius, bottom), FRAC_1_SQRT_2);
    line.line_to((right - radius, bottom));
    line.conic_to((right, bottom), (right, bottom - radius), FRAC_1_SQRT_2);
    ContourMeasureIter::new(&line.detach(), false, None)
        .next()
        .ok_or_else(|| WidgetError::Render("The map attribution has no edge to follow".into()))
}

fn band_height(font: &Font, padding: f32) -> f32 {
    let (_, metrics) = font.metrics();
    metrics.descent - metrics.ascent + padding * 2.0
}

struct Attribution {
    blob: Option<TextBlob>,
    origin: Point,
    backing: Path,
    text_paint: Paint,
    backing_paint: Paint,
}

impl Attribution {
    /// A required credit, always drawn, so it stays whole inside the shape. It draws under the shape's
    /// clip, which gives its backing the same edge as the map. With room on the straight bottom edge it
    /// sits bottom right. Otherwise it follows the curve, centred along the bottom. `opacity` is the
    /// widget's own.
    fn new(
        config: &ValidatedMapWidget,
        scene: &ValidatedScenePresentation,
        frame: Rect,
        corner_radius: f32,
        border: f32,
        opacity: f32,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let padding = ATTRIBUTION_PADDING * scene.scale;
        let area = Rect::new(
            frame.left + border,
            frame.top + border,
            frame.right - border,
            frame.bottom - border,
        );
        let radius = (corner_radius - border).max(0.0);
        let mut font: Font = resolve_font(
            fonts,
            Some(&config.font),
            ATTRIBUTION_FONT_SIZE * scene.scale,
        )?;
        let paint = |share: f32, rgb: u8| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(Color::from_rgb(rgb, rgb, rgb));
            paint.set_alpha_f(share * opacity);
            paint
        };
        let text_paint = paint(ATTRIBUTION_TEXT_OPACITY, 0);
        let mut backing_paint = paint(ATTRIBUTION_BACKING_OPACITY, 255);

        let (natural, _) = font.measure_str(ATTRIBUTION, None);
        if natural + padding * 2.0 <= area.width() - radius * 2.0 {
            let (_, metrics) = font.metrics();
            let text_left = area.right - radius - padding - natural;
            let top = area.bottom - band_height(&font, padding);
            return Ok(Self {
                blob: TextBlob::from_str(ATTRIBUTION, &font),
                origin: Point::new(text_left, top + padding - metrics.ascent),
                backing: Path::rect(
                    Rect::new(text_left - padding, top, frame.right, frame.bottom),
                    None,
                ),
                text_paint,
                backing_paint,
            });
        }

        let room = bottom_edge(area, radius, band_height(&font, padding) / 2.0)?.length()
            * ATTRIBUTION_MAX_EDGE_SHARE
            - padding * 2.0;
        if natural > room {
            font.set_size(font.size() * (room / natural).max(f32::EPSILON));
        }
        let band = band_height(&font, padding);
        let edge = bottom_edge(area, radius, band / 2.0)?;
        let glyphs = font.str_to_glyphs_vec(ATTRIBUTION);
        let mut widths = vec![0.0; glyphs.len()];
        font.get_widths(&glyphs, &mut widths);
        let length: f32 = widths.iter().sum();
        let start = (edge.length() - length) / 2.0;
        let (_, metrics) = font.metrics();
        let baseline = -(metrics.ascent + metrics.descent) / 2.0;
        let mut along = start;
        let mut placed = Vec::with_capacity(glyphs.len());
        for width in widths {
            let (at, tangent) = edge.pos_tan(along + width / 2.0).ok_or_else(|| {
                WidgetError::Render("The map attribution ran off its edge".into())
            })?;
            placed.push(RSXform::new(
                tangent.x,
                tangent.y,
                (
                    at.x - tangent.x * width / 2.0 - tangent.y * baseline,
                    at.y - tangent.y * width / 2.0 + tangent.x * baseline,
                ),
            ));
            along += width;
        }
        let mut backing = PathBuilder::new();
        if !edge.get_segment(start - padding, start + length + padding, &mut backing, true) {
            return Err(WidgetError::Render(
                "The map attribution ran off its edge".into(),
            ));
        }
        backing_paint.set_style(PaintStyle::Stroke);
        backing_paint.set_stroke_width(band);
        Ok(Self {
            blob: TextBlob::from_rsxform(ATTRIBUTION, &placed, &font),
            origin: Point::new(0.0, 0.0),
            backing: backing.detach(),
            text_paint,
            backing_paint,
        })
    }

    fn draw(&self, canvas: &Canvas) {
        canvas.draw_path(&self.backing, &self.backing_paint);
        if let Some(blob) = &self.blob {
            canvas.draw_text_blob(blob, self.origin, &self.text_paint);
        }
    }
}

/// One widget's scene geometry and paints, rebuilt on any config change.
pub struct MapLayout {
    /// The frame in scene pixels.
    pub frame: Rect,
    /// The widget's opacity with the scene's, which the whole frame draws at.
    pub opacity: f32,
    pub route_style: MapRouteStyle,
    /// The frame's visible shape.
    clip: RRect,
    /// The shape's radius in output pixels, which the renderer crops the map to.
    pub corner_radius_px: f32,
    border: Option<(RRect, Paint)>,
    placeholder_paint: Paint,
    image_paint: Paint,
    completed_paint: Paint,
    remaining_paint: Paint,
    marker: PlotMarker,
    attribution: Attribution,
}

impl MapLayout {
    pub fn new(
        config: &ValidatedMapWidget,
        scene: &ValidatedScenePresentation,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let frame = Rect::from_xywh(
            config.x,
            config.y,
            config.width * scene.scale,
            config.height * scene.scale,
        );
        let path = &config.path;
        let completed_width = (path.completed_line_width * scene.scale).max(1.0);
        let remaining_width = (path.remaining_line_width * scene.scale).max(1.0);
        let opacity = config.opacity * scene.opacity;
        let corner_radius = match config.shape {
            MapShape::Rectangle { corner_radius } => {
                (corner_radius * scene.scale).min(frame.width().min(frame.height()) / 2.0)
            }
            MapShape::Circle => frame.width() / 2.0,
        };
        let border_width = config.border.thickness * scene.scale;
        let border = (border_width > 0.0 && config.border.opacity > 0.0).then(|| {
            let mut paint = stroke_paint(
                &config.border.color,
                border_width,
                config.border.opacity * opacity,
            );
            // A round join would leave the map showing outside a square corner.
            paint.set_stroke_join(PaintJoin::Miter);
            let half = border_width / 2.0;
            let ring = Rect::new(
                frame.left + half,
                frame.top + half,
                frame.right - half,
                frame.bottom - half,
            );
            let radius = (corner_radius - half).max(0.0);
            (RRect::new_rect_xy(ring, radius, radius), paint)
        });
        let mut placeholder_paint = Paint::default();
        placeholder_paint.set_color(placeholder_color(config.spec.style));
        placeholder_paint.set_alpha_f(opacity);
        let mut image_paint = Paint::default();
        image_paint.set_alpha_f(opacity);
        Ok(Self {
            frame,
            opacity,
            clip: RRect::new_rect_xy(frame, corner_radius, corner_radius),
            corner_radius_px: corner_radius * raster_scale.value(),
            border,
            // Widget and scene opacity apply to the finished frame as a whole.
            route_style: MapRouteStyle {
                completed_color: color_floats(
                    &path.completed_line_color,
                    path.completed_line_opacity,
                ),
                completed_width_px: completed_width * raster_scale.value(),
                remaining_color: color_floats(
                    &path.remaining_line_color,
                    path.remaining_line_opacity,
                ),
                remaining_width_px: remaining_width * raster_scale.value(),
            },
            placeholder_paint,
            image_paint,
            completed_paint: stroke_paint(
                &path.completed_line_color,
                completed_width,
                path.completed_line_opacity * opacity,
            ),
            remaining_paint: stroke_paint(
                &path.remaining_line_color,
                remaining_width,
                path.remaining_line_opacity * opacity,
            ),
            marker: PlotMarker::new(
                path.marker_variant,
                path.marker_size,
                path.marker_variant_diameter,
                &path.marker_color,
                path.marker_opacity * opacity,
                scene.scale,
            ),
            attribution: Attribution::new(
                config,
                scene,
                frame,
                corner_radius,
                border_width,
                opacity,
                fonts,
            )?,
        })
    }

    /// Everything the widget draws stays inside the shape.
    pub fn clip_shape(&self, canvas: &Canvas) {
        canvas.clip_rrect(self.clip, None, true);
    }

    pub fn draw_border(&self, canvas: &Canvas) {
        if let Some((ring, paint)) = &self.border {
            canvas.draw_rrect(ring, paint);
        }
    }

    /// The finished frame over the widget's bounds, already cropped to its shape.
    pub fn draw_image(&self, canvas: &Canvas, image: &Image) {
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            self.frame,
            FilterMode::Linear,
            &self.image_paint,
        );
    }

    pub fn draw_placeholder(&self, canvas: &Canvas) {
        canvas.draw_rect(self.frame, &self.placeholder_paint);
    }

    /// The stand-in route when there's no GPS, in Skia because no frame exists to carry it.
    pub fn draw_placeholder_route(&self, canvas: &Canvas, progress: f32) -> (Point, f32) {
        let at = |(x, y): (f32, f32)| {
            Point::new(
                self.frame.left + x * self.frame.width(),
                self.frame.top + y * self.frame.height(),
            )
        };
        let segments = (PLACEHOLDER_ROUTE.len() - 1) as f32;
        let along = progress.clamp(0.0, 1.0) * segments;
        let index = (along as usize).min(PLACEHOLDER_ROUTE.len() - 2);
        let (a, b) = (at(PLACEHOLDER_ROUTE[index]), at(PLACEHOLDER_ROUTE[index + 1]));
        let mix = along - index as f32;
        let split = Point::new(a.x + (b.x - a.x) * mix, a.y + (b.y - a.y) * mix);

        let mut remaining = PathBuilder::new();
        remaining.move_to(split);
        for point in &PLACEHOLDER_ROUTE[index + 1..] {
            remaining.line_to(at(*point));
        }
        canvas.draw_path(&remaining.detach(), &self.remaining_paint);

        let mut completed = PathBuilder::new();
        completed.move_to(at(PLACEHOLDER_ROUTE[0]));
        for point in &PLACEHOLDER_ROUTE[1..=index] {
            completed.line_to(at(*point));
        }
        completed.line_to(split);
        canvas.draw_path(&completed.detach(), &self.completed_paint);
        (split, (b.x - a.x).atan2(a.y - b.y).to_degrees())
    }

    pub fn draw_marker(&self, canvas: &Canvas, center: Point, heading_deg: f32) {
        self.marker.draw_heading(canvas, center, heading_deg);
    }

    pub fn draw_attribution(&self, canvas: &Canvas) {
        self.attribution.draw(canvas);
    }
}
