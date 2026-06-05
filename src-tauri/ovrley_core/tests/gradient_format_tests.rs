//! Gradient widget formatting behavior tests.
//!
//! Verifies the gradient widget formats correctly through the public
//! `format_validated_gradient` API, including sign handling, decimal precision,
//! placeholder fallback, and prefix/suffix application.
//!
//! ## Type
//! Integration-adjacent test. Constructs config and validates gradient widgets
//! in memory. No I/O, no fixtures.

use ovrley_core::normalize::validate_gradient_widget;
use ovrley_core::render::format::format_validated_gradient;

fn gradient_config(overrides: &[(&str, &str)]) -> ovrley_core::normalize::raw::ValueConfig {
    let mut fields = ("\"value\": \"gradient\", \"x\": 100, \"y\": 200, \"font\": \"Arial.ttf\", \
         \"font_size\": 48.0, \"color\": \"#ffffff\", \"opacity\": 1.0, \
         \"triangle_width\": 72.0, \"value_offset\": 0.0, \"show_triangle\": true, \
         \"unit_color\": \"#ffffff\", \
         \"triangle_positive_color\": \"#40e0d0\", \"triangle_negative_color\": \"#c65102\"")
        .to_string();
    for (key, val) in overrides {
        fields.push_str(&format!(", \"{}\": {}", key, val));
    }
    let json_str = format!("{{{}}}", fields);
    serde_json::from_str(&json_str).unwrap()
}

fn format_gradient(raw: Option<f64>, overrides: &[(&str, &str)]) -> String {
    let mut defaults: Vec<(&str, &str)> = overrides.to_vec();
    if !overrides.iter().any(|(k, _)| *k == "decimals")
        && !overrides.iter().any(|(k, _)| *k == "decimal_rounding")
    {
        defaults.push(("decimals", "1"));
    }
    if !overrides.iter().any(|(k, _)| *k == "show_sign") {
        defaults.push(("show_sign", "true"));
    }
    if !overrides.iter().any(|(k, _)| *k == "prefix") {
        defaults.push(("prefix", "\"\""));
    }
    if !overrides.iter().any(|(k, _)| *k == "suffix") {
        defaults.push(("suffix", "\"\""));
    }
    let config = gradient_config(&defaults);
    let validated = validate_gradient_widget(config, 0).unwrap();
    format_validated_gradient(&validated, raw)
}

#[test]
fn positive_gradient_with_sign() {
    assert_eq!(format_gradient(Some(5.0), &[]), "+5%");
}

#[test]
fn negative_gradient_with_sign() {
    assert_eq!(format_gradient(Some(-3.2), &[]), "-3.2%");
}

#[test]
fn zero_gradient_no_sign() {
    assert_eq!(format_gradient(Some(0.0), &[]), "0%");
}

#[test]
fn missing_data_shows_placeholder() {
    assert_eq!(format_gradient(None, &[]), "--%");
}

#[test]
fn sign_hidden_when_show_sign_false() {
    assert_eq!(format_gradient(Some(5.0), &[("show_sign", "false")]), "5%");
}

#[test]
fn negative_hidden_when_show_sign_false() {
    assert_eq!(format_gradient(Some(-3.0), &[("show_sign", "false")]), "3%");
}

#[test]
fn zero_decimals() {
    assert_eq!(format_gradient(Some(5.7), &[("decimals", "0")]), "+6%");
}

#[test]
fn two_decimals() {
    assert_eq!(format_gradient(Some(5.123), &[("decimals", "2")]), "+5.12%");
}

#[test]
fn prefix_applied() {
    assert_eq!(
        format_gradient(Some(5.0), &[("prefix", "\"Grade: \"")]),
        "Grade: +5%"
    );
}

#[test]
fn suffix_applied() {
    assert_eq!(
        format_gradient(Some(5.0), &[("suffix", "\" slope\"")]),
        "+5% slope"
    );
}

#[test]
fn prefix_and_suffix_applied() {
    assert_eq!(
        format_gradient(Some(5.0), &[("prefix", "\"[\""), ("suffix", "\"]\"")]),
        "[+5%]"
    );
}

#[test]
fn large_positive_gradient() {
    assert_eq!(format_gradient(Some(25.0), &[]), "+25%");
}

#[test]
fn large_negative_gradient() {
    assert_eq!(format_gradient(Some(-25.0), &[]), "-25%");
}

#[test]
fn small_gradient_rounds_correctly() {
    assert_eq!(format_gradient(Some(0.001), &[("decimals", "0")]), "+0%");
}

#[test]
fn decimal_rounding_field() {
    assert_eq!(
        format_gradient(Some(5.55), &[("decimal_rounding", "1")]),
        "+5.6%"
    );
}

#[test]
fn both_precision_fields_rejected() {
    let config = gradient_config(&[
        ("decimals", "1"),
        ("decimal_rounding", "1"),
        ("show_sign", "true"),
        ("prefix", "\"\""),
        ("suffix", "\"\""),
    ]);
    let e = validate_gradient_widget(config, 0).unwrap_err().to_string();
    assert!(e.contains("exactly one precision"), "{e}");
}

#[test]
fn missing_precision_rejected() {
    let config = gradient_config(&[
        ("decimals", "null"),
        ("decimal_rounding", "null"),
        ("show_sign", "true"),
        ("prefix", "\"\""),
        ("suffix", "\"\""),
    ]);
    let e = validate_gradient_widget(config, 0).unwrap_err().to_string();
    assert!(
        e.contains("decimals") || e.contains("decimal_rounding"),
        "{e}"
    );
}
