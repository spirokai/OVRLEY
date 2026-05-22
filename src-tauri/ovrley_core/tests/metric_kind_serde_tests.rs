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
