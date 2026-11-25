//! Friction circle: the G-force marker on two selected axes, with its label.
use crate::activity::ActivitySampler;
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_positive_f32, require_string,
    require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    layout_text_path, parse_color, resolve_font, shadow_paint, FontResolver, ResolvedTextStyle,
    TextPaints,
};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::widgets::value::format::format_number;
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::paint::Style;
use skia_safe::{BlendMode, Canvas, ClipOp, Paint, Path, PathBuilder, PathFillType, Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GForceAxis {
    X,
    Y,
    Z,
}

impl GForceAxis {
    fn parse(value: &str, field: &str) -> WidgetResult<Self> {
        match value {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            "z" => Ok(Self::Z),
            _ => Err(WidgetError::Config(format!(
                "{field}: invalid value '{value}', expected 'x', 'y', or 'z'"
            ))),
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedGForceWidget {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub diameter: f32,
    pub fill_color: String,
    pub fill_opacity: f32,
    pub border_thickness: f32,
    pub border_color: String,
    pub border_opacity: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub axis_horizontal: GForceAxis,
    pub axis_vertical: GForceAxis,
    pub invert_horizontal: bool,
    pub invert_vertical: bool,
    pub clip_percentile: f32,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_color: String,
    pub label_decimals: usize,
    pub label_unit: String,
    pub label_unit_color: String,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
}

pub fn validate_g_force(value: ValueConfig, index: usize) -> WidgetResult<ValidatedGForceWidget> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::GForce {
        return Err(WidgetError::Config(format!(
            "{}: expected g_force metric",
            p("value")
        )));
    }
    if value.display_type != DisplayType::GForce {
        return Err(WidgetError::Config(format!(
            "{}: expected g_force display_type",
            p("display_type")
        )));
    }

    let width = require_positive_f32(value.width, &p("width"))?;
    let height = require_positive_f32(value.height, &p("height"))?;
    let diameter = require_positive_f32(value.diameter, &p("diameter"))?;
    if diameter > width.min(height) {
        return Err(WidgetError::Config(format!(
            "{}: must fit inside the widget frame",
            p("diameter")
        )));
    }

    let border_thickness = require_f32(value.border_thickness, &p("border_thickness"))?;
    if border_thickness < 0.0 || border_thickness * 2.0 >= diameter {
        return Err(WidgetError::Config(format!(
            "{}: must be non-negative and leave a positive inner circle",
            p("border_thickness")
        )));
    }
    let marker_size = require_positive_f32(value.marker_size, &p("marker_size"))?;
    let label_font_size = require_positive_f32(value.label_font_size, &p("label_font_size"))?;
    let clip_percentile = require_f32(value.clip_percentile, &p("clip_percentile"))?;
    if !(0.0..=100.0).contains(&clip_percentile) || clip_percentile == 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be greater than 0 and at most 100",
            p("clip_percentile")
        )));
    }

    let axis_horizontal = GForceAxis::parse(
        &require_string(value.axis_horizontal, &p("axis_horizontal"))?,
        &p("axis_horizontal"),
    )?;
    let axis_vertical = GForceAxis::parse(
        &require_string(value.axis_vertical, &p("axis_vertical"))?,
        &p("axis_vertical"),
    )?;
    if axis_horizontal == axis_vertical {
        return Err(WidgetError::Config(format!(
            "{} and {}: axes must be different",
            p("axis_horizontal"),
            p("axis_vertical")
        )));
    }

    let fill_color = require_hex_color(value.fill_color.as_deref(), &p("fill_color"))?;
    let border_color = require_hex_color(value.border_color.as_deref(), &p("border_color"))?;
    let marker_color = require_hex_color(value.marker_color.as_deref(), &p("marker_color"))?;
    let label_color = require_hex_color(value.label_color.as_deref(), &p("label_color"))?;
    let label_unit_color =
        require_hex_color(value.label_unit_color.as_deref(), &p("label_unit_color"))?;
    let label_unit = require_string(value.label_unit, &p("label_unit"))?;
    if label_unit != "G" {
        return Err(WidgetError::Config(format!(
            "{}: must be 'G'",
            p("label_unit")
        )));
    }

    Ok(ValidatedGForceWidget {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width,
        height,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        diameter,
        fill_color,
        fill_opacity: require_unit_opacity(value.fill_opacity, &p("fill_opacity"))?,
        border_thickness,
        border_color,
        border_opacity: require_unit_opacity(value.border_opacity, &p("border_opacity"))?,
        marker_size,
        marker_color,
        marker_opacity: require_unit_opacity(value.marker_opacity, &p("marker_opacity"))?,
        axis_horizontal,
        axis_vertical,
        invert_horizontal: require_bool(value.invert_horizontal, &p("invert_horizontal"))?,
        invert_vertical: require_bool(value.invert_vertical, &p("invert_vertical"))?,
        clip_percentile,
        label_font: require_string(value.label_font, &p("label_font"))?,
        label_font_size,
        label_color,
        label_decimals: value
            .label_decimals
            .ok_or_else(|| WidgetError::Config(format!("{}: required", p("label_decimals"))))?,
        label_unit,
        label_unit_color,
        label_offset_x: require_f32(value.label_offset_x, &p("label_offset_x"))?,
        label_offset_y: require_f32(value.label_offset_y, &p("label_offset_y"))?,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GForceSample {
    pub horizontal: Option<f64>,
    pub vertical: Option<f64>,
    pub max_g: f64,
}

impl GForceSample {
    pub fn new(
        widget: &ValidatedGForceWidget,
        horizontal: Option<f64>,
        vertical: Option<f64>,
        max_g: f64,
    ) -> Self {
        Self {
            horizontal: horizontal.map(|value| {
                if widget.invert_horizontal {
                    -value
                } else {
                    value
                }
            }),
            vertical: vertical.map(|value| {
                if widget.invert_vertical {
                    -value
                } else {
                    value
                }
            }),
            max_g,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GForceFrameState {
    pub marker_x: f32,
    pub marker_y: f32,
    pub magnitude: Option<f64>,
    pub label: String,
}

/// Whole-activity paired magnitudes, prepared once for nearest-rank percentile lookup.
pub fn sorted_magnitudes(horizontal: &[Option<f64>], vertical: &[Option<f64>]) -> Vec<f64> {
    let mut magnitudes: Vec<_> = horizontal
        .iter()
        .zip(vertical)
        .filter_map(|(h, v)| h.zip(*v).map(|(h, v)| h.hypot(v)))
        .collect();
    magnitudes.sort_unstable_by(f64::total_cmp);
    magnitudes
}

pub fn percentile_max_g(magnitudes: &[f64], clip_percentile: f32) -> f64 {
    if magnitudes.is_empty() {
        return 0.0;
    }
    let rank = ((f64::from(clip_percentile) / 100.0) * magnitudes.len() as f64).ceil() as usize;
    magnitudes[rank.saturating_sub(1)]
}

pub fn derive_max_g(
    horizontal: &[Option<f64>],
    vertical: &[Option<f64>],
    clip_percentile: f32,
) -> f64 {
    percentile_max_g(&sorted_magnitudes(horizontal, vertical), clip_percentile)
}

pub fn g_force_frame_state(
    horizontal: Option<f64>,
    vertical: Option<f64>,
    max_g: f64,
    center_x: f32,
    center_y: f32,
    radius: f32,
    label_decimals: usize,
) -> GForceFrameState {
    let (Some(horizontal), Some(vertical)) = (horizontal, vertical) else {
        return GForceFrameState {
            marker_x: center_x,
            marker_y: center_y,
            magnitude: None,
            label: "--".to_string(),
        };
    };

    let magnitude = horizontal.hypot(vertical);
    let (offset_x, offset_y) = if max_g > 0.0 && magnitude > 0.0 {
        let scale = radius as f64 / max_g;
        let clamp = if magnitude > max_g {
            max_g / magnitude
        } else {
            1.0
        };
        (horizontal * scale * clamp, vertical * scale * clamp)
    } else {
        (0.0, 0.0)
    };

    GForceFrameState {
        marker_x: center_x + offset_x as f32,
        marker_y: center_y + offset_y as f32,
        magnitude: Some(magnitude),
        label: format!("{} G", format_number(magnitude, label_decimals)),
    }
}

const GUIDE_DEFAULT_THICKNESS_PX: f32 = 2.0;
const GUIDE_REFERENCE_DIAMETER_PX: f32 = 400.0;
const GUIDE_OPACITY: f32 = 0.4;
const GUIDE_RING_RADIUS_RATIOS: [f32; 2] = [0.33, 0.66];

#[derive(Clone, Copy, Debug)]
pub struct GForceLayout {
    pub width: f32,
    pub height: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

pub fn g_force_layout(widget: &ValidatedGForceWidget, scale: f32) -> GForceLayout {
    let width = widget.width * scale;
    let height = widget.height * scale;
    GForceLayout {
        width,
        height,
        center_x: width * 0.5,
        center_y: height * 0.5,
        radius: widget.diameter * scale * 0.5,
    }
}

#[derive(Clone, Debug)]
pub struct GForceCache {
    config: ValidatedGForceWidget,
    scene: ValidatedScenePresentation,
    /// Static dial relative to the widget origin (x, y).
    pub parent_circle: StaticRaster,
    pub layout: GForceLayout,
    pub x: f32,
    pub y: f32,
}

impl GForceCache {
    pub fn prepare_frame(
        &self,
        sample: GForceSample,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<GForceFrame> {
        prepare_g_force_frame(&self.config, &self.scene, sample, fonts)
    }
}

/// Rasterizes the static dial at the host's density.
///
/// Matching the host's density keeps thin guides intact; resampling a scene-density
/// dial onto a reduced preview raster drops whole pixel rows and columns.
pub fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedScenePresentation,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<GForceCache> {
    let scale = scene.scale;
    let layout = g_force_layout(widget, scale);
    let GForceLayout {
        center_x,
        center_y,
        radius,
        ..
    } = layout;
    let border_thickness = widget.border_thickness * scale;
    let guide_thickness =
        GUIDE_DEFAULT_THICKNESS_PX * (widget.diameter / GUIDE_REFERENCE_DIAMETER_PX) * scale;
    let shadow_extent = if scene.shadow_strength > 0.0 {
        (scene.shadow_distance + scene.shadow_strength * 3.0) * scale
    } else {
        0.0
    };
    let padding = (border_thickness.max(1.0) * 0.5 + shadow_extent).ceil();
    let parent_circle = StaticRaster::new(
        raster_scale,
        Point::new(widget.x, widget.y),
        Rect::new(
            -padding,
            -padding,
            layout.width.ceil() + padding,
            layout.height.ceil() + padding,
        ),
        SHAPE_COLOR_TYPE,
        |canvas| {
            if border_thickness > 0.0 {
                let mut source = Paint::default();
                source.set_anti_alias(true);
                if let Some(paint) = shadow_paint(
                    &source,
                    parse_color(&scene.shadow_color, widget.opacity),
                    scene.shadow_strength * scale,
                ) {
                    let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
                    ring_path.add_circle(Point::new(center_x, center_y), radius, None);
                    ring_path.add_circle(
                        Point::new(center_x, center_y),
                        radius - border_thickness,
                        None,
                    );
                    canvas.save();
                    canvas
                        .translate((scene.shadow_distance * scale, scene.shadow_distance * scale));
                    canvas.draw_path(&ring_path.detach(), &paint);
                    canvas.restore();
                }
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(parse_color(
                    &widget.border_color,
                    widget.border_opacity * widget.opacity,
                ));
                canvas.draw_circle(Point::new(center_x, center_y), radius, &paint);
                paint.set_blend_mode(BlendMode::Clear);
                canvas.draw_circle(
                    Point::new(center_x, center_y),
                    radius - border_thickness,
                    &paint,
                );
            }
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(parse_color(
                &widget.fill_color,
                widget.fill_opacity * widget.opacity,
            ));
            canvas.draw_circle(
                Point::new(center_x, center_y),
                radius - border_thickness,
                &paint,
            );
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(guide_thickness);
            paint.set_color(parse_color(&widget.marker_color, 1.0));
            let mut clip = PathBuilder::new();
            clip.add_circle(
                Point::new(center_x, center_y),
                radius - border_thickness,
                None,
            );
            canvas.save();
            canvas.clip_path(&clip.detach(), ClipOp::Intersect, true);
            canvas.save_layer_alpha_f(None, GUIDE_OPACITY * widget.opacity);
            let mut guides = PathBuilder::new();
            guides.move_to(Point::new(center_x - radius, center_y));
            guides.line_to(Point::new(center_x + radius, center_y));
            guides.move_to(Point::new(center_x, center_y - radius));
            guides.line_to(Point::new(center_x, center_y + radius));
            for ratio in GUIDE_RING_RADIUS_RATIOS {
                guides.add_circle(Point::new(center_x, center_y), radius * ratio, None);
            }
            canvas.draw_path(&guides.detach(), &paint);
            canvas.restore();
            canvas.restore();
            Ok(())
        },
    )?;
    Ok(GForceCache {
        config: widget.clone(),
        scene: scene.clone(),
        layout,
        x: widget.x,
        y: widget.y,
        parent_circle,
    })
}

/// Labels are placed outlines so they track the dial continuously while it resizes.
struct TextRun {
    path: Path,
    paints: TextPaints,
}

impl TextRun {
    fn draw(&self, canvas: &Canvas) {
        self.paints.draw_path(canvas, &self.path);
    }
}

pub struct GForceFrame {
    value: TextRun,
    unit: Option<TextRun>,
    marker_radius: f32,
    marker_paint: Paint,
    pub state: GForceFrameState,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
}

impl GForceFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &GForceCache) {
        canvas.save();
        canvas.translate((cache.x, cache.y));
        cache.parent_circle.draw(canvas, None);
        canvas.restore();
        canvas.draw_circle(
            Point::new(cache.x + self.state.marker_x, cache.y + self.state.marker_y),
            self.marker_radius,
            &self.marker_paint,
        );
        self.value.draw(canvas);
        if let Some(unit) = &self.unit {
            unit.draw(canvas);
        }
    }
}

pub fn prepare_g_force_frame(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedScenePresentation,
    sample: GForceSample,
    fonts: &dyn FontResolver,
) -> WidgetResult<GForceFrame> {
    let scale = scene.scale;
    let layout = g_force_layout(widget, scale);
    let state = g_force_frame_state(
        sample.horizontal,
        sample.vertical,
        sample.max_g,
        layout.center_x,
        layout.center_y,
        layout.radius,
        widget.label_decimals,
    );
    let font_size = widget.label_font_size * scale;
    let line_height = font_size * LINE_HEIGHT_RATIO;
    let margin = font_size * 0.05;
    let top = widget.y + layout.center_y + layout.radius - line_height - margin
        + widget.label_offset_y * scale;
    let style = ResolvedTextStyle {
        x: widget.x + layout.center_x + layout.radius + margin + widget.label_offset_x * scale,
        y: top,
        border_color: None,
        border_thickness: 0.0,
        ..ResolvedTextStyle::widget(
            scene,
            scale,
            Some(widget.label_font.clone()),
            widget.label_font_size,
            parse_color(&widget.label_color, widget.opacity),
            widget.opacity,
        )
    };
    let font = resolve_font(fonts, Some(&widget.label_font), font_size)?;
    let mut content_bounds = Rect::from_xywh(
        widget.x + layout.center_x - layout.radius,
        widget.y + layout.center_y - layout.radius,
        layout.radius * 2.0,
        layout.radius * 2.0,
    );
    let marker_radius = widget.marker_size * scale * 0.5;
    content_bounds.join(Rect::from_xywh(
        widget.x + state.marker_x - marker_radius,
        widget.y + state.marker_y - marker_radius,
        marker_radius * 2.0,
        marker_radius * 2.0,
    ));
    let mut bounds = content_bounds;
    let mut prepare_text = |text: &str, style: ResolvedTextStyle| -> (TextRun, f32) {
        let (path, width) = layout_text_path(text, &font, style.numerals);
        // Outline bounds scale continuously; glyph-mask bounds step in whole pixels.
        let ink = path.compute_tight_bounds();
        let baseline = style.y + (line_height - ink.height()) * 0.5 - ink.top;
        bounds.join(ink.with_offset((style.x, baseline)));
        content_bounds.join(Rect::new(
            style.x,
            baseline + ink.top,
            style.x + width,
            baseline + ink.bottom,
        ));
        let run = TextRun {
            path: path.with_offset((style.x, baseline)),
            paints: TextPaints::new(&style),
        };
        (run, width)
    };
    let value_text = if state.magnitude.is_some() {
        state
            .label
            .strip_suffix(" G")
            .expect("formatted G-force text")
    } else {
        &state.label
    };
    let (value, value_width) = prepare_text(value_text, style.clone());
    let unit = state.magnitude.is_some().then(|| {
        prepare_text(
            &widget.label_unit,
            ResolvedTextStyle {
                x: style.x + value_width + 3.0,
                color: parse_color(&widget.label_unit_color, widget.opacity),
                ..style
            },
        )
        .0
    });
    let shadow_padding = if scene.shadow_strength > 0.0 {
        (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
    } else {
        0.0
    };
    bounds.outset((2.0 + shadow_padding, 2.0 + shadow_padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    let mut marker_paint = Paint::default();
    marker_paint.set_anti_alias(true);
    marker_paint.set_color(parse_color(
        &widget.marker_color,
        widget.marker_opacity * widget.opacity,
    ));
    Ok(GForceFrame {
        value,
        unit,
        marker_radius,
        marker_paint,
        state,
        bounds,
        content_bounds,
        content_width: layout.width,
    })
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub g_force: GForceResizeVariant,
}

#[derive(Serialize)]
pub struct GForceResizeVariant {
    pub width: f32,
    pub height: f32,
    pub diameter: f32,
    pub border_thickness: f32,
    pub marker_size: f32,
    pub label_font_size: f32,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
}

pub fn resize_g_force(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    let mut config = validate_g_force(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let width = (config.width * factor).clamp(8.0, 10_000.0);
    let frame_factor = width / config.width;
    config.width = width;
    config.height = width;
    config.diameter = (config.diameter * frame_factor).clamp(8.0, width);
    config.border_thickness =
        (config.border_thickness * frame_factor).clamp(0.0, (config.diameter - 1.0) * 0.5);
    config.marker_size = (config.marker_size * frame_factor).clamp(1.0, config.diameter);
    config.label_font_size = (config.label_font_size * frame_factor).clamp(8.0, 400.0);
    config.label_offset_x = (config.label_offset_x * frame_factor).clamp(-10_000.0, 10_000.0);
    config.label_offset_y = (config.label_offset_y * frame_factor).clamp(-10_000.0, 10_000.0);
    let sample = activity.map_or_else(GForceSample::default, |activity| {
        activity.g_force_at(&config, second)
    });
    let bounds = prepare_g_force_frame(&config, &scene, sample, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: ResizeVariants {
            g_force: GForceResizeVariant {
                width: config.width,
                height: config.height,
                diameter: config.diameter,
                border_thickness: config.border_thickness,
                marker_size: config.marker_size,
                label_font_size: config.label_font_size,
                label_offset_x: config.label_offset_x,
                label_offset_y: config.label_offset_y,
            },
        },
    })
}

pub struct PreviewGForce {
    config: ValidatedGForceWidget,
    cache: GForceCache,
    preview_raster_scale: PreviewRasterScale,
    last_sample: Option<GForceSample>,
    pub tile: Tile,
}

impl PreviewGForce {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_g_force(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(config, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: ValidatedGForceWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let cache = prepare_g_force_cache(&config, scene, host.raster_scale)?;
        Ok(Self {
            config,
            cache,
            preview_raster_scale: host.raster_scale,
            last_sample: None,
            tile: Tile::default(),
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> GForceSample {
        activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        })
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        Ok(self.cache.prepare_frame(sample, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        let frame = self.cache.prepare_frame(sample, fonts)?;
        frame.draw(canvas, &self.cache);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        // Marker direction and clipping scale can change while the formatted magnitude stays equal.
        if self.last_sample == Some(sample) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(sample, fonts)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        self.last_sample = Some(sample);
        Ok(())
    }
}
