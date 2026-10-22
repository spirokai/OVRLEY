mod common;

use ovrley_core::normalize::RenderConfig;
use ovrley_core::normalize::validate_render_config;
use ovrley_widgets::widgets::lean_angle::layout::prepare_lean_angle_cache;
use std::collections::BTreeMap;

#[test]
fn full_lean_angle_config_validates_and_prepares_one_static_cache() {
    let config = validate_render_config(RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        backdrops: vec![],
        rasters: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(full_lean_angle_config()).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
    .unwrap();

    let ovrley_widgets::widgets::value::validation::ValidatedValue::LeanAngle(widget) = &config.values[0] else {
        panic!("lean_angle display type must use the dedicated prepared value");
    };
    assert_eq!(widget.diameter, 180.0);
    assert_eq!(widget.track_thickness, 24.0);

    let cache = prepare_lean_angle_cache(widget, &config.scene.presentation).unwrap();
    assert!((cache.layout.center_x - 77.94229).abs() < 0.0001);
    assert_eq!(cache.layout.center_y, 90.0);
    assert_eq!(
        (cache.layout.outer_radius, cache.layout.inner_radius),
        (90.0, 66.0)
    );
    assert_eq!(cache.track_thickness, 24.0);
    assert_eq!(cache.track_border_thickness, 2.0);
}

#[test]
fn lean_angle_rejects_track_thickness_that_consumes_the_inner_radius() {
    let mut value = full_lean_angle_config();
    value["track_thickness"] = serde_json::json!(90);

    let error = match validate_render_config(render_config_with_lean_angle(value)) {
        Ok(_) => panic!("track thickness consuming the inner radius must fail validation"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("track_thickness: must be less than diameter / 2"));
}

#[test]
fn lean_angle_rejects_border_that_consumes_the_track() {
    let mut value = full_lean_angle_config();
    value["track_border_thickness"] = serde_json::json!(12);

    let error = match validate_render_config(render_config_with_lean_angle(value)) {
        Ok(_) => panic!("border consuming the track must fail validation"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("track_border_thickness: must leave a positive usable width"));
}

#[test]
fn lean_angle_rejects_missing_diameter() {
    let mut value = full_lean_angle_config();
    value.as_object_mut().unwrap().remove("diameter");

    let error = match validate_render_config(render_config_with_lean_angle(value)) {
        Ok(_) => panic!("missing diameter must fail validation"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("diameter: required"));
}

#[test]
fn lean_angle_rejects_rectangular_geometry_fields() {
    let mut value = full_lean_angle_config();
    value["width"] = serde_json::json!(180);

    let error = match validate_render_config(render_config_with_lean_angle(value)) {
        Ok(_) => panic!("lean-angle width must fail validation"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("width"));
}

#[test]
fn gradient_fill_mirrors_from_upright_to_either_lean_direction() {
    let mut value = full_lean_angle_config();
    value["track_filled_gradient"] = serde_json::json!(true);
    value["track_filled_gradient_stops"] = serde_json::json!([
        { "offset": 0, "color": "#ff0000" },
        { "offset": 1, "color": "#0000ff" },
    ]);
    let config = validate_render_config(render_config_with_lean_angle(value)).unwrap();
    let ovrley_widgets::widgets::value::validation::ValidatedValue::LeanAngle(widget) = &config.values[0] else {
        panic!("lean_angle display type must use the dedicated prepared value");
    };
    let cache = prepare_lean_angle_cache(widget, &config.scene.presentation).unwrap();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let fonts = ovrley_core::fonts::NativeFonts(&font_dirs);
    let center_x = widget.x + cache.layout.center_x;
    let center_y = widget.y + cache.layout.center_y;
    let radius = (cache.layout.outer_radius + cache.layout.inner_radius) * 0.5;
    let render = |lean: f64, angle: f32| {
        let frame = cache.prepare_frame(Some(lean), &fonts).unwrap();
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        frame.draw(surface.canvas(), &cache);
        let radians = angle.to_radians();
        // Sample the pixel whose center mirrors exactly about the vertical axis.
        let x = (center_x + radius * radians.cos() - 0.5).round() as i32;
        let y = (center_y + radius * radians.sin()) as i32;
        surface.peek_pixels().unwrap().get_color((x, y))
    };
    let (right, left) = (render(50.0, 270.0 + 45.0), render(-50.0, 270.0 - 45.0));
    assert!(right.r().abs_diff(left.r()) <= 2 && right.b().abs_diff(left.b()) <= 2, "{right:?} {left:?}");
    assert!(right.b() > right.r(), "the outer lean leans toward the last stop: {right:?}");
    let upright = render(50.0, 270.0 + 3.0);
    assert!(upright.r() > 200 && upright.b() < 40, "the upright edge starts at the first stop: {upright:?}");
}

fn render_config_with_lean_angle(value: serde_json::Value) -> RenderConfig {
    RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        backdrops: vec![],
        rasters: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(value).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    }
}

fn full_lean_angle_config() -> serde_json::Value {
    serde_json::json!({
        "value": "lean_angle",
        "x": 20,
        "y": 30,
        "display_type": "lean_angle",
        "diameter": 180,
        "rotation": 0,
        "opacity": 1,
        "show_icon": false,
        "track_empty_color": "#222222",
        "track_empty_opacity": 0.5,
        "track_filled_color": "#dce2e8",
        "track_filled_opacity": 1,
        "track_filled_gradient": false,
        "track_filled_gradient_stops": [{ "offset": 0, "color": "#40e0d0" }, { "offset": 1, "color": "#c65102" }],
        "track_border_thickness": 2,
        "track_border_color": "#ffffff",
        "track_thickness": 24,
        "font": "Arial.ttf",
        "font_size": 60,
        "color": "#ffffff",
        "unit_color": "#ffffff",
        "show_units": true,
        "value_offset_x": 0,
        "value_offset_y": 0
    })
}
