mod common;

use ovrley_core::activity::schema::{DenseActivityReport, ParsedActivity};
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::{RenderConfig, ValueConfig};
use ovrley_core::normalize::validate_render_config;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::widgets::gauges::arc::{
    arc_gauge_geometry, arc_point, arc_radius, arc_start_end_angles,
};
use ovrley_core::render::widgets::types::PreparedValue;
use ovrley_core::render::{render_preview_with_report, widgets::prepare_render_assets};
use ovrley_core::types::{DisplayType, MetricKind, TrackFillStyle};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn value_config_deserializes_arc_gauge_specific_fields() {
    let value: ValueConfig = serde_json::from_value(serde_json::json!({
        "value": "speed",
        "x": 24,
        "y": 48,
        "display_type": "arc",
        "arc_angle": 210,
        "inner_widget_offset_x": -12,
        "inner_widget_offset_y": 8,
        "track_thickness": 14,
        "track_fill_flat": true
    }))
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Arc);
    assert_eq!(value.arc_angle, Some(210.0));
    assert_eq!(value.inner_widget_offset_x, Some(-12.0));
    assert_eq!(value.inner_widget_offset_y, Some(8.0));
    assert_eq!(value.track_thickness, Some(14.0));
    assert_eq!(value.track_fill_flat, Some(true));
}

#[test]
fn arc_angles_are_vertically_symmetric_for_small_half_and_full_sweeps() {
    assert_eq!(arc_start_end_angles(30.0), (255.0, 285.0));
    assert_eq!(arc_start_end_angles(180.0), (180.0, 360.0));
    assert_eq!(arc_start_end_angles(360.0), (90.0, 450.0));

    let half = arc_gauge_geometry(200.0, 160.0, 180.0, 12.0, 2.0);
    assert_eq!(half.center_x, 100.0);
    assert_eq!(half.center_y, 80.0);
    assert_eq!(half.radius, 72.0);
    assert_eq!(half.start_angle, 180.0);
    assert_eq!(half.sweep_angle, 180.0);

    let start = arc_point(half.center_x, half.center_y, half.radius, half.start_angle);
    let end = arc_point(
        half.center_x,
        half.center_y,
        half.radius,
        half.start_angle + half.sweep_angle,
    );
    assert!((start.x - 28.0).abs() < 0.001);
    assert!((start.y - 80.0).abs() < 0.001);
    assert!((end.x - 172.0).abs() < 0.001);
    assert!((end.y - 80.0).abs() < 0.001);
}

#[test]
fn radius_accounts_for_track_and_border_thickness() {
    assert_eq!(arc_radius(200.0, 160.0, 12.0, 2.0), 72.0);
    assert_eq!(arc_radius(40.0, 80.0, 30.0, 8.0), 0.0);
}

#[test]
fn arc_angle_validation_enforces_the_persisted_range() {
    let mut too_small = full_arc_gauge_config(20, 30);
    too_small["arc_angle"] = serde_json::json!(29);
    let Err(error) = validate_single_arc(too_small) else {
        panic!("an angle below the supported range should fail validation");
    };
    let error = error.to_string();
    assert!(
        error.contains("arc_angle") && error.contains("30..=360"),
        "{error}"
    );

    let mut too_large = full_arc_gauge_config(20, 30);
    too_large["arc_angle"] = serde_json::json!(361);
    let Err(error) = validate_single_arc(too_large) else {
        panic!("an angle above the supported range should fail validation");
    };
    let error = error.to_string();
    assert!(
        error.contains("arc_angle") && error.contains("30..=360"),
        "{error}"
    );
}

#[test]
fn prepare_assets_builds_arc_cache_with_static_unit_and_frame_values() {
    let mut raw_config = full_arc_gauge_config(20, 30);
    raw_config["track_fill_flat"] = serde_json::json!(true);
    let config = validate_single_arc(raw_config).unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(10.0), Some(30.0), Some(50.0)]);
    let mut profiler = RenderProfiler::default();

    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();

    let PreparedValue::ArcGauge(widget) = &assets.values()[0] else {
        panic!("arc gauge should prepare a gauge cache at value index 0");
    };
    let cache = widget
        .cache
        .as_ref()
        .expect("arc gauge cache must be prepared");
    assert_eq!(cache.frame_states[1].fill01, 0.5);
    assert_eq!(cache.frame_states[1].value_text, "108");
    assert!(
        cache.has_unit,
        "the unit belongs to the cached static layer"
    );
    assert_eq!(cache.track_thickness, 12.0);
    assert!(cache.track_fill_flat);
}

#[test]
fn bars_style_resolves_configured_arc_geometry_into_the_cache() {
    let mut raw_config = full_arc_gauge_config(20, 30);
    raw_config["track_fill_style"] = serde_json::json!("bars");
    raw_config["bar_count"] = serde_json::json!(8);
    raw_config["bar_gap"] = serde_json::json!(4);
    let config = validate_single_arc(raw_config).unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(0.0), Some(100.0)]);
    let mut profiler = RenderProfiler::default();
    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();

    let PreparedValue::ArcGauge(widget) = &assets.values()[0] else {
        panic!("arc bars should use the arc gauge cache");
    };
    let cache = widget
        .cache
        .as_ref()
        .expect("arc bars cache must be prepared");
    assert_eq!(cache.track_fill_style, TrackFillStyle::Bars);
    assert_eq!(cache.bar_geometry.unwrap().count, 8);
}

#[test]
fn preview_render_reports_arc_gauge_without_text_or_icon_fallback() {
    let mut scene = common::builders::scene_json();
    scene["width"] = serde_json::json!(320);
    scene["height"] = serde_json::json!(240);
    let config = validate_render_config(RenderConfig {
        scene: serde_json::from_value(scene).unwrap(),
        backdrops: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(full_arc_gauge_config(20, 30)).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
    .unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(0.0), Some(50.0), Some(100.0)]);
    let out_path = std::env::temp_dir().join("arc_gauge_preview_report.png");

    let report =
        render_preview_with_report(&paths, &config, &activity, &dense, 0.0, &out_path).unwrap();

    assert_eq!(report.metric_presentations.len(), 1);
    assert_eq!(
        report.metric_presentations[0].metric_kind,
        MetricKind::Speed
    );
    assert_eq!(
        report.metric_presentations[0].display_type,
        DisplayType::Arc
    );
    assert_eq!(
        report.metric_presentations[0].widget.geometry.widget_width,
        160
    );
    assert_eq!(report.metric_presentations[0].widget.frame.progress01, 0.0);
    let _ = std::fs::remove_file(out_path);
}

fn validate_single_arc(
    value: serde_json::Value,
) -> Result<ovrley_core::normalize::ValidatedRenderConfig, ovrley_core::CoreError> {
    validate_render_config(RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        backdrops: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(value).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
}

fn full_arc_gauge_config(x: i32, y: i32) -> serde_json::Value {
    serde_json::json!({
        "value": "speed",
        "x": x,
        "y": y,
        "display_type": "arc",
        "width": 160,
        "height": 160,
        "rotation": 0,
        "arc_angle": 180,
        "inner_widget_offset_x": 0,
        "inner_widget_offset_y": 0,
        "track_thickness": 12,
        "track_corner_radius": 6,
        "track_border_thickness": 2,
        "track_border_color": "#ffffff",
        "track_empty_color": "#222222",
        "track_empty_opacity": 0.5,
        "track_filled_color": "#40e0d0",
        "track_filled_opacity": 1,
        "track_fill_flat": false,
        "show_min_max_labels": true,
        "min_max_label_font": "Arial.ttf",
        "min_max_label_font_size": 12,
        "min_max_label_color": "#ffffff",
        "font": "Arial.ttf",
        "font_size": 40,
        "color": "#ffffff",
        "opacity": 1,
        "show_units": true,
        "unit_color": "#ffffff",
        "display_unit": "kmh",
        "prefix": "",
        "suffix": "",
        "decimals": 0
    })
}

fn dense_speed_activity(speed: Vec<Option<f64>>) -> DenseActivityReport {
    let frame_count = speed.len();
    let mut series = common::builders::empty_dense_series();
    series.speed = speed;
    DenseActivityReport {
        frame_count,
        frame_elapsed_seconds: (0..frame_count).map(|index| index as f64).collect(),
        frame_distance_progress: vec![Some(0.0); frame_count],
        full_activity_distance: None,
        full_activity_total_ascent: None,
        series,
    }
}

fn test_paths() -> AppPaths {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    AppPaths {
        repo_root: workspace_root.clone(),
        font_dirs: vec![workspace_root.join("fonts")],
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    }
}
