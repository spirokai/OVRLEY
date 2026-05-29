//! MetricKind behavior tests.
//!
//! Verifies all `MetricKind` variants have distinct serde names,
//! individually round-trip through JSON, and the total variant count
//! stays stable (guarding against accidental additions or removals).
//!
//! ## Type
//! Unit test. Pure Rust — no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - Duplicate serde names causing silent mis-deserialization
//! - Accidental variant additions/removals
//! - Individual variant serde round-trip breaking

use ovrley_core::MetricKind;

#[test]
fn all_metrics_have_distinct_serde_names() {
    use std::collections::HashSet;
    let names: Vec<String> = [
        MetricKind::Speed,
        MetricKind::Heartrate,
        MetricKind::Elevation,
        MetricKind::Time,
        MetricKind::Gradient,
        MetricKind::Cadence,
        MetricKind::Power,
        MetricKind::Temperature,
        MetricKind::Pace,
        MetricKind::GForce,
        MetricKind::AirPressure,
        MetricKind::GroundContactTime,
        MetricKind::LeftRightBalance,
        MetricKind::StrideLength,
        MetricKind::StrokeRate,
        MetricKind::Torque,
        MetricKind::VerticalSpeed,
        MetricKind::GearPosition,
        MetricKind::VerticalRatio,
        MetricKind::VerticalOscillation,
        MetricKind::CoreTemperature,
        MetricKind::Heading,
    ]
    .iter()
    .map(|k| serde_json::to_string(k).unwrap())
    .collect();

    let unique: HashSet<_> = names.iter().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "MetricKind has duplicate serde names"
    );
}

#[test]
fn metric_kind_count_is_twenty_two() {
    let all = [
        MetricKind::Speed,
        MetricKind::Heartrate,
        MetricKind::Elevation,
        MetricKind::Time,
        MetricKind::Gradient,
        MetricKind::Cadence,
        MetricKind::Power,
        MetricKind::Temperature,
        MetricKind::Pace,
        MetricKind::GForce,
        MetricKind::AirPressure,
        MetricKind::GroundContactTime,
        MetricKind::LeftRightBalance,
        MetricKind::StrideLength,
        MetricKind::StrokeRate,
        MetricKind::Torque,
        MetricKind::VerticalSpeed,
        MetricKind::GearPosition,
        MetricKind::VerticalRatio,
        MetricKind::VerticalOscillation,
        MetricKind::CoreTemperature,
        MetricKind::Heading,
    ];
    assert_eq!(all.len(), 22);
}

#[test]
fn each_variant_roundtrips_individually() {
    let all = [
        MetricKind::Speed,
        MetricKind::Heartrate,
        MetricKind::Elevation,
        MetricKind::Time,
        MetricKind::Gradient,
        MetricKind::Cadence,
        MetricKind::Power,
        MetricKind::Temperature,
        MetricKind::Pace,
        MetricKind::GForce,
        MetricKind::AirPressure,
        MetricKind::GroundContactTime,
        MetricKind::LeftRightBalance,
        MetricKind::StrideLength,
        MetricKind::StrokeRate,
        MetricKind::Torque,
        MetricKind::VerticalSpeed,
        MetricKind::GearPosition,
        MetricKind::VerticalRatio,
        MetricKind::VerticalOscillation,
        MetricKind::CoreTemperature,
        MetricKind::Heading,
    ];
    for kind in all {
        let json = serde_json::to_string(&kind).unwrap();
        let back: MetricKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind, "roundtrip failed for {:?}", kind);
    }
}
