//! Dynamic metric value widgets.
//!
//! Metric values can render as plain text, icon/value/unit groups, or the
//! special gradient display with a slope triangle. Icons are loaded from the
//! frontend SVG assets and parsed into a small subset of Skia primitives so the
//! Rust renderer visually matches the editor.
//!
//! Module ownership:
//! - `layout` — text positioning, icon/unit row layout, static icon cache logic.
//! - `gradient` — slope triangle rendering and triangle height math.
//! - `icons` — icon kind mapping, SVG cache, paint creation, primitives drawing.
//! - `svg` — lightweight SVG parser, path tokenizer, and Skia path conversion.

mod gradient;
mod icons;
mod layout;
mod svg;

pub(crate) use icons::metric_icon_kind_for_value;

use crate::activity::schema::DenseActivityReport;
use crate::error::CoreResult;
use crate::normalize::{ValidatedGradientWidget, ValidatedTimeValue, ValidatedValueWidget};
use crate::render::format::{format_validated_metric_parts, format_validated_time_parts};
use crate::render::text::ResolvedTextStyle;
use crate::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use crate::types::{DisplayType, MetricKind};
use skia_safe::Canvas;
use std::path::PathBuf;

pub use gradient::gradient_triangle_height;
pub use layout::{
    metric_icon_top_from_value_layout, metric_vertical_metrics_text, NUMERIC_VERTICAL_METRICS_TEXT,
};

pub(crate) use layout::{
    draw_metric_parts, draw_static_metric_icon_for_value_validated,
    has_static_metric_icon_validated,
};

/// Bundled parameters for drawing a metric value widget.
pub(crate) struct MetricWidgetRequest<'a> {
    pub canvas: &'a Canvas,
    pub metric_kind: MetricKind,
    pub display_type: DisplayType,
    pub base_style: &'a ResolvedTextStyle,
    pub dense_activity: &'a DenseActivityReport,
    pub frame_index: usize,
    pub scale: f32,
    pub font_dirs: &'a [PathBuf],
    pub static_icon_rendered: bool,
    /// Pre-validated value widget. When present, the validated path is used
    /// instead of reading from `value` — zero backend-owned defaults.
    pub validated: Option<&'a ValidatedValueWidget>,
    /// Pre-validated gradient widget. When present, the validated path is used
    /// instead of reading from `value` — zero backend-owned defaults.
    pub validated_gradient: Option<&'a ValidatedGradientWidget>,
    /// Pre-validated time widget. When present, the validated path is used
    /// instead of reading from legacy raw `ValueConfig`.
    pub validated_time: Option<&'a ValidatedTimeValue>,
}

/// Draws a configured metric widget and reports whether it handled the value.
///
/// Returns `true` when the value was handled — either by this module (intrinsic
/// text) or by the metric presentation pipeline (boxed display types). Returns
/// `false` for metric kinds without a formatter so callers can fall back to
/// generic formatted text drawing.
///
/// Boxed display types (heading_tape, linear, bars, arc, corner) are not drawn
/// here — the caller is responsible for invoking
/// [`super::metric_presentation::draw_metric_presentation`] for non-intrinsic
/// display types. This function returns `true` for those types to signal that
/// they should not fall through to the generic text path.
pub(crate) fn draw_metric_value_widget_with_config(
    request: MetricWidgetRequest<'_>,
) -> CoreResult<bool> {
    if request.metric_kind == MetricKind::Gradient {
        let validated_gradient = request
            .validated_gradient
            .expect("gradient widget must be validated before rendering");
        return gradient::draw_gradient_value_widget(
            request.canvas,
            validated_gradient,
            request.base_style,
            request.dense_activity,
            request.frame_index,
            request.scale,
            request.font_dirs,
        );
    }

    if request.metric_kind == MetricKind::Time {
        let validated_time = request
            .validated_time
            .expect("time widget must be validated before rendering");
        let raw_time = request
            .dense_activity
            .series
            .time
            .get(request.frame_index)
            .and_then(|value| value.as_deref());
        let parts = format_validated_time_parts(validated_time, raw_time);
        draw_metric_parts(
            request.canvas,
            request.base_style,
            &parts,
            request.scale,
            request.font_dirs,
            request.static_icon_rendered,
            &validated_time.base,
        )?;
        return Ok(true);
    }

    if display_type_layout_mode(request.display_type) == DisplayTypeLayoutMode::Boxed {
        return Ok(true);
    }

    // Validated path: use pre-validated type with zero backend-owned defaults.
    let validated = request.validated.expect(
        "standard metric text widget must be validated — validation happens at render entry point",
    );
    let Some(parts) =
        format_validated_metric_parts(validated, request.dense_activity, request.frame_index)
    else {
        return Ok(false);
    };
    draw_metric_parts(
        request.canvas,
        request.base_style,
        &parts,
        request.scale,
        request.font_dirs,
        request.static_icon_rendered,
        validated,
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::schema::{DenseActivityReport, DenseSeriesReport};
    use crate::render::surface::create_surface;
    use crate::render::text::ResolvedTextStyle;
    use crate::types::DisplayType;
    use skia_safe::Color;

    #[test]
    fn all_boxed_display_types_marked_handled() {
        let style = ResolvedTextStyle {
            x: 0.0,
            y: 0.0,
            font_name: None,
            font_size: 40.0,
            line_height: 36.8,
            color: Color::WHITE,
            opacity: 1.0,
            shadow_color: None,
            shadow_strength: 0.0,
            shadow_distance: 0.0,
            border_color: None,
            border_thickness: 0.0,
        };
        let dense = DenseActivityReport {
            frame_count: 1,
            frame_elapsed_seconds: vec![0.0],
            frame_distance_progress: vec![Some(0.0)],
            series: DenseSeriesReport {
                speed: vec![Some(10.0)],
                elevation: vec![],
                gradient: vec![],
                heartrate: vec![],
                cadence: vec![],
                power: vec![],
                temperature: vec![],
                pace: vec![],
                g_force: vec![],
                air_pressure: vec![],
                ground_contact_time: vec![],
                left_right_balance: vec![],
                stride_length: vec![],
                stroke_rate: vec![],
                torque: vec![],
                vertical_speed: vec![],
                gear_position: vec![],
                vertical_ratio: vec![],
                vertical_oscillation: vec![],
                core_temperature: vec![],
                heading: vec![],
                course_lat: vec![],
                course_lon: vec![],
                time: vec![],
            },
        };
        let mut surface = create_surface(400, 200).unwrap();

        for dt_str in ["heading_tape", "linear", "bars", "arc", "corner"] {
            let display_type = match dt_str {
                "heading_tape" => DisplayType::Tape,
                "linear" => DisplayType::Linear,
                "bars" => DisplayType::Bars,
                "arc" => DisplayType::Arc,
                "corner" => DisplayType::Corner,
                _ => unreachable!(),
            };
            assert!(
                draw_metric_value_widget_with_config(MetricWidgetRequest {
                    canvas: surface.canvas(),
                    metric_kind: crate::MetricKind::Speed,
                    display_type,
                    base_style: &style,
                    dense_activity: &dense,
                    frame_index: 0,
                    scale: 1.0,
                    font_dirs: &[],
                    static_icon_rendered: false,
                    validated: None,
                    validated_gradient: None,
                    validated_time: None,
                })
                .unwrap(),
                "Boxed display type {dt_str} should be marked handled by value module"
            );
        }
    }

    #[test]
    fn text_display_type_marked_handled_when_formatter_exists() {
        let style = ResolvedTextStyle {
            x: 0.0,
            y: 0.0,
            font_name: None,
            font_size: 40.0,
            line_height: 36.8,
            color: Color::WHITE,
            opacity: 1.0,
            shadow_color: None,
            shadow_strength: 0.0,
            shadow_distance: 0.0,
            border_color: None,
            border_thickness: 0.0,
        };
        let dense = DenseActivityReport {
            frame_count: 1,
            frame_elapsed_seconds: vec![0.0],
            frame_distance_progress: vec![Some(0.0)],
            series: DenseSeriesReport {
                speed: vec![Some(10.0)],
                elevation: vec![],
                gradient: vec![],
                heartrate: vec![],
                cadence: vec![],
                power: vec![],
                temperature: vec![],
                pace: vec![],
                g_force: vec![],
                air_pressure: vec![],
                ground_contact_time: vec![],
                left_right_balance: vec![],
                stride_length: vec![],
                stroke_rate: vec![],
                torque: vec![],
                vertical_speed: vec![],
                gear_position: vec![],
                vertical_ratio: vec![],
                vertical_oscillation: vec![],
                core_temperature: vec![],
                heading: vec![],
                course_lat: vec![],
                course_lon: vec![],
                time: vec![],
            },
        };
        let mut surface = create_surface(400, 200).unwrap();
        let validated = crate::normalize::ValidatedValueWidget {
            metric: crate::MetricKind::Speed,
            x: 10.0,
            y: 20.0,
            display_type: DisplayType::Text,
            font_name: "Arial.ttf".to_string(),
            font_size: 32.0,
            color: [0xff, 0xff, 0xff, 0xff],
            opacity: 1.0,
            show_icon: true,
            icon_color: [0x40, 0xe0, 0xd0, 0xff],
            icon_size: 28.0,
            icon_offset_x: 0.0,
            icon_offset_y: 0.0,
            show_units: true,
            unit_color: [0xff, 0xff, 0xff, 0xff],
            display_unit: "kmh".to_string(),
            prefix: String::new(),
            suffix: String::new(),
            formatting: crate::normalize::ValidatedValueFormatting::DecimalPlaces { decimals: 0 },
            hours_offset: None,
            format: None,
        };

        assert!(
            draw_metric_value_widget_with_config(MetricWidgetRequest {
                canvas: surface.canvas(),
                metric_kind: crate::MetricKind::Speed,
                display_type: DisplayType::Text,
                base_style: &style,
                dense_activity: &dense,
                frame_index: 0,
                scale: 1.0,
                font_dirs: &[],
                static_icon_rendered: false,
                validated: Some(&validated),
                validated_gradient: None,
                validated_time: None,
            })
            .unwrap(),
            "Text display type should be handled by value module"
        );
    }
}
