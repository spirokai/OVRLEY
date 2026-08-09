mod common;

use ovrley_core::activity::schema::{DenseActivityReport, ParsedActivity};
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::{RenderConfig, ValueConfig};
use ovrley_core::normalize::{validate_render_config, ValidatedCornerGaugeOrientation};
use ovrley_core::paths::AppPaths;
use ovrley_core::render::widgets::gauges::arc::{
    arc_point, corner_gauge_geometry, corner_start_end_angles,
};
use ovrley_core::render::widgets::types::PreparedValue;
use ovrley_core::render::{render_preview_with_report, widgets::prepare_render_assets};
use ovrley_core::types::{DisplayType, TrackFillStyle};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn value_config_deserializes_corner_orientation() {
    let value: ValueConfig = serde_json::from_value(serde_json::json!({
        "value": "speed",
        "x": 24,
        "y": 48,
        "display_type": "corner",
        "corner_orientation": "bottom-right"
    }))
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Corner);
    assert_eq!(value.corner_orientation.as_deref(), Some("bottom-right"));
}

#[test]
fn corner_geometry_places_the_track_opposite_the_gauge_corner() {
    assert_eq!(
        corner_start_end_angles(ValidatedCornerGaugeOrientation::BottomLeft),
        (0.0, -90.0)
    );
    assert_eq!(
        corner_start_end_angles(ValidatedCornerGaugeOrientation::BottomRight),
        (180.0, 270.0)
    );

    let bottom_left = corner_gauge_geometry(
        160.0,
        160.0,
        ValidatedCornerGaugeOrientation::BottomLeft,
        12.0,
        6.0,
        2.0,
    );
    let bottom_right = corner_gauge_geometry(
        160.0,
        160.0,
        ValidatedCornerGaugeOrientation::BottomRight,
        12.0,
        6.0,
        2.0,
    );

    assert_eq!(bottom_left.sweep_angle, -90.0);
    assert_eq!(bottom_right.sweep_angle, 90.0);

    let right_start = arc_point(
        bottom_left.center_x,
        bottom_left.center_y,
        bottom_left.radius,
        bottom_left.start_angle,
    );
    let top_end = arc_point(
        bottom_left.center_x,
        bottom_left.center_y,
        bottom_left.radius,
        bottom_left.start_angle + bottom_left.sweep_angle,
    );
    let left_start = arc_point(
        bottom_right.center_x,
        bottom_right.center_y,
        bottom_right.radius,
        bottom_right.start_angle,
    );
    let top_left_end = arc_point(
        bottom_right.center_x,
        bottom_right.center_y,
        bottom_right.radius,
        bottom_right.start_angle + bottom_right.sweep_angle,
    );

    assert!((right_start.x - 152.0).abs() < 0.001);
    assert!((top_end.y - 8.0).abs() < 0.001);
    assert!((left_start.x - 8.0).abs() < 0.001);
    assert!((top_left_end.y - 8.0).abs() < 0.001);
}

#[test]
fn corner_orientation_is_required_and_restricted_to_bottom_corners() {
    let mut missing = full_corner_gauge_config("bottom-left");
    missing
        .as_object_mut()
        .unwrap()
        .remove("corner_orientation");
    let Err(error) = validate_single_corner(missing) else {
        panic!("a missing corner orientation should fail validation");
    };
    let error = error.to_string();
    assert!(error.contains("corner_orientation") && error.contains("required"));

    let Err(error) = validate_single_corner(full_corner_gauge_config("top-left")) else {
        panic!("an unsupported corner orientation should fail validation");
    };
    let error = error.to_string();
    assert!(error.contains("corner_orientation") && error.contains("bottom-left"));
}

#[test]
fn corner_gauge_prepares_a_shared_arc_cache_and_renders_a_frame() {
    let config = validate_single_corner(full_corner_gauge_config("bottom-right")).unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(0.0), Some(50.0), Some(100.0)]);
    let mut profiler = RenderProfiler::default();

    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();
    let PreparedValue::ArcGauge(widget) = &assets.values()[0] else {
        panic!("corner gauge should use the shared arc cache");
    };
    let cache = widget
        .cache
        .as_ref()
        .expect("corner gauge cache must be prepared");
    assert_eq!(cache.start_angle, 180.0);
    assert_eq!(cache.sweep_angle, 90.0);
    assert_eq!(cache.frame_states[1].fill01, 0.5);

    let out_path = std::env::temp_dir().join("corner_gauge_preview_report.png");
    let report =
        render_preview_with_report(&paths, &config, &activity, &dense, 1.0, &out_path).unwrap();
    assert_eq!(
        report.metric_presentations[0].display_type,
        DisplayType::Corner
    );
    assert_eq!(report.metric_presentations[0].widget.frame.progress01, 1.0);
    let _ = std::fs::remove_file(out_path);
}

#[test]
fn corner_bars_keep_the_configured_segment_count() {
    let mut value = full_corner_gauge_config("bottom-left");
    value["track_fill_style"] = serde_json::json!("bars");
    value["bar_count"] = serde_json::json!(6);
    value["bar_gap"] = serde_json::json!(3);
    let config = validate_single_corner(value).unwrap();

    let gauge = match &config.values[0] {
        ovrley_core::render::widgets::types::PreparedValue::ArcGauge(gauge) => gauge,
        _ => panic!("corner should validate as an arc gauge"),
    };
    assert_eq!(gauge.validated.track_fill_style, TrackFillStyle::Bars);
    assert_eq!(gauge.validated.bar_geometry.unwrap().count, 6);
}

fn validate_single_corner(
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

fn full_corner_gauge_config(corner_orientation: &str) -> serde_json::Value {
    serde_json::json!({
        "value": "speed",
        "x": 20,
        "y": 30,
        "display_type": "corner",
        "width": 160,
        "height": 160,
        "rotation": 0,
        "corner_orientation": corner_orientation,
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
