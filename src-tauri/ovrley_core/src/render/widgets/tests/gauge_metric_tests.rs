//! Gauge fill-quantization tests.
//!
//! These cases protect range clamping and exact bar activation boundaries.
//! Label formatting is intentionally omitted because it is a thin formatting
//! expression with no meaningful branching contract.

use super::super::gauges::metric::{bar_fill_count, fill_percentage, metric_range};

#[test]
fn gauge_bounds_preserve_source_extrema_outside_export_window() {
    use crate::activity::interpolate::{densify_activity, frame_timeline_for_fps};
    use crate::activity::schema::ParsedActivity;
    use crate::activity::trim::trim_activity;
    use crate::normalize::RenderDataRequirements;
    use crate::MetricKind;

    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({
        "sample_elapsed_seconds": [0, 1, 2, 3, 4],
        "speed": [0, 10, 20, 30, 40],
        "elevation": [0, 100, 200, 300, 400],
        "barometric_altitude": [500, 510, 520, 530, 540]
    }))
    .unwrap();
    let requirements = RenderDataRequirements {
        speed: true,
        elevation: true,
        barometric_altitude: true,
        ..Default::default()
    };
    for (start, end, fps) in [(1.0, 3.0, 1.0), (1.5, 2.5, 30.0)] {
        let trimmed = trim_activity(&activity, start, end, &requirements).unwrap();
        let dense = densify_activity(
            &trimmed,
            frame_timeline_for_fps(end - start, fps).unwrap(),
            &requirements,
        );
        assert!(dense
            .series
            .speed
            .iter()
            .flatten()
            .all(|value| *value > 0.0 && *value < 40.0));
        let (min, max) = metric_range(&dense, MetricKind::Speed);
        assert_eq!((min, max), (0.0, 40.0));
        assert_eq!(fill_percentage(20.0, min, max), 0.5);
        assert_eq!(metric_range(&dense, MetricKind::Altitude), (500.0, 540.0));
    }
}

#[test]
fn fill_percentage_clamps_and_handles_degenerate_ranges() {
    assert_eq!(fill_percentage(50.0, 0.0, 100.0), 0.5);
    assert_eq!(fill_percentage(-20.0, 0.0, 100.0), 0.0);
    assert_eq!(fill_percentage(120.0, 0.0, 100.0), 1.0);
    assert_eq!(fill_percentage(42.0, 10.0, 10.0), 0.0);
}

#[test]
fn whole_bar_bucket_boundaries_are_discrete() {
    assert_eq!(bar_fill_count(0.0, 5), 0);
    assert_eq!(bar_fill_count(0.1999, 5), 0);
    assert_eq!(bar_fill_count(0.2, 5), 1);
    assert_eq!(bar_fill_count(0.9999, 5), 4);
    assert_eq!(bar_fill_count(1.0, 5), 5);
    assert_eq!(bar_fill_count(1.5, 5), 5);
}
