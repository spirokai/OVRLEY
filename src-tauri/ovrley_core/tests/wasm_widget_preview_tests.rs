//! One seam test: transported font + standalone cached surface must match native composition.
mod common;

use ovrley_core::fonts::{preview_font_resource, NativeFonts};
use ovrley_widgets::{
    model::scene::validate_scene_presentation,
    preview::PreviewSession,
    render::text::validated_value_style,
    widgets::value::preview::WidgetRequest,
    widgets::value::{
        format::{format_metric_parts, MetricSample},
        layout::{draw_metric_parts, draw_static_metric_parts_for_value},
        validation::validate_value_widget,
    },
};
use serde_json::{json, Value};
use skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};

fn g_force_preview() -> (PreviewSession, Value) {
    let (preview, mut request) = lean_angle_preview();
    request["type"] = json!("g_force");
    request["scene"]["end"] = json!(5);
    request["widget"] = json!({
        "value": "g_force", "display_type": "g_force", "x": 600.25, "y": 100.5,
        "width": 220.25, "height": 220.25, "diameter": 200, "opacity": 0.8,
        "fill_color": "#212121", "fill_opacity": 0.5,
        "border_thickness": 2, "border_color": "#ffffff", "border_opacity": 0.7,
        "marker_size": 12, "marker_color": "#40e0d0", "marker_opacity": 0.9,
        "axis_horizontal": "x", "axis_vertical": "y", "invert_horizontal": false, "invert_vertical": false,
        "clip_percentile": 80, "label_font": "Inter.ttf", "label_font_size": 24,
        "label_color": "#abcdef", "label_decimals": 1, "label_unit": "G", "label_unit_color": "#aaaaaa",
        "label_offset_x": 11.25, "label_offset_y": -17.5,
    });
    (preview, request)
}

#[test]
fn g_force_preview_matches_native_pixels_and_tracks_marker_and_percentile_changes() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        render::widgets::{g_force::{draw_g_force_widget, prepare_g_force_cache}, types::PreparedValue},
    };
    let fixture: Value = serde_json::from_str(include_str!("fixtures/g-force-frame-state.json")).unwrap();
    let mut activity: ParsedActivity = serde_json::from_value(fixture["activity"].clone()).unwrap();
    let (mut preview, request) = g_force_preview();
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let PreparedValue::GForce(widget) = &config.values[0] else { panic!("G-force dispatch"); };
    let requirements = config.render_data_requirements().unwrap();
    let trimmed = trim_activity(&activity, 0.0, 5.0, &requirements).unwrap();
    let seconds = [1.0, 1.002, 3.0];
    let dense = densify_activity(&trimmed, seconds.to_vec(), &requirements);
    let mut profiler = RenderProfiler::default();
    let cache = prepare_g_force_cache(&widget.validated, &config.scene, &activity, &dense, &mut profiler).unwrap();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let id = preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
    for index in [0, 1, 2, 0] {
        preview.render(seconds[index]).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let info = ImageInfo::new(
            (geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32),
            ColorType::RGBA8888, AlphaType::Premul, None,
        );
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
        draw_g_force_widget(canvas, &cache, index, &font_dirs, &mut profiler).unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0, 0),
        ));
        assert_eq!(&pixels, widget.pixels(), "G-force at {}", seconds[index]);
    }
    let revision = preview.widgets[&id].revision();
    preview.render(1.0).unwrap();
    assert_eq!(preview.widgets[&id].revision(), revision);
    let pixels = preview.widgets[&id].pixels().clone();
    activity.g_force_y[2] = Some(120.0); // Same current axes, different whole-activity clipping scale.
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
}

#[test]
fn g_force_resize_keeps_visible_corners_fixed_and_commits_fractional_variant_geometry() {
    let (mut preview, origin) = g_force_preview();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/g-force-frame-state.json")).unwrap();
    preview.replace_activity(&serde_json::to_vec(&fixture["activity"]).unwrap()).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    for fixed in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        let point = [
            bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
            bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
        ];
        for second in [1.0, 3.0] {
            let mut resize = origin.clone();
            resize["factor"] = json!(1.234375);
            resize["fixed"] = json!(fixed);
            resize["point"] = json!(point);
            resize["second"] = json!(second);
            let patch = serde_json::to_value(
                preview.resize_widget(serde_json::from_value(resize).unwrap()).unwrap(),
            ).unwrap();
            let variant = &patch["display_variants"]["g_force"];
            assert_eq!(variant["width"], json!(271.87109375));
            assert_eq!(variant["height"], variant["width"]);
            let mut committed = origin.clone();
            committed["widget"]["x"] = patch["x"].clone();
            committed["widget"]["y"] = patch["y"].clone();
            for (field, value) in variant.as_object().unwrap() { committed["widget"][field] = value.clone(); }
            preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
            preview.render(second).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            for (axis, start, size) in [(0, "left", "width"), (1, "top", "height")] {
                let actual = geometry["content"][start].as_f64().unwrap() + geometry["content"][size].as_f64().unwrap() * fixed[axis];
                assert!((actual - point[axis]).abs() < 0.003);
            }
        }
    }
}

fn lean_angle_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["end"] = json!(4);
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(1.5);
    (
        preview,
        json!({
            "type": "lean_angle", "scene": scene,
            "widget": {
                "value": "lean_angle", "display_type": "lean_angle", "x": 600.25, "y": 100.5,
                "diameter": 180, "rotation": 0, "opacity": 0.8, "show_icon": false,
                "track_empty_color": "#222222", "track_empty_opacity": 0.5,
                "track_filled_color": "#40e0d0", "track_filled_opacity": 0.7,
                "track_border_thickness": 2, "track_border_color": "#ffffff", "track_thickness": 24,
                "font": "Inter.ttf", "font_size": 60, "color": "#abcdef", "unit_color": "#aaaaaa",
                "show_units": true, "value_offset_x": 11.25, "value_offset_y": -17.5,
            },
        }),
    )
}

#[test]
fn lean_angle_preview_matches_native_export_and_tracks_signed_fill_resources_and_validation() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::widgets::{draw_metric_presentation, prepare_render_assets},
    };
    use ovrley_widgets::activity::ActivitySampler;
    let (mut preview, request) = lean_angle_preview();
    let parsed: ParsedActivity = serde_json::from_value(json!({
        "sample_elapsed_seconds": [0,1,2,3,4], "trim_end_seconds": 4,
        "lean_angle": [null,30,null,-70,null],
    }))
    .unwrap();
    let bytes = serde_json::to_vec(&parsed).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let requirements = config.render_data_requirements().unwrap();
    let trimmed = trim_activity(&parsed, 0.0, 4.0, &requirements).unwrap();
    let seconds = vec![0.0, 0.5, 1.0, 1.002, 2.0, 2.5, 3.0, 4.0];
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let root = common::test_config::repo_git_root();
    let paths = AppPaths {
        repo_root: root.clone(),
        font_dirs: vec![root.join("fonts")],
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    };
    let mut profiler = RenderProfiler::default();
    let assets = prepare_render_assets(&paths, &config, &parsed, &dense, &mut profiler).unwrap();
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for index in [0, 1, 2, 3, 4, 5, 6, 7, 2, 0] {
        let second = seconds[index];
        assert_eq!(
            sampler.lean_angle_at(second),
            dense.series.lean_angle[index]
        );
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let info = ImageInfo::new(
            (
                geometry["width"].as_i64().unwrap() as i32,
                geometry["height"].as_i64().unwrap() as i32,
            ),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((
            -geometry["left"].as_f64().unwrap() as f32,
            -geometry["top"].as_f64().unwrap() as f32,
        ));
        draw_metric_presentation(
            canvas,
            &assets.values()[0],
            &dense,
            index,
            config.scene.presentation.scale,
            &paths.font_dirs,
            &mut profiler,
        )
        .unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul),
            &mut pixels,
            info.width() as usize * 4,
            (0, 0)
        ));
        assert_eq!(&pixels, widget.pixels(), "lean-angle at {second}");
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let revision = widget.revision();
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
    }
    preview.render(1.0).unwrap();
    let previous = preview.widgets[&id].pixels().clone();
    let revision = preview.widgets[&id].revision();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].revision(),
        revision,
        "fill changes within the same integer label"
    );
    assert_ne!(preview.widgets[&id].pixels(), &previous);
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(&bytes).unwrap();
    preview.render(1.002).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,4],"trim_end_seconds":4,"lean_angle":[-30,-30]}"#,
        )
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].pixels(),
        &original,
        "signed fill changes even for equal magnitudes"
    );
    assert_eq!(sampler.lean_angle_at(-1.0), None);
    assert_eq!(sampler.lean_angle_at(5.0), None);
    for (field, malformed) in [
        ("diameter", Value::Null),
        ("width", json!(180)),
        ("track_thickness", json!(90)),
        ("track_border_thickness", json!(12)),
        ("track_empty_color", json!("invalid")),
        ("show_units", Value::Null),
        ("value_offset_x", Value::Null),
        ("font_size", json!(0)),
        ("opacity", json!(2)),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    let mut changed = request;
    changed["widget"]["track_filled_color"] = json!("#123456");
    let revision = preview.widgets[&id].revision();
    let pixels = preview.widgets[&id].pixels().clone();
    preview
        .update_widget(id, serde_json::from_value(changed).unwrap())
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
}

#[test]
fn lean_angle_resize_preserves_visible_corners_across_playback_and_commits_diameter_variants() {
    let (mut preview, mut origin) = lean_angle_preview();
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,1,2,3],"lean_angle":[30,70,-30,0],"trim_end_seconds":3}"#).unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for (scale, offset, show_units, border) in [
        (1.7, [11.25, -17.5], true, 2),
        (0.8, [200.0, 100.0], false, 0),
    ] {
        origin["scene"]["scale"] = json!(scale);
        origin["widget"]["value_offset_x"] = json!(offset[0]);
        origin["widget"]["value_offset_y"] = json!(offset[1]);
        origin["widget"]["show_units"] = json!(show_units);
        origin["widget"]["track_border_thickness"] = json!(border);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.01, 0.55, 1.234375] {
                for second in [2.25, 3.0, 0.25] {
                    let mut gesture = origin.clone();
                    gesture["factor"] = json!(factor);
                    gesture["fixed"] = json!(fixed);
                    gesture["point"] = json!(point);
                    gesture["second"] = json!(second);
                    let patch = serde_json::to_value(
                        preview
                            .resize_widget(serde_json::from_value(gesture).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    let variant = &patch["display_variants"]["lean_angle"];
                    assert!(patch.get("width").is_none() && patch.get("height").is_none());
                    assert!(
                        variant.get("font_size").is_none()
                            && variant.get("width").is_none()
                            && variant.get("height").is_none()
                    );
                    if factor == 1.234375 {
                        assert_eq!(variant["diameter"], json!(222.1875));
                        assert_eq!(variant["track_thickness"], json!(29.625));
                        assert_eq!(
                            variant["track_border_thickness"],
                            json!(f64::from(border) * factor)
                        );
                        assert_eq!(patch["font_size"], json!(74.0625));
                    }
                    // Match the frontend's variant merge and effective-config resolution.
                    let mut committed = origin.clone();
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(variant.as_object().unwrap().clone());
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(patch.as_object().unwrap().clone());
                    preview
                        .update_widget(id, serde_json::from_value(committed.clone()).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    let live_geometry = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&live_geometry).unwrap();
                    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
                        let corner = actual["content"][position].as_f64().unwrap()
                            + actual["content"][size].as_f64().unwrap() * fixed[axis];
                        assert!(
                            (corner - point[axis]).abs() < 0.002,
                            "lean-angle corner moved at {second}, factor {factor}"
                        );
                    }
                    preview
                        .update_widget(id, serde_json::from_value(committed).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &live_geometry);
                }
            }
        }
    }
}

fn gradient_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["end"] = json!(4);
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(1.5);
    (
        preview,
        json!({
            "type": "gradient", "scene": scene,
            "widget": {
                "value": "gradient", "display_type": "text", "x": 600.25, "y": 100.5,
                "font": "Inter.ttf", "font_size": 48, "color": "#abcdef", "opacity": 0.8,
                "prefix": "", "suffix": "", "decimals": 1, "show_sign": true,
                "show_triangle": true, "triangle_width": 72, "value_offset": 11.25,
                "unit_color": "#aaaaaa", "triangle_positive_color": "#40e0d0", "triangle_negative_color": "#c65102",
            },
        }),
    )
}

#[test]
fn gradient_preview_matches_native_samples_pixels_and_invalidates_continuous_triangle_changes() {
    use ovrley_core::activity::{
        interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity,
    };
    use ovrley_widgets::{
        activity::ActivitySampler,
        widgets::gradient::{
            format::format_validated_gradient,
            frame::{prepare_gradient_frame, validated_gradient_style},
            validation::validate_gradient_widget,
        },
    };
    let (mut preview, mut request) = gradient_preview();
    request["scene"]["opacity"] = json!(0.5);
    let parsed: ParsedActivity = serde_json::from_value(json!({
        "sample_elapsed_seconds": [0,1,2,3,4], "trim_end_seconds": 4,
        "gradient": [null,6,null,-6,null],
    }))
    .unwrap();
    let bytes = serde_json::to_vec(&parsed).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let requirements = config.render_data_requirements().unwrap();
    let trimmed = trim_activity(&parsed, 0.0, 4.0, &requirements).unwrap();
    let seconds = vec![0.0, 0.5, 1.0, 1.002, 2.0, 2.5, 3.0, 4.0];
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let validated = validate_gradient_widget(
        serde_json::from_value(request["widget"].clone()).unwrap(),
        0,
    )
    .unwrap();
    let scene =
        validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap())
            .unwrap();
    let style = validated_gradient_style(&validated, &scene, scene.scale);
    assert_eq!(style.opacity, 0.4);
    assert_eq!(style.color.a(), 102);
    let fonts = NativeFonts(&[common::test_config::repo_git_root().join("fonts")]);
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    // Forward playback and backward seeks include a bridged gap and missing coverage edges.
    for index in [0, 1, 2, 3, 4, 5, 6, 7, 2, 0] {
        let second = seconds[index];
        assert_eq!(sampler.gradient_at(second), dense.series.gradient[index]);
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let info = ImageInfo::new(
            (
                geometry["width"].as_i64().unwrap() as i32,
                geometry["height"].as_i64().unwrap() as i32,
            ),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((
            -geometry["left"].as_f64().unwrap() as f32,
            -geometry["top"].as_f64().unwrap() as f32,
        ));
        prepare_gradient_frame(
            &validated,
            &style,
            dense.series.gradient[index],
            scene.scale,
            scene.opacity,
            &fonts,
        )
        .unwrap()
        .draw(canvas, &fonts)
        .unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul),
            &mut pixels,
            info.width() as usize * 4,
            (0, 0)
        ));
        assert_eq!(&pixels, widget.pixels(), "gradient at {second}");
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let revision = widget.revision();
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
    }
    assert_eq!(
        format_validated_gradient(&validated, sampler.gradient_at(1.0)),
        format_validated_gradient(&validated, sampler.gradient_at(1.002))
    );
    preview.render(1.0).unwrap();
    let revision = preview.widgets[&id].revision();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].revision(),
        revision,
        "triangle changes even when the text does not"
    );
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    let missing = preview.widgets[&id].pixels().clone();
    preview.render(3.0).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &missing);
    preview.replace_activity(&bytes).unwrap();
    preview.render(1.002).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,4],"trim_end_seconds":4,"gradient":[-3,-3]}"#,
        )
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    assert_eq!(sampler.gradient_at(-1.0), None);
    assert_eq!(sampler.gradient_at(5.0), None);
    for (field, malformed) in [
        ("show_triangle", Value::Null),
        ("triangle_width", json!(-1)),
        ("value_offset", Value::Null),
        ("unit_color", json!("invalid")),
        ("font_size", json!(0)),
        ("opacity", json!(2)),
        ("display_type", json!("arc")),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    request["widget"]["triangle_positive_color"] = json!("#123456");
    request["scene"]["opacity"] = json!(0);
    let revision = preview.widgets[&id].revision();
    preview
        .update_widget(id, serde_json::from_value(request).unwrap())
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert!(preview.widgets[&id].pixels().chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn gradient_resize_preserves_all_opposite_corners_during_playback_and_commits_fractional_intrinsic_fields(
) {
    let (mut preview, mut origin) = gradient_preview();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2,3],"gradient":[6,12,-6,0],"trim_end_seconds":3}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for (show_triangle, offset, scale) in
        [(true, 11.25, 1.7), (true, -17.5, 0.8), (false, 11.25, 1.7)]
    {
        origin["widget"]["show_triangle"] = json!(show_triangle);
        origin["widget"]["value_offset"] = json!(offset);
        origin["scene"]["scale"] = json!(scale);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.55, 1.234375, 12.0] {
                for second in [2.25, 3.0, 0.25] {
                    let mut gesture = origin.clone();
                    gesture.as_object_mut().unwrap().extend(
                        json!({
                            "factor": factor, "fixed": fixed, "point": point, "second": second,
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    );
                    let patch = serde_json::to_value(
                        preview
                            .resize_widget(serde_json::from_value(gesture).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        patch.as_object().unwrap().len(),
                        5,
                        "gradient only resizes its intrinsic fields and position"
                    );
                    if factor == 1.234375 {
                        assert_eq!(patch["font_size"], json!(59.25));
                        assert_eq!(patch["triangle_width"], json!(88.875));
                        assert_eq!(patch["value_offset"].as_f64().unwrap(), offset * factor);
                    }
                    assert!(patch["font_size"].as_f64().unwrap() <= 400.0);
                    assert!(patch["triangle_width"].as_f64().unwrap() <= 600.0);
                    let mut committed = origin.clone();
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(patch.as_object().unwrap().clone());
                    preview
                        .update_widget(id, serde_json::from_value(committed.clone()).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    let live_geometry = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&live_geometry).unwrap();
                    let actual = &actual["content"];
                    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
                        let corner = actual[position].as_f64().unwrap()
                            + actual[size].as_f64().unwrap() * fixed[axis];
                        assert!((corner - point[axis]).abs() < 0.002, "gradient corner moved: triangle={show_triangle} offset={offset} factor={factor} second={second}");
                    }
                    preview
                        .update_widget(id, serde_json::from_value(committed).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &live_geometry);
                }
            }
        }
    }
}

#[test]
fn time_preview_matches_native_composition_and_resize_at_the_current_second() {
    use ovrley_widgets::widgets::time::{
        format::{format_validated_time_parts, ElapsedTimeValues},
        validation::validate_time_value,
    };
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut request = json!({"type": "time", "widget": common::builders::speed_value_json(), "scene": common::seam::explicit_scene_json()});
    request["widget"].as_object_mut().unwrap().extend(
        json!({
            "value": "time", "font": "Inter.ttf", "format": "date-time-12s", "hours_offset": 1,
            "time_mode": "daytime", "elapsed_origin": "export", "show_hundredths": true,
            "x": 600.25, "y": 100.5, "icon_offset_x": -12, "icon_offset_y": 17,
            "prefix": "[", "suffix": "]", "show_icon": true,
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    request["scene"]["export_start_seconds"] = json!(10.0);
    request["scene"]["scale"] = json!(1.7);
    request["scene"]["shadow_strength"] = json!(3);
    request["scene"]["shadow_distance"] = json!(4);
    request["scene"]["border_thickness"] = json!(2);
    let activity = br#"{"sample_elapsed_seconds":[0,1,2],"time":["2025-04-21T13:05:00Z",null,"2025-04-21T13:05:02Z"],"sync_time":"2000-01-01T00:00:00Z","metadata":{"timezone":"Europe/Berlin"},"trim_end_seconds":2}"#;
    preview.replace_activity(activity).unwrap();
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for (mode, origin) in [
        ("daytime", "activity"),
        ("elapsed", "activity"),
        ("elapsed", "export"),
    ] {
        request["widget"]["time_mode"] = json!(mode);
        request["widget"]["elapsed_origin"] = json!(origin);
        for alignment in ["left", "center", "right"] {
            request["widget"]["content_alignment"] = json!(alignment);
            preview
                .update_widget(id, serde_json::from_value(request.clone()).unwrap())
                .unwrap();
            for (second, timestamp) in [
                (1.5, Some("2025-04-21T13:05:01.500Z")),
                (0.25, Some("2025-04-21T13:05:00.250Z")),
                (-1.0, None),
            ] {
                preview.render(second).unwrap();
                let widget = &preview.widgets[&id];
                let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
                let validated = validate_time_value(
                    serde_json::from_value(request["widget"].clone()).unwrap(),
                    0,
                )
                .unwrap();
                let scene = validate_scene_presentation(
                    serde_json::from_value(request["scene"].clone()).unwrap(),
                )
                .unwrap();
                let style = validated_value_style(&validated.base, &scene, scene.scale);
                let parts = format_validated_time_parts(
                    &validated,
                    timestamp,
                    ElapsedTimeValues {
                        activity_seconds: second,
                        export_seconds: second - 10.0,
                    },
                    Some(chrono_tz::Europe::Berlin),
                );
                let info = ImageInfo::new(
                    (
                        geometry["width"].as_i64().unwrap() as i32,
                        geometry["height"].as_i64().unwrap() as i32,
                    ),
                    ColorType::RGBA8888,
                    AlphaType::Premul,
                    None,
                );
                let mut surface = surfaces::raster(&info, None, None).unwrap();
                let canvas = surface.canvas();
                canvas.clear(Color::TRANSPARENT);
                canvas.translate((
                    -geometry["left"].as_f64().unwrap() as f32,
                    -geometry["top"].as_f64().unwrap() as f32,
                ));
                let fonts = NativeFonts(&font_dirs);
                let cached = draw_static_metric_parts_for_value(
                    canvas,
                    &validated.base,
                    &style,
                    scene.scale,
                    &fonts,
                )
                .unwrap();
                draw_metric_parts(
                    canvas,
                    &style,
                    &parts,
                    scene.scale,
                    &fonts,
                    cached,
                    &validated.base,
                )
                .unwrap();
                let mut pixels = vec![0; widget.pixels().len()];
                assert!(surface.read_pixels(
                    &info.with_alpha_type(AlphaType::Unpremul),
                    &mut pixels,
                    info.width() as usize * 4,
                    (0, 0)
                ));
                assert_eq!(
                    &pixels,
                    widget.pixels(),
                    "{mode}/{origin}/{alignment}/{second}"
                );
                assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
                let revision = widget.revision();
                preview.render(second).unwrap();
                assert_eq!(preview.widgets[&id].revision(), revision);
            }
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let bounds = &geometry["content"];
            for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
                let point = [
                    bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                    bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
                ];
                let mut gesture = request.clone();
                gesture.as_object_mut().unwrap().extend(
                    json!({"second": 1.75, "factor": 1.234375, "fixed": fixed, "point": point})
                        .as_object()
                        .unwrap()
                        .clone(),
                );
                let patch = serde_json::to_value(
                    preview
                        .resize_widget(serde_json::from_value(gesture).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let mut committed = request.clone();
                committed["widget"]
                    .as_object_mut()
                    .unwrap()
                    .extend(patch.as_object().unwrap().clone());
                preview
                    .update_widget(id, serde_json::from_value(committed).unwrap())
                    .unwrap();
                preview.render(1.75).unwrap();
                let resized: Value =
                    serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
                let actual = &resized["content"];
                assert!(
                    (actual["left"].as_f64().unwrap()
                        + actual["width"].as_f64().unwrap() * fixed[0]
                        - point[0])
                        .abs()
                        < 0.002
                );
                assert!(
                    (actual["top"].as_f64().unwrap()
                        + actual["height"].as_f64().unwrap() * fixed[1]
                        - point[1])
                        .abs()
                        < 0.002
                );
            }
        }
    }
    request["widget"]["time_mode"] = json!("daytime");
    preview
        .update_widget(id, serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    preview.render(0.5).unwrap();
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(0.5).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(activity).unwrap();
    preview.render(0.5).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    for (field, malformed) in [
        ("format", json!("unknown")),
        ("elapsed_origin", json!("unknown")),
        ("time_mode", json!("unknown")),
        ("hours_offset", Value::Null),
        ("show_hundredths", Value::Null),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    request["scene"]
        .as_object_mut()
        .unwrap()
        .remove("export_start_seconds");
    assert!(preview
        .create_widget(serde_json::from_value(request).unwrap())
        .is_err());
}

fn label_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    preview.fonts.register(
        ovrley_widgets::preview::FontRequest {
            font: "Inter.ttf".into(), font_weight: 537.0, italic: true,
        },
        &preview_font_resource(&font_dirs, "Inter.ttf", 537.0, true).unwrap(),
    ).unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(2);
    (preview, json!({
        "type": "label", "scene": scene,
        "widget": {
            "text": "Label e\u{301}", "x": 120.25, "y": 60.5,
            "font": "Inter.ttf", "font_size": 48, "font_weight": 537,
            "italic": true, "letter_spacing": -1.25, "color": "#abcdef", "opacity": 0.8,
        },
    }))
}

#[test]
fn label_pixels_match_native_and_only_config_changes_invalidate_the_static_revision() {
    use ovrley_widgets::widgets::label::{style::validated_label_style, validation::validate_label};
    let (mut preview, mut request) = label_preview();
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    let widget = &preview.widgets[&id];
    let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
    let label = validate_label(&serde_json::from_value(request["widget"].clone()).unwrap(), 0).unwrap();
    let scene = validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap()).unwrap();
    let info = ImageInfo::new(
        (geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32),
        ColorType::RGBA8888, AlphaType::Premul, None,
    );
    let mut surface = surfaces::raster(&info, None, None).unwrap();
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
    ovrley_core::render::text::draw_text(
        canvas, &label.text, &validated_label_style(&label, &scene, scene.scale),
        &[common::test_config::repo_git_root().join("fonts")],
    ).unwrap();
    let mut pixels = vec![0; widget.pixels().len()];
    assert!(surface.read_pixels(&info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0, 0)));
    assert_eq!(&pixels, widget.pixels());
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    let revision = widget.revision();
    let original_geometry = widget.geometry().clone();
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,1],"speed":[0,20],"trim_end_seconds":1}"#).unwrap();
    for second in [0.0, 1.0, 0.25, 50.0] {
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
        assert_eq!(preview.widgets[&id].pixels(), &pixels);
        assert_eq!(preview.widgets[&id].geometry(), &original_geometry);
    }
    preview.replace_activity(&[]).unwrap();
    preview.render(0.0).unwrap();
    assert_eq!(preview.widgets[&id].revision(), revision);
    request["widget"]["text"] = json!("Edited label");
    preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
    request["widget"]["font_size"] = json!(0);
    assert!(preview.update_widget(id, serde_json::from_value(request).unwrap()).is_err());
}

#[test]
fn label_intrinsic_resize_preserves_every_opposite_corner_and_fractional_size() {
    let (mut preview, origin) = label_preview();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
    for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
        let point = [
            bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
            bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
        ];
        let mut gesture = origin.clone();
        gesture["factor"] = json!(1.234375);
        gesture["fixed"] = json!(fixed);
        gesture["point"] = json!(point);
        gesture["second"] = json!(17.5);
        let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
        assert_eq!(patch["font_size"], json!(59.25));
        assert_eq!(patch.as_object().unwrap().len(), 3, "labels resize only x, y and font_size");
        let mut committed = origin.clone();
        committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
        let resized: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let actual = &resized["content"];
        let corner = [
            actual["left"].as_f64().unwrap() + actual["width"].as_f64().unwrap() * fixed[0],
            actual["top"].as_f64().unwrap() + actual["height"].as_f64().unwrap() * fixed[1],
        ];
        assert!((corner[0] - point[0]).abs() < 0.002);
        assert!((corner[1] - point[1]).abs() < 0.002);
    }
}

#[test]
fn standalone_preview_matches_native_cached_composition_and_replaces_state() {
    let root = common::test_config::repo_git_root();
    let font_dirs = vec![root.join("fonts")];
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(common::test_config::simple_config_path()).unwrap(),
    )
    .unwrap();
    let mut request =
        json!({"type": "speed", "widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
    request["widget"]["font"] = json!("Inter.ttf");
    request["widget"]["x"] = json!(600.25);
    request["widget"]["y"] = json!(100.5);
    request["scene"]["shadow_strength"] = json!(3);
    request["scene"]["shadow_distance"] = json!(4);
    request["scene"]["border_thickness"] = json!(2);
    request["scene"]["opacity"] = json!(0.5);
    request["widget"]["opacity"] = json!(0.8);
    request["widget"]["color"] = json!("#abcdef");
    let mut preview = PreviewSession::default();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: ovrley_widgets::model::standard_widgets::widget_font_weight(), italic: false },
            &preview_font_resource(&font_dirs, "Inter.ttf", ovrley_widgets::model::standard_widgets::widget_font_weight(), false).unwrap(),
        )
        .unwrap();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2],"speed":[0,null,20],"trim_end_seconds":2}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for alignment in ["left", "center", "right"] {
        request["widget"]["content_alignment"] = json!(alignment);
        preview
            .update_widget(id, serde_json::from_value(request.clone()).unwrap())
            .unwrap();
        for (second, speed) in [
            (0.5, Some(5.0)),
            (1.5, Some(15.0)),
            (-1.0, None),
            (0.2, Some(2.0)),
        ] {
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(&widget.geometry()).unwrap();
            let input: WidgetRequest = serde_json::from_value(request.clone()).unwrap();
            let validated = validate_value_widget(input.widget, 0).unwrap();
            let scene = validate_scene_presentation(input.scene).unwrap();
            let style = validated_value_style(&validated, &scene, scene.scale);
            assert_eq!(style.opacity, 0.4);
            assert_eq!(style.color.a(), 102);
            let info = ImageInfo::new(
                (
                    geometry["width"].as_i64().unwrap() as i32,
                    geometry["height"].as_i64().unwrap() as i32,
                ),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            );
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((
                -geometry["left"].as_f64().unwrap() as f32,
                -geometry["top"].as_f64().unwrap() as f32,
            ));
            let fonts = NativeFonts(&font_dirs);
            let cached =
                draw_static_metric_parts_for_value(canvas, &validated, &style, scene.scale, &fonts)
                    .unwrap();
            draw_metric_parts(
                canvas,
                &style,
                &format_metric_parts(
                    &validated,
                    &MetricSample {
                        numeric: speed,
                        ..MetricSample::default()
                    },
                ),
                scene.scale,
                &fonts,
                cached,
                &validated,
            )
            .unwrap();
            let mut pixels = vec![0; widget.pixels().len()];
            let read_info = info.with_alpha_type(AlphaType::Unpremul);
            assert!(surface.read_pixels(
                &read_info,
                &mut pixels,
                info.width() as usize * 4,
                (0, 0)
            ));
            assert_eq!(&pixels, widget.pixels(), "{alignment}, t={second}");
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(
                preview.widgets[&id].revision(), revision,
                "unchanged presentation reuses its pixels"
            );
        }
    }
    preview.replace_activity(&[]).unwrap();
    preview.render(0.5).unwrap();
    let empty = preview.widgets[&id].pixels().clone();
    preview.render(1.5).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &empty);
    request["scene"]["opacity"] = json!(0);
    preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
    preview.render(0.5).unwrap();
    assert!(preview.widgets[&id].pixels().chunks_exact(4).all(|pixel| pixel[3] == 0));
    request["widget"]["font_size"] = json!(0);
    assert!(preview
        .update_widget(id, serde_json::from_value(request).unwrap())
        .is_err());
}

#[test]
fn live_resize_keeps_the_opposite_content_corner_fixed_and_commits_the_same_geometry() {
    let root = common::test_config::repo_git_root();
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(common::test_config::simple_config_path()).unwrap(),
    )
    .unwrap();
    let mut origin =
        json!({"type": "speed", "widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
    origin["widget"]["font"] = json!("Inter.ttf");
    origin["widget"]["x"] = json!(600.25);
    origin["widget"]["y"] = json!(100.5);
    origin["widget"]["icon_offset_x"] = json!(-12);
    origin["widget"]["icon_offset_y"] = json!(17);
    origin["scene"]["scale"] = json!(1.7);
    origin["scene"]["shadow_strength"] = json!(5);
    origin["scene"]["shadow_distance"] = json!(8);
    let mut preview = PreviewSession::default();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: ovrley_widgets::model::standard_widgets::widget_font_weight(), italic: false },
            &preview_font_resource(&[root.join("fonts")], "Inter.ttf", ovrley_widgets::model::standard_widgets::widget_font_weight(), false).unwrap(),
        )
        .unwrap();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2],"speed":[0,10,40],"trim_end_seconds":2}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for alignment in ["left", "center", "right"] {
        origin["widget"]["content_alignment"] = json!(alignment);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(&preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.55, 1.8] {
                let mut gesture = origin.clone();
                gesture["factor"] = json!(factor);
                gesture["fixed"] = json!(fixed);
                gesture["point"] = json!(point);
                gesture["second"] = json!(1.5); // Telemetry may change while the gesture is active.
                let patch = serde_json::to_value(
                    preview
                        .resize_widget(serde_json::from_value(gesture).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let mut committed = origin.clone();
                committed["widget"]
                    .as_object_mut()
                    .unwrap()
                    .extend(patch.as_object().unwrap().clone());
                preview
                    .update_widget(id, serde_json::from_value(committed).unwrap())
                    .unwrap();
                preview.render(1.5).unwrap();
                let resized: Value =
                    serde_json::from_slice(&preview.widgets[&id].geometry()).unwrap();
                let actual = &resized["content"];
                let corner = [
                    actual["left"].as_f64().unwrap() + actual["width"].as_f64().unwrap() * fixed[0],
                    actual["top"].as_f64().unwrap() + actual["height"].as_f64().unwrap() * fixed[1],
                ];
                assert!(
                    (corner[0] - point[0]).abs() < 0.002,
                    "{alignment}: horizontal anchor moved"
                );
                assert!(
                    (corner[1] - point[1]).abs() < 0.002,
                    "{alignment}: vertical anchor moved"
                );
            }
        }
    }
}
