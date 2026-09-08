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
use ovrley_core::render::format::{
    format_time_key, format_validated_metric_parts, MetricDisplayContent, MetricIconKind,
};

#[test]
fn formats_time_key_variants() {
    let timestamp = DateTime::parse_from_rfc3339("2025-04-21T13:05:00Z")
        .unwrap()
        .to_utc();
    assert_eq!(format_time_key("time-24", timestamp), "13:05");
    assert_eq!(format_time_key("time-24s", timestamp), "13:05:00");
    assert_eq!(format_time_key("time-12", timestamp), "01:05 PM");
    assert_eq!(format_time_key("time-12s", timestamp), "01:05:00 PM");
    assert_eq!(
        format_time_key("date-time-24s", timestamp),
        "21-04-2025 13:05:00"
    );
    assert_eq!(
        format_time_key("date-time-12s", timestamp),
        "21-04-2025 01:05:00 PM"
    );
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
        "content_alignment": "left",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.speed = vec![Some(10.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "36");
    assert_eq!(parts.standard_text().1, Some("KM/H"));
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
        "content_alignment": "left",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.temperature = vec![Some(20.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "68");
    assert_eq!(parts.standard_text().1, Some("\u{00B0}F"));
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
        "content_alignment": "left",
        "balance_format": "plain",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.pace = vec![Some(275.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "4:35");
    assert_eq!(parts.standard_text().1, Some("MIN/KM"));
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
        "content_alignment": "left",
        "balance_format": "plain",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.left_right_balance = vec![Some(54.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "54/46");
    assert_eq!(parts.standard_text().1, None);
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
        "content_alignment": "left",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| series.heading = vec![Some(91.0)]);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "91");
    assert_eq!(parts.standard_text().1, None);
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Compass));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_distance_with_fixed_decimals() {
    let validated = validated_standard_value(json!({
        "value": "distance",
        "x": 0.0,
        "y": 0.0,
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "prefix": "",
        "suffix": "",
        "decimals": 2,
        "show_icon": true,
        "icon_color": "#40e0d0",
        "icon_size": 28.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": true,
        "show_full_distance": true,
        "unit_color": "#ffffff",
        "display_unit": "km",
        "content_alignment": "left",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let mut dense = dense_report_with(|series| series.distance = vec![Some(2300.0)]);
    dense.full_activity_distance = Some(5000.0);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "2.30/5.00");
    assert_eq!(parts.standard_text().1, Some("KM"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Distance));
    assert!(parts.show_icon);
}

#[test]
fn formats_metric_parts_for_total_ascent_with_full_total() {
    let validated = validated_standard_value(json!({
        "value": "total_ascent",
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
        "show_full_ascent": true,
        "unit_color": "#ffffff",
        "display_unit": "m",
        "content_alignment": "left",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let mut dense = dense_report_with(|series| series.total_ascent = vec![Some(12.0)]);
    dense.full_activity_total_ascent = Some(25.0);

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    assert_eq!(parts.standard_text().0, "12/25");
    assert_eq!(parts.standard_text().1, Some("M"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::ArrowUpNarrowWide));
}

#[test]
fn formats_metric_parts_for_both_gps_coordinates_as_two_colored_lines() {
    let validated = validated_standard_value(json!({
        "value": "gps_coordinates",
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
        "unit_color": "#ff0000",
        "display_unit": "both",
        "content_alignment": "left",
        "coordinate_format": "dms",
        "triangle_width": 0.0,
        "display_type": "text"
    }));
    let dense = dense_report_with(|series| {
        series.course_lat = vec![Some(40.446111)];
        series.course_lon = vec![Some(-73.987222)];
    });

    let parts = format_validated_metric_parts(&validated, &dense, 0).unwrap();
    let MetricDisplayContent::Coordinates(coordinates) = &parts.content else {
        panic!("GPS coordinates must produce coordinate display content");
    };
    assert_eq!(coordinates.lines.len(), 2);
    assert_eq!(coordinates.lines[0].direction.as_deref(), Some("N"));
    assert_eq!(coordinates.lines[0].value_text, "40°26′46″");
    assert_eq!(coordinates.lines[1].value_text, "73°59′14″");
    assert_eq!(coordinates.lines[1].direction.as_deref(), Some("W"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Satellite));
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
