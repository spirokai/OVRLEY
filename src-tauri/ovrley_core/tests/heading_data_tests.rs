//! Heading data plumbing tests.
//!
//! These keep the activity/data-path coverage that remains relevant after the
//! validation seam took ownership of config health. Raw heading-config fallback
//! tests were intentionally dropped.

mod common;

use serde_json::json;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::normalize::RenderDataRequirements;

#[test]
fn parsed_activity_accepts_heading_from_json() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0],
        "sample_distance_progress": [0.0, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 1.0,
        "heading": [90.0, 91.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert_eq!(activity.heading, vec![Some(90.0), Some(91.0)]);
}

#[test]
fn parsed_activity_heading_defaults_to_empty_when_missing() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0],
        "sample_distance_progress": [0.0, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 1.0
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert!(activity.heading.is_empty());
}

#[test]
fn parsed_activity_heading_preserves_nulls() {
    let json = json!({
        "sample_elapsed_seconds": [0.0, 1.0, 2.0],
        "sample_distance_progress": [0.0, 0.5, 1.0],
        "trim_start_seconds": 0.0,
        "trim_end_seconds": 2.0,
        "heading": [90.0, null, 180.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();
    assert_eq!(activity.heading, vec![Some(90.0), None, Some(180.0)]);
}

#[test]
fn render_data_requirements_heading_derived_from_metric_kind() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [],
        "plots": []
    }));

    let reqs = config.render_data_requirements().unwrap();
    assert!(!reqs.heading);
}

#[test]
fn heading_uses_the_shortest_path_in_preview_trim_boundaries_and_native_frames() {
    use ovrley_core::activity::{interpolate::densify_activity, trim::trim_activity};
    use ovrley_widgets::activity::ActivitySampler;
    let mut requirements = RenderDataRequirements::default();
    requirements.heading = true;
    for (left, right, expected) in [
        (359.0, 1.0, [359.5, 359.75, 0.0, 0.25, 0.5]),
        (1.0, 359.0, [0.5, 0.25, 0.0, 359.75, 359.5]),
        (90.0, 180.0, [112.5, 123.75, 135.0, 146.25, 157.5]),
    ] {
        let activity: ParsedActivity = serde_json::from_value(json!({
            "sample_elapsed_seconds": [0.0, 0.5, 1.0], "trim_end_seconds": 1.0,
            "heading": [left, null, right],
        }))
        .unwrap();
        let sampler = ActivitySampler::from_json(&serde_json::to_vec(&activity).unwrap()).unwrap();
        let trimmed = trim_activity(&activity, 0.25, 0.75, &requirements).unwrap();
        assert_eq!(
            trimmed.heading,
            vec![Some(expected[0]), None, Some(expected[4])]
        );
        let seconds = vec![0.0, 0.125, 0.25, 0.375, 0.5];
        let report = densify_activity(&trimmed, seconds.clone(), &requirements);
        assert_eq!(report.series.heading, expected.map(Some));
        for index in [0, 1, 2, 3, 4, 2, 0] {
            assert_eq!(
                sampler.heading_at(seconds[index] + 0.25),
                report.series.heading[index]
            );
        }
    }
}

#[test]
fn heading_not_densified_when_not_required() {
    use ovrley_core::activity::interpolate::{densify_activity, frame_timeline_for_fps};

    let mut trimmed = common::builders::minimal_trimmed_activity(vec![0.0, 1.0]);
    trimmed.heading = vec![Some(90.0), Some(180.0)];
    let requirements = RenderDataRequirements::default();

    let report = densify_activity(
        &trimmed,
        frame_timeline_for_fps(1.0, 30.0).unwrap(),
        &requirements,
    );
    assert!(report.series.heading.is_empty());
}

#[test]
fn heading_values_triggers_heading_data_requirement() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [common::builders::heading_tape_json()],
        "plots": []
    }));

    let reqs = config.render_data_requirements().unwrap();
    assert!(
        reqs.heading,
        "heading value should trigger heading requirement"
    );
}
