//! Activity parsing, trimming, and densification integration tests.

mod common;

use std::fs;

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
        full_scene(30.0, 0.0, 4912.0),
        speed_value()
    ))
    .unwrap();
    let report = build_dense_activity_report_validated(&activity, &config).unwrap();

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
