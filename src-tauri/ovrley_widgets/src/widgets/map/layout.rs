//! What the widget draws itself: the flat placeholder, the marker and the attribution. The map and
//! the route arrive inside the frame.
use super::validation::ValidatedMapWidget;
use crate::{
    error::WidgetResult,
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
    Canvas, Color, FilterMode, Font, Image, Paint, PathBuilder, Point, Rect, TextBlob,
};

/// OpenFreeMap's required credit, as `map_tile_service.rs` serves it to MapLibre.
const ATTRIBUTION: &str = "OpenFreeMap \u{a9} OpenMapTiles Data from OpenStreetMap";
const ATTRIBUTION_FONT_SIZE: f32 = 11.0;
const ATTRIBUTION_PADDING: f32 = 3.0;
const ATTRIBUTION_TEXT_OPACITY: f32 = 0.8;
const ATTRIBUTION_BACKING_OPACITY: f32 = 0.65;

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

struct Attribution {
    blob: Option<TextBlob>,
    origin: Point,
    backing: Rect,
    text_paint: Paint,
    backing_paint: Paint,
}

impl Attribution {
    /// Fixed text at the bottom right, shrinking to fit the frame.
    fn new(
        config: &ValidatedMapWidget,
        scene: &ValidatedScenePresentation,
        frame: Rect,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let padding = ATTRIBUTION_PADDING * scene.scale;
        let mut font: Font = resolve_font(
            fonts,
            Some(&config.font),
            ATTRIBUTION_FONT_SIZE * scene.scale,
        )?;
        let room = frame.width() - padding * 2.0;
        let (natural, _) = font.measure_str(ATTRIBUTION, None);
        if natural > room {
            font.set_size(font.size() * (room / natural).max(f32::EPSILON));
        }
        let (width, _) = font.measure_str(ATTRIBUTION, None);
        let (_, metrics) = font.metrics();
        let height = metrics.descent - metrics.ascent + padding * 2.0;
        let backing = Rect::new(
            frame.right - width - padding * 2.0,
            frame.bottom - height,
            frame.right,
            frame.bottom,
        );
        let paint = |opacity: f32, rgb: u8| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(Color::from_rgb(rgb, rgb, rgb));
            paint.set_alpha_f(opacity * scene.opacity);
            paint
        };
        Ok(Self {
            blob: TextBlob::from_str(ATTRIBUTION, &font),
            origin: Point::new(backing.left + padding, backing.top + padding - metrics.ascent),
            backing,
            text_paint: paint(ATTRIBUTION_TEXT_OPACITY, 0),
            backing_paint: paint(ATTRIBUTION_BACKING_OPACITY, 255),
        })
    }

    fn draw(&self, canvas: &Canvas) {
        canvas.draw_rect(self.backing, &self.backing_paint);
        if let Some(blob) = &self.blob {
            canvas.draw_text_blob(blob, self.origin, &self.text_paint);
        }
    }
}

/// One widget's scene geometry and paints, rebuilt on any config change.
pub struct MapLayout {
    /// The frame in scene pixels.
    pub frame: Rect,
    pub route_style: MapRouteStyle,
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
        let mut placeholder_paint = Paint::default();
        placeholder_paint.set_color(placeholder_color(config.spec.style));
        placeholder_paint.set_alpha_f(scene.opacity);
        let mut image_paint = Paint::default();
        image_paint.set_alpha_f(scene.opacity);
        Ok(Self {
            frame,
            // Scene opacity applies to the finished frame as a whole.
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
                path.completed_line_opacity * scene.opacity,
            ),
            remaining_paint: stroke_paint(
                &path.remaining_line_color,
                remaining_width,
                path.remaining_line_opacity * scene.opacity,
            ),
            marker: PlotMarker::new(
                path.marker_variant,
                path.marker_size,
                path.marker_variant_diameter,
                &path.marker_color,
                path.marker_opacity * scene.opacity,
                scene.scale,
            ),
            attribution: Attribution::new(config, scene, frame, fonts)?,
        })
    }

    /// The finished frame over the widget's bounds. Rescales it during a live resize.
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
    pub fn draw_placeholder_route(&self, canvas: &Canvas, progress: f32) -> Point {
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
        split
    }

    pub fn draw_marker(&self, canvas: &Canvas, center: Point) {
        self.marker.draw(canvas, center);
    }

    pub fn draw_attribution(&self, canvas: &Canvas) {
        self.attribution.draw(canvas);
    }
}
