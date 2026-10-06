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

/// Materialized shared presentation for video-local planning/pipeline tests.
pub fn batch_template(
) -> ovrley_core::normalize::raw::RenderConfig<ovrley_core::normalize::raw::ScenePresentationConfig>
{
    let mut scene = super::seam::explicit_scene_json();
    for key in [
        "start",
        "end",
        "fps",
        "update_rate",
        "ffmpeg",
        "custom_export_range_active",
    ] {
        scene.as_object_mut().unwrap().remove(key);
    }
    serde_json::from_value(serde_json::json!({
        "scene": scene,
        "backdrops": [{"id":"art", "display_type":"circle", "x":8, "y":8, "diameter":8,
            "opacity":1, "fill_color":"#ffffff", "fill_opacity":1, "border_color":"#ffffff", "border_opacity":1, "border_thickness":0}],
        "rasters": [],
        "labels": [{"text":"A", "x":4, "y":4, "font":"Teko.ttf", "font_size":8, "color":"#ffffff", "opacity":1,
            "font_weight":400, "italic":false, "letter_spacing":0}],
        "values": [speed_value_json()], "plots": [],
    })).unwrap()
}

/// Captures the plan produced by real submission/preparation, replacing helper-only ingress tests.
pub fn batch_video_plan(
    paths: &ovrley_core::paths::AppPaths,
    inspection: &ovrley_core::render_jobs::inspection::VideoInspectionService,
    inspection_id: &str,
    request: ovrley_core::render_jobs::contracts::BatchRenderRequest,
) -> ovrley_core::error::CoreResult<ovrley_core::render_jobs::batch_plan::PlannedVideoRender> {
    use ovrley_core::activity::schema::ParsedActivity;
    use ovrley_core::error::{CoreError, CoreResult};
    use ovrley_core::render_jobs::{
        batch::BatchJobExecutor,
        batch_plan::PlannedVideoRender,
        execution::{RenderExecutionService, RendererReservation},
    };
    struct Capture(std::sync::Mutex<std::sync::mpsc::Sender<PlannedVideoRender>>);
    impl BatchJobExecutor for Capture {
        fn embedded_activity(
            &self,
            _: &ovrley_core::paths::AppPaths,
            _: &ovrley_core::media::prepared_video::InspectedVideoSource,
        ) -> CoreResult<ParsedActivity> {
            unreachable!("external activity")
        }
        fn execute(
            &self,
            _: &ovrley_core::paths::AppPaths,
            plan: PlannedVideoRender,
            _: &ParsedActivity,
            _: &RendererReservation,
            target: &ovrley_core::output::RenderOutputTarget,
        ) -> CoreResult<String> {
            self.0.lock().unwrap().send(plan).unwrap();
            Ok(target
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned())
        }
    }
    assert_eq!(request.inspection_id, inspection_id);
    let (send, receive) = std::sync::mpsc::channel();
    let service = RenderExecutionService::default();
    service
        .submit_batch_with_executor(
            paths,
            inspection,
            request,
            None,
            std::sync::Arc::new(Capture(std::sync::Mutex::new(send))),
        )
        .map_err(|error| CoreError::Config(format!("Submission failed: {error:?}")))?;
    drop(service); // Join the owning worker before observing its preparation outcome.
    receive
        .try_recv()
        .map_err(|_| CoreError::Config("Batch preparation failed".into()))
}

pub fn video_batch_request(
    inspection_id: &str,
    source: &ovrley_core::media::prepared_video::InspectedVideoSource,
    encoding: ovrley_core::render_jobs::contracts::BatchEncodingSettings,
    template: ovrley_core::normalize::raw::RenderConfig<
        ovrley_core::normalize::raw::ScenePresentationConfig,
    >,
    activity: ovrley_core::activity::schema::ParsedActivity,
    offset: f64,
    skip_overlay: bool,
) -> ovrley_core::render_jobs::contracts::BatchRenderRequest {
    use ovrley_core::render_jobs::contracts::*;
    let directory = std::path::Path::new(&source.metadata.path)
        .parent()
        .unwrap();
    BatchRenderRequest {
        inspection_id: inspection_id.into(),
        template,
        encoding,
        activity: BatchActivity::ExternalActivity {
            activity,
            timezone_mode: VideoSyncTimezoneMode::Utc,
            reference: None,
            automatic_offsets: [(source.source_id.clone(), offset)].into(),
        },
        output_directory: directory.to_str().unwrap().into(),
        jobs: vec![BatchRenderJob {
            id: "video".into(),
            source_id: source.source_id.clone(),
            skip_overlay,
        }],
    }
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
        full_activity_metric_ranges: Default::default(),
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
        full_activity_metric_ranges: Default::default(),
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
        full_activity_metric_ranges: Default::default(),
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
