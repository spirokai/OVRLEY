mod common;

use ovrley_core::activity::schema::{DenseActivityReport, ParsedActivity};
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::RenderConfig;
use ovrley_core::normalize::raw::ValueConfig;
use ovrley_core::normalize::validate_render_config;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::widgets::linear_gauge::{
    bar_fill_rect, bordered_bar_fill_rect, fill_percentage, format_linear_gauge_label,
    LinearGaugeOrientation,
};
use ovrley_core::render::widgets::types::PresentationCache;
use ovrley_core::render::{render_preview_with_report, widgets::prepare_render_assets};
use ovrley_core::types::{DisplayType, MetricKind};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn value_config_deserializes_linear_gauge_fields() {
    let value: ValueConfig = serde_json::from_value(serde_json::json!({
        "value": "speed",
        "x": 24,
        "y": 48,
        "display_type": "linear",
        "width": 240,
        "height": 44,
        "orientation": "vertical",
        "track_corner_radius": 8,
        "track_border_thickness": 2,
        "track_border_color": "#112233",
        "track_empty_color": "#445566",
        "track_empty_opacity": 0.4,
        "track_filled_color": "#778899",
        "track_filled_opacity": 0.8,
        "track_fill_flat": true,
        "show_min_max_labels": true,
        "min_max_label_font": "Teko.ttf",
        "min_max_label_font_size": 12,
        "min_max_label_color": "#aabbcc"
    }))
    .unwrap();

    assert_eq!(value.display_type.as_str(), "linear");
    assert_eq!(value.width, Some(240));
    assert_eq!(value.height, Some(44));
    assert_eq!(value.orientation.as_deref(), Some("vertical"));
    assert_eq!(value.track_corner_radius, Some(8.0));
    assert_eq!(value.track_border_thickness, Some(2.0));
    assert_eq!(value.track_border_color.as_deref(), Some("#112233"));
    assert_eq!(value.track_empty_color.as_deref(), Some("#445566"));
    assert_eq!(value.track_empty_opacity, Some(0.4));
    assert_eq!(value.track_filled_color.as_deref(), Some("#778899"));
    assert_eq!(value.track_filled_opacity, Some(0.8));
    assert_eq!(value.track_fill_flat, Some(true));
    assert_eq!(value.show_min_max_labels, Some(true));
    assert_eq!(value.min_max_label_font.as_deref(), Some("Teko.ttf"));
    assert_eq!(value.min_max_label_font_size, Some(12.0));
    assert_eq!(value.min_max_label_color.as_deref(), Some("#aabbcc"));
}

#[test]
fn fill_percentage_clamps_and_handles_degenerate_ranges() {
    assert_eq!(fill_percentage(50.0, 0.0, 100.0), 0.5);
    assert_eq!(fill_percentage(-20.0, 0.0, 100.0), 0.0);
    assert_eq!(fill_percentage(120.0, 0.0, 100.0), 1.0);
    assert_eq!(fill_percentage(42.0, 10.0, 10.0), 0.0);
}

#[test]
fn linear_fill_rect_respects_horizontal_and_vertical_orientation() {
    let horizontal = bar_fill_rect(
        10.0,
        20.0,
        200.0,
        40.0,
        0.25,
        LinearGaugeOrientation::Horizontal,
    );
    assert_eq!(horizontal, (10.0, 20.0, 50.0, 40.0));

    let vertical = bar_fill_rect(
        10.0,
        20.0,
        200.0,
        40.0,
        0.25,
        LinearGaugeOrientation::Vertical,
    );
    assert_eq!(vertical, (10.0, 50.0, 200.0, 10.0));
}

#[test]
fn linear_fill_rect_stays_inside_track_border() {
    let horizontal = bordered_bar_fill_rect(
        10.0,
        20.0,
        200.0,
        40.0,
        0.25,
        LinearGaugeOrientation::Horizontal,
        2.0,
    );
    assert_eq!(horizontal, (12.0, 22.0, 49.0, 36.0));

    let vertical = bordered_bar_fill_rect(
        10.0,
        20.0,
        200.0,
        40.0,
        0.25,
        LinearGaugeOrientation::Vertical,
        2.0,
    );
    assert_eq!(vertical, (12.0, 49.0, 196.0, 9.0));
}

#[test]
fn gauge_labels_format_integer_and_decimal_ranges() {
    assert_eq!(format_linear_gauge_label(10.0), "10");
    assert_eq!(format_linear_gauge_label(10.24), "10.2");
}

#[test]
fn prepare_assets_builds_linear_gauge_cache_with_activity_range() {
    let config = validate_render_config(RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        labels: vec![],
        values: vec![serde_json::from_value(full_linear_gauge_config(20, 30)).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
    .unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(10.0), Some(30.0), Some(50.0)]);
    let mut profiler = RenderProfiler::default();

    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();

    let Some(PresentationCache::LinearGauge(cache)) = assets.presentation_caches.get(&0) else {
        panic!("linear gauge should prepare a gauge cache at value index 0");
    };
    assert_eq!(cache.display_type, DisplayType::Linear);
    assert_eq!(cache.min_value, 10.0);
    assert_eq!(cache.max_value, 50.0);
    assert_eq!(cache.frame_states[1].fill01, 0.5);
}

#[test]
fn preview_render_reports_linear_gauge_without_text_fallback() {
    let mut scene = common::builders::scene_json();
    scene["width"] = serde_json::json!(320);
    scene["height"] = serde_json::json!(120);
    let config = validate_render_config(RenderConfig {
        scene: serde_json::from_value(scene).unwrap(),
        labels: vec![],
        values: vec![serde_json::from_value(full_linear_gauge_config(20, 30)).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
    .unwrap();
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = dense_speed_activity(vec![Some(0.0), Some(50.0), Some(100.0)]);
    let out_path = std::env::temp_dir().join("linear_gauge_preview_report.png");

    let report =
        render_preview_with_report(&paths, &config, &activity, &dense, 0.0, &out_path).unwrap();

    assert_eq!(report.metric_presentations.len(), 1);
    assert_eq!(
        report.metric_presentations[0].metric_kind,
        MetricKind::Speed
    );
    assert_eq!(
        report.metric_presentations[0].display_type,
        DisplayType::Linear
    );
    assert_eq!(
        report.metric_presentations[0].widget.geometry.widget_width,
        200
    );
    assert_eq!(report.metric_presentations[0].widget.frame.progress01, 0.0);
    let _ = std::fs::remove_file(out_path);
}

fn full_linear_gauge_config(x: i32, y: i32) -> serde_json::Value {
    serde_json::json!({
        "value": "speed",
        "x": x,
        "y": y,
        "display_type": "linear",
        "width": 200,
        "height": 40,
        "rotation": 0,
        "orientation": "horizontal",
        "track_corner_radius": 6,
        "track_border_thickness": 2,
        "track_border_color": "#ffffff",
        "track_empty_color": "#222222",
        "track_empty_opacity": 0.5,
        "track_filled_color": "#40e0d0",
        "track_filled_opacity": 1,
        "track_fill_flat": false,
        "show_min_max_labels": false,
        "min_max_label_font": "Arial.ttf",
        "min_max_label_font_size": 12,
        "min_max_label_position": "bottom",
        "min_max_label_color": "#ffffff"
    })
}

fn dense_speed_activity(speed: Vec<Option<f64>>) -> DenseActivityReport {
    let frame_count = speed.len();
    let mut series = common::builders::empty_dense_series();
    series.speed = speed;
    DenseActivityReport {
        frame_count,
        frame_elapsed_seconds: (0..frame_count).map(|i| i as f64).collect(),
        frame_distance_progress: vec![Some(0.0); frame_count],
        full_activity_distance: None,
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
