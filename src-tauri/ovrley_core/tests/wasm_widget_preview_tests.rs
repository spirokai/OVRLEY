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
