//! Activity parsing, trimming, and densification integration tests.
//!
//! Covers `parse_activity_json`, `trim_activity`, and
//! `build_dense_activity_report` against real GPX- and FIT-derived JSON
//! fixtures. Verifies frame count correctness at common FPS values,
//! non-integer trim windows, and that only explicitly requested telemetry
//! series are densified.
//!
//! ## Fixtures
//!
//! - `test_config::parsed_activity_path()` — GPX-derived debug JSON with
//!   full telemetry (speed, elevation, heartrate, course, timestamps).
//! - `test_config::fit_activity_path()` — FIT-derived debug JSON used
//!   for trim-window and selective-densification tests.
//!
//! ## Type
//! Integration test. Does not require ffmpeg or video fixtures — pure data
//! pipeline testing. No I/O beyond fixture reads.
//!
//! ## Regressions guarded
//! - Frame count mismatch after non-integer trim windows
//! - Unnecessary densification of unrequested telemetry series
//! - Distance progress breaking on trimmed exports
//! - JSON schema drift in activity fixtures

mod common;

use std::fs;

use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::parse_config_json;

#[test]
// Verifies that a complete GPX fixture densifies to the expected frame count.
fn builds_dense_report_for_full_fixture() {
    let activity_json = fs::read_to_string(common::test_config::parsed_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_config_json(
        r#"{
            "scene":{"fps":30,"start":0,"end":4912,"width":3840,"height":2160},
            "values":[{"value":"speed","x":0,"y":0}]
        }"#,
    )
    .unwrap();
    let report = build_dense_activity_report(&activity, &config).unwrap();

    assert_eq!(report.frame_count, 147360);
    assert_eq!(report.frame_elapsed_seconds.first().copied(), Some(0.0));
    assert!(
        report
            .frame_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default()
            < 4912.0
    );
    assert_eq!(report.series.speed.len(), report.frame_count);
    assert!(report.series.course_lat.is_empty());
}

#[test]
// Verifies non-integer trim windows produce stable frame counts at common FPS values.
fn trims_non_integer_window_across_multiple_fps() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();

    for (fps, expected_frames) in [(24.0, 708usize), (30.0, 885usize), (60.0, 1770usize)] {
        let config = parse_config_json(&format!(
            r#"{{
                "scene":{{"fps":{fps},"start":600.25,"end":629.75}},
                "values":[{{"value":"time","x":0,"y":0}}]
            }}"#
        ))
        .unwrap();
        let report = build_dense_activity_report(&activity, &config).unwrap();
        assert_eq!(report.frame_count, expected_frames);
        assert_eq!(report.frame_elapsed_seconds.first().copied(), Some(0.0));
        assert!(
            report
                .frame_elapsed_seconds
                .last()
                .copied()
                .unwrap_or_default()
                < 29.5
        );
        assert_eq!(report.series.time.len(), report.frame_count);
    }
}

#[test]
// Verifies only telemetry required by values/plots is densified.
fn only_densifies_series_requested_by_template() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_config_json(
        r#"{
            "scene":{"fps":30,"start":600,"end":630},
            "values":[{"value":"speed","x":0,"y":0}],
            "plots":{"course":{"value":"course","x":0,"y":0,"width":200,"height":100}}
        }"#,
    )
    .unwrap();

    let report = build_dense_activity_report(&activity, &config).unwrap();

    assert_eq!(report.series.speed.len(), report.frame_count);
    assert!(report.series.elevation.is_empty());
    assert!(report.series.gradient.is_empty());
    assert!(report.series.time.is_empty());
    assert!(report.series.course_lat.is_empty());
    assert!(report.series.course_lon.is_empty());
    assert_eq!(report.frame_distance_progress.len(), report.frame_count);
}

#[test]
// Verifies trimmed plot progress remains absolute to the full activity.
fn trimmed_exports_keep_absolute_distance_progress() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_config_json(
        r#"{
            "scene":{"fps":30,"start":600,"end":630},
            "plots":{"course":{"value":"course","x":0,"y":0,"width":200,"height":100}}
        }"#,
    )
    .unwrap();

    let report = build_dense_activity_report(&activity, &config).unwrap();

    let first_progress = report
        .frame_distance_progress
        .first()
        .and_then(|value| *value)
        .unwrap_or_default();
    let last_progress = report
        .frame_distance_progress
        .last()
        .and_then(|value| *value)
        .unwrap_or_default();

    assert!(first_progress > 0.0);
    assert!(last_progress > first_progress);
    assert!(last_progress < 1.0);
}
