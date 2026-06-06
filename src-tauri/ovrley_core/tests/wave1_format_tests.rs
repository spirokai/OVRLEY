//! Wave 1 standard metric formatting behavior tests.
//!
//! Verifies all nine Wave 1 standard metric widget types format correctly
//! through the public `format_validated_metric_parts` API, including unit
//! conversion, placeholder fallback, and left/right balance format variants.
//!
//! ## Type
//! Integration-adjacent test. Constructs config and activity data in memory.
//! No I/O, no fixtures.

mod common;

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use ovrley_core::render::format::format_validated_metric_parts;
use serde_json::json;

fn config_json(values_json: &str) -> ovrley_core::normalize::ValidatedRenderConfig {
    common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": serde_json::from_str::<serde_json::Value>(values_json).unwrap(),
        "plots": []
    }))
}

fn value_config_json(value_type: &str, overrides: &[(&str, &str)]) -> String {
    let mut fields = format!(
        "\"value\": \"{}\", \"x\": 100, \"y\": 100, \"font\": \"Arial.ttf\", \"font_size\": 32.0, \
         \"color\": \"#ffffff\", \"opacity\": 1.0, \"show_icon\": true, \"icon_color\": \"#40e0d0\", \
         \"icon_size\": 28.0, \"icon_offset_x\": 0.0, \"icon_offset_y\": 0.0, \
         \"unit_color\": \"#ffffff\", \"prefix\": \"\", \"suffix\": \"\"",
        value_type
    );
    for (key, val) in overrides {
        fields.push_str(&format!(", \"{}\": {}", key, val));
    }
    format!("{{{}}}", fields)
}

fn activity_for(series_key: &str, raw: Option<f64>) -> DenseActivityReport {
    let mut s = DenseSeriesReport {
        speed: vec![],
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
        torque: vec![],
        vertical_speed: vec![],
        gear_position: vec![],
        vertical_ratio: vec![],
        vertical_oscillation: vec![],
        core_temperature: vec![],
        heading: vec![],
        course_lat: vec![],
        course_lon: vec![],
        time: vec![],
    };
    let series = vec![raw];
    match series_key {
        "pace" => s.pace = series,
        "g_force" => s.g_force = series,
        "air_pressure" => s.air_pressure = series,
        "ground_contact_time" => s.ground_contact_time = series,
        "left_right_balance" => s.left_right_balance = series,
        "stride_length" => s.stride_length = series,
        "stroke_rate" => s.stroke_rate = series,
        "torque" => s.torque = series,
        "vertical_speed" => s.vertical_speed = series,
        "gear_position" => s.gear_position = series,
        "vertical_oscillation" => s.vertical_oscillation = series,
        _ => {}
    }
    DenseActivityReport {
        series: s,
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![],
    }
}

fn format_parts(
    kind: &str,
    series_key: &str,
    raw: Option<f64>,
    overrides: &[(&str, &str)],
) -> (String, Option<String>) {
    let mut defaults: Vec<(&str, &str)> = overrides.to_vec();
    if !overrides.iter().any(|(k, _)| *k == "show_units") {
        defaults.push(("show_units", "false"));
    }
    if !overrides.iter().any(|(k, _)| *k == "decimals") {
        defaults.push(("decimals", "0"));
    }
    if !overrides.iter().any(|(k, _)| *k == "balance_format") && kind == "left_right_balance" {
        defaults.push(("balance_format", r#""plain""#));
    }
    let vc_json = value_config_json(kind, &defaults);
    let config = config_json(&format!("[{}]", vc_json));
    let validated =
        common::seam::expect_standard_value(config.values.into_iter().next().unwrap(), 0);
    let report = activity_for(series_key, raw);
    let parts = format_validated_metric_parts(&validated, &report, 0);
    match parts {
        Some(p) => (p.value_text, p.unit_text),
        None => ("--".to_string(), None),
    }
}

#[test]
fn pace_formats_as_min_km() {
    let (value, unit) = format_parts(
        "pace",
        "pace",
        Some(275.0),
        &[("display_unit", r#""min_per_km""#), ("show_units", "true")],
    );
    assert_eq!(value, "4:35");
    assert_eq!(unit, Some("MIN/KM".to_string()));
}

#[test]
fn pace_formats_as_min_mi() {
    let (value, unit) = format_parts(
        "pace",
        "pace",
        Some(275.0),
        &[("display_unit", r#""min_per_mi""#), ("show_units", "true")],
    );
    assert_eq!(value, "7:23");
    assert_eq!(unit, Some("MIN/MI".to_string()));
}

#[test]
fn g_force_formats_as_g() {
    let (value, unit) = format_parts(
        "g_force",
        "g_force",
        Some(1.5),
        &[
            ("display_unit", r#""g""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "1.5");
    assert_eq!(unit, Some("G".to_string()));
}

#[test]
fn g_force_converts_to_mps2() {
    let (value, unit) = format_parts(
        "g_force",
        "g_force",
        Some(1.0),
        &[
            ("display_unit", r#""mps2""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "9.8");
    assert_eq!(unit, Some("M/S^2".to_string()));
}

#[test]
fn air_pressure_formats_as_hpa() {
    let (value, unit) = format_parts(
        "air_pressure",
        "air_pressure",
        Some(1.0),
        &[("display_unit", r#""hpa""#), ("show_units", "true")],
    );
    assert_eq!(value, "1000");
    assert_eq!(unit, Some("HPA".to_string()));
}

#[test]
fn air_pressure_converts_to_inhg() {
    let (value, unit) = format_parts(
        "air_pressure",
        "air_pressure",
        Some(1.0),
        &[("display_unit", r#""inhg""#), ("show_units", "true")],
    );
    assert_eq!(value, "30");
    assert_eq!(unit, Some("INHG".to_string()));
}

#[test]
fn ground_contact_time_formats_as_ms() {
    let (value, unit) = format_parts(
        "ground_contact_time",
        "ground_contact_time",
        Some(250.0),
        &[("display_unit", r#""ms""#), ("show_units", "true")],
    );
    assert_eq!(value, "250");
    assert_eq!(unit, Some("MS".to_string()));
}

#[test]
fn stride_length_formats_as_m() {
    let (value, unit) = format_parts(
        "stride_length",
        "stride_length",
        Some(1.25),
        &[
            ("display_unit", r#""m""#),
            ("decimals", "2"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "1.25");
    assert_eq!(unit, Some("M".to_string()));
}

#[test]
fn stroke_rate_formats_as_spm() {
    let (value, unit) = format_parts(
        "stroke_rate",
        "stroke_rate",
        Some(85.0),
        &[("display_unit", r#""spm""#), ("show_units", "true")],
    );
    assert_eq!(value, "85");
    assert_eq!(unit, Some("SPM".to_string()));
}

#[test]
fn torque_formats_as_nm() {
    let (value, unit) = format_parts(
        "torque",
        "torque",
        Some(35.5),
        &[
            ("display_unit", r#""nm""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "35.5");
    assert_eq!(unit, Some("NM".to_string()));
}

#[test]
fn vertical_speed_formats_as_mps() {
    let (value, unit) = format_parts(
        "vertical_speed",
        "vertical_speed",
        Some(5.2),
        &[
            ("display_unit", r#""mps""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "5.2");
    assert_eq!(unit, Some("M/S".to_string()));
}

#[test]
fn vertical_speed_converts_to_ftmin() {
    let (value, unit) = format_parts(
        "vertical_speed",
        "vertical_speed",
        Some(1.0),
        &[
            ("display_unit", r#""ftmin""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "196.9");
    assert_eq!(unit, Some("FT/MIN".to_string()));
}

#[test]
fn vertical_speed_converts_to_ftph() {
    let (value, unit) = format_parts(
        "vertical_speed",
        "vertical_speed",
        Some(1.0),
        &[
            ("display_unit", r#""ftph""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "11811");
    assert_eq!(unit, Some("FT/H".to_string()));
}

#[test]
fn vertical_oscillation_formats_as_mm() {
    let (value, unit) = format_parts(
        "vertical_oscillation",
        "vertical_oscillation",
        Some(85.0),
        &[
            ("display_unit", r#""mm""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "85");
    assert_eq!(unit, Some("MM".to_string()));
}

#[test]
fn vertical_oscillation_converts_to_cm() {
    let (value, unit) = format_parts(
        "vertical_oscillation",
        "vertical_oscillation",
        Some(100.0),
        &[
            ("display_unit", r#""cm""#),
            ("decimals", "1"),
            ("show_units", "true"),
        ],
    );
    assert_eq!(value, "10");
    assert_eq!(unit, Some("CM".to_string()));
}

#[test]
fn vertical_oscillation_shows_placeholder_when_missing() {
    let (value, _) = format_parts(
        "vertical_oscillation",
        "vertical_oscillation",
        None,
        &[("display_unit", r#""mm""#)],
    );
    assert_eq!(value, "--");
}

#[test]
fn gear_position_formats_as_integer_no_units_by_default() {
    let (value, unit) = format_parts(
        "gear_position",
        "gear_position",
        Some(5.0),
        &[("display_unit", r#""gear""#)],
    );
    assert_eq!(value, "5");
    assert_eq!(unit, None);
}

#[test]
fn gear_position_shows_unit_when_configured() {
    let (value, unit) = format_parts(
        "gear_position",
        "gear_position",
        Some(5.0),
        &[("display_unit", r#""gear""#), ("show_units", "true")],
    );
    assert_eq!(value, "5");
    assert_eq!(unit, Some("GEAR".to_string()));
}

#[test]
fn gear_position_shows_placeholder_when_missing() {
    let (value, _) = format_parts(
        "gear_position",
        "gear_position",
        None,
        &[("display_unit", r#""gear""#)],
    );
    assert_eq!(value, "--");
}

#[test]
fn missing_data_shows_placeholder() {
    let (value, _) = format_parts("g_force", "g_force", None, &[("display_unit", r#""g""#)]);
    assert_eq!(value, "--");
}

#[test]
fn balance_formats_plain_default() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        Some(52.0),
        &[("display_unit", r#""percent""#)],
    );
    assert_eq!(value, "52/48");
}

#[test]
fn balance_formats_percent_label() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        Some(52.0),
        &[
            ("display_unit", r#""percent""#),
            ("balance_format", r#""percent_label""#),
        ],
    );
    assert_eq!(value, "52%/48%");
}

#[test]
fn balance_formats_plain() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        Some(60.0),
        &[
            ("display_unit", r#""percent""#),
            ("balance_format", r#""plain""#),
        ],
    );
    assert_eq!(value, "60/40");
}

#[test]
fn balance_formats_l_prefix() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        Some(48.0),
        &[
            ("display_unit", r#""percent""#),
            ("balance_format", r#""l_prefix""#),
        ],
    );
    assert_eq!(value, "L48/R52");
}

#[test]
fn balance_formats_l_suffix() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        Some(70.0),
        &[
            ("display_unit", r#""percent""#),
            ("balance_format", r#""l_suffix""#),
        ],
    );
    assert_eq!(value, "70L/30R");
}

#[test]
fn balance_placeholder_shows_dashes() {
    let (value, _) = format_parts(
        "left_right_balance",
        "left_right_balance",
        None,
        &[("display_unit", r#""percent""#)],
    );
    assert_eq!(value, "--");
}
