//! MetricKind serde compatibility tests.
//!
//! Verifies each `MetricKind` variant serializes to the exact legacy
//! JSON string expected by the frontend (e.g., `"speed"`, `"heartrate"`),
//! and that deserialization of old string values works correctly.
//! Confirms unknown metric names produce deserialization errors.
//!
//! ## Type
//! Unit test. Pure JSON round-trip — no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - `#[serde(rename)]` changes breaking old templates/configs
//! - Unknown metric strings silently mapping to a default variant
//! - Variant name refactors breaking frontend communication

use ovrley_core::MetricKind;
use serde_json;

#[test]
fn metric_kind_round_trip_all_variants() {
    let cases = vec![
        (MetricKind::Speed, r#""speed""#),
        (MetricKind::Heartrate, r#""heartrate""#),
        (MetricKind::Elevation, r#""elevation""#),
        (MetricKind::Time, r#""time""#),
        (MetricKind::Gradient, r#""gradient""#),
        (MetricKind::Cadence, r#""cadence""#),
        (MetricKind::Power, r#""power""#),
        (MetricKind::Temperature, r#""temperature""#),
        (MetricKind::Pace, r#""pace""#),
        (MetricKind::GForce, r#""g_force""#),
        (MetricKind::AirPressure, r#""air_pressure""#),
        (MetricKind::GroundContactTime, r#""ground_contact_time""#),
        (MetricKind::LeftRightBalance, r#""left_right_balance""#),
        (MetricKind::StrideLength, r#""stride_length""#),
        (MetricKind::StrokeRate, r#""stroke_rate""#),
        (MetricKind::Torque, r#""torque""#),
        (MetricKind::VerticalSpeed, r#""vertical_speed""#),
        (MetricKind::GearPosition, r#""gear_position""#),
        (MetricKind::VerticalRatio, r#""vertical_ratio""#),
        (MetricKind::VerticalOscillation, r#""vertical_oscillation""#),
        (MetricKind::CoreTemperature, r#""core_temperature""#),
        (MetricKind::Heading, r#""heading""#),
        (MetricKind::Altitude, r#""altitude""#),
        (MetricKind::Iso, r#""iso""#),
        (MetricKind::Aperture, r#""aperture""#),
        (MetricKind::ShutterSpeed, r#""shutter_speed""#),
        (MetricKind::FocalLength, r#""focal_length""#),
        (MetricKind::Ev, r#""ev""#),
        (MetricKind::ColorTemperature, r#""color_temperature""#),
    ];

    for (kind, expected_json) in cases {
        let serialized = serde_json::to_string(&kind).unwrap();
        assert_eq!(
            serialized, expected_json,
            "serialization mismatch for {:?}",
            kind
        );

        let deserialized: MetricKind = serde_json::from_str(expected_json).unwrap();
        assert_eq!(
            deserialized, kind,
            "deserialization mismatch for {:?}",
            kind
        );
    }
}

#[test]
fn unknown_metric_deserialization_fails() {
    let result: Result<MetricKind, _> = serde_json::from_str(r#""unknown_metric""#);
    assert!(
        result.is_err(),
        "Unknown metric should fail deserialization"
    );
}
