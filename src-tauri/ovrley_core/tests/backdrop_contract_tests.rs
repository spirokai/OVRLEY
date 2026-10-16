mod common;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::commands::validate_config_value;
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::parse_config_value;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::{prepare_preview_assets, LabelCacheStatus, StaticLayer};
use ovrley_widgets::model::types::BackdropType;
use serde_json::json;

#[test]
fn backdrop_type_deserializes_strictly() {
    assert_eq!(
        serde_json::from_str::<BackdropType>(r#""circle""#).unwrap(),
        BackdropType::Circle
    );
    assert_eq!(
        serde_json::from_str::<BackdropType>(r#""rectangle""#).unwrap(),
        BackdropType::Rectangle
    );
    assert!(serde_json::from_str::<BackdropType>(r#""triangle""#).is_err());
    assert!(serde_json::from_str::<BackdropType>("null").is_err());
}

#[test]
fn missing_backdrops_loads_as_empty_list() {
    let raw = parse_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    assert!(raw.backdrops.is_empty());

    let validated = validate_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    assert!(validated.backdrops.is_empty());
}

#[test]
fn valid_rectangle_backdrop_promotes_and_validates_active_variant() {
    let validated = validate_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "backdrops": [{
            "id": "backdrop-1",
            "x": 10,
            "y": 20,
            "opacity": 0.75,
            "display_type": "rectangle",
            "fill_color": "#112233",
            "fill_opacity": 0.5,
            "border_thickness": 2,
            "border_color": "#445566",
            "border_opacity": 0.25,
            "display_variants": {
                "rectangle": {
                    "width": 100,
                    "height": 60,
                    "corner_radius": 12,
                    "round_top_left": true,
                    "round_top_right": false,
                    "round_bottom_left": true,
                    "round_bottom_right": false
                }
            }
        }],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();

    let backdrop = &validated.backdrops[0];
    assert_eq!(backdrop.id, "backdrop-1");
    assert_eq!(backdrop.display_type, BackdropType::Rectangle);
    assert_eq!(backdrop.width, 100.0);
    assert_eq!(backdrop.height, 60.0);
    assert_eq!(backdrop.corner_radius, 12.0);
    assert!(backdrop.round_top_left);
    assert!(!backdrop.round_top_right);
    assert_eq!(backdrop.fill_color, "#112233");
    assert_eq!(backdrop.border_color, "#445566");
}

#[test]
fn valid_circle_backdrop_promotes_and_validates_active_variant() {
    let validated = validate_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "backdrops": [{
            "id": "backdrop-circle",
            "x": 11,
            "y": 17,
            "opacity": 0.75,
            "display_type": "circle",
            "fill_color": "#112233",
            "fill_opacity": 0.5,
            "border_thickness": 3,
            "border_color": "#445566",
            "border_opacity": 0.25,
            "display_variants": {
                "circle": {
                    "diameter": 80
                },
                "rectangle": {
                    "width": 100,
                    "height": 60,
                    "corner_radius": 12,
                    "round_top_left": true,
                    "round_top_right": false,
                    "round_bottom_left": true,
                    "round_bottom_right": false
                }
            }
        }],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();

    let backdrop = &validated.backdrops[0];
    assert_eq!(backdrop.id, "backdrop-circle");
    assert_eq!(backdrop.display_type, BackdropType::Circle);
    assert_eq!(backdrop.diameter, 80.0);
    assert_eq!(backdrop.width, 0.0);
    assert_eq!(backdrop.height, 0.0);
    assert_eq!(backdrop.corner_radius, 0.0);
    assert_eq!(backdrop.fill_color, "#112233");
    assert_eq!(backdrop.border_color, "#445566");
}

#[test]
fn backdrop_styling_fields_are_required() {
    let result = validate_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "backdrops": [{
            "id": "backdrop-1",
            "x": 10,
            "y": 20,
            "opacity": 1,
            "display_type": "circle",
            "fill_opacity": 1,
            "border_thickness": 0,
            "border_color": "#ffffff",
            "border_opacity": 1,
            "display_variants": {
                "circle": {
                    "diameter": 100
                }
            }
        }],
        "labels": [],
        "values": [],
        "plots": []
    }));

    let Err(error) = result else {
        panic!("missing fill_color should be rejected");
    };

    assert!(
        error.to_string().contains("backdrops[0].fill_color"),
        "got: {error}"
    );
}

#[test]
fn rectangle_backdrop_rejects_invalid_styling_and_dimensions() {
    let mut invalid_fill_color = rectangle_backdrop_json("invalid-fill", "#ff0000");
    invalid_fill_color["fill_color"] = json!("#zzzzzz");
    assert_backdrop_error_contains(invalid_fill_color, "backdrops[0].fill_color");

    let mut invalid_opacity = rectangle_backdrop_json("invalid-opacity", "#ff0000");
    invalid_opacity["fill_opacity"] = json!(1.5);
    assert_backdrop_error_contains(invalid_opacity, "backdrops[0].fill_opacity");

    let mut zero_width = rectangle_backdrop_json("zero-width", "#ff0000");
    zero_width["display_variants"]["rectangle"]["width"] = json!(0);
    assert_backdrop_error_contains(zero_width, "backdrops[0].width");

    let mut missing_corner_flag = rectangle_backdrop_json("missing-corner-flag", "#ff0000");
    missing_corner_flag["display_variants"]["rectangle"]
        .as_object_mut()
        .unwrap()
        .remove("round_top_left");
    assert_backdrop_error_contains(missing_corner_flag, "backdrops[0].round_top_left");

    let mut negative_border = rectangle_backdrop_json("negative-border", "#ff0000");
    negative_border["border_thickness"] = json!(-1);
    assert_backdrop_error_contains(negative_border, "backdrops[0].border_thickness");

    let mut overflowing_border = rectangle_backdrop_json("overflowing-border", "#ff0000");
    overflowing_border["border_thickness"] = json!(5);
    assert_backdrop_error_contains(overflowing_border, "2 * border_thickness");
}

#[test]
fn circle_backdrop_rejects_invalid_styling_and_diameter() {
    let mut missing_diameter = circle_backdrop_json("missing-diameter", "#ff0000");
    missing_diameter["display_variants"]["circle"]
        .as_object_mut()
        .unwrap()
        .remove("diameter");
    assert_backdrop_error_contains(missing_diameter, "backdrops[0].diameter");

    let mut zero_diameter = circle_backdrop_json("zero-diameter", "#ff0000");
    zero_diameter["display_variants"]["circle"]["diameter"] = json!(0);
    assert_backdrop_error_contains(zero_diameter, "backdrops[0].diameter");

    let mut invalid_border_color = circle_backdrop_json("invalid-border-color", "#ff0000");
    invalid_border_color["border_color"] = json!("#zzzzzz");
    assert_backdrop_error_contains(invalid_border_color, "backdrops[0].border_color");

    let mut invalid_opacity = circle_backdrop_json("invalid-opacity", "#ff0000");
    invalid_opacity["border_opacity"] = json!(-0.1);
    assert_backdrop_error_contains(invalid_opacity, "backdrops[0].border_opacity");

    let mut negative_border = circle_backdrop_json("negative-border", "#ff0000");
    negative_border["border_thickness"] = json!(-1);
    assert_backdrop_error_contains(negative_border, "backdrops[0].border_thickness");

    let mut overflowing_border = circle_backdrop_json("overflowing-border", "#ff0000");
    overflowing_border["border_thickness"] = json!(6);
    assert_backdrop_error_contains(overflowing_border, "2 * border_thickness");
}

#[test]
fn rectangle_corner_radius_rejects_negative_and_clamps_valid_output() {
    let mut negative_radius = rectangle_backdrop_json("negative-radius", "#ff0000");
    negative_radius["display_variants"]["rectangle"]["corner_radius"] = json!(-1);
    assert_backdrop_error_contains(negative_radius, "backdrops[0].corner_radius");

    let mut oversized_radius = rectangle_backdrop_json("oversized-radius", "#ff0000");
    oversized_radius["display_variants"]["rectangle"]["width"] = json!(100);
    oversized_radius["display_variants"]["rectangle"]["height"] = json!(60);
    oversized_radius["display_variants"]["rectangle"]["corner_radius"] = json!(80);
    let validated = validate_config_value(&config_with_backdrop(oversized_radius)).unwrap();
    assert_eq!(validated.backdrops[0].corner_radius, 30.0);
}

#[test]
fn rectangle_corner_radius_adjusts_to_border_only_when_a_corner_is_rounded() {
    let mut rounded = rectangle_backdrop_json("rounded-radius", "#ff0000");
    rounded["border_thickness"] = json!(10);
    rounded["display_variants"]["rectangle"]["width"] = json!(100);
    rounded["display_variants"]["rectangle"]["height"] = json!(60);
    rounded["display_variants"]["rectangle"]["corner_radius"] = json!(4);
    rounded["display_variants"]["rectangle"]["round_top_left"] = json!(true);
    let rounded_validated = validate_config_value(&config_with_backdrop(rounded)).unwrap();
    assert_eq!(rounded_validated.backdrops[0].corner_radius, 10.0);

    let mut sharp = rectangle_backdrop_json("sharp-radius", "#ff0000");
    sharp["border_thickness"] = json!(10);
    sharp["display_variants"]["rectangle"]["width"] = json!(100);
    sharp["display_variants"]["rectangle"]["height"] = json!(60);
    sharp["display_variants"]["rectangle"]["corner_radius"] = json!(4);
    let sharp_validated = validate_config_value(&config_with_backdrop(sharp)).unwrap();
    assert_eq!(sharp_validated.backdrops[0].corner_radius, 4.0);
}

#[test]
fn static_base_rgba_renders_rectangle_backdrop() {
    let validated = validate_config_value(&json!({
        "scene": small_scene_json(32, 24),
        "backdrops": [rectangle_backdrop_json("static-base-rect", "#ff0000")],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let mut profiler = RenderProfiler::default();
    let pixels = StaticLayer {
        backdrops: &validated.backdrops,
        rasters: &[],
        labels: &validated.labels,
        values: &validated.values,
        scene: &validated.scene,
    }
    .prepare(&test_paths(), &mut profiler)
    .unwrap()
    .base_rgba;

    assert_eq!(rgba_at(&pixels, 32, 8, 8), [255, 0, 0, 255]);
    assert_eq!(rgba_at(&pixels, 32, 24, 20), [0, 0, 0, 0]);
}

#[test]
fn static_base_rgba_renders_rectangle_fill_inside_border() {
    let mut backdrop = rectangle_backdrop_json("bordered-static-rect", "#ff0000");
    backdrop["border_thickness"] = json!(2);
    backdrop["border_color"] = json!("#0000ff");
    let validated = validate_config_value(&json!({
        "scene": small_scene_json(32, 24),
        "backdrops": [backdrop],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let mut profiler = RenderProfiler::default();
    let pixels = StaticLayer {
        backdrops: &validated.backdrops,
        rasters: &[],
        labels: &validated.labels,
        values: &validated.values,
        scene: &validated.scene,
    }
    .prepare(&test_paths(), &mut profiler)
    .unwrap()
    .base_rgba;

    assert_eq!(rgba_at(&pixels, 32, 5, 8), [0, 0, 255, 255]);
    assert_eq!(rgba_at(&pixels, 32, 6, 8), [255, 0, 0, 255]);
}

#[test]
fn static_base_rgba_applies_rectangle_fill_alpha_multipliers() {
    let mut backdrop = rectangle_backdrop_json("alpha-static-rect", "#ff000080");
    backdrop["opacity"] = json!(0.5);
    backdrop["fill_opacity"] = json!(0.5);
    let validated = validate_config_value(&json!({
        "scene": small_scene_json(32, 24),
        "backdrops": [backdrop],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let mut profiler = RenderProfiler::default();
    let pixels = StaticLayer {
        backdrops: &validated.backdrops,
        rasters: &[],
        labels: &validated.labels,
        values: &validated.values,
        scene: &validated.scene,
    }
    .prepare(&test_paths(), &mut profiler)
    .unwrap()
    .base_rgba;

    assert_eq!(rgba_at(&pixels, 32, 8, 8), [255, 0, 0, 32]);
}

#[test]
fn static_base_rgba_renders_circle_fill_inside_total_diameter_border() {
    let mut backdrop = circle_backdrop_json("bordered-static-circle", "#ff0000");
    backdrop["border_thickness"] = json!(2);
    backdrop["border_color"] = json!("#0000ff");
    let validated = validate_config_value(&json!({
        "scene": small_scene_json(32, 24),
        "backdrops": [backdrop],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let mut profiler = RenderProfiler::default();
    let pixels = StaticLayer {
        backdrops: &validated.backdrops,
        rasters: &[],
        labels: &validated.labels,
        values: &validated.values,
        scene: &validated.scene,
    }
    .prepare(&test_paths(), &mut profiler)
    .unwrap()
    .base_rgba;

    assert_eq!(rgba_at(&pixels, 32, 10, 6), [0, 0, 255, 255]);
    assert_eq!(rgba_at(&pixels, 32, 10, 11), [255, 0, 0, 255]);
    assert_eq!(rgba_at(&pixels, 32, 22, 11), [0, 0, 0, 0]);
}

#[test]
fn static_base_rgba_applies_circle_fill_alpha_multipliers() {
    let mut backdrop = circle_backdrop_json("alpha-static-circle", "#ff000080");
    backdrop["opacity"] = json!(0.5);
    backdrop["fill_opacity"] = json!(0.5);
    let validated = validate_config_value(&json!({
        "scene": small_scene_json(32, 24),
        "backdrops": [backdrop],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let mut profiler = RenderProfiler::default();
    let pixels = StaticLayer {
        backdrops: &validated.backdrops,
        rasters: &[],
        labels: &validated.labels,
        values: &validated.values,
        scene: &validated.scene,
    }
    .prepare(&test_paths(), &mut profiler)
    .unwrap()
    .base_rgba;

    assert_eq!(rgba_at(&pixels, 32, 10, 11), [255, 0, 0, 32]);
}

#[test]
fn static_cache_key_includes_backdrops() {
    let paths = test_paths();
    let activity: ParsedActivity = serde_json::from_value(json!({})).unwrap();
    let dense = common::builders::minimal_dense_activity();
    let first_config = validate_config_value(&json!({
        "scene": small_scene_json(53, 47),
        "backdrops": [rectangle_backdrop_json("cache-rect", "#ff0000")],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();
    let second_config = validate_config_value(&json!({
        "scene": small_scene_json(53, 47),
        "backdrops": [rectangle_backdrop_json("cache-rect", "#0000ff")],
        "labels": [],
        "values": [],
        "plots": []
    }))
    .unwrap();

    let (_, first_status, _, _) =
        prepare_preview_assets(&paths, &first_config, &activity, &dense).unwrap();
    let (_, second_status, _, _) =
        prepare_preview_assets(&paths, &first_config, &activity, &dense).unwrap();
    let (_, changed_status, _, _) =
        prepare_preview_assets(&paths, &second_config, &activity, &dense).unwrap();

    assert!(matches!(first_status, LabelCacheStatus::Miss));
    assert!(matches!(second_status, LabelCacheStatus::Hit));
    assert!(matches!(changed_status, LabelCacheStatus::Miss));
}

#[test]
fn wasm_backdrops_match_native_opacity_and_ignore_playhead_and_activity() {
    use ovrley_widgets::preview::PreviewSession;
    for mut backdrop in [rectangle_backdrop_json("wasm-rect", "#abcdef80"), circle_backdrop_json("wasm-circle", "#abcdef80")] {
        backdrop["x"] = json!(20.25);
        backdrop["y"] = json!(30.5);
        backdrop["border_thickness"] = json!(2.25);
        backdrop["border_opacity"] = json!(0.6);
        backdrop["opacity"] = json!(0.75);
        if backdrop["display_type"] == "rectangle" {
            backdrop["display_variants"]["rectangle"]["corner_radius"] = json!(4.0);
            backdrop["display_variants"]["rectangle"]["round_top_left"] = json!(true);
            backdrop["display_variants"]["rectangle"]["round_bottom_right"] = json!(true);
        }
        let mut scene = small_scene_json(96, 96);
        scene["scale"] = json!(1.75);
        let mut request = json!({"type": "backdrop", "widget": backdrop, "scene": scene});
        let mut preview = PreviewSession::default(); // No font or activity resources.
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: serde_json::Value = serde_json::from_slice(widget.geometry()).unwrap();
        let pixels = widget.pixels().clone();
        let revision = widget.revision();
        let pixel_storage = widget.pixels().as_ptr();
        let mut config = validate_config_value(&json!({"scene": scene, "backdrops": [backdrop], "labels": [], "values": [], "plots": []})).unwrap();
        let native = StaticLayer {
            backdrops: &config.backdrops, rasters: &[], labels: &[], values: &[], scene: &config.scene,
        }.prepare(&test_paths(), &mut RenderProfiler::default()).unwrap().base_rgba;
        let width = geometry["width"].as_u64().unwrap() as usize;
        let height = geometry["height"].as_u64().unwrap() as usize;
        let left = geometry["left"].as_f64().unwrap() as usize;
        let top = geometry["top"].as_f64().unwrap() as usize;
        assert_eq!(rgba_at(&pixels, width, width / 2, height / 2), rgba_at(&native, 96, left + width / 2, top + height / 2));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        preview.replace_activity(br#"{"sample_elapsed_seconds":[0,1],"speed":[0,20],"trim_end_seconds":1}"#).unwrap();
        for second in [0.0, 1.0, 0.25, 100.0] {
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
            assert_eq!(preview.widgets[&id].pixels().as_ptr(), pixel_storage);
            assert_eq!(preview.widgets[&id].pixels(), &pixels);
        }
        preview.replace_activity(&[]).unwrap();
        preview.render(0.0).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
        request["scene"]["opacity"] = json!(0.5);
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        config.scene.presentation.opacity = 0.5;
        let native = StaticLayer { backdrops: &config.backdrops, rasters: &[], labels: &[], values: &[], scene: &config.scene }
            .prepare(&test_paths(), &mut RenderProfiler::default()).unwrap().base_rgba;
        assert_eq!(rgba_at(preview.widgets[&id].pixels(), width, width / 2, height / 2)[3], 48);
        assert_eq!(rgba_at(&native, 96, left + width / 2, top + height / 2)[3], 48);
        request["widget"]["fill_color"] = json!("#ff0000");
        request["scene"]["opacity"] = json!(0);
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        assert_ne!(preview.widgets[&id].revision(), revision);
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        assert!(preview.widgets[&id].pixels().chunks_exact(4).all(|pixel| pixel[3] == 0));
        request["widget"]["border_thickness"] = json!(-1);
        assert!(preview.update_widget(id, serde_json::from_value(request).unwrap()).is_err());
    }
}

#[test]
fn wasm_backdrop_resize_keeps_opposite_corners_and_fractional_dimensions() {
    use ovrley_widgets::preview::PreviewSession;
    for backdrop in [rectangle_backdrop_json("resize-rect", "#ff0000"), circle_backdrop_json("resize-circle", "#ff0000")] {
        let mut scene = small_scene_json(96, 96);
        scene["scale"] = json!(1.5);
        let origin = json!({"type": "backdrop", "widget": backdrop, "scene": scene});
        let mut preview = PreviewSession::default();
        let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
        let geometry: serde_json::Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0], bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1]];
            let mut gesture = origin.clone();
            gesture["size"] = json!([37.875, 28.125]);
            gesture["fixed"] = json!(fixed);
            gesture["point"] = json!(point);
            gesture["second"] = json!(19.5);
            let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
            let mut committed = origin.clone();
            committed["widget"]["x"] = patch["x"].clone();
            committed["widget"]["y"] = patch["y"].clone();
            let display_type = backdrop["display_type"].as_str().unwrap();
            committed["widget"]["display_variants"][display_type].as_object_mut().unwrap().extend(patch["display_variants"][display_type].as_object().unwrap().clone());
            preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
            let resized: serde_json::Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let actual = &resized["content"];
            assert_eq!(actual["width"], json!(37.875));
            assert_eq!(actual["height"], json!(if display_type == "circle" { 37.875 } else { 28.125 }));
            for (axis, position, dimension) in [(0, "left", "width"), (1, "top", "height")] {
                let corner = actual[position].as_f64().unwrap() + actual[dimension].as_f64().unwrap() * fixed[axis];
                assert!((corner - point[axis]).abs() < 0.002);
            }
        }
    }
}

fn small_scene_json(width: u32, height: u32) -> serde_json::Value {
    let mut scene = common::seam::explicit_scene_json();
    scene["width"] = json!(width);
    scene["height"] = json!(height);
    scene
}

fn rectangle_backdrop_json(id: &str, fill_color: &str) -> serde_json::Value {
    json!({
        "id": id,
        "x": 4,
        "y": 5,
        "opacity": 1,
        "display_type": "rectangle",
        "fill_color": fill_color,
        "fill_opacity": 1,
        "border_thickness": 0,
        "border_color": "#ffffff",
        "border_opacity": 1,
        "display_variants": {
            "rectangle": {
                "width": 12,
                "height": 10,
                "corner_radius": 0,
                "round_top_left": false,
                "round_top_right": false,
                "round_bottom_left": false,
                "round_bottom_right": false
            }
        }
    })
}

fn circle_backdrop_json(id: &str, fill_color: &str) -> serde_json::Value {
    json!({
        "id": id,
        "x": 4,
        "y": 5,
        "opacity": 1,
        "display_type": "circle",
        "fill_color": fill_color,
        "fill_opacity": 1,
        "border_thickness": 0,
        "border_color": "#ffffff",
        "border_opacity": 1,
        "display_variants": {
            "circle": {
                "diameter": 12
            }
        }
    })
}

fn config_with_backdrop(backdrop: serde_json::Value) -> serde_json::Value {
    json!({
        "scene": common::seam::explicit_scene_json(),
        "backdrops": [backdrop],
        "labels": [],
        "values": [],
        "plots": []
    })
}

fn assert_backdrop_error_contains(backdrop: serde_json::Value, expected: &str) {
    let result = validate_config_value(&config_with_backdrop(backdrop));
    let Err(error) = result else {
        panic!("backdrop config should be rejected");
    };

    let message = error.to_string();
    assert!(message.contains(expected), "got: {message}");
}

fn rgba_at(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn test_paths() -> AppPaths {
    let workspace_root = common::test_config::repo_git_root();
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
