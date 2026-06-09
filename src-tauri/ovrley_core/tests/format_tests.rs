//! Metric formatting tests.
//!
//! Verifies `format_validated_metric_parts` and `format_time_key` produce
//! correct display text, units, and icon assignments for representative
//! standard metrics.

mod common;

use chrono::DateTime;
use serde_json::json;

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use ovrley_core::normalize::ValidatedValueWidget;
use ovrley_core::render::format::{format_time_key, format_validated_metric_parts, MetricIconKind};

#[test]
fn formats_time_key_variants() {
    let timestamp = DateTime::parse_from_rfc3339("2025-04-21T13:05:00Z")
        .unwrap()
        .to_utc();
    assert_eq!(format_time_key("time-24", timestamp), "13:05");
    assert_eq!(format_time_key("time-12", timestamp), "01:05 PM");
    assert_eq!(
        format_time_key("date-dd-mmm-yyyy", timestamp),
        "21 APR 2025"
    );
}

#[test]
fn formats_metric_parts_for_speed() {
    let validated = validated_standard_value(json!({
        "value": "speed",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 0,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": true,
        "unit_color": "#ffffff",
        "display_unit": "kmh",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.speed = vec![Some(10.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "36");
    assert_eq!(parts.unit_text.as_deref(), Some("KM/H"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Gauge));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_temperature_with_degree_units() {
    let validated = validated_standard_value(json!({
        "value": "temperature",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 0,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": true,
        "unit_color": "#ffffff",
        "display_unit": "fahrenheit",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.temperature = vec![Some(20.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "68");
    assert_eq!(parts.unit_text.as_deref(), Some("\u{00B0}F"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Thermometer));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_pace() {
    let validated = validated_standard_value(json!({
        "value": "pace",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 0,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": true,
        "unit_color": "#ffffff",
        "display_unit": "min_per_km",
        "balance_format": "plain",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.pace = vec![Some(275.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "4:35");
    assert_eq!(parts.unit_text.as_deref(), Some("MIN/KM"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Footprints));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_left_right_balance() {
    let validated = validated_standard_value(json!({
        "value": "left_right_balance",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 0,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": false,
        "unit_color": "#ffffff",
        "display_unit": "percent",
        "balance_format": "plain",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.left_right_balance = vec![Some(54.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "54/46");
    assert_eq!(parts.unit_text, None);
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Scale));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_heading() {
    let validated = validated_standard_value(json!({
        "value": "heading",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 0,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": false,
        "unit_color": "#ffffff",
        "display_unit": "degrees",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.heading = vec![Some(91.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "91");
    assert_eq!(parts.unit_text, None);
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Compass));
    assert!(parts.show_icon);
}

fn validated_standard_value(value: serde_json::Value) -> ValidatedValueWidget {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [value],
        "plots": []
    }));
    common::seam::expect_standard_value(config.values.into_iter().next().unwrap(), 0)
}

fn dense_report_with(fill: impl FnOnce(&mut DenseSeriesReport)) -> DenseActivityReport {
    common::builders::dense_report_with(fill)
}
