//! Canvas-parity regression tests for backend preview formatting.
//!
//! These tests guard the last-mile display contract between the React canvas
//! preview and the Rust preview-PNG renderer. The underlying interpolated value
//! may already match, final displayed metric still needs to follow the same
//! rounding rules to avoid visible one-unit mismatches.

mod common;

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use serde_json::json;

const SPEED_VALUE: &str = r##"{"value":"speed","x":0,"y":0,"display_unit":"kmh","decimals":0,"show_units":true,"font":"Arial.ttf","font_size":32.0,"color":"#ffffff","opacity":1.0,"show_icon":true,"icon_color":"#40e0d0","icon_size":28.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"unit_color":"#ffffff","prefix":"","suffix":""}"##;
const TORQUE_VALUE: &str = r##"{"value":"torque","x":0,"y":0,"display_unit":"nm","decimals":0,"show_units":true,"font":"Arial.ttf","font_size":32.0,"color":"#ffffff","opacity":1.0,"show_icon":true,"icon_color":"#40e0d0","icon_size":28.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"unit_color":"#ffffff","prefix":"","suffix":""}"##;

fn config_json(values_json: &str) -> ovrley_core::normalize::ValidatedRenderConfig {
    common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": serde_json::from_str::<serde_json::Value>(values_json).unwrap(),
        "plots": []
    }))
}

fn dense_with_speed_and_torque(speed: f64, torque: f64) -> DenseActivityReport {
    DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![],
        series: DenseSeriesReport {
            speed: vec![Some(speed)],
            elevation: vec![],
            gradient: vec![],
            heartrate: vec![],
            cadence: vec![],
            power: vec![],
            temperature: vec![],
            pace: vec![],
            g_force: vec![],
            air_pressure: vec![],
            ground_contact_time: vec![],
            left_right_balance: vec![],
            stride_length: vec![],
            stroke_rate: vec![],
            torque: vec![Some(torque)],
            vertical_speed: vec![],
            gear_position: vec![],
            vertical_ratio: vec![],
            vertical_oscillation: vec![],
            core_temperature: vec![],
            heading: vec![],
            course_lat: vec![],
            course_lon: vec![],
            time: vec![],
        },
    }
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

    assert_eq!(speed_parts.value_text, "31");
    assert_eq!(torque_parts.value_text, "19");
}
