//! Elevation geometry from a validated config and an activity, through core's ingress.

use ovrley_core::activity::parse_activity_json;
use ovrley_core::normalize::{parse_config_json, validate_elevation_plots, validate_scene_config};
use ovrley_widgets::widgets::elevation::geometry::ElevationGeometry;

/// The builder returns valid geometry for a minimal activity with elevation data.
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
            "value": "elevation",
            "x": 0.0,
            "y": 0.0,
            "width": 400,
            "height": 100,
            "rotation": 0.0,
            "y_scale": 1.0,
            "simplify_tolerance_px": 1.0,
            "target_density": 1.0,
            "show_full_activity": true,
            "remaining_line_width": 2.0,
            "remaining_line_color": "#ffffff",
            "remaining_line_opacity": 1.0,
            "completed_line_width": 2.0,
            "completed_line_color": "#00ff00",
            "completed_line_opacity": 1.0,
            "area_remaining_color": "#333333",
            "area_remaining_opacity": 0.3,
            "area_completed_color": "#00ff00",
            "area_completed_opacity": 0.3,
            "area_gradient": false,
            "marker_variant": "single",
            "marker_variant_diameter": 10.0,
            "marker_size": 6.0,
            "marker_color": "#ffffff",
            "marker_opacity": 1.0,
            "show_elevation_metric": true,
            "show_elevation_imperial": false,
            "metric_label_offset_x": 10.0,
            "metric_label_offset_y": -10.0,
            "imperial_label_offset_x": 10.0,
            "imperial_label_offset_y": -10.0,
            "label_font_size": 14.0,
            "label_color": "#ffffff",
            "point_label": {
                "font_size": 14.0,
                "color": "#ffffff"
            }
        }],
        "rasters": [{
            "id": "widget-26", "path": std::env::current_dir().unwrap().join("image.png"), "resourceId": "session-image",
            "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0, "rotation": 0.0, "opacity": 100.0
        }]
    });
    let activity = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 2.0, 4.0, 6.0, 8.0, 10.0],
        "sample_distance_progress": [0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
        "elevation": [100.0, 110.0, 105.0, 120.0, 115.0, 130.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 10.0
    });

    let config = parse_config_json(&config.to_string()).unwrap();
    let scene = validate_scene_config(config.scene).unwrap();
    let plots = validate_elevation_plots(&config.plots, &scene.presentation).unwrap();
    let activity = parse_activity_json(&activity.to_string()).unwrap();
    // The plot shows the full activity, so no export window trims it.
    let samples = activity
        .elevation_activity()
        .samples(None);
    let geometry = ElevationGeometry::new(&plots[0], scene.presentation.scale, &samples).unwrap();

    // Geometry should have at least 2 points (endpoint preservation)
    assert!(
        geometry.points.len() >= 2,
        "geometry should have at least 2 points, got {}",
        geometry.points.len()
    );

    // All points should be within widget bounds
    for (x, y) in geometry.points.iter() {
        assert!(*x >= 0.0 && *x <= 400.0, "x out of range: {x}");
        assert!(*y >= 0.0 && *y <= 100.0, "y out of range: {y}");
    }

    // Progress values should be monotonically non-decreasing 0..1
    for window in geometry.progress_values.windows(2) {
        assert!(window[0] <= window[1], "progress not monotonic");
    }

    assert_eq!(geometry.elapsed_fractions.len(), geometry.points.len());
    assert_eq!(geometry.elevation_data_range, Some((100.0, 130.0)));

    // Widget dimensions should match config
    assert_eq!((geometry.key.width, geometry.key.height), (400.0, 100.0));

    // Source point count should match input
    assert_eq!(geometry.source_point_count, 6);
}
