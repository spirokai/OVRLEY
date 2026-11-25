//! Backdrop shapes: rectangles and circles with fill, border, coloured edge strips and shadow.
use crate::model::catalog::{BackdropEdge, BackdropType};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_bool, require_border_fits_dimension, require_f32, require_hex_color,
    require_positive_f32, require_string, require_unit_opacity, rgba_from_hex,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::parse_color;
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use skia_safe::paint::Style;
use skia_safe::{Canvas, ClipOp, Paint, Path, PathBuilder, Point, Rect};
use std::collections::BTreeMap;

/// Static geometric backdrop drawn below other widgets.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct BackdropConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
    pub display_type: BackdropType,
    #[serde(default)]
    pub fill_color: Option<String>,
    #[serde(default)]
    pub fill_opacity: Option<f32>,
    #[serde(default)]
    pub border_thickness: Option<f32>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_opacity: Option<f32>,
    #[serde(default)]
    pub display_variants: BTreeMap<String, Value>,
    #[serde(default)]
    pub diameter: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub corner_radius: Option<f32>,
    #[serde(default)]
    pub round_top_left: Option<bool>,
    #[serde(default)]
    pub round_top_right: Option<bool>,
    #[serde(default)]
    pub round_bottom_left: Option<bool>,
    #[serde(default)]
    pub round_bottom_right: Option<bool>,
    #[serde(default)]
    pub edge_strips: Option<Vec<BackdropEdge>>,
    #[serde(default)]
    pub edge_strip_thickness: Option<f32>,
    #[serde(default)]
    pub edge_strip_color: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl BackdropConfig {
    /// Resolve saved active-variant fields at ingress. Explicit flat fields win.
    pub(super) fn with_promoted_display_variant(&self, variant_key: &str) -> WidgetResult<Self> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| WidgetError::Config(format!("backdrop config serialization: {e}")))?;
        // Option fields serialize as null; omit them before promoting saved geometry.
        raw.as_object_mut()
            .expect("serialized backdrop is an object")
            .retain(|_, value| !value.is_null());
        if let Some(variant) = self
            .display_variants
            .get(variant_key)
            .and_then(Value::as_object)
        {
            let root = raw
                .as_object_mut()
                .expect("serialized backdrop is an object");
            for (key, value) in variant {
                root.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        serde_json::from_value(raw)
            .map_err(|e| WidgetError::Config(format!("backdrop config: {e}")))
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedBackdrop {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
    pub display_type: BackdropType,
    pub fill_color: String,
    pub fill_opacity: f32,
    pub border_thickness: f32,
    pub border_color: String,
    pub border_opacity: f32,
    pub diameter: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
    pub round_top_left: bool,
    pub round_top_right: bool,
    pub round_bottom_left: bool,
    pub round_bottom_right: bool,
    /// Rectangle edges painted with an inward strip; empty for circles.
    pub edge_strips: Vec<BackdropEdge>,
    pub edge_strip_thickness: f32,
    pub edge_strip_color: String,
}

struct BackdropShared {
    id: String,
    x: f32,
    y: f32,
    opacity: f32,
    fill_color: String,
    fill_opacity: f32,
    border_thickness: f32,
    border_color: String,
    border_opacity: f32,
}

pub fn validate_backdrop(
    backdrop: &BackdropConfig,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let promoted = backdrop.with_promoted_display_variant(backdrop.display_type.as_str())?;
    let p = |field: &str| format!("backdrops[{index}].{field}");

    let id = require_string(promoted.id.clone(), &p("id"))?;
    if id.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("id"))));
    }

    let fill_color = require_hex_color(promoted.fill_color.as_deref(), &p("fill_color"))?;
    rgba_from_hex(&fill_color, &p("fill_color"), 1.0)?;
    let border_color = require_hex_color(promoted.border_color.as_deref(), &p("border_color"))?;
    rgba_from_hex(&border_color, &p("border_color"), 1.0)?;

    let shared = BackdropShared {
        id,
        x: require_f32(promoted.x, &p("x"))?,
        y: require_f32(promoted.y, &p("y"))?,
        opacity: require_unit_opacity(promoted.opacity, &p("opacity"))?,
        fill_color,
        fill_opacity: require_unit_opacity(promoted.fill_opacity, &p("fill_opacity"))?,
        border_thickness: require_f32(promoted.border_thickness, &p("border_thickness"))?,
        border_color,
        border_opacity: require_unit_opacity(promoted.border_opacity, &p("border_opacity"))?,
    };

    if shared.border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {}",
            p("border_thickness"),
            shared.border_thickness
        )));
    }

    match promoted.display_type {
        BackdropType::Circle => validate_circle_backdrop(promoted, shared, index),
        BackdropType::Rectangle => validate_rectangle_backdrop(promoted, shared, index),
    }
}

fn validate_circle_backdrop(
    backdrop: BackdropConfig,
    shared: BackdropShared,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let p = |field: &str| format!("backdrops[{index}].{field}");
    let diameter = require_positive_f32(backdrop.diameter, &p("diameter"))?;
    require_border_fits_dimension(shared.border_thickness, diameter, &p("border_thickness"))?;

    Ok(ValidatedBackdrop {
        id: shared.id,
        x: shared.x,
        y: shared.y,
        opacity: shared.opacity,
        display_type: BackdropType::Circle,
        fill_color: shared.fill_color,
        fill_opacity: shared.fill_opacity,
        border_thickness: shared.border_thickness,
        border_color: shared.border_color,
        border_opacity: shared.border_opacity,
        diameter,
        width: 0.0,
        height: 0.0,
        corner_radius: 0.0,
        round_top_left: false,
        round_top_right: false,
        round_bottom_left: false,
        round_bottom_right: false,
        edge_strips: Vec::new(),
        edge_strip_thickness: 0.0,
        edge_strip_color: String::new(),
    })
}

fn validate_rectangle_backdrop(
    backdrop: BackdropConfig,
    shared: BackdropShared,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let p = |field: &str| format!("backdrops[{index}].{field}");
    let width = require_positive_f32(backdrop.width, &p("width"))?;
    let height = require_positive_f32(backdrop.height, &p("height"))?;
    require_border_fits_dimension(
        shared.border_thickness,
        width.min(height),
        &p("border_thickness"),
    )?;

    let round_top_left = require_bool(backdrop.round_top_left, &p("round_top_left"))?;
    let round_top_right = require_bool(backdrop.round_top_right, &p("round_top_right"))?;
    let round_bottom_left = require_bool(backdrop.round_bottom_left, &p("round_bottom_left"))?;
    let round_bottom_right = require_bool(backdrop.round_bottom_right, &p("round_bottom_right"))?;
    let mut corner_radius = require_f32(backdrop.corner_radius, &p("corner_radius"))?;
    if corner_radius < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {corner_radius}",
            p("corner_radius")
        )));
    }
    corner_radius = corner_radius.min(width.min(height) * 0.5);
    if [
        round_top_left,
        round_top_right,
        round_bottom_left,
        round_bottom_right,
    ]
    .iter()
    .any(|rounded| *rounded)
        && shared.border_thickness > corner_radius
    {
        corner_radius = shared.border_thickness;
    }

    let edge_strips = backdrop
        .edge_strips
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("edge_strips"))))?;
    for (position, edge) in edge_strips.iter().enumerate() {
        if edge_strips[..position].contains(edge) {
            return Err(WidgetError::Config(format!(
                "{}: duplicate edge {edge:?}",
                p("edge_strips")
            )));
        }
    }
    let edge_strip_thickness =
        require_f32(backdrop.edge_strip_thickness, &p("edge_strip_thickness"))?;
    if edge_strip_thickness < 0.0 || edge_strip_thickness * 2.0 > width.min(height) {
        return Err(WidgetError::Config(format!(
            "{}: must be within 0..=half the smaller dimension, got {edge_strip_thickness}",
            p("edge_strip_thickness")
        )));
    }
    let edge_strip_color =
        require_hex_color(backdrop.edge_strip_color.as_deref(), &p("edge_strip_color"))?;

    Ok(ValidatedBackdrop {
        id: shared.id,
        x: shared.x,
        y: shared.y,
        opacity: shared.opacity,
        display_type: BackdropType::Rectangle,
        fill_color: shared.fill_color,
        fill_opacity: shared.fill_opacity,
        border_thickness: shared.border_thickness,
        border_color: shared.border_color,
        border_opacity: shared.border_opacity,
        diameter: 0.0,
        width,
        height,
        corner_radius,
        round_top_left,
        round_top_right,
        round_bottom_left,
        round_bottom_right,
        edge_strips,
        edge_strip_thickness,
        edge_strip_color,
    })
}

#[derive(Clone, Copy)]
struct RectangleCorners {
    top_left: bool,
    top_right: bool,
    bottom_right: bool,
    bottom_left: bool,
}

pub(crate) fn draw_backdrop(
    canvas: &Canvas,
    backdrop: &ValidatedBackdrop,
    scale: f32,
    opacity: f32,
) {
    match backdrop.display_type {
        BackdropType::Circle => draw_circle_backdrop(canvas, backdrop, scale, opacity),
        BackdropType::Rectangle => draw_rectangle_backdrop(canvas, backdrop, scale, opacity),
    }
}

fn backdrop_paint(color: &str, opacity: f32, style: Style) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(style);
    paint.set_color(parse_color(color, opacity));
    paint
}

fn draw_circle_backdrop(canvas: &Canvas, backdrop: &ValidatedBackdrop, scale: f32, opacity: f32) {
    let diameter = backdrop.diameter * scale;
    let border = (backdrop.border_thickness * scale).max(0.0);
    let center = Point::new(backdrop.x + diameter * 0.5, backdrop.y + diameter * 0.5);
    let fill_alpha = opacity * backdrop.fill_opacity;
    let fill_paint = backdrop_paint(&backdrop.fill_color, fill_alpha, Style::Fill);

    canvas.draw_circle(center, (diameter * 0.5 - border).max(0.0), &fill_paint);

    if backdrop.border_thickness <= 0.0 {
        return;
    }

    let border_alpha = opacity * backdrop.border_opacity;
    let mut border_paint = backdrop_paint(&backdrop.border_color, border_alpha, Style::Stroke);
    border_paint.set_stroke_width(border);
    canvas.draw_circle(center, (diameter - border).max(0.0) * 0.5, &border_paint);
}

fn draw_rectangle_backdrop(
    canvas: &Canvas,
    backdrop: &ValidatedBackdrop,
    scale: f32,
    opacity: f32,
) {
    let width = backdrop.width * scale;
    let height = backdrop.height * scale;
    let border = (backdrop.border_thickness * scale).max(0.0);
    let radius = (backdrop.corner_radius * scale).max(0.0);
    let corners = RectangleCorners {
        top_left: backdrop.round_top_left,
        top_right: backdrop.round_top_right,
        bottom_right: backdrop.round_bottom_right,
        bottom_left: backdrop.round_bottom_left,
    };
    let fill_alpha = opacity * backdrop.fill_opacity;
    let fill_paint = backdrop_paint(&backdrop.fill_color, fill_alpha, Style::Fill);
    let fill_path = rounded_rect_path(
        backdrop.x + border,
        backdrop.y + border,
        (width - border * 2.0).max(0.0),
        (height - border * 2.0).max(0.0),
        (radius - border).max(0.0),
        corners,
    );

    canvas.draw_path(&fill_path, &fill_paint);

    if backdrop.border_thickness > 0.0 {
        let border_alpha = opacity * backdrop.border_opacity;
        let mut border_paint = backdrop_paint(&backdrop.border_color, border_alpha, Style::Stroke);
        border_paint.set_stroke_width(border);
        let stroke_inset = border * 0.5;
        let stroke_path = rounded_rect_path(
            backdrop.x + stroke_inset,
            backdrop.y + stroke_inset,
            (width - border).max(0.0),
            (height - border).max(0.0),
            radius,
            corners,
        );

        canvas.draw_path(&stroke_path, &border_paint);
    }

    if !backdrop.edge_strips.is_empty() && backdrop.edge_strip_thickness > 0.0 {
        let outer_path = rounded_rect_path(backdrop.x, backdrop.y, width, height, radius, corners);
        draw_rectangle_edge_strips(
            canvas,
            backdrop,
            &outer_path,
            scale,
            radius,
            corners,
            opacity,
        );
    }
}

/// Strips are painted inward over the fill and border, cut from the outer
/// shape so they follow its rounded corners without changing the frame.
fn draw_rectangle_edge_strips(
    canvas: &Canvas,
    backdrop: &ValidatedBackdrop,
    outer_path: &Path,
    scale: f32,
    radius: f32,
    corners: RectangleCorners,
    opacity: f32,
) {
    let strip = backdrop.edge_strip_thickness * scale;
    let has = |edge| backdrop.edge_strips.contains(&edge);
    let (top, right, bottom, left) = (
        has(BackdropEdge::Top),
        has(BackdropEdge::Right),
        has(BackdropEdge::Bottom),
        has(BackdropEdge::Left),
    );
    // Sides without a strip push the cut-out past the outer shape so no seam is left there.
    let inset = |active: bool| if active { strip } else { -1.0 };
    let inner_left = backdrop.x + inset(left);
    let inner_top = backdrop.y + inset(top);
    let inner_right = backdrop.x + backdrop.width * scale - inset(right);
    let inner_bottom = backdrop.y + backdrop.height * scale - inset(bottom);
    // A corner between two strips stays concentric with the outer curve.
    let inner_corners = RectangleCorners {
        top_left: corners.top_left && top && left,
        top_right: corners.top_right && top && right,
        bottom_right: corners.bottom_right && bottom && right,
        bottom_left: corners.bottom_left && bottom && left,
    };
    let inner_path = rounded_rect_path(
        inner_left,
        inner_top,
        (inner_right - inner_left).max(0.0),
        (inner_bottom - inner_top).max(0.0),
        (radius - strip).max(0.0),
        inner_corners,
    );
    let strip_paint = backdrop_paint(&backdrop.edge_strip_color, opacity, Style::Fill);

    canvas.save();
    canvas.clip_path(&inner_path, ClipOp::Difference, true);
    canvas.draw_path(outer_path, &strip_paint);
    canvas.restore();
}

fn rounded_rect_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    corners: RectangleCorners,
) -> Path {
    let radius = radius.max(0.0).min(width * 0.5).min(height * 0.5);
    let top_left = if corners.top_left { radius } else { 0.0 };
    let top_right = if corners.top_right { radius } else { 0.0 };
    let bottom_right = if corners.bottom_right { radius } else { 0.0 };
    let bottom_left = if corners.bottom_left { radius } else { 0.0 };
    let right = x + width;
    let bottom = y + height;
    let mut path = PathBuilder::new();

    path.move_to(Point::new(x + top_left, y));
    path.line_to(Point::new(right - top_right, y));
    if top_right > 0.0 {
        path.quad_to(Point::new(right, y), Point::new(right, y + top_right));
    } else {
        path.line_to(Point::new(right, y));
    }

    path.line_to(Point::new(right, bottom - bottom_right));
    if bottom_right > 0.0 {
        path.quad_to(
            Point::new(right, bottom),
            Point::new(right - bottom_right, bottom),
        );
    } else {
        path.line_to(Point::new(right, bottom));
    }

    path.line_to(Point::new(x + bottom_left, bottom));
    if bottom_left > 0.0 {
        path.quad_to(Point::new(x, bottom), Point::new(x, bottom - bottom_left));
    } else {
        path.line_to(Point::new(x, bottom));
    }

    path.line_to(Point::new(x, y + top_left));
    if top_left > 0.0 {
        path.quad_to(Point::new(x, y), Point::new(x + top_left, y));
    } else {
        path.line_to(Point::new(x, y));
    }
    path.close();
    path.detach()
}

pub struct BackdropFrame<'a> {
    pub bounds: Rect,
    pub content_bounds: Rect,
    backdrop: &'a ValidatedBackdrop,
    scale: f32,
}

impl BackdropFrame<'_> {
    pub fn draw(&self, canvas: &Canvas, scene_opacity: f32) {
        draw_backdrop(
            canvas,
            self.backdrop,
            self.scale,
            self.backdrop.opacity * scene_opacity,
        );
    }
}

pub fn prepare_backdrop_frame(backdrop: &ValidatedBackdrop, scale: f32) -> BackdropFrame<'_> {
    let (width, height) = match backdrop.display_type {
        BackdropType::Circle => (backdrop.diameter, backdrop.diameter),
        BackdropType::Rectangle => (backdrop.width, backdrop.height),
    };
    let content_bounds = Rect::from_xywh(backdrop.x, backdrop.y, width * scale, height * scale);
    // One pixel of raster padding retains antialiasing at fractional edges.
    let bounds = Rect::new(
        content_bounds.left.floor() - 1.0,
        content_bounds.top.floor() - 1.0,
        content_bounds.right.ceil() + 1.0,
        content_bounds.bottom.ceil() + 1.0,
    );
    BackdropFrame {
        bounds,
        content_bounds,
        backdrop,
        scale,
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: BTreeMap<String, Value>,
}

pub fn resize_backdrop(
    widget: BackdropConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid backdrop resize size".into()));
    }
    let mut backdrop = validate_backdrop(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    // Preserve the editor's eight-pixel minimum, the required interior of the border,
    // and room for opposing edge strips.
    let minimum = 8.0_f32
        .max(backdrop.border_thickness * 2.0 + 1.0)
        .max(backdrop.edge_strip_thickness * 2.0);
    let width = (size[0] / scene.scale).max(minimum);
    let height = (size[1] / scene.scale).max(minimum);
    let variant = match backdrop.display_type {
        BackdropType::Circle => {
            backdrop.diameter = width;
            json!({"diameter": width})
        }
        BackdropType::Rectangle => {
            backdrop.width = width;
            backdrop.height = height;
            json!({"width": width, "height": height})
        }
    };
    let bounds = prepare_backdrop_frame(&backdrop, scene.scale).content_bounds;
    Ok(ResizeUpdate {
        x: backdrop.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: backdrop.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: BTreeMap::from([(backdrop.display_type.as_str().to_owned(), variant)]),
    })
}

/// Backdrops produce pixels at creation/update only, without fonts or activity.
pub struct PreviewBackdrop {
    pub tile: Tile,
    /// Fixed geometry: every sample draws into the same rectangle.
    bounds: TileBounds,
    backdrop: ValidatedBackdrop,
    scale: f32,
    scene_opacity: f32,
}

impl PreviewBackdrop {
    pub fn new(
        widget: BackdropConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let backdrop = validate_backdrop(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(backdrop, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        backdrop: ValidatedBackdrop,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let frame = prepare_backdrop_frame(&backdrop, scene.scale);
        let mut tile = Tile::default();
        tile.render(
            host.raster_scale,
            SHAPE_COLOR_TYPE,
            frame.bounds,
            Presented::content(frame.content_bounds),
            |canvas| {
                frame.draw(canvas, scene.opacity);
                Ok(())
            },
        )?;
        Ok(Self {
            tile,
            bounds: frame.bounds.into(),
            scale: scene.scale,
            scene_opacity: scene.opacity,
            backdrop,
        })
    }

    pub fn layout_bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Draws the backdrop onto a host canvas in scene coordinates.
    pub fn draw(&self, canvas: &Canvas) -> TileBounds {
        prepare_backdrop_frame(&self.backdrop, self.scale).draw(canvas, self.scene_opacity);
        self.bounds
    }
}
