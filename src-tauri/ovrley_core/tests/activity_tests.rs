//! Activity parsing, trimming, and densification integration tests.

mod common;

use std::fs;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::activity::{build_dense_activity_report_validated, parse_activity_json};
use ovrley_core::commands::parse_and_validate_config;

fn full_scene(fps: f64, start: f64, end: f64) -> String {
    format!(
        r##"{{"fps":{fps},"start":{start},"end":{end},"width":1920,"height":1080,"scale":1.0,"shadow_color":"#000000","shadow_strength":0.0,"shadow_distance":0.0,"border_color":"#000000","border_thickness":0.0,"update_rate":1}}"##
    )
}

fn speed_value() -> &'static str {
    r##"{"value":"speed","x":0,"y":0,"font":"f","font_size":12.0,"color":"#ffffff","opacity":1.0,"show_icon":false,"icon_color":"#000000","icon_size":1.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"show_units":false,"unit_color":"#000000","display_unit":"","prefix":"","suffix":"","format":"{v}","decimals":0}"##
}

fn time_value() -> String {
    format!(
        r##"{{"value":"time","x":0,"y":0,"font":"f","font_size":12.0,"color":"#ffffff","opacity":1.0,"show_icon":false,"icon_color":"#000000","icon_size":1.0,"icon_offset_x":0.0,"icon_offset_y":0.0,"show_units":false,"unit_color":"#000000","display_unit":"","prefix":"","suffix":"","format":"{{v}}","decimals":0}}"##
    )
}

#[test]
fn builds_dense_report_for_full_fixture() {
    let activity_json = fs::read_to_string(common::test_config::parsed_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_and_validate_config(&format!(
        r##"{{"scene":{},"values":[{}]}}"##,
        full_scene(30.0, 0.0, 4672.0),
        speed_value()
    ))
    .unwrap();
    let report = build_dense_activity_report_validated(&activity, &config).unwrap();

    assert_eq!(report.frame_count, 140160);
    assert_eq!(report.frame_elapsed_seconds.first().copied(), Some(0.0));
    assert!(
        report
            .frame_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default()
            < 4672.0
    );
    assert_eq!(report.series.speed.len(), report.frame_count);
    assert!(report.series.course_lat.is_empty());
}

#[test]
fn trims_non_integer_window_across_multiple_fps() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();

    for (fps, expected_frames) in [(24.0, 708usize), (30.0, 885usize), (60.0, 1770usize)] {
        let config = parse_and_validate_config(&format!(
            r##"{{"scene":{},"values":[{}]}}"##,
            full_scene(fps, 600.25, 629.75),
            time_value()
        ))
        .unwrap();
        let report = build_dense_activity_report_validated(&activity, &config).unwrap();
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
fn only_densifies_series_requested_by_template() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_and_validate_config(&format!(
        r##"{{"scene":{},"values":[{}],"plots":{{"course":{{"value":"course","x":0,"y":0,"width":200,"height":100,"simplify_tolerance_px":1.0,"target_density":1.0,"completed_line_width":2.0,"completed_line_color":"#000000","completed_line_opacity":1.0,"remaining_line_width":2.0,"remaining_line_color":"#888888","remaining_line_opacity":1.0,"marker_variant":"single","marker_variant_diameter":12.0,"marker_size":8.0,"marker_color":"#ff0000","marker_opacity":1.0,"show_full_activity":false}}}}}}"##,
        full_scene(30.0, 600.0, 630.0),
        speed_value()
    ))
    .unwrap();

    let report = build_dense_activity_report_validated(&activity, &config).unwrap();

    assert_eq!(report.series.speed.len(), report.frame_count);
    assert!(report.series.elevation.is_empty());
    assert!(report.series.gradient.is_empty());
    assert!(report.series.time.is_empty());
    assert!(report.series.course_lat.is_empty());
    assert!(report.series.course_lon.is_empty());
    assert_eq!(report.frame_distance_progress.len(), report.frame_count);
}

#[test]
fn trimmed_exports_keep_absolute_distance_progress() {
    let activity_json = fs::read_to_string(common::test_config::fit_activity_path()).unwrap();
    let activity = parse_activity_json(&activity_json).unwrap();
    let config = parse_and_validate_config(&format!(
        r##"{{"scene":{},"plots":{{"course":{{"value":"course","x":0,"y":0,"width":200,"height":100,"simplify_tolerance_px":1.0,"target_density":1.0,"completed_line_width":2.0,"completed_line_color":"#000000","completed_line_opacity":1.0,"remaining_line_width":2.0,"remaining_line_color":"#888888","remaining_line_opacity":1.0,"marker_variant":"single","marker_variant_diameter":12.0,"marker_size":8.0,"marker_color":"#ff0000","marker_opacity":1.0,"show_full_activity":false}}}}}}"##,
        full_scene(30.0, 600.0, 630.0)
    ))
    .unwrap();

    let report = build_dense_activity_report_validated(&activity, &config).unwrap();

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

#[test]
fn parsed_activity_deserializes_new_srt_series() {
    let json = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 1.0, 2.0],
        "altitude": [100.0, 110.0, 120.0],
        "iso": [200.0, 400.0, 800.0],
        "aperture": [1.7, 2.8, 4.0],
        "shutter_speed": [0.0003125, 0.001, 0.002],
        "focal_length": [24.0, 35.0, 50.0],
        "ev": [0.0, -1.0, 1.5],
        "color_temperature": [5491.0, 5600.0, 3200.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();

    assert_eq!(activity.altitude, vec![Some(100.0), Some(110.0), Some(120.0)]);
    assert_eq!(activity.iso, vec![Some(200.0), Some(400.0), Some(800.0)]);
    assert_eq!(activity.aperture, vec![Some(1.7), Some(2.8), Some(4.0)]);
    assert_eq!(
        activity.shutter_speed,
        vec![Some(0.0003125), Some(0.001), Some(0.002)]
    );
    assert_eq!(
        activity.focal_length,
        vec![Some(24.0), Some(35.0), Some(50.0)]
    );
    assert_eq!(activity.ev, vec![Some(0.0), Some(-1.0), Some(1.5)]);
    assert_eq!(
        activity.color_temperature,
        vec![Some(5491.0), Some(5600.0), Some(3200.0)]
    );
}

#[test]
fn parsed_activity_new_series_default_to_empty() {
    let json = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 1.0]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();

    assert!(activity.altitude.is_empty());
    assert!(activity.iso.is_empty());
    assert!(activity.aperture.is_empty());
    assert!(activity.shutter_speed.is_empty());
    assert!(activity.focal_length.is_empty());
    assert!(activity.ev.is_empty());
    assert!(activity.color_temperature.is_empty());
}

#[test]
fn parsed_activity_handles_nulls_in_new_series() {
    let json = serde_json::json!({
        "sample_elapsed_seconds": [0.0, 1.0, 2.0],
        "iso": [200.0, null, 800.0],
        "ev": [null, -1.0, null]
    });
    let activity: ParsedActivity = serde_json::from_value(json).unwrap();

    assert_eq!(activity.iso, vec![Some(200.0), None, Some(800.0)]);
    assert_eq!(activity.ev, vec![None, Some(-1.0), None]);
}
