//! Shearwater extraction and the shared preview/export scuba metric contract.
mod common;

use ovrley_core::activity::{
    csv::parse_csv_activity_reader, interpolate::densify_activity, schema::ParsedActivity,
    trim::trim_activity,
};
use ovrley_widgets::{
    activity::ActivitySampler,
    widgets::value::{
        format::{format_metric_parts, format_number, MetricSample},
        validation::{validate_value_widget, ValidatedValueWidget},
    },
};
use serde_json::{json, Value};

const TYPES: [&str; 7] = [
    "depth",
    "first_stop_depth",
    "first_stop_time",
    "time_to_surface",
    "average_ppo2",
    "gas_mix",
    "water_temperature",
];
const HEADER: &str = "Time (sec),Depth,First Stop Depth,Time To Surface (min),Average PPO2,Fraction O2,Fraction He,First Stop Time,Water Temp\n";

fn parse(csv: &str) -> ParsedActivity {
    parse_csv_activity_reader(csv.as_bytes(), "Shearwater.csv")
        .unwrap()
        .parsed_activity
}

fn widget(kind: &str) -> Value {
    let widgets: Value =
        serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    let metrics: Value =
        serde_json::from_str(include_str!("../../../assets/standard-metrics.json")).unwrap();
    let definition = metrics["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["type"] == kind)
        .unwrap();
    let mut widget = widgets["dataDefaults"]["value"].clone();
    widget
        .as_object_mut()
        .unwrap()
        .extend(definition["defaults"].as_object().unwrap().clone());
    widget["value"] = json!(kind);
    widget["font"] = json!("Inter.ttf");
    widget["font_size"] = json!(32);
    widget
}

fn validated(kind: &str) -> ValidatedValueWidget {
    validate_value_widget(serde_json::from_value(widget(kind)).unwrap(), 0).unwrap()
}

fn text(widget: &ValidatedValueWidget, sample: &MetricSample<'_>) -> String {
    format_metric_parts(widget, sample)
        .standard_text()
        .0
        .to_string()
}

#[test]
fn rounding_ties_ignore_interpolation_noise_without_changing_nearby_or_large_values() {
    for value in [1.275, 1.275 - f64::EPSILON, 1.275 + f64::EPSILON] {
        assert_eq!(format_number(value, 2), "1.28");
        assert_eq!(format_number(-value, 2), "-1.28");
    }
    assert_eq!(format_number(1.2749, 2), "1.27");
    assert_eq!(format_number(-1.2749, 2), "-1.27");
    assert_eq!(format_number(1e15, 0), "1000000000000000");
    assert_eq!(format_number(1e15, 2), "1000000000000000.00");
}

#[test]
fn shearwater_fixture_preserves_every_requested_observation_and_announces_seven_metrics() {
    let csv = include_str!("fixtures/activity/Shearwater.csv");
    let activity = parse(csv);
    assert_eq!(activity.sample_elapsed_seconds.len(), 1891);
    assert_eq!(activity.trim_end_seconds, 3780.0);
    let raw = csv.lines().skip(3).collect::<Vec<_>>().join("\n");
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(raw.as_bytes());
    for (index, record) in reader.records().enumerate() {
        let record = record.unwrap();
        for (column, series) in [
            (1, &activity.depth),
            (2, &activity.first_stop_depth),
            (3, &activity.time_to_surface),
            (4, &activity.average_ppo2),
            (5, &activity.fraction_o2),
            (6, &activity.fraction_he),
            (7, &activity.first_stop_time),
            (11, &activity.water_temperature),
        ] {
            assert_eq!(
                series[index],
                Some(record[column].parse::<f64>().unwrap()),
                "row {index}, column {column}"
            );
        }
    }
    let json = serde_json::to_value(&activity).unwrap();
    for kind in TYPES {
        assert!(
            json["extended_attributes"]
                .as_array()
                .unwrap()
                .contains(&json!(kind)),
            "{kind}"
        );
    }
    assert_eq!(activity.first_stop_depth.first(), Some(&Some(0.0)));
    assert!(
        activity.temperature.is_empty(),
        "water temperature has its own canonical column"
    );
}

#[test]
fn predictions_and_gas_switches_hold_while_physical_measurements_interpolate_in_both_hosts() {
    let activity = parse(&format!(
        "{HEADER}\
        0,40,9,10,1.20,0.21,0.35,2,20\n\
        2,42,6,12,1.30,0.50,0,4,18\n\
        4,N/A,N/A,N/A,N/A,0.99,N/A,N/A,N/A\n\
        6,44,0,0,1.00,0.99,0,0,16\n"
    ));
    assert_eq!(activity.fraction_o2[2], None);
    assert_eq!(activity.fraction_he[2], None);
    let widgets = TYPES.map(widget);
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(), "values": widgets,
    }));
    let requirements = config.render_data_requirements();
    assert!(requirements.fraction_o2 && requirements.fraction_he);
    let trimmed = trim_activity(&activity, 0.5, 6.0, &requirements).unwrap();
    let seconds = vec![0.0, 1.0, 1.5, 3.5, 5.5];
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let preview = ActivitySampler::from_json(&serde_json::to_vec(&activity).unwrap()).unwrap();
    let expected = ["40.5", "9.0", "2", "10", "1.23", "21/35", "20"];
    for (kind_index, kind) in TYPES.iter().enumerate() {
        let widget = validated(kind);
        for (index, second) in seconds.iter().enumerate() {
            let native = dense.metric_sample(widget.metric, index, 0.0);
            let preview = preview.sample(&widget, second + 0.5);
            assert_eq!(
                text(&widget, &native),
                text(&widget, &preview),
                "{kind} at {second}"
            );
            if index == 0 {
                assert_eq!(text(&widget, &native), expected[kind_index]);
            }
        }
    }
    let gas = validated("gas_mix");
    assert_eq!(text(&gas, &preview.sample(&gas, 1.999)), "21/35");
    assert_eq!(text(&gas, &preview.sample(&gas, 2.0)), "50/00");
    assert_eq!(text(&gas, &preview.sample(&gas, 4.0)), "50/00");
    assert_eq!(text(&gas, &preview.sample(&gas, 6.0)), "99/00");
    assert_eq!(text(&gas, &preview.sample(&gas, 6.001)), "--/--");
}

#[test]
fn recorded_zero_values_remain_available_for_dives_without_decompression_stops() {
    let activity = parse(&format!(
        "{HEADER}\
        0,0,0,0,0,0.21,0,0,0\n\
        2,0,0,0,0,0.21,0,0,0\n"
    ));
    let json = serde_json::to_value(&activity).unwrap();
    for kind in TYPES {
        assert!(
            json["extended_attributes"]
                .as_array()
                .unwrap()
                .contains(&json!(kind)),
            "{kind}"
        );
    }
}

#[test]
fn gas_observations_are_atomic_and_invalid_external_values_remain_missing() {
    let activity = parse(&format!(
        "{HEADER}\
        0,-1,-3,-1,-0.2,0.8,0.5,-2,-2\n\
        2,20,6,10,1.2,0.18,0.45,3,18\n\
        2,20,6,10,1.2,0.50,N/A,3,18\n\
        4,NaN,0,1.5,NaN,1.1,-0.1,0.5,N/A\n"
    ));
    assert_eq!(activity.depth, [None, Some(20.0), None]);
    assert_eq!(activity.first_stop_depth, [None, Some(6.0), Some(0.0)]);
    assert_eq!(activity.time_to_surface, [None, Some(10.0), None]);
    assert_eq!(activity.first_stop_time, [None, Some(3.0), None]);
    assert_eq!(activity.average_ppo2, [None, Some(1.2), None]);
    assert_eq!(activity.fraction_o2, [None, Some(0.18), None]);
    assert_eq!(activity.fraction_he, [None, Some(0.45), None]);
    assert_eq!(activity.water_temperature, [Some(-2.0), Some(18.0), None]);
    let gas = validated("gas_mix");
    assert_eq!(
        text(
            &gas,
            &MetricSample {
                numeric: Some(0.21),
                ..Default::default()
            }
        ),
        "--/--"
    );
}

#[test]
fn imperial_source_units_and_common_display_units_convert_without_relabeling_values() {
    let activity = parse(&format!(
        "Dive Number,Imperial Units\n101,True\n{HEADER}\
        0,100,20,10,1.2,0.21,0,3,68\n\
        2,100,20,10,1.2,0.21,0,3,68\n"
    ));
    assert_eq!(activity.depth, [Some(30.48), Some(30.48)]);
    assert_eq!(activity.first_stop_depth, [Some(6.096), Some(6.096)]);
    assert_eq!(activity.water_temperature, [Some(20.0), Some(20.0)]);
    for (kind, unit, raw, expected, label) in [
        ("depth", "m", 30.48, "30.5", "M"),
        ("depth", "ft", 30.48, "100.0", "FT"),
        ("first_stop_depth", "m", 6.0, "6.0", "M"),
        ("first_stop_depth", "ft", 6.0, "19.7", "FT"),
        ("first_stop_time", "min", 3.0, "3", "MIN"),
        ("time_to_surface", "min", 10.0, "10", "MIN"),
        ("average_ppo2", "ata", 1.2, "1.20", "ATM"),
        ("average_ppo2", "bar", 1.2, "1.22", "BAR"),
        ("water_temperature", "fahrenheit", 21.0, "70", "°F"),
    ] {
        let mut config = widget(kind);
        config["display_unit"] = json!(unit);
        let widget = validate_value_widget(serde_json::from_value(config).unwrap(), 0).unwrap();
        let parts = format_metric_parts(
            &widget,
            &MetricSample {
                numeric: Some(raw),
                ..Default::default()
            },
        );
        assert_eq!(parts.standard_text(), (expected, Some(label)));
    }
}

#[test]
fn all_seven_icons_have_geometry_in_the_native_svg_subset() {
    for kind in TYPES {
        let widget = validated(kind);
        let parts = format_metric_parts(&widget, &MetricSample::default());
        let bounds = ovrley_widgets::render::icons::metric_icon_bounds(
            parts.icon_asset_file.unwrap(),
            skia_safe::Rect::from_xywh(0.0, 0.0, 24.0, 24.0),
        );
        assert!(
            bounds.width() > 12.0 && bounds.height() > 12.0,
            "{kind}: {bounds:?}"
        );
    }
}

#[test]
fn dive_times_allow_decimals_but_show_whole_minutes_by_default() {
    let sample = MetricSample {
        numeric: Some(3.0),
        ..Default::default()
    };
    for kind in ["first_stop_time", "time_to_surface"] {
        assert_eq!(text(&validated(kind), &sample), "3");
        let mut config = widget(kind);
        config["decimals"] = json!(1);
        let widget = validate_value_widget(serde_json::from_value(config).unwrap(), 0).unwrap();
        assert_eq!(text(&widget, &sample), "3.0");
    }
}

#[test]
fn scuba_widgets_render_in_the_preview_cache_and_export_frame_with_matching_pixels() {
    use ovrley_core::{
        debug::RenderProfiler,
        fonts::preview_font_resource,
        paths::AppPaths,
        render::render_full_frames,
    };
    use ovrley_widgets::preview::{FontRequest, PreviewSession};

    let activity = parse(include_str!("fixtures/activity/Shearwater.csv"));
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let font = preview_font_resource(&paths.font_dirs, "Inter.ttf", 700.0, false).unwrap();
    for kind in TYPES {
        let widget = widget(kind);
        let mut scene = common::seam::explicit_scene_json();
        scene["end"] = json!(3780);
        scene["width"] = json!(700);
        scene["height"] = json!(500);
        let config =
            common::seam::validated_config_from_value(json!({"scene": scene, "values": [widget]}));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 3780.0, &requirements).unwrap();
        let seconds = vec![0.0, 400.5, 1800.0, 3500.0, 3780.0];
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(
            &paths.font_dirs,
            &config,
            &activity,
            &dense,
            &[0, 1, 2, 3, 4],
            &mut RenderProfiler::default(),
        )
        .unwrap();
        let mut preview = PreviewSession::default();
        let request: FontRequest = serde_json::from_value(json!({"font": "Inter.ttf"})).unwrap();
        preview.fonts.register(request, &font).unwrap();
        preview.replace_activity(&bytes).unwrap();
        let id = preview
            .create_widget(
                serde_json::from_value(json!({
                    "type": kind, "widget": widget, "scene": scene,
                }))
                .unwrap(),
            )
            .unwrap();
        for index in [0, 1, 2, 3, 4, 1, 0] {
            preview.render(seconds[index]).unwrap();
            let rendered = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(rendered.geometry()).unwrap();
            let (width, height, left, top) = (
                geometry["width"].as_u64().unwrap() as usize,
                geometry["height"].as_u64().unwrap() as usize,
                geometry["left"].as_f64().unwrap() as usize,
                geometry["top"].as_f64().unwrap() as usize,
            );
            let native = &frames[index];
            let mut difference = 0;
            for (y, row) in rendered
                .pixels()
                .chunks_exact(width * 4)
                .take(height)
                .enumerate()
            {
                let offset = ((y + top) * 700 + left) * 4;
                for (a, b) in native[offset..offset + row.len()]
                    .chunks_exact(4)
                    .zip(row.chunks_exact(4))
                {
                    difference = difference.max(a[3].abs_diff(b[3]) as u16);
                    for channel in 0..3 {
                        difference = difference.max(
                            (a[channel] as u16 * a[3] as u16)
                                .abs_diff(b[channel] as u16 * b[3] as u16)
                                / 255,
                        );
                    }
                }
            }
            assert!(
                difference <= 8,
                "{kind} at {}: pixel difference {difference}",
                seconds[index]
            );
            assert!(rendered.pixels().chunks_exact(4).any(|pixel| pixel[3] > 0));
            let revision = rendered.revision();
            preview.render(seconds[index]).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
        }
    }
}
