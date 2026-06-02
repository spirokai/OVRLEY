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

use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use crate::render::format::format_metric_parts;
use crate::render::text::ResolvedTextStyle;
use crate::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use crate::types::MetricKind;
use skia_safe::Canvas;
use std::path::PathBuf;

pub use gradient::gradient_triangle_height;
pub use layout::{
    metric_icon_top_from_value_layout, metric_vertical_metrics_text, NUMERIC_VERTICAL_METRICS_TEXT,
};

pub(crate) use layout::{
    draw_metric_parts, draw_static_metric_icon_for_value, has_static_metric_icon,
};

/// Bundled parameters for drawing a metric value widget.
pub(crate) struct MetricWidgetRequest<'a> {
    pub canvas: &'a Canvas,
    pub config: &'a RenderConfig,
    pub value: &'a ValueConfig,
    pub base_style: &'a ResolvedTextStyle,
    pub dense_activity: &'a DenseActivityReport,
    pub frame_index: usize,
    pub scale: f32,
    pub font_dirs: &'a [PathBuf],
    pub static_icon_rendered: bool,
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
pub(crate) fn draw_metric_value_widget_with_config(request: MetricWidgetRequest<'_>) -> bool {
    if request.value.value == MetricKind::Gradient {
        return gradient::draw_gradient_value_widget(
            request.canvas,
            request.config,
            request.value,
            request.base_style,
            request.dense_activity,
            request.frame_index,
            request.scale,
            request.font_dirs,
        );
    }

    if display_type_layout_mode(request.value.display_type) == DisplayTypeLayoutMode::Boxed {
        return true;
    }

    let Some(parts) = format_metric_parts(
        request.config,
        request.value,
        request.dense_activity,
        request.frame_index,
    ) else {
        return false;
    };
    draw_metric_parts(
        request.canvas,
        request.value,
        request.base_style,
        &parts,
        request.scale,
        request.font_dirs,
        request.static_icon_rendered,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::schema::{DenseActivityReport, DenseSeriesReport};
    use crate::config::RenderConfig;
    use crate::render::surface::create_surface;
    use crate::render::text::ResolvedTextStyle;
    use skia_safe::Color;
    use std::collections::BTreeMap;

    fn value_config_json(display_type: &str) -> ValueConfig {
        serde_json::from_value(serde_json::json!({
            "value": "speed",
            "x": 10,
            "y": 20,
            "display_type": display_type
        }))
        .unwrap()
    }

    #[test]
    fn all_boxed_display_types_marked_handled() {
        let config = RenderConfig {
            scene: serde_json::from_value(serde_json::json!({
                "fps": 30.0,
                "start": 0.0,
                "end": 10.0
            }))
            .unwrap(),
            labels: vec![],
            values: vec![],
            plots: serde_json::Value::Object(serde_json::Map::new()),
            extra: BTreeMap::new(),
        };
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
            border_distance: 0.0,
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
            let value = value_config_json(dt_str);
            assert!(
                draw_metric_value_widget_with_config(MetricWidgetRequest {
                    canvas: surface.canvas(),
                    config: &config,
                    value: &value,
                    base_style: &style,
                    dense_activity: &dense,
                    frame_index: 0,
                    scale: 1.0,
                    font_dirs: &[],
                    static_icon_rendered: false,
                }),
                "Boxed display type {dt_str} should be marked handled by value module"
            );
        }
    }

    #[test]
    fn text_display_type_marked_handled_when_formatter_exists() {
        let config = RenderConfig {
            scene: serde_json::from_value(serde_json::json!({
                "fps": 30.0,
                "start": 0.0,
                "end": 10.0
            }))
            .unwrap(),
            labels: vec![],
            values: vec![],
            plots: serde_json::Value::Object(serde_json::Map::new()),
            extra: BTreeMap::new(),
        };
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
            border_distance: 0.0,
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
        let value = value_config_json("text");

        assert!(
            draw_metric_value_widget_with_config(MetricWidgetRequest {
                canvas: surface.canvas(),
                config: &config,
                value: &value,
                base_style: &style,
                dense_activity: &dense,
                frame_index: 0,
                scale: 1.0,
                font_dirs: &[],
                static_icon_rendered: false,
            }),
            "Text display type should be handled by value module"
        );
    }
}
