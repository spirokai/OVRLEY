//! Linear gauge metric widget rendering.
//!
//! Handles the linear gauge presentation: a filled bar track with optional
//! border, rounded corners, and min/max labels. The static layer (track
//! background + border + labels) is pre-rendered into a cached image; the
//! dynamic filled portion is drawn per-frame on top.

use crate::activity::schema::{DenseActivityReport, DenseSeriesReport};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{
    ValidatedLinearGaugeOrientation, ValidatedLinearGaugeWidget, ValidatedSceneConfig,
};
use crate::render::surface::create_surface;
use crate::render::text::{origin_x_for_centered_text, parse_color, resolve_font};
use crate::render::widgets::types::{
    LinearGaugeCache, LinearGaugeFrameState, WidgetFrameReport, WidgetGeometryReport,
    WidgetRenderReport,
};
use crate::types::{DisplayType, MetricKind};
use skia_safe::{image_filters, BlendMode, Canvas, Paint, Point, RRect, Rect};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearGaugeOrientation {
    Horizontal,
    Vertical,
}

impl From<ValidatedLinearGaugeOrientation> for LinearGaugeOrientation {
    fn from(value: ValidatedLinearGaugeOrientation) -> Self {
        match value {
            ValidatedLinearGaugeOrientation::Horizontal => Self::Horizontal,
            ValidatedLinearGaugeOrientation::Vertical => Self::Vertical,
        }
    }
}

/// Computes the fill fraction for a value within a min-max range.
/// Returns a value clamped between 0.0 and 1.0, or 0.0 if the range is invalid.
pub fn fill_percentage(value: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// Computes the filled-bar rect without border insetting.
pub fn bar_fill_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill01: f32,
    orientation: LinearGaugeOrientation,
) -> (f32, f32, f32, f32) {
    let fill01 = fill01.clamp(0.0, 1.0);
    match orientation {
        LinearGaugeOrientation::Horizontal => (x, y, width * fill01, height),
        LinearGaugeOrientation::Vertical => {
            let filled_height = height * fill01;
            (x, y + height - filled_height, width, filled_height)
        }
    }
}

/// Computes the filled-bar rect, inset by the border thickness.
pub fn bordered_bar_fill_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill01: f32,
    orientation: LinearGaugeOrientation,
    border_thickness: f32,
) -> (f32, f32, f32, f32) {
    let inset = border_thickness.max(0.0);
    let inner_width = (width - inset * 2.0).max(0.0);
    let inner_height = (height - inset * 2.0).max(0.0);
    bar_fill_rect(
        x + inset,
        y + inset,
        inner_width,
        inner_height,
        fill01,
        orientation,
    )
}

/// Prepares a pre-rendered static image and per-frame fill states for a
/// linear gauge widget. The static layer (track background, border, labels)
/// is drawn once; the dynamic filled bar is composited per-frame.
pub fn prepare_linear_gauge_cache(
    gauge: &ValidatedLinearGaugeWidget,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LinearGaugeCache> {
    prepare_profiler.measure("gauge.linear.prepare", || {
        let scaled_width = ((gauge.width as f32) * scale).round().max(1.0) as u32;
        let scaled_height = ((gauge.height as f32) * scale).round().max(1.0) as u32;
        let (min_value, max_value) = metric_range(&dense_activity.series, gauge.metric);
        let frame_states = metric_values(&dense_activity.series, gauge.metric)
            .iter()
            .map(|value| {
                let value = value.unwrap_or(min_value);
                LinearGaugeFrameState {
                    value,
                    fill01: fill_percentage(value, min_value, max_value),
                }
            })
            .collect::<Vec<_>>();

        let mut surface = create_surface(scaled_width, scaled_height)?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::TRANSPARENT);
        draw_static_linear_layer(
            canvas,
            gauge,
            scene,
            scaled_width,
            scaled_height,
            scale,
            font_dirs,
            min_value,
            max_value,
        )?;

        Ok(LinearGaugeCache {
            static_image: surface.image_snapshot(),
            x: gauge.x,
            y: gauge.y,
            width: scaled_width,
            height: scaled_height,
            rotation: gauge.rotation,
            display_type: DisplayType::Linear,
            orientation: gauge.orientation,
            track_corner_radius: gauge.track_corner_radius * scale,
            track_border_thickness: gauge.track_border_thickness * scale,
            track_filled_color: gauge.track_filled_color.clone(),
            track_filled_opacity: gauge.track_filled_opacity,
            track_fill_flat: gauge.track_fill_flat,
            min_value,
            max_value,
            frame_states,
        })
    })
}

/// Draws the linear gauge for a single frame: paints the pre-rendered static
/// image, then composites the filled bar on top using the frame's fill state.
pub fn draw_linear_gauge_widget(
    canvas: &Canvas,
    cache: &LinearGaugeCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    if cache.display_type != DisplayType::Linear {
        return None;
    }

    frame_profiler.measure("gauge.linear.draw", || {
        canvas.draw_image(&cache.static_image, (cache.x, cache.y), None);

        let state = cache
            .frame_states
            .get(frame_index)
            .or_else(|| cache.frame_states.last())?;
        let (x, y, width, height) = bordered_bar_fill_rect(
            cache.x,
            cache.y,
            cache.width as f32,
            cache.height as f32,
            state.fill01,
            cache.orientation.into(),
            cache.track_border_thickness,
        );
        if width > 0.0 && height > 0.0 {
            let mut fill_paint = Paint::default();
            fill_paint.set_anti_alias(true);
            fill_paint.set_color(parse_color(
                &cache.track_filled_color,
                cache.track_filled_opacity,
            ));
            let fill_rect = Rect::from_xywh(x, y, width, height);
            let radius = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
            if cache.track_fill_flat && radius > 0.0 {
                let inset = cache.track_border_thickness.max(0.0);
                let inner_rect = Rect::from_xywh(
                    cache.x + inset,
                    cache.y + inset,
                    (cache.width as f32 - inset * 2.0).max(0.0),
                    (cache.height as f32 - inset * 2.0).max(0.0),
                );
                canvas.save();
                canvas.clip_rect(fill_rect, skia_safe::ClipOp::Intersect, true);
                canvas.draw_rrect(RRect::new_rect_xy(inner_rect, radius, radius), &fill_paint);
                canvas.restore();
            } else {
                canvas.draw_rrect(RRect::new_rect_xy(fill_rect, radius, radius), &fill_paint);
            }
        }

        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "linear_gauge".to_string(),
                bbox: [cache.x, cache.y, cache.width as f32, cache.height as f32],
                widget_width: cache.width,
                widget_height: cache.height,
                rotation_deg: cache.rotation,
            },
            frame: WidgetFrameReport {
                progress01: state.fill01,
                marker_x: cache.width as f32 * state.fill01,
                marker_y: cache.height as f32 * (1.0 - state.fill01),
                marker_abs_x: cache.x + cache.width as f32 * state.fill01,
                marker_abs_y: cache.y + cache.height as f32 * (1.0 - state.fill01),
            },
        })
    })
}

fn draw_static_linear_layer(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedSceneConfig,
    width: u32,
    height: u32,
    scale: f32,
    font_dirs: &[PathBuf],
    min_value: f64,
    max_value: f64,
) -> CoreResult<()> {
    let w = width as f32;
    let h = height as f32;
    let radius = gauge.track_corner_radius * scale;
    let border = gauge.track_border_thickness * scale;

    let shadow_active = scene.shadow_strength > 0.0 || scene.shadow_distance > 0.0;
    let shadow_color = if shadow_active && !scene.shadow_color.is_empty() {
        Some(parse_color(&scene.shadow_color, 1.0))
    } else {
        None
    };
    let shadow_filter = shadow_color.and_then(|color| {
        image_filters::drop_shadow_only(
            (scene.shadow_distance * scale, scene.shadow_distance * scale),
            (scene.shadow_strength * scale, scene.shadow_strength * scale),
            color,
            None,
            None,
        )
    });

    if border > 0.0 {
        let outer_rect = Rect::from_xywh(0.0, 0.0, w, h);
        let outer_rrect = RRect::new_rect_xy(outer_rect, radius, radius);
        let mut border_paint = Paint::default();
        border_paint.set_anti_alias(true);
        border_paint.set_color(parse_color(&gauge.track_border_color, 1.0));
        if let Some(ref filter) = shadow_filter {
            border_paint.set_image_filter(filter.clone());
        }
        canvas.draw_rrect(outer_rrect, &border_paint);

        let inner_rect = Rect::from_xywh(
            border,
            border,
            (w - border * 2.0).max(0.0),
            (h - border * 2.0).max(0.0),
        );
        let inner_radius = (radius - border).max(0.0);
        let inner_rrect = RRect::new_rect_xy(inner_rect, inner_radius, inner_radius);
        let mut clear_paint = Paint::default();
        clear_paint.set_anti_alias(true);
        clear_paint.set_blend_mode(BlendMode::Clear);
        canvas.draw_rrect(inner_rrect, &clear_paint);
    }

    let inner_rect = Rect::from_xywh(
        border,
        border,
        (w - border * 2.0).max(0.0),
        (h - border * 2.0).max(0.0),
    );
    let inner_radius = (radius - border).max(0.0);
    let inner_rrect = RRect::new_rect_xy(inner_rect, inner_radius, inner_radius);
    let mut empty_paint = Paint::default();
    empty_paint.set_anti_alias(true);
    empty_paint.set_color(parse_color(
        &gauge.track_empty_color,
        gauge.track_empty_opacity,
    ));
    if border <= 0.0 {
        if let Some(ref filter) = shadow_filter {
            empty_paint.set_image_filter(filter.clone());
        }
    }
    canvas.draw_rrect(inner_rrect, &empty_paint);

    if gauge.show_min_max_labels {
        let font_size = gauge.min_max_label_font_size * scale;
        let font = resolve_font(font_dirs, Some(&gauge.min_max_label_font), font_size)?;
        let mut text_paint = Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color(parse_color(&gauge.min_max_label_color, 1.0));
        let baseline = (h + font_size) / 2.0 - font_size * 0.15;
        let min_label = format_linear_gauge_label(min_value);
        let max_label = format_linear_gauge_label(max_value);
        canvas.draw_str(
            &min_label,
            Point::new(4.0 * scale, baseline),
            &font,
            &text_paint,
        );
        let max_x = origin_x_for_centered_text(&max_label, w - 12.0 * scale, &font);
        canvas.draw_str(&max_label, Point::new(max_x, baseline), &font, &text_paint);
    }

    Ok(())
}

/// Formats a gauge boundary value for display. Integers show no decimal;
/// non-integers show one decimal place.
pub fn format_linear_gauge_label(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn metric_range(series: &DenseSeriesReport, metric: MetricKind) -> (f64, f64) {
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    for value in metric_values(series, metric).iter().flatten() {
        min_value = min_value.min(*value);
        max_value = max_value.max(*value);
    }
    if min_value.is_finite() && max_value.is_finite() && max_value > min_value {
        (min_value, max_value)
    } else {
        (0.0, 100.0)
    }
}

fn metric_values(series: &DenseSeriesReport, metric: MetricKind) -> &[Option<f64>] {
    match metric {
        MetricKind::Speed => &series.speed,
        MetricKind::Elevation => &series.elevation,
        MetricKind::Heartrate => &series.heartrate,
        MetricKind::Cadence => &series.cadence,
        MetricKind::Power => &series.power,
        MetricKind::Temperature => &series.temperature,
        MetricKind::Pace => &series.pace,
        MetricKind::GForce => &series.g_force,
        MetricKind::AirPressure => &series.air_pressure,
        MetricKind::GroundContactTime => &series.ground_contact_time,
        MetricKind::StrideLength => &series.stride_length,
        MetricKind::StrokeRate => &series.stroke_rate,
        MetricKind::Torque => &series.torque,
        MetricKind::VerticalSpeed => &series.vertical_speed,
        MetricKind::Altitude => &series.altitude,
        MetricKind::Iso => &series.iso,
        MetricKind::Aperture => &series.aperture,
        MetricKind::ShutterSpeed => &series.shutter_speed,
        MetricKind::FocalLength => &series.focal_length,
        MetricKind::Ev => &series.ev,
        MetricKind::ColorTemperature => &series.color_temperature,
        MetricKind::GearPosition => &series.gear_position,
        MetricKind::VerticalRatio => &series.vertical_ratio,
        MetricKind::VerticalOscillation => &series.vertical_oscillation,
        MetricKind::CoreTemperature => &series.core_temperature,
        MetricKind::Heading => &series.heading,
        MetricKind::LeftRightBalance | MetricKind::Gradient | MetricKind::Time => &[],
    }
}
