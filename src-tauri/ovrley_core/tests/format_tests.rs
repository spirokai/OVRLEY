//! Metric formatting tests.
//!
//! Verifies `format_metric_parts` and `format_time_key` produce correct
//! display text, units, and icon assignments for speed (with unit
//! conversion) and temperature (with Fahrenheit conversion). Confirms
//! built-in time format presets generate the expected time strings.
//!
//! ## Type
//! Unit test. Constructs `RenderConfig`/`ValueConfig`/`DenseActivityReport`
//! in memory — no fixtures, no I/O.
//!
//! ## Regressions guarded
//! - Speed unit conversion (m/s → km/h) incorrect
//! - Temperature conversion (C → F) incorrect
//! - Time format presets producing wrong strings
//! - Icon kind assignment diverging from MetricKind

use chrono::DateTime;
use serde_json::json;

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use ovrley_core::config::{RenderConfig, SceneConfig, ValueConfig};
use ovrley_core::render::format::{format_metric_parts, format_time_key, MetricIconKind};
use ovrley_core::MetricKind;

#[test]
// Verifies representative built-in time format presets.
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
// Verifies speed metric parts include converted value, units, and icon.
fn formats_metric_parts_for_speed() {
    let config = RenderConfig {
        scene: SceneConfig {
            width: None,
            height: None,
            fps: 30.0,
            start: 0.0,
            end: 1.0,
            font: None,
            font_size: None,
            color: None,
            decimal_rounding: None,
            overlay_filename: None,
            update_rate: None,
            composite_video_path: None,
            composite_bitrate: None,
            composite_sync_offset: None,
            composite_video_fps_num: None,
            composite_video_fps_den: None,
            composite_video_duration: None,
            composite_render_duration: None,
            composite_video_trim_start: None,
            composite_widget_update_rate: None,
            ffmpeg: json!({}),
            opacity: None,
            scale: None,
            time_format: None,
            shadow_color: None,
            shadow_strength: None,
            shadow_distance: None,
            border_color: None,
            border_thickness: None,
            border_strength: None,
            border_distance: None,
            custom_export_range_active: None,
            extra: Default::default(),
        },
        labels: vec![],
        values: vec![],
        plots: json!([]),
        extra: Default::default(),
    };
    let value = ValueConfig {
        value: MetricKind::Speed,
        x: 0.0,
        y: 0.0,
        font: None,
        font_family: None,
        font_size: None,
        color: None,
        opacity: None,
        suffix: None,
        prefix: None,
        unit: None,
        hours_offset: None,
        time_format: None,
        format: None,
        decimal_rounding: None,
        decimals: Some(0),
        show_icon: None,
        icon_color: None,
        icon_size: None,
        icon_offset_x: None,
        icon_offset_y: None,
        show_units: Some(true),
        speed_unit: Some("kmh".to_string()),
        temperature_unit: None,
        value_offset: None,
        triangle_positive_color: None,
        triangle_negative_color: None,
        show_sign: None,
        show_triangle: None,
        triangle_width: None,
        shadow_color: None,
        shadow_strength: None,
        shadow_distance: None,
        border_color: None,
        border_thickness: None,
        border_strength: None,
        border_distance: None,
        extra: Default::default(),
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
            course_lat: vec![],
            course_lon: vec![],
            time: vec![],
        },
    };

    let parts = format_metric_parts(&config, &value, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "36");
    assert_eq!(parts.unit_text.as_deref(), Some("KM/H"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Gauge));
    assert!(parts.show_icon);
}

#[test]
// Verifies temperature metric parts preserve degree-unit display text.
fn formats_metric_parts_for_temperature_with_degree_units() {
    let config = RenderConfig {
        scene: SceneConfig {
            width: None,
            height: None,
            fps: 30.0,
            start: 0.0,
            end: 1.0,
            font: None,
            font_size: None,
            color: None,
            decimal_rounding: None,
            overlay_filename: None,
            update_rate: None,
            composite_video_path: None,
            composite_bitrate: None,
            composite_sync_offset: None,
            composite_video_fps_num: None,
            composite_video_fps_den: None,
            composite_video_duration: None,
            composite_render_duration: None,
            composite_video_trim_start: None,
            composite_widget_update_rate: None,
            ffmpeg: json!({}),
            opacity: None,
            scale: None,
            time_format: None,
            shadow_color: None,
            shadow_strength: None,
            shadow_distance: None,
            border_color: None,
            border_thickness: None,
            border_strength: None,
            border_distance: None,
            custom_export_range_active: None,
            extra: Default::default(),
        },
        labels: vec![],
        values: vec![],
        plots: json!([]),
        extra: Default::default(),
    };
    let value = ValueConfig {
        value: MetricKind::Temperature,
        x: 0.0,
        y: 0.0,
        font: None,
        font_family: None,
        font_size: None,
        color: None,
        opacity: None,
        suffix: None,
        prefix: None,
        unit: None,
        hours_offset: None,
        time_format: None,
        format: None,
        decimal_rounding: None,
        decimals: Some(0),
        show_icon: None,
        icon_color: None,
        icon_size: None,
        icon_offset_x: None,
        icon_offset_y: None,
        show_units: Some(true),
        speed_unit: None,
        temperature_unit: Some("fahrenheit".to_string()),
        value_offset: None,
        triangle_positive_color: None,
        triangle_negative_color: None,
        show_sign: None,
        show_triangle: None,
        triangle_width: None,
        shadow_color: None,
        shadow_strength: None,
        shadow_distance: None,
        border_color: None,
        border_thickness: None,
        border_strength: None,
        border_distance: None,
        extra: Default::default(),
    };
    let dense = DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        series: DenseSeriesReport {
            speed: vec![],
            elevation: vec![],
            gradient: vec![],
            heartrate: vec![],
            cadence: vec![],
            power: vec![],
            temperature: vec![Some(20.0)],
            course_lat: vec![],
            course_lon: vec![],
            time: vec![],
        },
    };

    let parts = format_metric_parts(&config, &value, &dense, 0).unwrap();
    assert_eq!(parts.value_text, "68");
    assert_eq!(parts.unit_text.as_deref(), Some("\u{00B0}F"));
    assert_eq!(parts.icon_kind, Some(MetricIconKind::Thermometer));
    assert!(parts.show_icon);
}
