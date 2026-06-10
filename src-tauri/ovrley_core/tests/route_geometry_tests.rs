#![recursion_limit = "256"]

//! Route geometry IPC command tests.
//!
//! Exercises `build_route_geometry_command` — the Tauri IPC command
//! that returns pre-built route geometry for the frontend preview.
//!
//! ## Type
//! Integration test. Tests the command function directly (no Tauri shell).
//!
//! ## Regressions guarded
//! - Response struct serialization format (points as [[x,y], not tuple objects)
//! - Missing course_plot in config produces descriptive error
//! - Command returns valid geometry for a minimal activity + config

mod common;

use ovrley_core::commands::route_geometry::{
    build_route_geometry_command, RouteGeometryResponse,
};

/// Verifies the response struct serializes `points` as `[[x,y], ...]` JSON
/// arrays, not as tuple objects `{"0": x, "1": y}`. This is critical because
/// the JS frontend expects array notation for SVG path construction.
#[test]
fn test_response_serializes_points_as_arrays() {
    let response = RouteGeometryResponse {
        points: vec![[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
        progress_values: vec![0.0, 0.5, 1.0],
        bbox: [0.0, 0.0, 100.0, 50.0],
        source_point_count: 42,
        simplification: "lttb_density_1.00_rdp_px_1.00".to_string(),
        widget_width: 200,
        widget_height: 100,
    };

    let json = serde_json::to_value(&response).unwrap();

    // Points must be arrays, not objects
    let points = json.get("points").unwrap().as_array().unwrap();
    assert_eq!(points.len(), 3);
    assert_eq!(points[0], serde_json::json!([1.0, 2.0]));
    assert_eq!(points[1], serde_json::json!([3.0, 4.0]));
    assert_eq!(points[2], serde_json::json!([5.0, 6.0]));

    // progress_values must be flat array
    let progress = json.get("progressValues").unwrap().as_array().unwrap();
    assert_eq!(progress.len(), 3);
    assert_eq!(progress[0], 0.0);
    assert_eq!(progress[1], 0.5);
    assert_eq!(progress[2], 1.0);

    // bbox must be [min_x, min_y, max_x, max_y]
    let bbox = json.get("bbox").unwrap().as_array().unwrap();
    assert_eq!(bbox[0], 0.0);
    assert_eq!(bbox[1], 0.0);
    assert_eq!(bbox[2], 100.0);
    assert_eq!(bbox[3], 50.0);
}

/// Verifies camelCase serialization of field names matches the JS contract.
#[test]
fn test_response_uses_camel_case_field_names() {
    let response = RouteGeometryResponse {
        points: vec![],
        progress_values: vec![],
        bbox: [0.0; 4],
        source_point_count: 0,
        simplification: String::new(),
        widget_width: 0,
        widget_height: 0,
    };

    let json = serde_json::to_value(&response).unwrap();
    let keys: Vec<&str> = json.as_object().unwrap().keys().map(|k| k.as_str()).collect();

    assert!(keys.contains(&"points"), "expected camelCase 'points'");
    assert!(keys.contains(&"progressValues"), "expected camelCase 'progressValues'");
    assert!(keys.contains(&"bbox"), "expected camelCase 'bbox'");
    assert!(keys.contains(&"sourcePointCount"), "expected camelCase 'sourcePointCount'");
    assert!(keys.contains(&"simplification"), "expected camelCase 'simplification'");
    assert!(keys.contains(&"widgetWidth"), "expected camelCase 'widgetWidth'");
    assert!(keys.contains(&"widgetHeight"), "expected camelCase 'widgetHeight'");
    assert!(!keys.contains(&"widget_width"), "should not have snake_case widget_width");
}

/// Command must return a descriptive error when the config has no course_plot.
#[test]
fn test_command_errors_when_no_course_plot() {
    let config = serde_json::json!({
        "scene": {
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "start": 0.0,
            "end": 60.0,
            "scale": 1.0,
            "shadow_color": "#000000",
            "shadow_strength": 0.5,
            "shadow_distance": 2.0,
            "border_color": "#000000",
            "border_thickness": 0.0,
            "update_rate": 1,
            "custom_export_range_active": false,
            "ffmpeg": {}
        },
        "values": [],
        "labels": [],
        "plots": []
    });
    let activity = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 10.0],
        "sample_distance_progress": [0.0, 1.0],
        "sample_course_points": [[47.6062, -122.3321], [47.6097, -122.3400]],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 10.0
    });

    let error = build_route_geometry_command(
        &config.to_string(),
        &activity.to_string(),
    )
    .unwrap_err();

    let msg = error.to_string();
    assert!(
        msg.contains("course_plot"),
        "error should mention course_plot, got: {msg}"
    );
}

/// Command returns valid geometry for a minimal activity with course points.
#[test]
fn test_command_returns_geometry_for_valid_input() {
    let config = serde_json::json!({
        "scene": {
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "start": 0.0,
            "end": 10.0,
            "scale": 1.0,
            "shadow_color": "#000000",
            "shadow_strength": 0.5,
            "shadow_distance": 2.0,
            "border_color": "#000000",
            "border_thickness": 0.0,
            "update_rate": 1,
            "custom_export_range_active": false,
            "ffmpeg": {}
        },
        "values": [],
        "labels": [],
        "plots": [{
            "value": "course",
            "x": 0.0,
            "y": 0.0,
            "width": 400,
            "height": 400,
            "rotation": 0.0,
            "simplify_tolerance_px": 1.0,
            "target_density": 1.0,
            "show_full_activity": true,
            "remaining_line_width": 2.0,
            "remaining_line_color": "#ffffff",
            "remaining_line_opacity": 1.0,
            "completed_line_width": 2.0,
            "completed_line_color": "#00ff00",
            "completed_line_opacity": 1.0,
            "marker_variant": "circle",
            "marker_variant_diameter": 10.0,
            "marker_size": 6.0,
            "marker_color": "#ffffff",
            "marker_opacity": 1.0
        }]
    });
    let activity = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 2.0, 4.0, 6.0, 8.0, 10.0],
        "sample_distance_progress": [0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
        "sample_course_points": [
            [47.6062, -122.3321],
            [47.6065, -122.3325],
            [47.6068, -122.3330],
            [47.6070, -122.3335],
            [47.6072, -122.3340],
            [47.6075, -122.3345]
        ],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 10.0
    });

    let response = build_route_geometry_command(
        &config.to_string(),
        &activity.to_string(),
    )
    .unwrap();

    // Geometry should have at least 2 points (endpoint preservation)
    assert!(
        response.points.len() >= 2,
        "geometry should have at least 2 points, got {}",
        response.points.len()
    );

    // All points should be within widget bounds
    for [x, y] in &response.points {
        assert!(*x >= 0.0 && *x <= 400.0, "x out of range: {x}");
        assert!(*y >= 0.0 && *y <= 400.0, "y out of range: {y}");
    }

    // Progress values should be monotonically non-decreasing 0..1
    for window in response.progress_values.windows(2) {
        assert!(window[0] <= window[1], "progress not monotonic");
    }

    // Widget dimensions should match config
    assert_eq!(response.widget_width, 400);
    assert_eq!(response.widget_height, 400);

    // Source point count should match input
    assert_eq!(response.source_point_count, 6);
}
