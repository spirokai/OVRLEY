//! Lean-angle sector: a mirrored track filled to the current lean, with its readout.
use crate::activity::ActivitySampler;
use crate::model::catalog::{catalog, widget_font_weight, DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, Numerals, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_positive_f32;
use crate::model::validation::{
    require_bool, require_f32, require_str, require_string, rgba_from_hex,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    draw_text_blob, layout_text, measure_text_with_font, parse_color, resolve_styled_font,
    shadow_paint, text_selection_bounds, FontResolver, ResolvedTextStyle,
};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::render::track_fill::{validate_track_fill_color, TrackFillColor};
use crate::widgets::gauges::track::path_tangent;
use crate::widgets::value::format::format_number;
use crate::widgets::value::layout::{
    metric_vertical_metrics_text, METRIC_WIDGET_UNITS_GAP_PX, METRIC_WIDGET_UNIT_RATIO,
    MIN_UNITS_FONT_SIZE,
};
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::paint::Style;
use skia_safe::{
    BlendMode, Canvas, ClipOp, Paint, Path, PathBuilder, PathFillType, Point, Rect, TextBlob,
};

#[derive(Clone, Debug)]
pub struct ValidatedLeanAngleWidget {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub show_icon: bool,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: TrackFillColor,
    pub track_filled_opacity: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_thickness: f32,
    pub font: String,
    pub font_size: f32,
    pub color: String,
    pub unit_color: String,
    pub show_units: bool,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn validate_lean_angle(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedLeanAngleWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.value != MetricKind::LeanAngle {
        return Err(WidgetError::Config(format!(
            "{}: expected lean_angle metric",
            p("value")
        )));
    }
    if value.display_type != DisplayType::LeanAngle {
        return Err(WidgetError::Config(format!(
            "{}: expected lean_angle display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    if value.width.is_some() || value.height.is_some() {
        return Err(WidgetError::Config(format!(
            "{} and {}: must be absent for diameter-based geometry",
            p("width"),
            p("height")
        )));
    }

    let diameter = require_positive_f32(value.diameter, &p("diameter"))?;

    let opacity = require_f32(value.opacity, &p("opacity"))?;
    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    for (field, field_opacity) in [
        ("opacity", opacity),
        ("track_empty_opacity", track_empty_opacity),
        ("track_filled_opacity", track_filled_opacity),
    ] {
        if !(0.0..=1.0).contains(&field_opacity) {
            return Err(WidgetError::Config(format!(
                "{}: must be 0.0..1.0, got {field_opacity}",
                p(field)
            )));
        }
    }

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0",
            p("track_border_thickness")
        )));
    }
    let track_thickness = require_positive_f32(value.track_thickness, &p("track_thickness"))?;
    if track_thickness >= diameter * 0.5 {
        return Err(WidgetError::Config(format!(
            "{}: must be less than diameter / 2",
            p("track_thickness")
        )));
    }
    if track_border_thickness * 2.0 >= track_thickness {
        return Err(WidgetError::Config(format!(
            "{}: must leave a positive usable width inside {}",
            p("track_border_thickness"),
            p("track_thickness")
        )));
    }
    let font_size = require_positive_f32(value.font_size, &p("font_size"))?;

    let track_empty_color = require_string(value.track_empty_color, &p("track_empty_color"))?;
    let track_filled_color = validate_track_fill_color(
        value.track_filled_color.as_deref(),
        value.track_filled_gradient,
        value.track_filled_gradient_stops,
        p,
    )?;
    let track_border_color = require_string(value.track_border_color, &p("track_border_color"))?;
    let color = require_string(value.color, &p("color"))?;
    let unit_color = require_string(value.unit_color, &p("unit_color"))?;
    for (field, hex) in [
        ("track_empty_color", track_empty_color.as_str()),
        ("track_border_color", track_border_color.as_str()),
        ("color", color.as_str()),
        ("unit_color", unit_color.as_str()),
    ] {
        rgba_from_hex(require_str(Some(hex), &p(field))?, &p(field), 1.0)?;
    }

    Ok(ValidatedLeanAngleWidget {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        diameter,
        // Rotation is optional metadata; this presentation has no rotation gesture.
        rotation: value
            .rotation
            .map(|rotation| require_f32(Some(rotation), &p("rotation")))
            .transpose()?
            .unwrap_or(0.0),
        opacity,
        show_icon: require_bool(value.show_icon, &p("show_icon"))?,
        track_empty_color,
        track_empty_opacity,
        track_filled_color,
        track_filled_opacity,
        track_border_thickness,
        track_border_color,
        track_thickness,
        font: require_string(value.font, &p("font"))?,
        font_size,
        color,
        unit_color,
        show_units: require_bool(value.show_units, &p("show_units"))?,
        value_offset_x: require_f32(value.value_offset_x, &p("value_offset_x"))?,
        value_offset_y: require_f32(value.value_offset_y, &p("value_offset_y"))?,
    })
}

pub fn format_lean_angle_value(raw: Option<f64>) -> String {
    raw.map_or_else(|| "--".into(), |value| format_number(value.abs(), 0))
}

pub fn lean_angle_fill_sweep(raw: Option<f64>) -> f32 {
    raw.map_or(0.0, |value| {
        value.clamp(
            -f64::from(LEAN_ANGLE_MAX_FILL_SWEEP),
            f64::from(LEAN_ANGLE_MAX_FILL_SWEEP),
        ) as f32
    })
}

const START_ANGLE: f32 = 210.0;
const SWEEP_ANGLE: f32 = 120.0;
pub const LEAN_ANGLE_MAX_FILL_SWEEP: f32 = SWEEP_ANGLE * 0.5;
/// Upright: the sector's top, where the fill starts in either lean direction.
pub const LEAN_ANGLE_CENTER_ANGLE: f32 = START_ANGLE + LEAN_ANGLE_MAX_FILL_SWEEP;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeanAngleLayout {
    pub width: f32,
    pub height: f32,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
}

/// Derives the logical lean-angle frame from the canonical diameter geometry.
pub fn lean_angle_layout(diameter: f32, track_thickness: f32, font_size: f32) -> LeanAngleLayout {
    let outer_radius = diameter * 0.5;
    let inner_radius = outer_radius - track_thickness;
    let half_sweep_radians = (SWEEP_ANGLE * 0.5).to_radians();
    let sector_min_x = -outer_radius * half_sweep_radians.sin();
    let sector_max_x = outer_radius * half_sweep_radians.sin();
    let sector_min_y = -outer_radius;
    let sector_max_y = -inner_radius * half_sweep_radians.cos();
    let label_line_height = font_size * LINE_HEIGHT_RATIO;
    let min_x = sector_min_x;
    let max_x = sector_max_x;
    let min_y = sector_min_y.min(-label_line_height * 0.5);
    let max_y = sector_max_y.max(label_line_height * 0.5);

    LeanAngleLayout {
        width: max_x - min_x,
        height: max_y - min_y,
        min_x,
        min_y,
        max_x,
        max_y,
        center_x: -min_x,
        center_y: -min_y,
        outer_radius,
        inner_radius,
        start_angle: START_ANGLE,
        sweep_angle: SWEEP_ANGLE,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LeanAngleGeometry {
    pub center_x: f32,
    pub center_y: f32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
}

pub(crate) fn geometry_from_layout(layout: LeanAngleLayout) -> LeanAngleGeometry {
    LeanAngleGeometry {
        center_x: layout.center_x,
        center_y: layout.center_y,
        outer_radius: layout.outer_radius,
        inner_radius: layout.inner_radius,
        start_angle: layout.start_angle,
        sweep_angle: layout.sweep_angle,
    }
}

pub(crate) fn inset_geometry(
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) -> LeanAngleGeometry {
    let track_width = geometry.outer_radius - geometry.inner_radius - border_thickness * 2.0;
    let outer_radius = geometry.outer_radius - border_thickness;
    LeanAngleGeometry {
        outer_radius,
        inner_radius: outer_radius - track_width,
        ..geometry
    }
}

/// Static empty track, border and zero guide, reused by both hosts.
#[derive(Clone, Debug)]
pub struct LeanAngleCache {
    config: ValidatedLeanAngleWidget,
    scene: ValidatedScenePresentation,
    /// Empty track, border and zero guide in widget-local coordinates; the origin is (x, y).
    pub static_layer: StaticRaster,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub layout: LeanAngleLayout,
    pub track_thickness: f32,
    pub track_border_thickness: f32,
}

impl LeanAngleCache {
    pub fn prepare_frame(
        &self,
        raw: Option<f64>,
        fonts: &dyn crate::render::text::FontResolver,
    ) -> WidgetResult<LeanAngleFrame> {
        prepare_lean_angle_frame(&self.config, &self.scene, raw, fonts)
    }
}

pub fn prepare_lean_angle_cache(
    widget: &ValidatedLeanAngleWidget,
    scene: &ValidatedScenePresentation,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<LeanAngleCache> {
    let scale = scene.scale;
    let layout = lean_angle_layout(
        widget.diameter * scale,
        widget.track_thickness * scale,
        widget.font_size * scale,
    );
    let geometry = geometry_from_layout(layout);
    let border_thickness = widget.track_border_thickness * scale;
    let outer_path = annular_sector_path(geometry);
    let inner_path = annular_sector_inset_path(geometry, border_thickness);
    let border_path = annular_sector_border_path(geometry, border_thickness);
    let shadow_extent = if border_thickness > 0.0 && scene.shadow_strength > 0.0 {
        (scene.shadow_distance + scene.shadow_strength * 3.0) * scale
    } else {
        0.0
    };
    let padding = (border_thickness.max(1.0) * 0.5 + shadow_extent).ceil();
    let track_bounds = outer_path.compute_tight_bounds();
    let static_bounds = Rect::new(
        track_bounds.left.floor() - padding,
        track_bounds.top.floor() - padding,
        track_bounds.right.ceil() + padding,
        track_bounds.bottom.ceil() + padding,
    );
    let static_layer = StaticRaster::new(
        raster_scale,
        Point::new(widget.x, widget.y),
        static_bounds,
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
                    canvas.save();
                    canvas
                        .translate((scene.shadow_distance * scale, scene.shadow_distance * scale));
                    canvas.draw_path(&border_path, &paint);
                    canvas.restore();
                }
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(Style::Fill);
                paint.set_color(parse_color(&widget.track_border_color, widget.opacity));
                canvas.draw_path(&outer_path, &paint);
                paint.set_blend_mode(BlendMode::Clear);
                canvas.draw_path(&inner_path, &paint);
            }
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(Style::Fill);
            paint.set_color(parse_color(
                &widget.track_empty_color,
                widget.track_empty_opacity * widget.opacity,
            ));
            canvas.draw_path(&inner_path, &paint);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(widget.diameter / 200.0 * scale);
            paint.set_color(parse_color(widget.track_filled_color.leading_color(), 1.0));
            canvas.save();
            canvas.clip_path(&inner_path, ClipOp::Intersect, true);
            canvas.save_layer_alpha_f(None, 0.4 * widget.opacity);
            canvas.draw_line(
                Point::new(geometry.center_x, geometry.center_y - geometry.outer_radius),
                Point::new(geometry.center_x, geometry.center_y - geometry.inner_radius),
                &paint,
            );
            canvas.restore();
            canvas.restore();
            Ok(())
        },
    )?;
    Ok(LeanAngleCache {
        config: widget.clone(),
        scene: scene.clone(),
        static_layer,
        x: widget.x,
        y: widget.y,
        width: layout.width.ceil() as u32,
        height: layout.height.ceil() as u32,
        rotation: widget.rotation,
        layout,
        track_thickness: widget.track_thickness * scale,
        track_border_thickness: border_thickness,
    })
}

pub(crate) fn annular_sector_path(geometry: LeanAngleGeometry) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_contour(&mut path, geometry);
    path.detach()
}

fn annular_sector_border_path(outer_geometry: LeanAngleGeometry, border_thickness: f32) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_contour(&mut path, outer_geometry);
    append_annular_sector_inset_contour(&mut path, outer_geometry, border_thickness);
    path.detach()
}

pub(crate) fn annular_sector_inset_path(
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_inset_contour(&mut path, geometry, border_thickness);
    path.detach()
}

fn append_annular_sector_inset_contour(
    builder: &mut PathBuilder,
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) {
    let inner_geometry = inset_geometry(geometry, border_thickness);
    let direction = geometry.sweep_angle.signum();
    let max_angle_offset = geometry.sweep_angle.abs() * 0.5;
    let outer_angle_offset = parallel_side_angle_offset(
        border_thickness,
        inner_geometry.outer_radius,
        max_angle_offset,
    );
    let inner_angle_offset = parallel_side_angle_offset(
        border_thickness,
        inner_geometry.inner_radius,
        max_angle_offset,
    );
    let outer_start_angle = geometry.start_angle + direction * outer_angle_offset;
    let outer_sweep_angle = geometry.sweep_angle - direction * outer_angle_offset * 2.0;
    let inner_end_angle =
        geometry.start_angle + geometry.sweep_angle - direction * inner_angle_offset;
    let inner_sweep_angle = -(geometry.sweep_angle - direction * inner_angle_offset * 2.0);

    builder.move_to(polar_point(
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.outer_radius,
        outer_start_angle,
    ));
    append_circular_arc(
        builder,
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.outer_radius,
        outer_start_angle,
        outer_sweep_angle,
    );
    builder.line_to(polar_point(
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.inner_radius,
        inner_end_angle,
    ));
    append_circular_arc(
        builder,
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.inner_radius,
        inner_end_angle,
        inner_sweep_angle,
    );
    builder.close();
}

fn parallel_side_angle_offset(border_thickness: f32, radius: f32, max_angle_offset: f32) -> f32 {
    if border_thickness == 0.0 {
        return 0.0;
    }

    (border_thickness / radius)
        .asin()
        .to_degrees()
        .min(max_angle_offset)
}

fn append_annular_sector_contour(builder: &mut PathBuilder, geometry: LeanAngleGeometry) {
    let end_angle = geometry.start_angle + geometry.sweep_angle;
    builder.move_to(polar_point(
        geometry.center_x,
        geometry.center_y,
        geometry.outer_radius,
        geometry.start_angle,
    ));
    append_circular_arc(
        builder,
        geometry.center_x,
        geometry.center_y,
        geometry.outer_radius,
        geometry.start_angle,
        geometry.sweep_angle,
    );
    builder.line_to(polar_point(
        geometry.center_x,
        geometry.center_y,
        geometry.inner_radius,
        end_angle,
    ));
    append_circular_arc(
        builder,
        geometry.center_x,
        geometry.center_y,
        geometry.inner_radius,
        end_angle,
        -geometry.sweep_angle,
    );
    builder.close();
}

fn append_circular_arc(
    builder: &mut PathBuilder,
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
        let start = polar_point(center_x, center_y, radius, angle0);
        let end = polar_point(center_x, center_y, radius, angle1);
        let start_tangent = path_tangent(angle0);
        let end_tangent = path_tangent(angle1);
        builder.cubic_to(
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

pub(crate) fn polar_point(center_x: f32, center_y: f32, radius: f32, angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(
        center_x + radius * radians.cos(),
        center_y + radius * radians.sin(),
    )
}

struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct LeanAngleFrame {
    value: TextRun,
    unit: Option<TextRun>,
    fill: Option<(Path, Paint)>,
    inner_track: Path,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    pub fill_sweep: f32,
    pub marker: Point,
}

impl LeanAngleFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &LeanAngleCache) {
        canvas.save();
        canvas.translate((cache.x, cache.y));
        cache.static_layer.draw(canvas, None);
        canvas.restore();
        if let Some((path, paint)) = &self.fill {
            canvas.save();
            canvas.clip_path(&self.inner_track, ClipOp::Intersect, true);
            canvas.draw_path(path, paint);
            canvas.restore();
        }
        draw_text_blob(
            canvas,
            &self.value.blob,
            self.value.baseline,
            &self.value.style,
        );
        if let Some(unit) = &self.unit {
            draw_text_blob(canvas, &unit.blob, unit.baseline, &unit.style);
        }
    }
}

pub fn prepare_lean_angle_frame(
    widget: &ValidatedLeanAngleWidget,
    scene: &ValidatedScenePresentation,
    raw: Option<f64>,
    fonts: &dyn FontResolver,
) -> WidgetResult<LeanAngleFrame> {
    let scale = scene.scale;
    let layout = lean_angle_layout(
        widget.diameter * scale,
        widget.track_thickness * scale,
        widget.font_size * scale,
    );
    let geometry = LeanAngleGeometry {
        center_x: widget.x + layout.center_x,
        center_y: widget.y + layout.center_y,
        ..geometry_from_layout(layout)
    };
    let border = widget.track_border_thickness * scale;
    let fill_sweep = lean_angle_fill_sweep(raw);
    let fill = if fill_sweep != 0.0 {
        let paint = widget.track_filled_color.mirrored_sweep_paint(
            widget.track_filled_opacity * widget.opacity,
            Point::new(geometry.center_x, geometry.center_y),
            LEAN_ANGLE_CENTER_ANGLE,
            LEAN_ANGLE_MAX_FILL_SWEEP,
        )?;
        Some((
            annular_sector_path(LeanAngleGeometry {
                start_angle: LEAN_ANGLE_CENTER_ANGLE,
                sweep_angle: fill_sweep,
                ..inset_geometry(geometry, border)
            }),
            paint,
        ))
    } else {
        None
    };
    let font_size = widget.font_size * scale;
    let mut value_style = ResolvedTextStyle::widget(
        scene,
        scale,
        Some(widget.font.clone()),
        widget.font_size,
        parse_color(&widget.color, widget.opacity),
        widget.opacity,
    );
    let value_text = format_lean_angle_value(raw);
    let value_font = resolve_styled_font(
        fonts,
        Some(&widget.font),
        font_size,
        widget_font_weight(),
        false,
    )?;
    let value = layout_text(&value_text, &value_font, 0.0, value_style.numerals);
    let value_vertical = measure_text_with_font(
        metric_vertical_metrics_text(&value_text),
        &value_font,
        Numerals::Default,
    );
    let unit = if raw.is_some() && widget.show_units {
        let unit_text = catalog().unit_label(MetricKind::LeanAngle, None);
        let unit_size = (font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        let font = resolve_styled_font(
            fonts,
            Some(&widget.font),
            unit_size,
            widget_font_weight(),
            false,
        )?;
        let text = layout_text(unit_text, &font, 0.0, Numerals::Default);
        // The degree glyph shares the existing degree/C baseline reference.
        let vertical = measure_text_with_font(
            metric_vertical_metrics_text("\u{00b0}C"),
            &font,
            Numerals::Default,
        );
        let style = ResolvedTextStyle {
            font_size: unit_size,
            line_height: unit_size * LINE_HEIGHT_RATIO,
            color: parse_color(&widget.unit_color, widget.opacity),
            ..value_style.clone()
        };
        Some((text, vertical, style))
    } else {
        None
    };
    let units_width = unit.as_ref().map_or(0.0, |(text, _, _)| {
        METRIC_WIDGET_UNITS_GAP_PX * scale + text.measurement.width
    });
    let group_width = value.measurement.width + units_width;
    let group_height = unit
        .as_ref()
        .map_or(value_style.line_height, |(_, _, style)| {
            value_style.line_height.max(style.line_height)
        });
    let degree_offset = if unit.is_some() { font_size * 0.1 } else { 0.0 };
    value_style.x =
        geometry.center_x + widget.value_offset_x * scale + degree_offset - group_width * 0.5;
    let group_bottom = geometry.center_y + widget.value_offset_y * scale + group_height * 0.5;
    let value_baseline = group_bottom - value_vertical.bounds_bottom;
    let mut content_bounds = annular_sector_path(geometry).compute_tight_bounds();
    let mut bounds = content_bounds;
    let text_bounds = |ink: &crate::render::text::MeasuredText, x: f32, baseline: f32| {
        let mut bounds = Rect::new(
            x + ink.bounds_left,
            baseline + ink.bounds_top,
            x + ink.bounds_right,
            baseline + ink.bounds_bottom,
        );
        bounds.outset((
            value_style.border_thickness * 0.5,
            value_style.border_thickness * 0.5,
        ));
        let mut content = text_selection_bounds(ink, x, baseline);
        content.outset((
            value_style.border_thickness * 0.5,
            value_style.border_thickness * 0.5,
        ));
        (bounds, content)
    };
    let (value_bounds, value_content) =
        text_bounds(&value.measurement, value_style.x, value_baseline);
    bounds.join(value_bounds);
    content_bounds.join(value_content);
    let unit = unit
        .map(|(text, vertical, mut style)| -> WidgetResult<TextRun> {
            style.x = value_style.x + value.measurement.width + METRIC_WIDGET_UNITS_GAP_PX * scale;
            let baseline = group_bottom - vertical.bounds_bottom;
            let (unit_bounds, unit_content) = text_bounds(&text.measurement, style.x, baseline);
            bounds.join(unit_bounds);
            content_bounds.join(unit_content);
            Ok(TextRun {
                blob: text.blob.ok_or_else(|| {
                    WidgetError::Render("unable to lay out lean-angle unit".into())
                })?,
                baseline,
                style,
            })
        })
        .transpose()?;
    let shadow_padding = if scene.shadow_strength > 0.0 {
        (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
    } else {
        0.0
    };
    let padding = 2.0 + border * 0.5 + shadow_padding;
    bounds.outset((padding, padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    Ok(LeanAngleFrame {
        value: TextRun {
            blob: value
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out lean-angle value".into()))?,
            baseline: value_baseline,
            style: value_style,
        },
        unit,
        fill,
        inner_track: annular_sector_inset_path(geometry, border),
        bounds,
        content_bounds,
        content_width: layout.width,
        fill_sweep,
        marker: polar_point(
            geometry.center_x,
            geometry.center_y,
            geometry.outer_radius,
            LEAN_ANGLE_CENTER_ANGLE + fill_sweep,
        ),
    })
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub lean_angle: LeanAngleResizeVariant,
}

#[derive(Serialize)]
pub struct LeanAngleResizeVariant {
    pub diameter: f32,
    pub track_thickness: f32,
    pub track_border_thickness: f32,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn resize_lean_angle(
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
    let mut config = validate_lean_angle(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.diameter = (config.diameter * factor).clamp(8.0, 10_000.0);
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    // Keep both strict geometric inequalities valid even at product size limits.
    config.track_thickness =
        (config.track_thickness * factor).clamp(1.0, config.diameter * 0.5 * (1.0 - f32::EPSILON));
    config.track_border_thickness = (config.track_border_thickness * factor)
        .clamp(0.0, config.track_thickness * 0.5 * (1.0 - f32::EPSILON));
    config.value_offset_x = (config.value_offset_x * factor).clamp(-10_000.0, 10_000.0);
    config.value_offset_y = (config.value_offset_y * factor).clamp(-10_000.0, 10_000.0);
    let raw = activity.and_then(|activity| activity.lean_angle_at(second));
    let bounds = prepare_lean_angle_frame(&config, &scene, raw, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        display_variants: ResizeVariants {
            lean_angle: LeanAngleResizeVariant {
                diameter: config.diameter,
                track_thickness: config.track_thickness,
                track_border_thickness: config.track_border_thickness,
                value_offset_x: config.value_offset_x,
                value_offset_y: config.value_offset_y,
            },
        },
    })
}

pub struct PreviewLeanAngle {
    cache: LeanAngleCache,
    preview_raster_scale: PreviewRasterScale,
    last_sample: Option<Option<f64>>,
    pub tile: Tile,
}

impl PreviewLeanAngle {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_lean_angle(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&config, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: &ValidatedLeanAngleWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_lean_angle_cache(config, scene, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            last_sample: None,
            tile: Tile::default(),
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.lean_angle_at(second))
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        Ok(self.cache.prepare_frame(raw, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        let frame = self.cache.prepare_frame(raw, fonts)?;
        frame.draw(canvas, &self.cache);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        // Signed fill changes continuously, even when the magnitude label stays the same.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw, fonts)?;
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
        self.last_sample = Some(raw);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{format_lean_angle_value, lean_angle_fill_sweep, lean_angle_layout};

    #[test]
    fn lean_angle_layout_uses_diameter_and_includes_the_stable_label_frame() {
        let geometry = lean_angle_layout(300.0, 100.0, 60.0);

        assert_eq!(geometry.start_angle, 210.0);
        assert_eq!(geometry.sweep_angle, 120.0);
        assert_eq!(geometry.outer_radius, 150.0);
        assert_eq!(geometry.inner_radius, 50.0);
        assert!((geometry.width - 259.80762).abs() < 0.0001);
        assert!((geometry.height - 177.6).abs() < 0.0001);
        assert!((geometry.center_x - 129.90381).abs() < 0.0001);
        assert_eq!(geometry.center_y, 150.0);
    }

    #[test]
    fn lean_angle_inner_radius_respects_track_thickness() {
        let geometry = lean_angle_layout(180.0, 24.0, 60.0);

        assert_eq!(geometry.outer_radius, 90.0);
        assert_eq!(geometry.inner_radius, 66.0);
        assert_eq!(geometry.outer_radius - geometry.inner_radius, 24.0);
    }

    #[test]
    fn lean_angle_maps_signed_samples_and_formats_missing_values() {
        assert_eq!(lean_angle_fill_sweep(Some(30.0)), 30.0);
        assert_eq!(lean_angle_fill_sweep(Some(-70.0)), -60.0);
        assert_eq!(lean_angle_fill_sweep(Some(0.0)), 0.0);
        assert_eq!(lean_angle_fill_sweep(None), 0.0);
        assert_eq!(format_lean_angle_value(Some(-30.4)), "30");
        assert_eq!(format_lean_angle_value(None), "--");
    }
}
