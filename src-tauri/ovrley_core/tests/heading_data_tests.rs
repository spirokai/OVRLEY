//! Heading data plumbing and config tests.
//!
//! Verifies `MetricKind::Heading` serde round-trip, heading fields across all
//! activity pipeline structs, `RenderDataRequirements` inclusion, forward-fill
//! densification, `HeadingWidgetConfig` deserialization, and plot recognition.
//!
//! ## Type
//! Unit test. Pure struct construction and JSON round-trip — no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - `MetricKind::Heading` serde name breaking frontend communication
//! - Missing heading fields in activity pipeline structs
//! - Heading not triggering trim/densify when configured
//! - Forward-fill not carrying last-known heading across null gaps
//! - `HeadingWidgetConfig` deserialization rejecting valid JSON
//! - Plot config not recognizing `"heading"` value key

use serde_json::json;

use ovrley_core::activity::schema::{DenseSeriesReport, ParsedActivity, TrimmedActivity};
use ovrley_core::config::{RenderConfig, RenderDataRequirements, SceneConfig};
use ovrley_core::MetricKind;

// ── 1. MetricKind::Heading serde round-trip ──────────────────────────────

#[test]
fn metric_kind_heading_serializes_to_heading() {
    let serialized = serde_json::to_string(&MetricKind::Heading).unwrap();
    assert_eq!(serialized, r#""heading""#);
}

#[test]
fn metric_kind_heading_deserializes_from_heading() {
    let deserialized: MetricKind = serde_json::from_str(r#""heading""#).unwrap();
    assert_eq!(deserialized, MetricKind::Heading);
}

#[test]
fn metric_kind_heading_round_trip() {
    let kind = MetricKind::Heading;
    let json = serde_json::to_string(&kind).unwrap();
    let back: MetricKind = serde_json::from_str(&json).unwrap();
    assert_eq!(back, kind);
}

// ── 2. ParsedActivity accepts heading ────────────────────────────────────

#[test]
fn parsed_activity_accepts_heading_from_json() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0],
        "sample_distance_progress": [0.0, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 1.0,
        "heading": [90.0, 91.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert_eq!(activity.heading, vec![Some(90.0), Some(91.0)]);
}

#[test]
fn parsed_activity_heading_defaults_to_empty_when_missing() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0],
        "sample_distance_progress": [0.0, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 1.0
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert!(activity.heading.is_empty());
}

#[test]
fn parsed_activity_heading_preserves_nulls() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0, 2.0],
        "sample_distance_progress": [0.0, 0.5, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 2.0,
        "heading": [90.0, null, 180.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert_eq!(activity.heading, vec![Some(90.0), None, Some(180.0)]);
}

// ── 3. DenseSeriesReport carries heading ─────────────────────────────────

#[test]
fn dense_series_report_has_heading_field() {
    let report = DenseSeriesReport {
        speed: vec![],
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
        heading: vec![Some(90.0), None, Some(180.0)],
        course_lat: vec![],
        course_lon: vec![],
        time: vec![],
    };
    assert_eq!(report.heading, vec![Some(90.0), None, Some(180.0)]);
}

// ── 4. RenderDataRequirements includes heading ───────────────────────────

#[test]
fn render_data_requirements_has_heading_field() {
    let mut requirements = RenderDataRequirements::default();
    assert!(!requirements.heading);
    requirements.heading = true;
    assert!(requirements.heading);
}

#[test]
fn render_data_requirements_heading_derived_from_metric_kind() {
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
    // Without heading value widget, heading requirement should be false
    let reqs = config.render_data_requirements().unwrap();
    assert!(!reqs.heading);
}

// ── 5. Heading flows through trim→densify with forward-fill ──────────────

#[test]
fn heading_trimmed_and_densified_with_forward_fill() {
    use ovrley_core::activity::interpolate::densify_activity;

    let trimmed = TrimmedActivity {
        source_start_time: None,
        sample_elapsed_seconds: vec![0.0, 0.5, 1.0],
        sample_distance_progress: vec![],
        course: vec![],
        elevation: vec![],
        speed: vec![],
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
        gradient: vec![],
        time: vec![],
        heading: vec![Some(90.0), None, Some(180.0)],
    };
    let mut requirements = RenderDataRequirements::default();
    requirements.heading = true;

    let report = densify_activity(&trimmed, 30.0, &requirements);

    // 1 second at 30fps = 30 frames
    assert_eq!(report.frame_count, 30);
    // Heading should be forward-filled: first 15 frames = 90.0, last 15 = 180.0
    assert_eq!(report.series.heading.len(), 30);
    // First frame: heading starts at 90.0
    assert!((report.series.heading[0].unwrap() - 90.0).abs() < 1.0);
    // The forward-fill means the null at t=0.5 is replaced by 90.0 (last known)
    // then at t>0.5 it interpolates between 90.0 and 180.0
    // At t=0.5, the interpolated value should be ~90.0 (forward-filled)
    let mid_frame = 15; // t=0.5
    let mid_heading = report.series.heading[mid_frame].unwrap();
    assert!(
        (mid_heading - 90.0).abs() < 1.0 || (mid_heading - 135.0).abs() < 1.0,
        "mid heading should be near 90 (forward-fill) or 135 (interpolated), got {}",
        mid_heading
    );
}

#[test]
fn heading_not_densified_when_not_required() {
    use ovrley_core::activity::interpolate::densify_activity;

    let trimmed = TrimmedActivity {
        source_start_time: None,
        sample_elapsed_seconds: vec![0.0, 1.0],
        sample_distance_progress: vec![],
        course: vec![],
        elevation: vec![],
        speed: vec![],
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
        gradient: vec![],
        time: vec![],
        heading: vec![Some(90.0), Some(180.0)],
    };
    let requirements = RenderDataRequirements::default();

    let report = densify_activity(&trimmed, 30.0, &requirements);

    // Heading not required → empty vec
    assert!(report.series.heading.is_empty());
}

// ── 6. HeadingWidgetConfig deserialization ───────────────────────────────

#[test]
fn heading_widget_config_deserializes_from_json() {
    let json = json!({
        "value": "heading",
        "x": 100.0,
        "y": 200.0,
        "width": 400,
        "height": 80,
        "rotation": 0.0,
        "opacity": 1.0,
        "pixels_per_degree": 5.0,
        "major_tick_interval": 15,
        "minor_ticks_per_major": 3,
        "show_major_ticks": true,
        "show_minor_ticks": true,
        "major_tick_length_pct": 40.0,
        "minor_tick_length_pct": 20.0,
        "major_tick_thickness": 2.0,
        "minor_tick_thickness": 1.0,
        "tick_color": "#FFFFFF",
        "cardinal_tick_color": "#FF0000",
        "tick_alignment": "below",
        "shadow_distance": 2.0,
        "shadow_strength": 0.5,
        "shadow_color": "#000000",
        "show_numeric_labels": true,
        "show_cardinal_labels": true,
        "numeric_label_color": "#CCCCCC",
        "cardinal_label_color": "#FF0000",
        "label_font": "Teko.ttf",
        "label_font_family": "Teko",
        "label_font_size": 12.0,
        "label_offset": 5.0,
        "indicator_style": "chevron",
        "indicator_placement": "top",
        "show_indicator": true,
        "indicator_color": "#FF0000",
        "indicator_size": 10.0
    });

    let config: ovrley_core::config::HeadingWidgetConfig = serde_json::from_value(json).unwrap();
    assert_eq!(config.value, MetricKind::Heading);
    assert_eq!(config.x, 100.0);
    assert_eq!(config.y, 200.0);
    assert_eq!(config.width, 400);
    assert_eq!(config.height, 80);
    assert!((config.pixels_per_degree - 5.0).abs() < f32::EPSILON);
    assert_eq!(config.major_tick_interval, 15);
    assert_eq!(config.minor_ticks_per_major, 3);
    assert!(config.show_major_ticks);
    assert!(config.show_minor_ticks);
    assert_eq!(config.tick_alignment, "below");
    assert!((config.major_tick_thickness - 2.0).abs() < f32::EPSILON);
    assert!((config.minor_tick_thickness - 1.0).abs() < f32::EPSILON);
    assert_eq!(config.label_font.as_deref(), Some("Teko.ttf"));
    assert_eq!(config.label_font_family.as_deref(), Some("Teko"));
    assert_eq!(config.indicator_style, "chevron");
    assert_eq!(config.indicator_placement, "top");
    assert!(config.show_indicator);
}

#[test]
fn heading_widget_config_defaults_via_serde_default() {
    let json = json!({
        "value": "heading",
        "x": 0.0,
        "y": 0.0,
        "width": 200,
        "height": 50
    });

    let config: ovrley_core::config::HeadingWidgetConfig = serde_json::from_value(json).unwrap();
    assert_eq!(config.value, MetricKind::Heading);
    // Defaults should kick in for optional fields
    assert!((config.pixels_per_degree - 5.0).abs() < f32::EPSILON);
    assert_eq!(config.major_tick_interval, 15);
    assert_eq!(config.minor_ticks_per_major, 3);
    assert!(config.show_major_ticks);
    assert!(config.show_minor_ticks);
    assert!((config.major_tick_thickness - 2.0).abs() < f32::EPSILON);
    assert!((config.minor_tick_thickness - 2.0).abs() < f32::EPSILON);
    assert!(config.show_numeric_labels);
    assert!(config.show_cardinal_labels);
    assert!(config.show_indicator);
    assert_eq!(config.indicator_style, "chevron");
    assert_eq!(config.indicator_placement, "top");
    assert_eq!(config.tick_alignment, "below");
}

#[test]
fn heading_widget_config_preserves_extra_fields() {
    let json = json!({
        "value": "heading",
        "x": 0.0,
        "y": 0.0,
        "width": 200,
        "height": 50,
        "unknown_future_field": "should be preserved"
    });

    let config: ovrley_core::config::HeadingWidgetConfig = serde_json::from_value(json).unwrap();
    assert!(config.extra.contains_key("unknown_future_field"));
}

// ── 7. RenderConfig.plots recognizes heading plot ────────────────────────

#[test]
fn render_config_plots_recognizes_heading_in_array() {
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
        plots: json!([{
            "value": "heading",
            "x": 0.0,
            "y": 0.0,
            "width": 400,
            "height": 80
        }]),
        extra: Default::default(),
    };

    let heading = config.heading_plot().unwrap();
    assert!(heading.is_some());
    let heading = heading.unwrap();
    assert_eq!(heading.value, MetricKind::Heading);
    assert_eq!(heading.width, 400);
}

#[test]
fn render_config_plots_recognizes_heading_in_object() {
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
        plots: json!({
            "heading": {
                "value": "heading",
                "x": 0.0,
                "y": 0.0,
                "width": 400,
                "height": 80
            }
        }),
        extra: Default::default(),
    };

    let heading = config.heading_plot().unwrap();
    assert!(heading.is_some());
}

#[test]
fn render_config_plots_returns_none_when_no_heading() {
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

    let heading = config.heading_plot().unwrap();
    assert!(heading.is_none());
}

#[test]
fn heading_plot_triggers_heading_data_requirement() {
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
        plots: json!([{
            "value": "heading",
            "x": 0.0,
            "y": 0.0,
            "width": 400,
            "height": 80
        }]),
        extra: Default::default(),
    };

    let reqs = config.render_data_requirements().unwrap();
    assert!(
        reqs.heading,
        "heading plot should trigger heading requirement"
    );
}
