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
fn heading_trimmed_and_densified_with_forward_fill() {
    use ovrley_core::activity::interpolate::densify_activity;

    let mut trimmed = common::builders::minimal_trimmed_activity(vec![0.0, 0.5, 1.0]);
    trimmed.heading = vec![Some(90.0), None, Some(180.0)];
    let mut requirements = RenderDataRequirements::default();
    requirements.heading = true;

    let report = densify_activity(&trimmed, 30.0, &requirements);

    assert_eq!(report.frame_count, 30);
    assert_eq!(report.series.heading.len(), 30);
    assert!((report.series.heading[0].unwrap() - 90.0).abs() < 1.0);
    let mid_frame = 15;
    let mid_heading = report.series.heading[mid_frame].unwrap();
    assert!(
        (mid_heading - 90.0).abs() < 1.0 || (mid_heading - 135.0).abs() < 1.0,
        "mid heading should be near 90 (forward-fill) or 135 (interpolated), got {}",
        mid_heading
    );
}

#[test]
fn heading_not_densified_when_not_required() {
    use ovrley_core::activity::interpolate::densify_activity;

    let mut trimmed = common::builders::minimal_trimmed_activity(vec![0.0, 1.0]);
    trimmed.heading = vec![Some(90.0), Some(180.0)];
    let requirements = RenderDataRequirements::default();

    let report = densify_activity(&trimmed, 30.0, &requirements);
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
