//! Canvas-parity regression tests for backend preview formatting.
//!
//! These tests guard the last-mile display contract between the React canvas
//! preview and the Rust preview-PNG renderer. The underlying interpolated value
//! may already match, but the final displayed metric still needs to follow the
//! same rounding rules to avoid visible one-unit mismatches.

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use ovrley_core::config::parse_config_json;
use ovrley_core::render::format::format_metric_parts;

fn config_json(values_json: &str) -> ovrley_core::config::RenderConfig {
    let input = format!(
        r#"{{
        "scene": {{"fps": 30, "start": 0, "end": 10}},
        "values": {values_json}
    }}"#
    );
    parse_config_json(&input).unwrap()
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
    let config = config_json(
        r#"[
          {"value":"speed","x":0,"y":0,"display_unit":"kmh","decimals":0,"show_units":true},
          {"value":"torque","x":0,"y":0,"display_unit":"nm","decimals":0,"show_units":true}
        ]"#,
    );
    let dense = dense_with_speed_and_torque(8.5, 18.6);

    let speed_parts = format_metric_parts(&config, &config.values[0], &dense, 0).unwrap();
    let torque_parts = format_metric_parts(&config, &config.values[1], &dense, 0).unwrap();

    assert_eq!(speed_parts.value_text, "31");
    assert_eq!(torque_parts.value_text, "19");
}
