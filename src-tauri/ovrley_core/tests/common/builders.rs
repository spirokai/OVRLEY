//! Shared test-data builders.
//!
//! Centralises repeated JSON blobs and struct constructors so that a new
//! metric-series field only needs to be touched in one place.

#![allow(dead_code)]

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport, TrimmedActivity};
use serde_json::Value;

const COMMON: &str = include_str!("../fixtures/config/test-common.json");

fn root() -> Value {
    serde_json::from_str(COMMON).expect("fixtures/config/test-common.json must be valid JSON")
}

// ── JSON blobs ──────────────────────────────────────────────────────────

pub fn scene_json() -> Value {
    root()["scene"].clone()
}

pub fn speed_value_json() -> Value {
    root()["values"]["speed"].clone()
}

pub fn heading_tape_json() -> Value {
    root()["values"]["heading_tape"].clone()
}

// ── Dense series / activity ─────────────────────────────────────────────

pub fn empty_dense_series() -> DenseSeriesReport {
    DenseSeriesReport {
        speed: vec![],
        distance: vec![],
        elevation: vec![],
        calories: vec![],
        distance_to_home: vec![],
        total_ascent: vec![],
        barometric_altitude: vec![],
        gradient: vec![],
        heartrate: vec![],
        cadence: vec![],
        power: vec![],
        engine_power: vec![],
        engine_load: vec![],
        temperature: vec![],
        pace: vec![],
        g_force: vec![],
        g_force_x: vec![],
        g_force_y: vec![],
        g_force_z: vec![],
        rpm: vec![],
        throttle_position: vec![],
        brake_position: vec![],
        lean_angle: vec![],
        air_pressure: vec![],
        ground_contact_time: vec![],
        left_right_balance: vec![],
        stride_length: vec![],
        stroke_rate: vec![],
        torque: vec![],
        vertical_speed: vec![],
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
        heading: vec![],
        course_lat: vec![],
        course_lon: vec![],
        time: vec![],
        lap_number: vec![],
        lap_time_seconds: vec![],
        lap_start_elapsed_seconds: vec![],
        delta_to_best_lap_seconds: vec![],
        lap_durations_seconds: vec![],
        lap_durations_best_so_far_seconds: vec![],
    }
}

pub fn minimal_dense_activity() -> DenseActivityReport {
    DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        full_activity_distance: None,
        full_activity_total_ascent: None,
        series: empty_dense_series(),
    }
}

pub fn dense_report_with(fill: impl FnOnce(&mut DenseSeriesReport)) -> DenseActivityReport {
    let mut series = empty_dense_series();
    fill(&mut series);
    DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        full_activity_distance: None,
        full_activity_total_ascent: None,
        series,
    }
}

// ── TrimmedActivity ─────────────────────────────────────────────────────

pub fn minimal_trimmed_activity(times: Vec<f64>) -> TrimmedActivity {
    TrimmedActivity {
        sync_time: None,
        sample_elapsed_seconds: times,
        sample_distance_progress: vec![],
        course: vec![],
        elevation: vec![],
        calories: vec![],
        distance_to_home: vec![],
        total_ascent: vec![],
        barometric_altitude: vec![],
        speed: vec![],
        distance: vec![],
        full_activity_distance: None,
        full_activity_total_ascent: None,
        heartrate: vec![],
        cadence: vec![],
        power: vec![],
        engine_power: vec![],
        engine_load: vec![],
        temperature: vec![],
        pace: vec![],
        g_force: vec![],
        g_force_x: vec![],
        g_force_y: vec![],
        g_force_z: vec![],
        rpm: vec![],
        throttle_position: vec![],
        brake_position: vec![],
        lean_angle: vec![],
        air_pressure: vec![],
        ground_contact_time: vec![],
        left_right_balance: vec![],
        stride_length: vec![],
        stroke_rate: vec![],
        torque: vec![],
        vertical_speed: vec![],
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
        heading: vec![],
        lap_number: vec![],
        lap_time_seconds: vec![],
        lap_start_elapsed_seconds: vec![],
        delta_to_best_lap_seconds: vec![],
        lap_durations_seconds: vec![],
        lap_durations_best_so_far_seconds: vec![],
    }
}
