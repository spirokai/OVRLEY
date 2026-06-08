//! Heading data plumbing tests.
//!
//! These keep the activity/data-path coverage that remains relevant after the
//! validation seam took ownership of config health. Raw heading-config fallback
//! tests were intentionally dropped.

mod common;

use serde_json::json;

use ovrley_core::activity::schema::{ParsedActivity, TrimmedActivity};
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

    let trimmed = TrimmedActivity {
        source_start_time: None,
        sample_elapsed_seconds: vec![0.0, 0.5, 1.0],
        sample_distance_progress: vec![],
        course: vec![],
        elevation: vec![],
        speed: vec![],
        heartrate: vec![],
        cadence: vec![],
        power: vec![],
        temperature: vec![],
        pace: vec![],
        g_force: vec![],
        air_pressure: vec![],
        ground_contact_time: vec![],
        left_right_balance: vec![],
        stride_length: vec![],
        stroke_rate: vec![],
        torque: vec![],
        vertical_speed: vec![],
        altitude: vec![],
        iso: vec![],
        aperture: vec![],
        shutter_speed: vec![],
        focal_length: vec![],
        ev: vec![],
        color_temperature: vec![],
        gear_position: vec![],
        vertical_ratio: vec![],
        vertical_oscillation: vec![],
        core_temperature: vec![],
        gradient: vec![],
        time: vec![],
        heading: vec![Some(90.0), None, Some(180.0)],
    };
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

    let trimmed = TrimmedActivity {
        source_start_time: None,
        sample_elapsed_seconds: vec![0.0, 1.0],
        sample_distance_progress: vec![],
        course: vec![],
        elevation: vec![],
        speed: vec![],
        heartrate: vec![],
        cadence: vec![],
        power: vec![],
        temperature: vec![],
        pace: vec![],
        g_force: vec![],
        air_pressure: vec![],
        ground_contact_time: vec![],
        left_right_balance: vec![],
        stride_length: vec![],
        stroke_rate: vec![],
        torque: vec![],
        vertical_speed: vec![],
        altitude: vec![],
        iso: vec![],
        aperture: vec![],
        shutter_speed: vec![],
        focal_length: vec![],
        ev: vec![],
        color_temperature: vec![],
        gear_position: vec![],
        vertical_ratio: vec![],
        vertical_oscillation: vec![],
        core_temperature: vec![],
        gradient: vec![],
        time: vec![],
        heading: vec![Some(90.0), Some(180.0)],
    };
    let requirements = RenderDataRequirements::default();

    let report = densify_activity(&trimmed, 30.0, &requirements);
    assert!(report.series.heading.is_empty());
}

#[test]
fn heading_values_triggers_heading_data_requirement() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "heading",
            "x": 0.0,
            "y": 0.0,
            "display_type": "heading_tape",
            "width": 400,
            "height": 80,
            "rotation": 0.0,
            "opacity": 1.0,
            "pixels_per_degree": 5.0,
            "major_tick_interval": 15,
            "minor_ticks_per_major": 3,
            "show_major_ticks": true,
            "show_minor_ticks": true,
            "major_tick_length_pct": 40.0,
            "minor_tick_length_pct": 20.0,
            "major_tick_thickness": 2.0,
            "minor_tick_thickness": 1.0,
            "tick_color": "#FFFFFF",
            "cardinal_tick_color": "#FF0000",
            "tick_alignment": "below",
            "show_minor_labels": true,
            "show_major_labels": true,
            "label_color": "#CCCCCC",
            "cardinal_label_color": "#FF0000",
            "label_font": "Teko.ttf",
            "label_font_size": 12.0,
            "label_offset": 5.0,
            "indicator_style": "chevron",
            "indicator_placement": "top",
            "show_indicator": true,
            "indicator_color": "#FF0000",
            "indicator_size": 10.0
        }],
        "plots": []
    }));

    let reqs = config.render_data_requirements().unwrap();
    assert!(
        reqs.heading,
        "heading value should trigger heading requirement"
    );
}
