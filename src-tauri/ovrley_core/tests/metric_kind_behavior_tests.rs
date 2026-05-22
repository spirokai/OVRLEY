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
fn metric_kind_count_is_eight() {
    let all = [
        MetricKind::Speed,
        MetricKind::Heartrate,
        MetricKind::Elevation,
        MetricKind::Time,
        MetricKind::Gradient,
        MetricKind::Cadence,
        MetricKind::Power,
        MetricKind::Temperature,
    ];
    assert_eq!(all.len(), 8);
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
    ];
    for kind in all {
        let json = serde_json::to_string(&kind).unwrap();
        let back: MetricKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind, "roundtrip failed for {:?}", kind);
    }
}
