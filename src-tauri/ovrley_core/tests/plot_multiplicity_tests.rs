use ovrley_core::normalize::{raw::RenderConfig, validate_render_config};

#[test]
fn validation_preserves_multiple_route_and_elevation_plots() {
    let route = serde_json::json!({
        "value": "course", "x": 10.0, "y": 20.0, "width": 400, "height": 400,
        "rotation": 0.0, "simplify_tolerance_px": 1.0, "target_density": 1.0,
        "show_full_activity": true,
        "remaining_line_width": 2.0, "remaining_line_color": "#ffffff", "remaining_line_opacity": 1.0,
        "completed_line_width": 2.0, "completed_line_color": "#00ff00", "completed_line_opacity": 1.0,
        "marker_variant": "circle", "marker_variant_diameter": 10.0,
        "marker_size": 6.0, "marker_color": "#ffffff", "marker_opacity": 1.0
    });
    let elevation = serde_json::json!({
        "value": "elevation", "x": 30.0, "y": 40.0, "width": 400, "height": 100,
        "rotation": 0.0, "y_scale": 1.0, "simplify_tolerance_px": 1.0,
        "target_density": 1.0, "show_full_activity": true,
        "remaining_line_width": 2.0, "remaining_line_color": "#ffffff", "remaining_line_opacity": 1.0,
        "completed_line_width": 2.0, "completed_line_color": "#00ff00", "completed_line_opacity": 1.0,
        "area_remaining_color": "#333333", "area_remaining_opacity": 0.3,
        "area_completed_color": "#00ff00", "area_completed_opacity": 0.3,
        "marker_variant": "circle", "marker_variant_diameter": 10.0,
        "marker_size": 6.0, "marker_color": "#ffffff", "marker_opacity": 1.0,
        "show_elevation_metric": true, "show_elevation_imperial": false,
        "metric_label_offset_x": 10.0, "metric_label_offset_y": -10.0,
        "imperial_label_offset_x": 10.0, "imperial_label_offset_y": -10.0,
        "label_font_size": 14.0, "label_color": "#ffffff",
        "point_label": { "font_size": 14.0, "color": "#ffffff" }
    });
    let mut second_route = route.clone();
    second_route["x"] = serde_json::json!(110.0);
    let mut second_elevation = elevation.clone();
    second_elevation["x"] = serde_json::json!(130.0);

    let raw: RenderConfig = serde_json::from_value(serde_json::json!({
        "scene": {
            "width": 1920, "height": 1080, "fps": 30.0, "start": 0.0, "end": 10.0,
            "scale": 1.0, "shadow_color": "#000000", "shadow_strength": 0.5,
            "shadow_distance": 2.0, "border_color": "#000000", "border_thickness": 0.0,
            "update_rate": 1, "custom_export_range_active": false, "ffmpeg": {}
        },
        "backdrops": [], "labels": [], "values": [],
        "plots": [route, elevation, second_route, second_elevation]
    }))
    .unwrap();

    let validated = validate_render_config(raw).unwrap();

    assert_eq!(validated.course_plots.len(), 2);
    assert_eq!(validated.elevation_plots.len(), 2);
    assert_eq!(validated.course_plots[1].x, 110.0);
    assert_eq!(validated.elevation_plots[1].x, 130.0);
}
