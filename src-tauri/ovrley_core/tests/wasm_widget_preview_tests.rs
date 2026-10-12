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

#[test]
fn standalone_preview_matches_native_cached_composition_and_replaces_state() {
    let root = common::test_config::repo_git_root();
    let font_dirs = vec![root.join("fonts")];
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(common::test_config::simple_config_path()).unwrap(),
    )
    .unwrap();
    let mut request =
        json!({"widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
    request["widget"]["font"] = json!("Inter.ttf");
    request["widget"]["x"] = json!(600.25);
    request["widget"]["y"] = json!(100.5);
    request["scene"]["shadow_strength"] = json!(3);
    request["scene"]["shadow_distance"] = json!(4);
    request["scene"]["border_thickness"] = json!(2);
    let mut preview = PreviewSession::default();
    preview
        .fonts
        .register(
            "Inter.ttf".into(),
            &preview_font_resource(&font_dirs, "Inter.ttf").unwrap(),
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
            let geometry: Value = serde_json::from_slice(&widget.geometry).unwrap();
            let input: WidgetRequest = serde_json::from_value(request.clone()).unwrap();
            let validated = validate_value_widget(input.widget, 0).unwrap();
            let scene = validate_scene_presentation(input.scene).unwrap();
            let style = validated_value_style(&validated, &scene, scene.scale);
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
            let mut pixels = vec![0; widget.pixels.len()];
            let read_info = info.with_alpha_type(AlphaType::Unpremul);
            assert!(surface.read_pixels(
                &read_info,
                &mut pixels,
                info.width() as usize * 4,
                (0, 0)
            ));
            assert_eq!(pixels, widget.pixels, "{alignment}, t={second}");
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            let revision = widget.revision;
            preview.render(second).unwrap();
            assert_eq!(
                preview.widgets[&id].revision, revision,
                "unchanged presentation reuses its pixels"
            );
        }
    }
    preview.replace_activity(&[]).unwrap();
    preview.render(0.5).unwrap();
    let empty = preview.widgets[&id].pixels.clone();
    preview.render(1.5).unwrap();
    assert_eq!(preview.widgets[&id].pixels, empty);
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
        json!({"widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
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
            "Inter.ttf".into(),
            &preview_font_resource(&[root.join("fonts")], "Inter.ttf").unwrap(),
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
        let geometry: Value = serde_json::from_slice(&preview.widgets[&id].geometry).unwrap();
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
                    serde_json::from_slice(&preview.widgets[&id].geometry).unwrap();
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
