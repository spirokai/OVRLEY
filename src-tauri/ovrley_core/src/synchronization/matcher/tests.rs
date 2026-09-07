use super::*;
use crate::activity::{parse_activity_json, schema::ParsedActivity};
use crate::synchronization::{
    fingerprint,
    motion_estimation::{MotionEstimate, MotionInterval, MotionOutcome, QualityReason},
    optical_flow::TrackingQuality,
};

fn signal(t: f64) -> f64 {
    0.06 * ((t * 0.039 + t * t * 0.00013).sin() + 0.6 * (t * 0.117).cos() + 0.4 * (t * 0.023).sin())
}

fn activity(value: impl Fn(f64) -> f64) -> Fingerprint {
    let mut activity: ParsedActivity = serde_json::from_str("{}").unwrap();
    let mut heading = 180.0;
    for i in 0..=1200 {
        activity.sample_elapsed_seconds.push(i as f64);
        activity.heading.push(Some(heading));
        heading = (heading + value(i as f64 + 0.5).to_degrees()).rem_euclid(360.0);
        activity.speed.push(Some(5.0));
        activity
            .course
            .push((Some(45.0 + i as f64 * 0.0001), Some(10.0)));
    }
    fingerprint::prepare_activity(&activity)
}

fn video(value: impl Fn(f64) -> f64, missing: impl Fn(usize) -> bool) -> Fingerprint {
    fingerprint::prepare_video((0..1800).map(|i| MotionInterval {
        start_seconds: i as f64 / 5.0,
        end_seconds: (i + 1) as f64 / 5.0,
        outcome: if missing(i) {
            MotionOutcome::Unavailable(QualityReason::PoorFit)
        } else {
            MotionOutcome::Estimated(MotionEstimate {
                horizontal_per_second: value((i as f64 + 0.5) / 5.0),
                vertical_per_second: 0.0,
                roll_per_second: 0.0,
                expansion_per_second: 0.1,
                residual_per_second: 0.01,
                inliers: 100,
                inlier_fraction: 0.8,
                inlier_coverage: 0.75,
                rms_residual: 0.001,
                tracking: TrackingQuality {
                    detected: 150,
                    retained: 125,
                    coverage: 0.8,
                    mean_forward_backward_error_pixels: 0.1,
                },
            })
        },
    }))
}

fn sample(start: f64, end: f64, value: f64, span: usize) -> FeatureInterval {
    FeatureInterval {
        start_seconds: start,
        end_seconds: end,
        value,
        quality: 1.0,
        support_seconds: 0.2,
        span,
    }
}

#[test]
fn bounded_holes_preserve_only_observed_duration_and_values() {
    let bin = aggregate(&[sample(0.0, 1.0, 2.0, 0), sample(2.0, 4.0, 5.0, 1)], 0.0).unwrap();
    assert_eq!(bin.duration(), 3.0);
    assert_eq!(bin.value, 4.0);
    assert_eq!(bin.observed, vec![0.0..1.0, 2.0..4.0]);
    assert!(aggregate(&[sample(0.0, 0.5, 2.0, 0), sample(3.5, 4.0, 2.0, 1)], 0.0).is_none());
    assert!(aggregate(&[sample(0.0, 2.0, 2.0, 0), sample(2.0, 4.0, 2.0, 1)], 0.0).is_none());
}

#[test]
fn explicit_video_discontinuity_blocks_a_bin_even_with_sufficient_observations() {
    let mut builder = fingerprint::VideoFingerprintBuilder::default();
    builder.push(MotionInterval {
        start_seconds: 1.0,
        end_seconds: 1.2,
        outcome: MotionOutcome::Unavailable(QualityReason::GeometryChange),
    });
    let breaks = discontinuities(&builder.finish());
    assert_eq!(breaks, vec![1.0]);
    let samples = [sample(0.0, 1.0, 2.0, 0), sample(1.2, 4.0, 3.0, 1)];
    assert!(aggregate(&samples, 0.0).is_some());
    assert!(prepare_bin(&samples, &breaks, 0.0).is_none());
}

#[test]
fn disjoint_observations_cannot_become_paired_evidence() {
    let a = aggregate(&[sample(0.0, 2.0, 1.0, 0)], 0.0).unwrap();
    let v = aggregate(&[sample(2.0, 4.0, 1.0, 0)], 0.0).unwrap();
    assert_eq!(intersection(&a, &v), 0.0);
    let mut moments = Moments::default();
    moments.add(&a, &v);
    assert_eq!(moments.duration, 0.0);
    assert_eq!(moments.bins, 0);
}

#[test]
fn fragmented_turning_recovers_offset_without_speed_or_events() {
    let a = activity(signal);
    let mut v = video(|t| -signal(t + 200.0), |i| i % 7 == 0);
    v.events.clear();
    let result = match_fingerprints(&a, &v, &Cancellation::default()).unwrap();
    assert!(result.accepted, "{result:#?}");
    let best = &result.candidates[0];
    assert!((best.offset_seconds - 200.0).abs() <= 1.0, "{best:#?}");
    assert_eq!(best.turn_polarity, -1);
    assert!(best.observed_seconds <= result.video_observed_seconds + 1e-8);
    assert!(best.sections.iter().all(|s| s.agrees));
    assert!(best.sections[2].held_out);
}

#[test]
fn held_out_contradiction_cannot_be_rescued_by_training_match() {
    let a = activity(signal);
    let v = video(
        |t| {
            if t < 240.0 {
                -signal(t + 200.0)
            } else {
                signal(t + 200.0)
            }
        },
        |_| false,
    );
    let result = match_fingerprints(&a, &v, &Cancellation::default()).unwrap();
    assert!(!result.accepted, "{result:#?}");
    assert!((result.candidates[0].offset_seconds - 200.0).abs() <= 1.0);
    assert!(result.candidates[0]
        .rejection_reasons
        .contains(&"held_out_section_disagrees"));
}

#[test]
fn repeated_patterns_are_ambiguous() {
    let periodic = |t: f64| (t * std::f64::consts::TAU / 40.0).sin() * 0.1;
    let result = match_fingerprints(
        &activity(periodic),
        &video(periodic, |_| false),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(!result.accepted, "{result:#?}");
    assert!(result.candidates[0]
        .rejection_reasons
        .contains(&"ambiguous_offset"));
}

#[test]
fn sparse_or_flat_video_is_inconclusive() {
    let a = activity(signal);
    for v in [
        video(|t| -signal(t + 200.0), |i| i % 20 != 0),
        video(|_| 0.0, |_| false),
    ] {
        let result = match_fingerprints(&a, &v, &Cancellation::default()).unwrap();
        assert!(!result.accepted);
        assert!(result.candidates.is_empty());
    }
}

#[test]
fn cancellation_stops_search() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert!(matches!(
        match_fingerprints(&activity(signal), &video(signal, |_| false), &cancellation),
        Err(AnalysisError::Cancelled)
    ));
}

/// Explicit local-data regression, not a portable fixture or threshold calibration set.
#[test]
#[ignore = "requires the user's local Pragelpass activity and motion dump"]
fn pragelpass_known_match_and_excluded_match() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let debug: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.join("debug/activities/Pragelpass_nearly_killed_me-parse-debug.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut parsed =
        parse_activity_json(&serde_json::to_string(&debug["parsed_activity"]).unwrap()).unwrap();
    let intervals =
        std::fs::read_to_string(root.join("target/debug/motion-analysis/Pragelpass.motion.jsonl"))
            .unwrap();
    let v = fingerprint::prepare_video(
        intervals
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<MotionInterval>(l).unwrap()),
    );
    let positive = match_fingerprints(
        &fingerprint::prepare_activity(&parsed),
        &v,
        &Cancellation::default(),
    )
    .unwrap();
    // Exclude the matching clip plus a boundary buffer, retaining unrelated ride data.
    for (i, &time) in parsed.sample_elapsed_seconds.iter().enumerate() {
        if (17100.0..18300.0).contains(&time) {
            parsed.heading[i] = None;
            parsed.course[i] = (None, None);
        }
    }
    let negative = match_fingerprints(
        &fingerprint::prepare_activity(&parsed),
        &v,
        &Cancellation::default(),
    )
    .unwrap();
    let output = serde_json::json!({"known_offset_range":[17236,17252],"positive":positive,"matching_region_removed":negative});
    std::fs::write(
        root.join("target/debug/motion-analysis/matcher-regression.json"),
        serde_json::to_string_pretty(&output).unwrap(),
    )
    .unwrap();
    assert!(positive.accepted, "{positive:#?}");
    assert!((17236.0..=17252.0).contains(&positive.candidates[0].offset_seconds));
    assert!(!negative.accepted, "{negative:#?}");
}
