//! Canvas-parity regression tests for backend preview formatting.
//!
//! These tests guard the last-mile display contract between the React canvas
//! preview and the Rust preview-PNG renderer. The underlying interpolated value
//! may already match, final displayed metric still needs to follow the same
//! rounding rules to avoid visible one-unit mismatches.

mod common;

use ovrley_core::activity::schema::DenseActivityReport;
use serde_json::json;

const SPEED_VALUE: &str = r##"{"value":"speed","x":0,"y":0,"display_unit":"kmh","decimals":0,"show_units":true,"font":"Arial.ttf","font_size":32.0,"color":"#ffffff","opacity":1.0,"show_icon":true,"icon_color":"#40e0d0","icon_size":28.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"unit_color":"#ffffff","prefix":"","suffix":""}"##;
const TORQUE_VALUE: &str = r##"{"value":"torque","x":0,"y":0,"display_unit":"nm","decimals":0,"show_units":true,"font":"Arial.ttf","font_size":32.0,"color":"#ffffff","opacity":1.0,"show_icon":true,"icon_color":"#40e0d0","icon_size":28.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"unit_color":"#ffffff","prefix":"","suffix":""}"##;
const ENGINE_POWER_VALUE: &str = r##"{"value":"engine_power","x":0,"y":0,"display_unit":"w","decimals":0,"show_units":true,"font":"Arial.ttf","font_size":32.0,"color":"#ffffff","opacity":1.0,"show_icon":true,"icon_color":"#40e0d0","icon_size":28.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"unit_color":"#ffffff","prefix":"","suffix":""}"##;

fn config_json(values_json: &str) -> ovrley_core::normalize::ValidatedRenderConfig {
    common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": serde_json::from_str::<serde_json::Value>(values_json).unwrap(),
        "plots": []
    }))
}

fn dense_with_speed_and_torque(speed: f64, torque: f64) -> DenseActivityReport {
    common::builders::dense_report_with(|s| {
        s.speed = vec![Some(speed)];
        s.torque = vec![Some(torque)];
    })
}

#[test]
fn zero_decimal_standard_metrics_round_like_canvas_preview() {
    let config = config_json(&format!("[{}, {}]", SPEED_VALUE, TORQUE_VALUE));
    let dense = dense_with_speed_and_torque(8.5, 18.6);

    let mut values = config.values.into_iter();
    let speed_validated = common::seam::expect_standard_value(values.next().unwrap(), 0);
    let torque_validated = common::seam::expect_standard_value(values.next().unwrap(), 1);

    let speed_parts =
        ovrley_core::render::format::format_validated_metric_parts(&speed_validated, &dense, 0)
            .unwrap();
    let torque_parts =
        ovrley_core::render::format::format_validated_metric_parts(&torque_validated, &dense, 0)
            .unwrap();

    assert_eq!(speed_parts.standard_text().0, "31");
    assert_eq!(torque_parts.standard_text().0, "19");
}

#[test]
fn engine_power_uses_the_standard_numeric_renderer_lookup() {
    let config = config_json(&format!("[{}]", ENGINE_POWER_VALUE));
    let dense = common::builders::dense_report_with(|s| {
        s.engine_power = vec![Some(12_345.6)];
    });
    let engine_power = common::seam::expect_standard_value(config.values.into_iter().next().unwrap(), 0);

    let parts =
        ovrley_core::render::format::format_validated_metric_parts(&engine_power, &dense, 0)
            .unwrap();

    assert_eq!(parts.standard_text().0, "12346");
}
