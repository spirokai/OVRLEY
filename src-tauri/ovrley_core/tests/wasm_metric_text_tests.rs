//! Every manifest text metric crosses the real finalized-activity/font preview seam.
mod common;

use ovrley_core::{
    activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
    fonts::{preview_font_resource, NativeFonts},
};
use ovrley_widgets::{
    activity::ActivitySampler,
    model::{scene::validate_scene_presentation, standard_metrics::is_display_type_supported},
    preview::PreviewSession,
    render::text::validated_value_style,
    widgets::value::{
        format::format_metric_parts,
        layout::{draw_metric_parts, draw_static_metric_parts_for_value},
        preview::WidgetRequest,
        validation::validate_value_widget,
    },
};
use serde_json::{json, Value};
use skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};

fn manifest() -> Value {
    serde_json::from_str(include_str!("../../../assets/standard-metrics.json")).unwrap()
}

fn request(metric: &str, unit: &str) -> Value {
    let mut widget = common::builders::speed_value_json();
    widget["value"] = json!(metric);
    widget["display_unit"] = json!(unit);
    widget["font"] = json!("Inter.ttf");
    widget["font_size"] = json!(32);
    widget["x"] = json!(600.25);
    widget["y"] = json!(100.5);
    widget["prefix"] = json!("[");
    widget["suffix"] = json!("]");
    widget["icon_offset_x"] = json!(-9.5);
    widget["icon_offset_y"] = json!(11.25);
    match metric {
        "distance" => widget["show_full_distance"] = json!(true),
        "total_ascent" => widget["show_full_ascent"] = json!(true),
        "gps_coordinates" => widget["coordinate_format"] = json!("dms"),
        "left_right_balance" => widget["balance_format"] = json!("percent_label"),
        _ => {}
    }
    let mut scene = common::seam::explicit_scene_json();
    scene["end"] = json!(3);
    scene["scale"] = json!(1.3);
    scene["border_thickness"] = json!(1.5);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    json!({"type": metric, "widget": widget, "scene": scene})
}

#[test]
fn every_text_metric_unit_matches_native_sampling_formatting_and_pixels() {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: ovrley_widgets::model::standard_widgets::widget_font_weight(), italic: false },
            &preview_font_resource(&font_dirs, "Inter.ttf", ovrley_widgets::model::standard_widgets::widget_font_weight(), false).unwrap(),
        )
        .unwrap();
    for definition in manifest()["definitions"].as_array().unwrap() {
        let kind = serde_json::from_value(definition["type"].clone()).unwrap();
        if !is_display_type_supported(kind, "text") {
            continue;
        }
        let metric = definition["type"].as_str().unwrap();
        let mut activity = json!({
            "sample_elapsed_seconds": [0,1,2,3], "trim_end_seconds": 3,
            "course": [[0,-8], [null,null], [8,0], [9,9]],
            "gear_position": ["0",null,"3","4"],
            "barometric_altitude": [100,null,120,130], "elevation": [900,910,920,930],
        });
        if !matches!(metric, "gps_coordinates" | "gear_position" | "altitude") {
            activity[metric] = json!([2, null, 8, 12]);
        }
        let parsed: ParsedActivity = serde_json::from_value(activity).unwrap();
        let bytes = serde_json::to_vec(&parsed).unwrap();
        let sampler = ActivitySampler::from_json(&bytes).unwrap();
        preview.replace_activity(&bytes).unwrap();
        for unit in definition["supportedDisplayUnits"].as_array().unwrap() {
            let mut request = request(metric, unit["value"].as_str().unwrap());
            let config = common::seam::validated_config_from_value(json!({
                "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
            }));
            let requirements = config.render_data_requirements();
            let trimmed = trim_activity(&parsed, 0.0, 3.0, &requirements).unwrap();
            let seconds = vec![0.0, 0.5, 1.0, 1.5, 2.5];
            let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
            for alignment in ["left", "center", "right"] {
                request["widget"]["content_alignment"] = json!(alignment);
                let id = preview
                    .create_widget(serde_json::from_value(request.clone()).unwrap())
                    .unwrap();
                let input: WidgetRequest = serde_json::from_value(request.clone()).unwrap();
                let validated = validate_value_widget(input.widget, 0).unwrap();
                let scene = validate_scene_presentation(input.scene).unwrap();
                let style = validated_value_style(&validated, &scene, scene.scale);
                // Include backward seeks after sequential playback.
                for index in [0, 1, 2, 3, 4, 1] {
                    let second = seconds[index];
                    let expected =
                        ovrley_widgets::widgets::value::format::format_metric_parts(&validated, &dense.metric_sample(validated.metric, index, 0.0));
                    let sampled =
                        format_metric_parts(&validated, &sampler.sample(&validated, second));
                    assert_eq!(sampled, expected, "{metric}/{unit:?} at {second}");
                    preview.render(second).unwrap();
                    let widget = &preview.widgets[&id];
                    let geometry: Value = serde_json::from_slice(&widget.geometry()).unwrap();
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
                        &validated,
                        &style,
                        scene.scale,
                        &fonts,
                    )
                    .unwrap();
                    draw_metric_parts(
                        canvas,
                        &style,
                        &expected,
                        scene.scale,
                        &fonts,
                        cached,
                        &validated,
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
                        &pixels, widget.pixels(),
                        "pixels: {metric}/{unit:?}/{alignment} at {second}"
                    );
                    assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
                    let revision = widget.revision();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].revision(), revision);
                }
                preview.widgets.remove(&id);
            }
        }
    }
}

#[test]
fn sample_policies_altitude_totals_and_activity_removal_preserve_display_semantics() {
    let bytes = br#"{"sample_elapsed_seconds":[0,1,2,3,4],"trim_end_seconds":4,
        "power":[null,100,null,300,null],"speed":[null,10,null,30,null],
        "iso":[null,100,null,400,null],"gear_position":[null,"0",null,"3",null],
        "barometric_altitude":[null,100,null,120,null],"elevation":[900,910,920,930,940],
        "distance":[0,10,20,30,40],"total_ascent":[0,1,2,3,4]}"#;
    let sampler = ActivitySampler::from_json(bytes).unwrap();
    for (metric, unit, second, expected) in [
        ("power", "w", 0.0, "--"),
        ("power", "w", 2.0, "0"),
        ("power", "w", 1.5, "50"),
        ("power", "w", 4.0, "--"),
        ("speed", "mps", 2.0, "20"),
        ("iso", "iso", 2.0, "100"),
        ("gear_position", "gear", 0.0, "--"),
        ("gear_position", "gear", 2.0, "N"),
        ("distance", "m", 2.0, "20/40"),
        ("total_ascent", "m", 2.0, "2/4"),
    ] {
        let mut input = request(metric, unit);
        input["widget"]["decimals"] = json!(0);
        let widget =
            validate_value_widget(serde_json::from_value(input["widget"].clone()).unwrap(), 0)
                .unwrap();
        assert_eq!(
            format_metric_parts(&widget, &sampler.sample(&widget, second))
                .standard_text()
                .0,
            format!("[{expected}]")
        );
        assert_eq!(
            format_metric_parts(&widget, &sampler.sample(&widget, -1.0))
                .standard_text()
                .0,
            "[--]"
        );
        assert_eq!(
            format_metric_parts(&widget, &sampler.sample(&widget, 5.0))
                .standard_text()
                .0,
            "[--]"
        );
    }
    let mut input = request("altitude", "ft");
    input["widget"]["starting_altitude"] = json!(3280.84);
    let widget =
        validate_value_widget(serde_json::from_value(input["widget"].clone()).unwrap(), 0).unwrap();
    assert!((sampler.sample(&widget, 2.0).numeric.unwrap() - 1010.0).abs() < 1e-6);
    assert!(ActivitySampler::from_json(
        br#"{"sample_elapsed_seconds":[0,1],"power":[1],"trim_end_seconds":1}"#
    )
    .is_err());
}
