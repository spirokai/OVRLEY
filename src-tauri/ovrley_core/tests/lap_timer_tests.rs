mod common;

use common::builders::empty_dense_series;
use ovrley_core::activity::parse_activity_json;
use ovrley_core::activity::schema::DenseActivityReport;
use ovrley_core::commands::validate_config_value;
use ovrley_core::normalize::LapTimerMode;
use ovrley_core::render::widgets::{
    lap_log_text_state, lap_timer_value_text, types::PreparedValue,
};
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
            "label_font": "Teko.ttf",
            "label_font_size": 24.0,
            "label_color": "#123456",
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
    assert_eq!(widget.validated.mode, LapTimerMode::Delta);
    assert_eq!(widget.validated.label, "Delta");
    assert_eq!(widget.validated.label_font_name, "Teko.ttf");
    assert_eq!(widget.validated.label_font_size, 24.0);
    assert_eq!(widget.validated.label_color, [18, 52, 86, 255]);
    assert_eq!(widget.validated.positive_delta_color, [0, 255, 0, 255]);
    assert_eq!(widget.validated.negative_delta_color, [255, 0, 0, 255]);

    let requirements = config.render_data_requirements().unwrap();
    assert!(requirements.delta_to_best_lap_seconds);
    assert!(!requirements.lap_number);
    assert!(!requirements.lap_time_seconds);
}

#[test]
fn lap_timer_label_typography_is_a_strict_ingress_contract() {
    let base = json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "lap_timer",
            "display_type": "lap_timer",
            "lap_timer_mode": "current_lap",
            "x": 10.0,
            "y": 20.0,
            "font": "Arial.ttf",
            "font_size": 72.0,
            "color": "#abcdef",
            "opacity": 1.0,
            "show_label": true,
            "label": "Current Lap",
            "label_font": "Teko.ttf",
            "label_font_size": 24.0,
            "label_color": "#123456",
            "positive_delta_color": "#00ff00",
            "negative_delta_color": "#ff0000"
        }],
        "plots": []
    });

    let mut missing_font = base.clone();
    missing_font["values"][0]
        .as_object_mut()
        .unwrap()
        .remove("label_font");
    let error = validate_config_value(&missing_font)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("values[0].label_font"));

    let mut invalid_size = base.clone();
    invalid_size["values"][0]["label_font_size"] = json!(0);
    let error = validate_config_value(&invalid_size)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("values[0].label_font_size"));

    let mut invalid_color = base;
    invalid_color["values"][0]["label_color"] = json!("red");
    let error = validate_config_value(&invalid_color)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("values[0].label_color"));
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
fn renderer_lap_log_covers_out_lap_first_lap_and_activity_wide_completions() {
    let mut activity = lap_activity();

    let out_lap = lap_log_text_state(&activity, 0).unwrap();
    assert!(out_lap.completed_rows.is_empty());
    assert_eq!(out_lap.current_row, None);

    let first_lap = lap_log_text_state(&activity, 1).unwrap();
    assert!(first_lap.completed_rows.is_empty());
    let first_lap_current = first_lap.current_row.unwrap();
    assert_eq!(first_lap_current.cells, ["1", "00:00.50", "+0.00"]);
    assert_eq!(first_lap_current.delta_seconds, None);

    let after_two_completions = lap_log_text_state(&activity, 3).unwrap();
    assert_eq!(
        after_two_completions
            .completed_rows
            .iter()
            .map(|row| row.cells.clone())
            .collect::<Vec<_>>(),
        vec![
            ["2".to_string(), "00:03.00".to_string(), "-1.00".to_string(),],
            ["1".to_string(), "00:04.00".to_string(), "+0.00".to_string(),],
        ]
    );
    assert_eq!(
        after_two_completions.completed_rows[0].delta_seconds,
        Some(-1.0)
    );
    let current_row = after_two_completions.current_row.unwrap();
    assert_eq!(current_row.cells, ["3", "00:01.00", "-0.25"]);
    assert_eq!(current_row.delta_seconds, Some(-0.25));

    activity.series.lap_number[3] = Some(-1);
    activity.series.lap_time_seconds[3] = None;
    let returned_to_out_lap = lap_log_text_state(&activity, 3).unwrap();
    assert_eq!(returned_to_out_lap.completed_rows.len(), 2);
    assert_eq!(returned_to_out_lap.current_row, None);
}

#[test]
fn renderer_lap_log_keeps_original_lap_numbers_when_scene_starts_mid_lap() {
    let mut activity = lap_activity();
    activity.frame_count = 2;
    activity.frame_elapsed_seconds = vec![0.0, 1.0];
    activity.series.lap_number = vec![Some(1), Some(2)];
    activity.series.lap_time_seconds = vec![Some(2.5), Some(0.0)];
    activity.series.delta_to_best_lap_seconds = vec![Some(-0.5), Some(0.0)];
    activity.series.lap_start_elapsed_seconds = vec![-4.5, -0.5, 1.0];

    let scene_start = lap_log_text_state(&activity, 0).unwrap();
    assert_eq!(scene_start.completed_rows[0].cells[0], "1");
    assert_eq!(scene_start.current_row.unwrap().cells[0], "2");

    let completion_inside_scene = lap_log_text_state(&activity, 1).unwrap();
    assert_eq!(completion_inside_scene.completed_rows[0].cells[0], "2");
    assert_eq!(completion_inside_scene.current_row.unwrap().cells[0], "3");
}

#[test]
fn lap_log_config_requests_all_canonical_lap_series() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "lap_timer",
            "display_type": "lap_timer",
            "lap_timer_mode": "lap_log",
            "x": 10.0,
            "y": 20.0,
            "font": "Arial.ttf",
            "font_size": 72.0,
            "color": "#abcdef",
            "label_font": "Teko.ttf",
            "label_font_size": 24.0,
            "label_color": "#123456",
            "opacity": 1.0,
            "show_label": true,
            "label": "Lap Times",
            "positive_delta_color": "#00ff00",
            "negative_delta_color": "#ff0000"
        }],
        "plots": []
    }));

    let PreparedValue::LapTimer(widget) = &config.values[0] else {
        panic!("expected a validated lap timer");
    };
    assert_eq!(widget.validated.mode, LapTimerMode::LapLog);
    assert_eq!(widget.validated.label, "Lap Times");

    let requirements = config.render_data_requirements().unwrap();
    assert!(requirements.lap_number);
    assert!(requirements.lap_time_seconds);
    assert!(requirements.delta_to_best_lap_seconds);
}

#[test]
fn renderer_text_uses_canonical_aligned_lap_time() {
    let mut activity = lap_activity();
    activity.series.lap_time_seconds[2] = Some(12.34);

    assert_eq!(
        lap_timer_value_text(LapTimerMode::CurrentLap, &activity, 2).unwrap(),
        "00:12.34"
    );
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
