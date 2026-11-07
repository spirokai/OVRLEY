//! What the widget draws itself: the flat placeholder, the marker, the attribution and the border.
//! The map and the route arrive inside the frame.
//!
//! The renderer crops the map to the shape, so preview and export crop it the same way. This clips
//! what the widget draws over it to that shape.
//!
//! The caret on a real map is a small extruded solid with a flat top, standing on the road at the
//! rider, seen through the map's own camera and lit by the buildings' light, with a soft shadow on the
//! ground beneath it. It draws in Skia over the finished frame, so preview and export share it and
//! buildings never hide it.
use super::validation::{MapShape, ValidatedMapWidget};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
    render::{
        preview::PreviewRasterScale,
        text::{parse_color, resolve_font, FontResolver},
    },
    widgets::{
        plot_marker::{plot_marker_radius, MarkerVariant, PlotMarker},
        route::{
            geometry::ProgressLine,
            layout::{stroke_paint, SplitStroke},
        },
    },
};
use ovrley_map::{MapCamera, MapRouteStyle, MapStyleInfo, MapViewport, ViewState, LIGHT_XY};
use skia_safe::{
    paint::Style, AlphaType, BlurStyle, Canvas, Color, ContourMeasure, ContourMeasureIter,
    FilterMode, Font, IRect, Image, MaskFilter, Paint, PaintJoin, PaintStyle, Path, PathBuilder,
    Point, RRect, RSXform, Rect, TextBlob,
};
use std::{
    f32::consts::FRAC_1_SQRT_2,
    sync::{Arc, LazyLock},
};

/// The map's sources by name: the tile host, the tile schema and the data.
const ATTRIBUTION: &str = "OpenFreeMap \u{a9} OpenMapTiles \u{a9} OpenStreetMap";
const ATTRIBUTION_FONT_SIZE: f32 = 8.0;
const ATTRIBUTION_PADDING: f32 = 2.0;
const ATTRIBUTION_TEXT_OPACITY: f32 = 0.8;
const ATTRIBUTION_BACKING_OPACITY: f32 = 0.65;
/// The most of a curved edge the credit fills, so it never wraps up the sides.
const ATTRIBUTION_MAX_EDGE_SHARE: f32 = 0.9;

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
        if !edge.get_segment(
            start - padding,
            start + length + padding,
            &mut backing,
            true,
        ) {
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

/// The top-down route as the widget strokes it over a frame that never changes: its line in the
/// viewport's output pixels, which the frame shows whole, and that line's stroke under one layout.
pub struct ScreenRoute {
    pub line: Arc<ProgressLine>,
    viewport: MapViewport,
    stroke: SplitStroke,
}

/// The caret stands in the scene. The circles mark a point, so they stay flat on the frame.
enum MapMarker {
    Arrow(MapArrow),
    Flat(PlotMarker),
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
    /// Whole scene pixels each corner reaches along the frame's edges.
    corner_margin: i32,
    border: Option<(RRect, Paint)>,
    placeholder_paint: Paint,
    image_paint: Paint,
    /// The remaining and completed sides of the top-down route, in output pixels as wide as
    /// `route_style`'s, for [`Self::screen_route`].
    screen_paints: [Paint; 2],
    /// The stand-in route across the frame, in scene pixels, and its stroke.
    placeholder: ProgressLine,
    placeholder_stroke: SplitStroke,
    marker: MapMarker,
    attribution: Attribution,
}

impl MapLayout {
    /// `style` is the installed style the widget's own resolved to.
    pub fn new(
        config: &ValidatedMapWidget,
        style: &MapStyleInfo,
        scene: &ValidatedScenePresentation,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        // On whole pixels, like the frame the renderer draws, so export copies it without resampling.
        // Performance: a fractional frame fails the 1:1 test in `copy_opaque_interior` and silently
        // falls back to blending: frame draw time went from about 34 to 22 ms on the 4K benchmark.
        let frame = Rect::from_xywh(
            config.x.round(),
            config.y.round(),
            (config.width * scene.scale).round(),
            (config.height * scene.scale).round(),
        );
        let path = &config.path;
        let line_width = (config.line_width * scene.scale).max(1.0);
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
        // The style's own `background-color`, shown until its frame arrives.
        let [red, green, blue] = style.background;
        placeholder_paint.set_color(Color::from_rgb(red, green, blue));
        placeholder_paint.set_alpha_f(opacity);
        let mut image_paint = Paint::default();
        image_paint.set_alpha_f(opacity);
        // Widget and scene opacity apply to the finished frame as a whole.
        let route_style = MapRouteStyle {
            completed_color: color_floats(&path.completed_line_color, path.completed_line_opacity),
            remaining_color: color_floats(&path.remaining_line_color, path.remaining_line_opacity),
            width_px: line_width * raster_scale.value(),
        };
        // What the widget strokes itself draws over the frame, so it carries the widget's opacity.
        let route_paints = |width: f32| {
            [
                stroke_paint(
                    &path.remaining_line_color,
                    width,
                    path.remaining_line_opacity * opacity,
                ),
                stroke_paint(
                    &path.completed_line_color,
                    width,
                    path.completed_line_opacity * opacity,
                ),
            ]
        };
        let placeholder = ProgressLine::placeholder(frame);
        let [remaining, completed] = route_paints(line_width);
        let placeholder_stroke = SplitStroke::new(&placeholder, frame, remaining, completed);
        Ok(Self {
            frame,
            opacity,
            clip: RRect::new_rect_xy(frame, corner_radius, corner_radius),
            corner_radius_px: corner_radius * raster_scale.value(),
            corner_margin: corner_radius.ceil() as i32,
            border,
            screen_paints: route_paints(route_style.width_px),
            route_style,
            placeholder_paint,
            image_paint,
            placeholder,
            placeholder_stroke,
            marker: match path.marker_variant {
                MarkerVariant::Caret => MapMarker::Arrow(MapArrow::new(
                    plot_marker_radius(path.marker_size, scene.scale),
                    &path.marker_color,
                    path.marker_opacity * opacity,
                )),
                variant => MapMarker::Flat(PlotMarker::new(
                    variant,
                    path.marker_size,
                    path.marker_variant_diameter,
                    &path.marker_color,
                    path.marker_opacity * opacity,
                    scene.scale,
                )),
            },
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

    /// The finished frame over the widget's bounds, already cropped to its shape. Only its corners
    /// can show what is beneath, so a frame that lands 1:1 copies the rest instead of blending it.
    pub fn draw_image(&self, canvas: &Canvas, image: &Image) {
        if !self.copy_opaque_interior(canvas, image) {
            self.blend_image(canvas, image);
            return;
        }
        if self.corner_margin == 0 {
            return;
        }
        let margin = self.corner_margin as f32;
        let frame = self.frame;
        for (x, y) in [
            (frame.left, frame.top),
            (frame.right - margin, frame.top),
            (frame.left, frame.bottom - margin),
            (frame.right - margin, frame.bottom - margin),
        ] {
            canvas.save();
            canvas.clip_rect(Rect::from_xywh(x, y, margin, margin), None, false);
            self.blend_image(canvas, image);
            canvas.restore();
        }
    }

    fn blend_image(&self, canvas: &Canvas, image: &Image) {
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            self.frame,
            FilterMode::Linear,
            &self.image_paint,
        );
    }

    /// Copies everything outside the corners, and tells whether it did. Only styles with an opaque
    /// background install (see `ovrley_map::read_style_file`) and the renderer's shape only crops
    /// inside the corners, so these pixels are opaque. Writing ignores the clip, so it stays inside the
    /// rectangle that bounds it: the host's slot and this shape, whose only non-rectangular parts are
    /// the corners left out here. Registration reads the background at one zoom, so a style that turns
    /// it translucent at another still breaks this assumption, and no test catches it.
    fn copy_opaque_interior(&self, canvas: &Canvas, image: &Image) -> bool {
        let (width, height) = (image.width(), image.height());
        let margin = self.corner_margin;
        let matrix = canvas.local_to_device_as_3x3();
        let (left, top) = (
            self.frame.left + matrix.translate_x(),
            self.frame.top + matrix.translate_y(),
        );
        let lands_one_to_one = matrix.is_translate()
            && left.fract() == 0.0
            && top.fract() == 0.0
            && self.frame.width() == width as f32
            && self.frame.height() == height as f32;
        if self.opacity != 1.0 || !lands_one_to_one || margin * 2 >= width.min(height) {
            return false;
        }
        let (Some(pixmap), Some(clip)) = (image.peek_pixels(), canvas.device_clip_bounds()) else {
            return false;
        };
        let Some(bytes) = pixmap.bytes() else {
            return false;
        };
        let device = IRect::from_xywh(left as i32, top as i32, width, height);
        let row_bytes = pixmap.row_bytes();
        let pixel_bytes = image.image_info().bytes_per_pixel();
        [
            IRect::new(
                device.left + margin,
                device.top,
                device.right - margin,
                device.top + margin,
            ),
            IRect::new(
                device.left,
                device.top + margin,
                device.right,
                device.bottom - margin,
            ),
            IRect::new(
                device.left + margin,
                device.bottom - margin,
                device.right - margin,
                device.bottom,
            ),
        ]
        .iter()
        .all(|band| {
            let Some(visible) = IRect::intersect(band, &clip) else {
                return true;
            };
            let start = (visible.top - device.top) as usize * row_bytes
                + (visible.left - device.left) as usize * pixel_bytes;
            let info = image
                .image_info()
                .with_dimensions(visible.size())
                .with_alpha_type(AlphaType::Opaque);
            canvas.write_pixels(
                &info,
                &bytes[start..],
                row_bytes,
                (visible.left, visible.top),
            )
        })
    }

    pub fn draw_placeholder(&self, canvas: &Canvas) {
        canvas.draw_rect(self.frame, &self.placeholder_paint);
    }

    /// The stand-in route when there's no GPS, in Skia because no frame exists to carry it. Returns
    /// where its marker sits, with the heading there.
    pub fn draw_placeholder_route(&self, canvas: &Canvas, progress: f32) -> (Point, f32) {
        self.placeholder_stroke
            .draw(canvas, &self.placeholder, progress, [1.0, 1.0])
    }

    /// The top-down route's stroke for `line`, which is in `viewport`'s output pixels. Only what
    /// crosses the frame is kept, however far the map is zoomed in.
    pub fn screen_route(&self, line: Arc<ProgressLine>, viewport: MapViewport) -> ScreenRoute {
        let [remaining, completed] = self.screen_paints.clone();
        let within = Rect::from_wh(viewport.width_px() as f32, viewport.height_px() as f32);
        ScreenRoute {
            stroke: SplitStroke::new(&line, within, remaining, completed),
            line,
            viewport,
        }
    }

    /// The top-down route over the frame, split at `travelled_metres`, with no shadow. The frame
    /// shows the route's viewport whole, so the route lands where the renderer put its roads.
    pub fn draw_route(&self, canvas: &Canvas, route: &ScreenRoute, travelled_metres: f32) {
        let viewport = route.viewport;
        canvas.save();
        canvas.translate((self.frame.left, self.frame.top));
        route.stroke.draw(
            canvas,
            &route.line,
            travelled_metres,
            [
                self.frame.width() / viewport.width_px() as f32,
                self.frame.height() / viewport.height_px() as f32,
            ],
        );
        canvas.restore();
    }

    pub fn draw_marker(&self, canvas: &Canvas, pose: &MarkerPose) {
        match &self.marker {
            MapMarker::Arrow(arrow) => arrow.draw(canvas, pose),
            MapMarker::Flat(marker) => marker.draw(canvas, pose.center),
        }
    }

    pub fn draw_attribution(&self, canvas: &Canvas) {
        self.attribution.draw(canvas);
    }
}

/// Scene pixels each axis is measured over: long enough to survive rounding, short enough to stay
/// straight under perspective.
const AXIS_STEP_PX: f64 = 8.0;

/// The solid at radius 1: `x` right, `y` forward, `z` up. Walls rise from the ground to [`TOP`]. The
/// underside always faces away from a camera above the ground, so it isn't drawn.
const TOP: f32 = 0.4;
/// The outline's corners, clockwise seen from above: tip, right, notch, left. The rounded tip reaches
/// the origin and the body trails behind. The sharp corner sits past the origin by what
/// [`CORNER_ROUNDING`] takes off it.
const CORNERS: [[f32; 2]; 4] = [[0.0, 0.14], [0.95, -2.21], [0.0, -1.71], [-0.95, -2.21]];
/// From this pitch on, the tip sits on the rider. A pitched view looks along the road, where a solid
/// centred on the rider would seem to run ahead of them. Straight down, its middle sits on the rider so
/// it turns in place.
const TIP_ANCHOR_PITCH_DEG: f32 = 60.0;
/// How far along its edges each corner's rounding starts.
const CORNER_ROUNDING: f32 = 0.3;
/// Segments each rounded corner is drawn with.
const CORNER_SEGMENTS: usize = 6;

/// One flat side of the extrusion.
struct Wall {
    quad: [[f32; 3]; 4],
    /// Outward and unit length.
    normal: [f32; 3],
}

struct ArrowSolid {
    outline: Vec<[f32; 2]>,
    walls: Vec<Wall>,
    /// Forward extent of the rounded outline: its tip, and halfway from its tail to its tip.
    tip: f32,
    middle: f32,
}

impl ArrowSolid {
    /// The forward position on the solid that sits on the rider at this camera pitch.
    fn anchor_at(&self, pitch_deg: f32) -> f32 {
        let toward_tip = (pitch_deg / TIP_ANCHOR_PITCH_DEG).min(1.0);
        self.middle + (self.tip - self.middle) * toward_tip
    }
}

/// The same shape for every arrow, so it's built once.
static SOLID: LazyLock<ArrowSolid> = LazyLock::new(|| {
    let outline = rounded_outline();
    let walls = (0..outline.len())
        .map(|index| {
            let (a, b) = (outline[index], outline[(index + 1) % outline.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = dx.hypot(dy);
            Wall {
                quad: [
                    [a[0], a[1], 0.0],
                    [b[0], b[1], 0.0],
                    [b[0], b[1], TOP],
                    [a[0], a[1], TOP],
                ],
                // A clockwise outline has the outside on its left.
                normal: [-dy / length, dx / length, 0.0],
            }
        })
        .collect();
    let (tail, tip) = outline
        .iter()
        .fold((f32::MAX, f32::MIN), |(tail, tip), point| {
            (tail.min(point[1]), tip.max(point[1]))
        });
    ArrowSolid {
        outline,
        walls,
        tip,
        middle: (tail + tip) / 2.0,
    }
});

/// [`CORNERS`] with each corner replaced by a quadratic curve between its two edges.
fn rounded_outline() -> Vec<[f32; 2]> {
    let count = CORNERS.len();
    let toward = |from: [f32; 2], to: [f32; 2]| {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = dx.hypot(dy);
        [
            from[0] + dx / length * CORNER_ROUNDING,
            from[1] + dy / length * CORNER_ROUNDING,
        ]
    };
    let mut outline = Vec::with_capacity(count * (CORNER_SEGMENTS + 1));
    for index in 0..count {
        let corner = CORNERS[index];
        let start = toward(corner, CORNERS[(index + count - 1) % count]);
        let end = toward(corner, CORNERS[(index + 1) % count]);
        for step in 0..=CORNER_SEGMENTS {
            let t = step as f32 / CORNER_SEGMENTS as f32;
            let (from_start, from_corner, from_end) =
                ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
            outline.push([
                from_start * start[0] + from_corner * corner[0] + from_end * end[0],
                from_start * start[1] + from_corner * corner[1] + from_end * end[1],
            ]);
        }
    }
    outline
}

/// The building shader's light, so the arrow's walls turn as a building's do. MapLibre's light is longer
/// than 1, which saturates anything facing up; [`LIGHT_LENGTH`] keeps the top from washing out.
const LIGHT_Z: f32 = 0.996;
const LIGHT_LENGTH: f32 = 1.15;
const LIGHT_INTENSITY: f32 = 0.5;
const AMBIENT: f32 = 0.03;

const SHADOW_OPACITY: f32 = 0.45;
/// Blur sigma as a share of the radius.
const SHADOW_BLUR: f32 = 0.16;
/// How far the light pushes the shadow from under the arrow, as a share of the radius.
const SHADOW_SHIFT: f32 = 0.2;
/// Faces grow by this stroke in scene pixels, so antialiased edges leave no seam between them.
const SEAM_STROKE: f32 = 0.6;

fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}

fn scaled(vector: Point, by: f32) -> Point {
    Point::new(vector.x * by, vector.y * by)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `extrusion.vertex.wgsl`'s lighting of one surface, with its light at unit length.
fn shaded(color: Color, normal: [f32; 3], light_xy: [f32; 2]) -> Color {
    let channels = [color.r(), color.g(), color.b()].map(|channel| f32::from(channel) / 255.0);
    let light = (dot(normal, [light_xy[0], light_xy[1], LIGHT_Z]) / LIGHT_LENGTH).clamp(0.0, 1.0);
    let luminance = 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
    let highlight = (1.0 - luminance + LIGHT_INTENSITY).max(1.0);
    let directional = (1.0 - LIGHT_INTENSITY) + (highlight - (1.0 - LIGHT_INTENSITY)) * light;
    let [r, g, b] =
        channels.map(|channel| (((channel + AMBIENT) * directional).clamp(0.0, 1.0) * 255.0) as u8);
    Color::from_rgb(r, g, b)
}

/// How one frame sees the arrow. It is tiny against the view, so one linear map is enough: scene pixels
/// per scene pixel of the solid along its axes.
#[derive(Clone, Copy, Debug)]
pub struct MarkerPose {
    pub center: Point,
    right: Point,
    forward: Point,
    up: Point,
    /// The ground part of the light, along `right` and `forward`.
    light: [f32; 2],
    /// The solid's forward position that sits on `center`, from [`ArrowSolid::anchor_at`].
    anchor: f32,
}

impl MarkerPose {
    /// The arrow on the road at a world position, facing `bearing_deg` clockwise from north. `None`
    /// when any of it is behind the camera. `frame` is the widget's frame in scene pixels.
    pub fn on_camera(
        camera: &MapCamera,
        viewport: MapViewport,
        frame: Rect,
        (world_x, world_y): (f64, f64),
        bearing_deg: f64,
    ) -> Option<Self> {
        let view = ViewState::new(camera, viewport);
        let projection = view.ground_projection();
        let project = |x: f64, y: f64, height: f64| {
            let clip = projection.clip_above(x, y, height);
            (clip.z >= 0.0).then(|| {
                let (x, y) = clip.window_share();
                (
                    f64::from(frame.left) + x * f64::from(frame.width()),
                    f64::from(frame.top) + y * f64::from(frame.height()),
                )
            })
        };
        // World pixels are output pixels, which the frame's scene pixels are rasterised to.
        let world_px_per_scene_px = f64::from(viewport.width_px()) / f64::from(frame.width());
        let step = AXIS_STEP_PX * world_px_per_scene_px / view.world_size();
        let center = project(world_x, world_y, 0.0)?;
        let axis = |at: (f64, f64)| {
            Point::new(
                ((at.0 - center.0) / AXIS_STEP_PX) as f32,
                ((at.1 - center.1) / AXIS_STEP_PX) as f32,
            )
        };
        // World y runs south.
        let (sin, cos) = bearing_deg.to_radians().sin_cos();
        let right = axis(project(world_x + cos * step, world_y + sin * step, 0.0)?);
        let forward = axis(project(world_x + sin * step, world_y - cos * step, 0.0)?);
        let up = axis(project(
            world_x,
            world_y,
            AXIS_STEP_PX * world_px_per_scene_px,
        )?);
        let [light_x, light_y] = view.light_xy().map(f64::from);
        Some(Self {
            center: Point::new(center.0 as f32, center.1 as f32),
            right,
            forward,
            up,
            light: [
                (light_x * cos + light_y * sin) as f32,
                (light_x * sin - light_y * cos) as f32,
            ],
            anchor: SOLID.anchor_at(camera.pitch_deg() as f32),
        })
    }

    /// Straight down with north up, for a flat stand-in route with no camera.
    pub fn top_down(center: Point, heading_deg: f32) -> Self {
        let (sin, cos) = heading_deg.to_radians().sin_cos();
        let (light_x, light_y) = (LIGHT_XY.0 as f32, LIGHT_XY.1 as f32);
        Self {
            center,
            right: Point::new(cos, sin),
            forward: Point::new(sin, -cos),
            up: Point::new(0.0, 0.0),
            light: [light_x * cos + light_y * sin, light_x * sin - light_y * cos],
            anchor: SOLID.middle,
        }
    }

    /// Scene position of a point on the solid at `radius`.
    fn place(&self, [x, y, z]: [f32; 3], radius: f32) -> Point {
        add(
            add(self.center, scaled(self.right, x * radius)),
            add(
                scaled(self.forward, (y - self.anchor) * radius),
                scaled(self.up, z * radius),
            ),
        )
    }

    /// The solid's axis toward the camera: the one direction this linear map flattens away.
    fn toward_camera(&self) -> [f32; 3] {
        let axis = cross(
            [self.right.x, self.forward.x, self.up.x],
            [self.right.y, self.forward.y, self.up.y],
        );
        if axis[2] < 0.0 {
            axis.map(|value| -value)
        } else {
            axis
        }
    }

    /// The walls the camera sees, far to near, so nearer ones paint over. The top paints over them all.
    fn visible_walls(&self) -> Vec<&'static Wall> {
        let toward = self.toward_camera();
        let mut walls: Vec<_> = SOLID
            .walls
            .iter()
            .filter(|wall| dot(wall.normal, toward) > 0.0)
            .collect();
        let depth = |wall: &Wall| dot(wall.quad[0], toward) + dot(wall.quad[1], toward);
        walls.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
        walls
    }
}

/// Prepared colours and scene-pixel size of one arrow style.
#[derive(Clone, Debug)]
pub struct MapArrow {
    radius: f32,
    color: Color,
    opacity: f32,
    shadow: Paint,
}

impl MapArrow {
    /// `radius` is in scene pixels; `opacity` is the final opacity.
    pub fn new(radius: f32, color: &str, opacity: f32) -> Self {
        let mut shadow = Paint::default();
        shadow.set_anti_alias(true);
        shadow.set_color(Color::BLACK);
        shadow.set_alpha_f(SHADOW_OPACITY * opacity);
        shadow.set_mask_filter(MaskFilter::blur(
            BlurStyle::Normal,
            radius * SHADOW_BLUR,
            None,
        ));
        Self {
            radius,
            color: parse_color(color, 1.0),
            opacity,
            shadow,
        }
    }

    pub fn draw(&self, canvas: &Canvas, pose: &MarkerPose) {
        self.draw_shadow(canvas, pose);
        // Faces overlap by their seam stroke, so opacity applies to the finished solid.
        if self.opacity < 1.0 {
            canvas.save_layer_alpha_f(None, self.opacity);
        }
        let face = |points: &mut dyn Iterator<Item = [f32; 3]>, normal: [f32; 3]| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(Style::StrokeAndFill);
            paint.set_stroke_width(SEAM_STROKE);
            paint.set_color(shaded(self.color, normal, pose.light));
            let scene = points.map(|point| pose.place(point, self.radius));
            canvas.draw_path(&closed_path(scene), &paint);
        };
        for wall in pose.visible_walls() {
            face(&mut wall.quad.iter().copied(), wall.normal);
        }
        face(
            &mut SOLID.outline.iter().map(|[x, y]| [*x, *y, TOP]),
            [0.0, 0.0, 1.0],
        );
        if self.opacity < 1.0 {
            canvas.restore();
        }
    }

    /// The footprint on the ground, pushed away from the light.
    fn draw_shadow(&self, canvas: &Canvas, pose: &MarkerPose) {
        let shift = [-pose.light[0] * SHADOW_SHIFT, -pose.light[1] * SHADOW_SHIFT];
        let ground = SOLID
            .outline
            .iter()
            .map(|[x, y]| pose.place([x + shift[0], y + shift[1], 0.0], self.radius));
        canvas.draw_path(&closed_path(ground), &self.shadow);
    }
}

fn closed_path(mut points: impl Iterator<Item = Point>) -> Path {
    let mut path = PathBuilder::new();
    if let Some(first) = points.next() {
        path.move_to(first);
    }
    for point in points {
        path.line_to(point);
    }
    path.close();
    path.detach()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (f32, f32) = (400.0, 300.0);

    fn pose(pitch: f64, camera_bearing: f64, marker_bearing: f64) -> MarkerPose {
        let camera = MapCamera::new(14.42, 50.08, 16.0, pitch, camera_bearing, 0.0).unwrap();
        let viewport = MapViewport::new(800, 600, 1.0).unwrap();
        let frame = Rect::from_wh(FRAME.0, FRAME.1);
        let world = ovrley_map::mercator::lon_lat_to_world(14.42, 50.08);
        MarkerPose::on_camera(&camera, viewport, frame, world, marker_bearing).unwrap()
    }

    fn close(a: Point, b: (f32, f32)) {
        assert!(
            (a.x - b.0).abs() < 1e-3 && (a.y - b.1).abs() < 1e-3,
            "{a:?} is not {b:?}"
        );
    }

    #[test]
    fn looking_straight_down_at_north_keeps_scene_pixels() {
        let pose = pose(0.0, 0.0, 0.0);
        close(pose.center, (200.0, 150.0));
        close(pose.right, (1.0, 0.0));
        close(pose.forward, (0.0, -1.0));
        close(pose.up, (0.0, 0.0));
    }

    #[test]
    fn straight_down_matches_the_placeholder_pose() {
        let (camera, flat) = (
            pose(0.0, 0.0, 90.0),
            MarkerPose::top_down(Point::new(200.0, 150.0), 90.0),
        );
        close(camera.right, (flat.right.x, flat.right.y));
        close(camera.forward, (flat.forward.x, flat.forward.y));
        assert!((camera.light[0] - flat.light[0]).abs() < 1e-3);
        assert!((camera.light[1] - flat.light[1]).abs() < 1e-3);
    }

    #[test]
    fn pitch_foreshortens_the_road_and_stands_the_arrow_up() {
        let pose = pose(60.0, 0.0, 0.0);
        assert!(pose.up.y < -0.5, "up should rise on screen: {:?}", pose.up);
        assert!(
            pose.forward.y > -0.75,
            "forward should foreshorten: {:?}",
            pose.forward
        );
        assert!(pose.forward.y < 0.0);
    }

    #[test]
    fn a_follow_camera_sees_the_rear_walls_and_a_top_view_does_not() {
        let behind = pose(60.0, 0.0, 0.0);
        let walls = behind.visible_walls();
        assert!(!walls.is_empty());
        assert!(walls.iter().all(|wall| wall.normal[1] < 0.0));
        assert!(pose(0.0, 0.0, 0.0).visible_walls().is_empty());
    }

    #[test]
    fn the_top_is_flat_and_every_corner_is_rounded() {
        for wall in &SOLID.walls {
            assert_eq!([wall.quad[2][2], wall.quad[3][2]], [TOP, TOP]);
        }
        for corner in CORNERS {
            assert!(!SOLID.outline.contains(&corner), "{corner:?} stayed sharp");
        }
    }

    #[test]
    fn the_rounded_tip_reaches_the_rider_and_the_body_trails_behind() {
        let tip = SOLID
            .outline
            .iter()
            .map(|point| point[1])
            .fold(f32::MIN, f32::max);
        assert!(tip.abs() < 0.01, "{tip}");
        let middle = SOLID
            .outline
            .iter()
            .position(|point| point[1] == tip)
            .unwrap();
        assert!(SOLID.outline[middle][0].abs() < 0.01);
    }

    #[test]
    fn the_rider_holds_the_middle_from_above_and_the_tip_from_a_pitched_view() {
        let on_rider = |pose: &MarkerPose, forward: f32| {
            close(
                pose.place([0.0, forward, 0.0], 10.0),
                (pose.center.x, pose.center.y),
            )
        };
        on_rider(&pose(0.0, 0.0, 0.0), SOLID.middle);
        on_rider(
            &MarkerPose::top_down(Point::new(200.0, 150.0), 30.0),
            SOLID.middle,
        );
        on_rider(&pose(30.0, 0.0, 0.0), (SOLID.middle + SOLID.tip) / 2.0);
        on_rider(&pose(60.0, 0.0, 0.0), SOLID.tip);
        assert!(
            SOLID.middle < -0.9,
            "the middle trails the tip: {}",
            SOLID.middle
        );
    }

    #[test]
    fn the_camera_turning_with_the_road_keeps_the_arrow_on_screen_unchanged() {
        let (north, east) = (pose(50.0, 0.0, 0.0), pose(50.0, 90.0, 90.0));
        close(north.right, (east.right.x, east.right.y));
        close(north.forward, (east.forward.x, east.forward.y));
        close(north.up, (east.up.x, east.up.y));
        assert!((north.light[0] - east.light[0]).abs() < 1e-3);
        assert!((north.light[1] - east.light[1]).abs() < 1e-3);
    }

    #[test]
    fn the_lit_side_is_brighter_than_the_shaded_side() {
        let color = Color::from_rgb(220, 60, 40);
        let top = shaded(color, [0.0, 0.0, 1.0], [0.3, 0.5]);
        let rear = shaded(color, [0.0, -1.0, 0.0], [0.3, 0.5]);
        assert!(top.r() > rear.r());
    }
}
