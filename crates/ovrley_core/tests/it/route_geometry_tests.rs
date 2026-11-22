//! Route geometry from a validated config and an activity, through core's ingress.

use ovrley_core::activity::parse_activity_json;
use ovrley_core::normalize::{parse_config_json, validate_route_plots};
use ovrley_widgets::widgets::route::geometry::RouteGeometry;

/// The builder returns valid geometry for a minimal activity with course points.
#[test]
fn test_returns_geometry_for_valid_input() {
    let config = serde_json::json!({
        "scene": {
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "start": 0.0,
            "end": 10.0,
            "scale": 1.0,
            "opacity": 1.0,
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
            "marker_variant": "single",
            "marker_variant_diameter": 10.0,
            "marker_size": 6.0,
            "marker_color": "#ffffff",
            "marker_opacity": 1.0
        }],
        "rasters": [{
            "id": "widget-26", "path": std::env::current_dir().unwrap().join("image.png"), "resourceId": "session-image",
            "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0, "rotation": 0.0, "opacity": 100.0
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

    let config = parse_config_json(&config.to_string()).unwrap();
    let plots = validate_route_plots(&config.plots).unwrap();
    let activity = parse_activity_json(&activity.to_string()).unwrap();
    // The plot shows the full activity, so no export window trims it.
    let samples = activity
        .route_activity()
        .samples(None);
    let geometry = RouteGeometry::new(&plots[0], &samples).unwrap();

    // Geometry should have at least 2 points (endpoint preservation)
    let points = geometry.line.points();
    assert!(
        points.len() >= 2,
        "geometry should have at least 2 points, got {}",
        points.len()
    );

    // All points should be within widget bounds
    for (x, y) in points.iter() {
        assert!(*x >= 0.0 && *x <= 400.0, "x out of range: {x}");
        assert!(*y >= 0.0 && *y <= 400.0, "y out of range: {y}");
    }

    // Progress values should be monotonically non-decreasing 0..1
    for window in geometry.line.progress().windows(2) {
        assert!(window[0] <= window[1], "progress not monotonic");
    }

    // Source point count should match input
    assert_eq!(geometry.source_point_count, 6);
}
