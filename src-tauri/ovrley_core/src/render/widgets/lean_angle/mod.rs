//! Static lean-angle annular-sector preparation and drawing.

use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{
    lean_angle_layout, LeanAngleLayout, ValidatedLeanAngleWidget, ValidatedSceneConfig,
    LEAN_ANGLE_MAX_FILL_SWEEP,
};
use crate::render::format::{format_lean_angle_value, resolve_metric_display_value};
use crate::render::surface::create_surface;
use crate::render::text::{
    draw_text_with_vertical_metrics_text, measure_text, parse_color, ResolvedTextStyle,
};
use crate::render::widgets::common::{normalize_shadow_style_validated, static_layer_padding};
use crate::render::widgets::types::{
    LeanAngleCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
};
use crate::render::widgets::value::{
    metric_vertical_metrics_text, METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNITS_GAP_PX,
    METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
};
use crate::standard_metrics::standard_metric_unit_label;
use crate::types::MetricKind;
use skia_safe::{
    image_filters, paint::Style, BlendMode, Canvas, ClipOp, Paint, Path, PathBuilder, PathFillType,
    Point,
};
use std::path::PathBuf;

const CENTER_ANGLE: f32 = 270.0;
const DEGREE_UNIT_CENTERING_OFFSET_RATIO: f32 = 0.1;
const GUIDE_DEFAULT_THICKNESS_PX: f32 = 1.0;
const GUIDE_REFERENCE_DIAMETER_PX: f32 = 200.0;
const GUIDE_OPACITY: f32 = 0.4;

#[derive(Clone, Copy)]
struct LeanAngleGeometry {
    center_x: f32,
    center_y: f32,
    outer_radius: f32,
    inner_radius: f32,
    start_angle: f32,
    sweep_angle: f32,
}

fn geometry_from_layout(layout: LeanAngleLayout) -> LeanAngleGeometry {
    LeanAngleGeometry {
        center_x: layout.center_x,
        center_y: layout.center_y,
        outer_radius: layout.outer_radius,
        inner_radius: layout.inner_radius,
        start_angle: layout.start_angle,
        sweep_angle: layout.sweep_angle,
    }
}

fn inset_geometry(geometry: LeanAngleGeometry, border_thickness: f32) -> LeanAngleGeometry {
    let track_width = geometry.outer_radius - geometry.inner_radius - border_thickness * 2.0;
    let outer_radius = geometry.outer_radius - border_thickness;
    LeanAngleGeometry {
        outer_radius,
        inner_radius: outer_radius - track_width,
        ..geometry
    }
}

/// Maps a signed lean-angle sample to the dynamic fill sweep.
pub fn lean_angle_fill_sweep(raw: Option<f64>) -> f32 {
    let Some(raw) = raw else {
        return 0.0;
    };

    let magnitude = raw.abs().min(LEAN_ANGLE_MAX_FILL_SWEEP as f64) as f32;
    if raw > 0.0 {
        magnitude
    } else if raw < 0.0 {
        -magnitude
    } else {
        0.0
    }
}

pub fn prepare_lean_angle_cache(
    widget: &ValidatedLeanAngleWidget,
    scene: &ValidatedSceneConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LeanAngleCache> {
    prepare_profiler.measure("lean_angle.prepare", || {
        let scale = scene.scale;
        let layout = lean_angle_layout(
            widget.diameter * scale,
            widget.track_thickness * scale,
            widget.font_size * scale,
        );
        let width = layout.width.ceil().max(1.0) as u32;
        let height = layout.height.ceil().max(1.0) as u32;
        let track_thickness = widget.track_thickness * scale;
        let border_thickness = widget.track_border_thickness * scale;
        let guide_thickness =
            GUIDE_DEFAULT_THICKNESS_PX * (widget.diameter / GUIDE_REFERENCE_DIAMETER_PX) * scale;
        let geometry = geometry_from_layout(layout);

        let outer_path = annular_sector_path(geometry);
        let inner_path = annular_sector_inset_path(geometry, border_thickness);
        let border_path = annular_sector_border_path(geometry, border_thickness);
        let shadow = normalize_shadow_style_validated(
            &scene.shadow_color,
            scene.shadow_strength,
            scene.shadow_distance,
            scale,
        );
        let track_shadow = if border_thickness > 0.0 {
            shadow.clone()
        } else {
            None
        };
        let shadow_filter = track_shadow.as_ref().and_then(|shadow| {
            image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, widget.opacity),
                None,
                None,
                None,
            )
        });
        let padding = static_layer_padding(border_thickness, track_shadow.as_ref());
        let layer_width = width.saturating_add(padding.saturating_mul(2)).max(1);
        let layer_height = height.saturating_add(padding.saturating_mul(2)).max(1);
        let mut surface = create_surface(layer_width, layer_height)?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::TRANSPARENT);
        canvas.translate((padding as f32, padding as f32));

        if let Some(filter) = shadow_filter {
            let mut shadow_paint = Paint::default();
            shadow_paint.set_anti_alias(true);
            shadow_paint.set_color(skia_safe::Color::BLACK);
            shadow_paint.set_image_filter(filter);
            canvas.draw_path(&border_path, &shadow_paint);
        }

        if border_thickness > 0.0 {
            let mut border_paint = Paint::default();
            border_paint.set_anti_alias(true);
            border_paint.set_style(Style::Fill);
            border_paint.set_color(parse_color(&widget.track_border_color, widget.opacity));
            canvas.draw_path(&outer_path, &border_paint);

            let mut clear_paint = Paint::default();
            clear_paint.set_anti_alias(true);
            clear_paint.set_blend_mode(BlendMode::Clear);
            canvas.draw_path(&inner_path, &clear_paint);
        }

        let mut empty_paint = Paint::default();
        empty_paint.set_anti_alias(true);
        empty_paint.set_style(Style::Fill);
        empty_paint.set_color(parse_color(
            &widget.track_empty_color,
            widget.track_empty_opacity * widget.opacity,
        ));
        canvas.draw_path(&inner_path, &empty_paint);

        let mut guide_paint = Paint::default();
        guide_paint.set_anti_alias(true);
        guide_paint.set_style(Style::Stroke);
        guide_paint.set_stroke_width(guide_thickness);
        guide_paint.set_color(parse_color(&widget.track_filled_color, 1.0));
        canvas.save();
        canvas.clip_path(&inner_path, ClipOp::Intersect, true);
        canvas.save_layer_alpha_f(None, GUIDE_OPACITY * widget.opacity);
        canvas.draw_line(
            Point::new(geometry.center_x, geometry.center_y - geometry.outer_radius),
            Point::new(geometry.center_x, geometry.center_y - geometry.inner_radius),
            &guide_paint,
        );
        canvas.restore();
        canvas.restore();

        Ok(LeanAngleCache {
            static_image: surface.image_snapshot(),
            static_image_x: widget.x - padding as f32,
            static_image_y: widget.y - padding as f32,
            x: widget.x,
            y: widget.y,
            width,
            height,
            rotation: widget.rotation,
            layout,
            track_thickness,
            track_border_thickness: border_thickness,
            shadow,
            opacity: widget.opacity,
            track_filled_color: widget.track_filled_color.clone(),
            track_filled_opacity: widget.track_filled_opacity,
            font: widget.font.clone(),
            font_size: widget.font_size * scale,
            color: widget.color.clone(),
            unit_color: widget.unit_color.clone(),
            text_border_color: scene.border_color.clone(),
            text_border_thickness: scene.border_thickness * scale,
            show_units: widget.show_units,
            value_offset_x: widget.value_offset_x * scale,
            value_offset_y: widget.value_offset_y * scale,
        })
    })
}

pub fn draw_lean_angle_widget(
    canvas: &Canvas,
    cache: &LeanAngleCache,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("lean_angle.draw", || {
        canvas.draw_image(
            &cache.static_image,
            (cache.static_image_x, cache.static_image_y),
            None,
        );

        let raw = dense_activity
            .series
            .lean_angle
            .get(frame_index)
            .copied()
            .flatten();
        let display_value =
            resolve_metric_display_value(MetricKind::LeanAngle, raw, dense_activity);
        let fill_sweep = lean_angle_fill_sweep(display_value);
        let center_x = cache.x + cache.layout.center_x;
        let center_y = cache.y + cache.layout.center_y;

        if fill_sweep != 0.0 {
            let track_geometry = LeanAngleGeometry {
                center_x,
                center_y,
                ..geometry_from_layout(cache.layout)
            };
            let fill_geometry = LeanAngleGeometry {
                start_angle: CENTER_ANGLE,
                sweep_angle: fill_sweep,
                ..inset_geometry(track_geometry, cache.track_border_thickness)
            };
            let inner_track_path =
                annular_sector_inset_path(track_geometry, cache.track_border_thickness);
            let mut fill_paint = Paint::default();
            fill_paint.set_anti_alias(true);
            fill_paint.set_style(Style::Fill);
            fill_paint.set_color(parse_color(
                &cache.track_filled_color,
                cache.track_filled_opacity * cache.opacity,
            ));
            canvas.save();
            canvas.clip_path(&inner_track_path, ClipOp::Intersect, true);
            canvas.draw_path(&annular_sector_path(fill_geometry), &fill_paint);
            canvas.restore();
        }

        draw_lean_angle_value(
            canvas,
            cache,
            display_value,
            center_x,
            center_y,
            scale,
            font_dirs,
        )
        .ok()?;

        let marker = polar_point(
            center_x,
            center_y,
            cache.layout.outer_radius,
            CENTER_ANGLE + fill_sweep,
        );
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "lean_angle".to_string(),
                bbox: [cache.x, cache.y, cache.layout.width, cache.layout.height],
                widget_width: cache.width,
                widget_height: cache.height,
                rotation_deg: cache.rotation,
            },
            frame: WidgetFrameReport {
                progress01: fill_sweep.abs() / LEAN_ANGLE_MAX_FILL_SWEEP,
                marker_x: marker.x - cache.x,
                marker_y: marker.y - cache.y,
                marker_abs_x: marker.x,
                marker_abs_y: marker.y,
            },
        })
    })
}

fn draw_lean_angle_value(
    canvas: &Canvas,
    cache: &LeanAngleCache,
    raw: Option<f64>,
    center_x: f32,
    center_y: f32,
    scale: f32,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let value_text = format_lean_angle_value(raw);
    let value_font_size = cache.font_size;
    let unit_text = if raw.is_some() && cache.show_units {
        standard_metric_unit_label(MetricKind::LeanAngle, None)
    } else {
        ""
    };
    let unit_font_size =
        (value_font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
    let value_line_height = value_font_size * METRIC_WIDGET_LINE_HEIGHT;
    let unit_line_height = unit_font_size * METRIC_WIDGET_LINE_HEIGHT;

    let value_style = ResolvedTextStyle {
        x: 0.0,
        y: 0.0,
        font_name: Some(cache.font.clone()),
        font_size: value_font_size,
        line_height: value_line_height,
        color: parse_color(&cache.color, cache.opacity),
        opacity: cache.opacity,
        shadow_color: cache
            .shadow
            .as_ref()
            .map(|shadow| parse_color(&shadow.color, cache.opacity)),
        shadow_strength: cache.shadow.as_ref().map_or(0.0, |shadow| shadow.strength),
        shadow_distance: cache.shadow.as_ref().map_or(0.0, |shadow| shadow.distance),
        border_color: Some(parse_color(&cache.text_border_color, cache.opacity)),
        border_thickness: cache.text_border_thickness,
    };
    let value_measure = measure_text(&value_text, &value_style, font_dirs)?;
    let value_vertical_measure = measure_text(
        metric_vertical_metrics_text(&value_text),
        &value_style,
        font_dirs,
    )?;

    let (unit_measure, unit_vertical_measure) = if unit_text.is_empty() {
        (None, None)
    } else {
        let unit_style = ResolvedTextStyle {
            font_size: unit_font_size,
            line_height: unit_line_height,
            color: parse_color(&cache.unit_color, cache.opacity),
            ..value_style.clone()
        };
        let unit_vertical_text = if unit_text == "\u{00b0}" {
            "\u{00b0}C"
        } else {
            unit_text
        };
        (
            Some(measure_text(unit_text, &unit_style, font_dirs)?),
            Some(measure_text(
                metric_vertical_metrics_text(unit_vertical_text),
                &unit_style,
                font_dirs,
            )?),
        )
    };

    let units_width = unit_measure
        .as_ref()
        .map(|measure| METRIC_WIDGET_UNITS_GAP_PX * scale + measure.width)
        .unwrap_or(0.0);
    let group_width = value_measure.width + units_width;
    let group_height = unit_measure
        .is_some()
        .then_some(value_line_height.max(unit_line_height))
        .unwrap_or(value_line_height);
    let degree_unit_offset = if unit_text.is_empty() {
        0.0
    } else {
        value_font_size * DEGREE_UNIT_CENTERING_OFFSET_RATIO
    };
    let group_left = center_x + cache.value_offset_x + degree_unit_offset - group_width * 0.5;
    let group_top = center_y + cache.value_offset_y - group_height * 0.5;
    let group_bottom = group_top + group_height;
    let value_glyph_height =
        (value_vertical_measure.bounds_bottom - value_vertical_measure.bounds_top).abs();

    let mut value_style = value_style;
    value_style.x = group_left;
    value_style.y = group_bottom - (value_line_height + value_glyph_height) * 0.5;
    draw_text_with_vertical_metrics_text(
        canvas,
        &value_text,
        metric_vertical_metrics_text(&value_text),
        &value_style,
        font_dirs,
    )?;

    if let (Some(_unit_measure), Some(unit_vertical_measure)) =
        (unit_measure, unit_vertical_measure)
    {
        let unit_vertical_text = if unit_text == "\u{00b0}" {
            "\u{00b0}C"
        } else {
            unit_text
        };
        let mut unit_style = value_style.clone();
        unit_style.font_size = unit_font_size;
        unit_style.line_height = unit_line_height;
        unit_style.color = parse_color(&cache.unit_color, cache.opacity);
        unit_style.x = group_left + value_measure.width + METRIC_WIDGET_UNITS_GAP_PX * scale;
        let unit_glyph_height =
            (unit_vertical_measure.bounds_bottom - unit_vertical_measure.bounds_top).abs();
        unit_style.y = group_bottom - (unit_line_height + unit_glyph_height) * 0.5;
        draw_text_with_vertical_metrics_text(
            canvas,
            unit_text,
            metric_vertical_metrics_text(unit_vertical_text),
            &unit_style,
            font_dirs,
        )?;
    }

    Ok(())
}

fn annular_sector_path(geometry: LeanAngleGeometry) -> Path {
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

fn annular_sector_inset_path(geometry: LeanAngleGeometry, border_thickness: f32) -> Path {
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

fn polar_point(center_x: f32, center_y: f32, radius: f32, angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(
        center_x + radius * radians.cos(),
        center_y + radius * radians.sin(),
    )
}

fn path_tangent(angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(-radians.sin(), radians.cos())
}
