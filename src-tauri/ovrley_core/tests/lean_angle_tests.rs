mod common;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::RenderConfig;
use ovrley_core::normalize::validate_render_config;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::widgets::prepare_render_assets;
use ovrley_core::render::widgets::types::PreparedValue;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn full_lean_angle_config_validates_and_prepares_one_static_cache() {
    let config = validate_render_config(RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        backdrops: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(full_lean_angle_config()).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
    .unwrap();

    let PreparedValue::LeanAngle(widget) = &config.values[0] else {
        panic!("lean_angle display type must use the dedicated prepared value");
    };
    assert_eq!(widget.validated.diameter, 180.0);
    assert_eq!(widget.validated.track_thickness, 24.0);

    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = common::builders::minimal_dense_activity();
    let mut profiler = RenderProfiler::default();
    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();

    let PreparedValue::LeanAngle(widget) = &assets.values()[0] else {
        panic!("lean-angle value index must own the static lean-angle cache");
    };
    let cache = widget
        .cache
        .as_ref()
        .expect("lean-angle cache must be prepared");
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

fn render_config_with_lean_angle(value: serde_json::Value) -> RenderConfig {
    RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        backdrops: vec![],
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
