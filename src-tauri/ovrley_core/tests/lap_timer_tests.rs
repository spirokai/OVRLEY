mod common;

use common::builders::empty_dense_series;
use ovrley_core::activity::parse_activity_json;
use ovrley_core::activity::schema::DenseActivityReport;
use ovrley_core::normalize::LapTimerMode;
use ovrley_core::render::widgets::{lap_timer_value_text, types::PreparedValue};
use serde_json::json;

fn lap_activity() -> DenseActivityReport {
    let mut series = empty_dense_series();
    series.lap_number = vec![Some(-1), Some(0), Some(1), Some(2)];
    series.lap_time_seconds = vec![None, Some(0.5), Some(0.0), Some(1.0)];
    series.delta_to_best_lap_seconds = vec![None, None, Some(0.0), Some(-0.25)];
    series.lap_start_elapsed_seconds = vec![0.5, 4.5, 7.5];
    series.lap_durations_seconds = vec![4.0, 3.0];
    series.lap_durations_best_so_far_seconds = vec![4.0, 3.0];
    DenseActivityReport {
        frame_count: 4,
        frame_elapsed_seconds: vec![0.0, 1.0, 4.5, 8.5],
        frame_distance_progress: Vec::new(),
        full_activity_distance: None,
        full_activity_total_ascent: None,
        series,
    }
}

#[test]
fn renderer_text_covers_no_reference_zero_and_signed_deltas() {
    let mut activity = lap_activity();

    assert_eq!(
        lap_timer_value_text(LapTimerMode::Delta, &activity, 0).unwrap(),
        "+0.00"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::Delta, &activity, 2).unwrap(),
        "+0.00"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::Delta, &activity, 3).unwrap(),
        "-0.25"
    );

    activity.series.delta_to_best_lap_seconds[3] = Some(0.75);
    assert_eq!(
        lap_timer_value_text(LapTimerMode::Delta, &activity, 3).unwrap(),
        "+0.75"
    );
}

#[test]
fn delta_config_uses_its_canonical_mode_colors_and_data_requirement() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "lap_timer",
            "display_type": "lap_timer",
            "lap_timer_mode": "delta",
            "x": 10.0,
            "y": 20.0,
            "font": "Arial.ttf",
            "font_size": 72.0,
            "color": "#abcdef",
            "opacity": 1.0,
            "show_label": true,
            "label": "Delta",
            "positive_delta_color": "#00ff00",
            "negative_delta_color": "#ff0000"
        }],
        "plots": []
    }));

    let PreparedValue::LapTimer(widget) = &config.values[0] else {
        panic!("expected a validated lap timer");
    };
    assert_eq!(widget.mode, LapTimerMode::Delta);
    assert_eq!(widget.label, "Delta");
    assert_eq!(widget.positive_delta_color, [0, 255, 0, 255]);
    assert_eq!(widget.negative_delta_color, [255, 0, 0, 255]);

    let requirements = config.render_data_requirements().unwrap();
    assert!(requirements.delta_to_best_lap_seconds);
    assert!(!requirements.lap_number);
    assert!(!requirements.lap_time_seconds);
}

#[test]
fn renderer_text_covers_out_lap_first_lap_boundary_and_later_best() {
    let activity = lap_activity();

    assert_eq!(
        lap_timer_value_text(LapTimerMode::CurrentLap, &activity, 0).unwrap(),
        "--:--.--"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::BestLap, &activity, 1).unwrap(),
        "00:00.50"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::CurrentLap, &activity, 2).unwrap(),
        "00:00.00"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::BestLap, &activity, 2).unwrap(),
        "00:04.00"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::BestLap, &activity, 3).unwrap(),
        "00:03.00"
    );
}

#[test]
fn renderer_text_uses_pretrim_best_history_for_a_scene_starting_mid_lap() {
    let mut activity = lap_activity();
    activity.frame_count = 1;
    activity.frame_elapsed_seconds = vec![0.0];
    activity.series.lap_number = vec![Some(2)];
    activity.series.lap_time_seconds = vec![Some(1.0)];
    activity.series.lap_start_elapsed_seconds = vec![-8.0, -4.0, -1.0];

    assert_eq!(
        lap_timer_value_text(LapTimerMode::CurrentLap, &activity, 0).unwrap(),
        "00:01.00"
    );
    assert_eq!(
        lap_timer_value_text(LapTimerMode::BestLap, &activity, 0).unwrap(),
        "00:03.00"
    );
}

#[test]
fn renderer_text_uses_pretrim_delta_reference_for_a_scene_starting_mid_lap() {
    let mut activity = lap_activity();
    activity.frame_count = 1;
    activity.frame_elapsed_seconds = vec![0.0];
    activity.series.lap_number.clear();
    activity.series.lap_time_seconds.clear();
    activity.series.delta_to_best_lap_seconds = vec![Some(-0.42)];

    assert_eq!(
        lap_timer_value_text(LapTimerMode::Delta, &activity, 0).unwrap(),
        "-0.42"
    );
}

#[test]
fn renderer_text_rejects_an_active_lap_without_a_time() {
    let mut activity = lap_activity();
    activity.series.lap_time_seconds[2] = None;

    let error = lap_timer_value_text(LapTimerMode::CurrentLap, &activity, 2).unwrap_err();
    assert!(error
        .to_string()
        .contains("active lap 1 is missing lap_time_seconds at frame 2"));
}

#[test]
fn activity_ingress_rejects_an_active_lap_without_canonical_time() {
    let error = parse_activity_json(
        r#"{
            "sample_elapsed_seconds": [0.0, 1.0],
            "trim_end_seconds": 1.0,
            "lap_number": [0, 0],
            "lap_time_seconds": [null, 1.0],
            "lap_start_elapsed_seconds": [0.0],
            "lap_durations_seconds": [],
            "lap_durations_best_so_far_seconds": []
        }"#,
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("lap_time_seconds[0] does not match lap_start_elapsed_seconds"));
}

#[test]
fn activity_ingress_accepts_canonical_lap_timing() {
    let activity = parse_activity_json(
        r#"{
            "sample_elapsed_seconds": [0.0, 2.0, 4.0],
            "trim_end_seconds": 4.0,
            "lap_number": [0, 0, 1],
            "lap_time_seconds": [0.0, 2.0, 0.0],
            "delta_to_best_lap_seconds": [null, null, null],
            "lap_start_elapsed_seconds": [0.0, 4.0],
            "best_lap_time_seconds": 4.0,
            "lap_durations_seconds": [4.0],
            "lap_durations_best_so_far_seconds": [4.0]
        }"#,
    )
    .unwrap();

    assert_eq!(activity.lap_start_elapsed_seconds, vec![0.0, 4.0]);
}
