//! Command orchestration integration tests.
//!
//! Exercises `backend_render`, `derive_composite_render_plan`, and
//! composite timing normalization — the
//! main Tauri-command dispatch layer. Verifies transparent vs composite
//! branch routing, missing-field validation, sync-offset timing, overlay
//! update-rate calculations, and default render duration derivation.
//!
//! ## Fixtures
//!
//! - `test_config::sample_video_path()` — representative MP4 for composite
//!   render-through tests (requires ffmpeg).
//! - Synthetic activity JSON and config JSON constructed in-line for
//!   controlled timing scenarios.
//!
//! ## Type
//! Integration test. Runs live renders through the full pipeline for
//! composite paths; transparent-branch tests are pure data path.
//! Requires ffmpeg for composite render-through tests.
//!
//! ## Regressions guarded
//! - Transparent/composite branch misrouting
//! - Composite validation rejecting required fields
//! - Sync-offset misaligning dense activity timing
//! - Lower update rate producing wrong overlay FPS
//! - Impossible trim windows silently defaulting instead of erroring

mod common;

use std::path::PathBuf;

use serde_json::Value;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::activity::{build_dense_activity_report_validated, parse_activity_json};
use ovrley_core::commands::backend_render;
use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::pipeline::composite_plan::derive_composite_render_plan;
use ovrley_core::encode::progress::RenderController;
use ovrley_core::error::CoreError;
use ovrley_core::normalize::raw::parse_config_json;
use ovrley_core::normalize::raw::RenderConfig;
use ovrley_core::normalize::validate_render_config;
use ovrley_core::output::RenderOutputKind;
use ovrley_core::paths::AppPaths;

/// Verifies the transparent render branch does not alter dense activity
/// timing. A transparent config with 5–15s window at 30 FPS should produce
/// exactly 300 frames with the first speed sample at 5.0 (the synthetic
/// activity uses elapsed seconds as speed values).
#[test]
fn test_3_1_transparent_render_branch_keeps_original_dense_timing() {
    let config = transparent_config(5.0, 15.0, 30.0);
    let activity = synthetic_activity();

    let validated = validate_render_config(config.clone()).unwrap();
    assert!(validated.scene.composite_video_path.is_none());
    let dense = build_dense_activity_report_validated(&activity, &validated).unwrap();

    assert_eq!(dense.frame_count, 300);
    assert_eq!(dense.series.speed.first().copied().flatten(), Some(5.0));
    assert_eq!(dense.frame_elapsed_seconds.first().copied(), Some(0.0));
}

/// Verifies the composite branch gate: `backend_render` must
/// activate the composite branch only when `composite_video_path` is set.
/// Uses a synthetic activity and validates the render starts (controller
/// reports `total > 0`).
#[test]
fn test_3_2_composite_branch_activates_only_when_video_path_is_present() {
    let paths = AppPaths::from_repo_root(PathBuf::from("."));
    let controller = RenderController::default();
    let result = backend_render(
        &paths,
        &controller,
        &composite_config_json(
            r#"
                "composite_video_path": "input.mp4",
                "composite_bitrate": "60M",
                "composite_sync_offset": 0.0,
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 10.0,
                "composite_video_trim_start": 0.0,
                "composite_widget_update_rate": 1
                "#,
        ),
        &synthetic_activity_json(),
        &render_output_path("branch"),
        RenderOutputKind::Composite,
        false,
    )
    .unwrap();

    assert_eq!(result.get("started").and_then(Value::as_bool), Some(true));
    assert!(controller.progress().total > 0);
}

#[test]
fn output_rejection_precedes_malformed_activity_processing() {
    let paths = AppPaths::from_repo_root(PathBuf::from("."));
    let controller = RenderController::default();
    let output_path = std::env::temp_dir().join(format!(
        "ovrley-command-rejection-{}.mov",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&output_path, b"existing").unwrap();

    let error = backend_render(
        &paths,
        &controller,
        "not json",
        "not json",
        output_path.to_str().unwrap(),
        RenderOutputKind::Transparent,
        false,
    )
    .unwrap_err();

    assert!(matches!(error, CoreError::OutputExists(_)));
    assert_eq!(controller.progress().status, "idle");
    std::fs::remove_file(output_path).unwrap();
}

#[test]
fn test_3_2b_composite_clamps_tiny_video_overrun_to_activity_end() {
    let paths = AppPaths::from_repo_root(PathBuf::from("."));
    let controller = RenderController::default();
    let result = backend_render(
        &paths,
        &controller,
        &composite_config_json(
            r#"
                "composite_video_path": "input.mp4",
                "composite_bitrate": "60M",
                "composite_sync_offset": 0.0,
                "composite_video_fps_num": 30,
                "composite_video_fps_den": 1,
                "composite_video_duration": 384.384,
                "composite_render_duration": 384.384,
                "composite_video_trim_start": 0.0,
                "composite_widget_update_rate": 1
                "#,
        ),
        &short_fractional_activity_json(),
        &render_output_path("clamp"),
        RenderOutputKind::Composite,
        false,
    )
    .unwrap();

    assert_eq!(result.get("started").and_then(Value::as_bool), Some(true));
    assert_eq!(controller.progress().total, 11530);
}

#[test]
/// Verifies the composite render branch is activated and reaches the pipeline
/// shell for a short composite render using a real video fixture.
///
/// Uses `test_config::sample_video_path()` (test-1080p.mp4). Configures a
/// 0.2s composite render at 29.97 FPS with 2x widget update rate and 300s
/// sync offset. Polls the controller until completion and verifies the
/// output filename matches the explicit custom target.
///
/// Requires live ffmpeg and the video fixture on disk.
///
/// Regressions guarded: composite branch silently falling back to transparent
/// path, progress never reaching `complete`, encoded frame count mismatch.
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_4_3_composite_branch_reaches_pipeline_shell() {
    let ws_root = common::test_config::workspace_root();
    let paths = test_paths(ws_root.clone());
    let controller = RenderController::default();
    let video_path = common::test_config::sample_video_path();

    let result = backend_render(
        &paths,
        &controller,
        &composite_config_json(&format!(
            r#"
                "composite_video_path": "{}",
                "composite_bitrate": "60M",
                "composite_sync_offset": 300.0,
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 0.2,
                "composite_widget_update_rate": 2,
                "composite_video_trim_start": 0.0
                "#,
            video_path.to_string_lossy().replace('\\', "\\\\")
        )),
        &synthetic_activity_json(),
        &render_output_path("pipeline"),
        RenderOutputKind::Composite,
        false,
    )
    .unwrap();

    assert_eq!(result.get("started").and_then(Value::as_bool), Some(true));
    let progress = wait_for_completed_progress(&controller);
    assert_eq!(progress.status, "complete", "{}", progress.message);
    assert_eq!(progress.total, 6);
    assert_eq!(progress.encoded, 6);
    assert!(progress
        .filename
        .as_deref()
        .unwrap_or_default()
        .starts_with("ovrley-command-pipeline-"));
}

/// Plan derivation must reject composite configs that omit `composite_bitrate`.
#[test]
fn test_3_3_missing_bitrate_validation() {
    let mut scene = composite_validated_scene(
        r#"
            "composite_video_path": "input.mp4",
            "composite_sync_offset": 0.0,
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 1
            "#,
    );
    let error = derive_composite_render_plan(&mut scene, None).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid configuration: scene.composite_bitrate required for composite render"
    );
}

/// Plan derivation must reject composite configs that omit either FPS field
/// (numerator or denominator), giving a field-specific error for each.
#[test]
fn test_3_4_missing_fps_validation() {
    let mut missing_num = composite_validated_scene(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 0.0,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 1
            "#,
    );
    let mut missing_den = composite_validated_scene(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 0.0,
            "composite_video_fps_num": 30000,
            "composite_video_duration": 20.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 1
            "#,
    );

    assert_eq!(
        derive_composite_render_plan(&mut missing_num, None)
            .unwrap_err()
            .to_string(),
        "Invalid configuration: scene.composite_video_fps_num required for composite render"
    );
    assert_eq!(
        derive_composite_render_plan(&mut missing_den, None)
            .unwrap_err()
            .to_string(),
        "Invalid configuration: scene.composite_video_fps_den required for composite render"
    );
}

#[test]
/// Verifies the sync offset is correctly applied to dense activity timing.
///
/// Configures a composite render with a 300-second sync offset, derives the
/// composite render plan, applies scene timing, and builds a dense activity
/// report. Verifies that scene.start equals the sync offset and that the
/// first speed value in the dense report matches the offset (synthetic
/// activity data uses elapsed seconds as speed values).
///
/// Regressions guarded: sync offset not propagated to scene timing,
/// dense activity report using wrong time base after trim.
fn test_3_5_dense_report_timing_for_sync_offset() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 300.0,
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_render_duration": 10.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 1
            "#,
    );
    let mut validated = validate_render_config(config).unwrap();
    derive_composite_render_plan(&mut validated.scene, None).unwrap();
    let dense = build_dense_activity_report_validated(&synthetic_activity(), &validated).unwrap();

    assert_eq!(validated.scene.start, 300.0);
    assert_eq!(validated.scene.end, 310.0);
    assert!((validated.scene.fps - (30000.0 / 1001.0)).abs() < 1e-9);
    assert_eq!(dense.series.speed.first().copied().flatten(), Some(300.0));
}

#[test]
/// Verifies overlay update rate reduces the overlay pipe FPS by exactly
/// the integer factor while preserving the output FPS.
///
/// Configures a 59.94 FPS source with 2x widget update rate. Expects the
/// overlay pipe FPS to be halved (29.97 FPS), scene.fps to match the
/// overlay pipe FPS, and widget_update_rate() to return 1 (the per-frame
/// multiplier, not the divisor).
///
/// Regressions guarded: update rate applied as a multiplier instead of
/// divisor, output FPS incorrectly overridden by overlay pipe FPS.
fn test_3_6_dense_report_timing_for_lower_overlay_update_rate() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 0.0,
            "composite_video_fps_num": 60000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_render_duration": 10.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 2
            "#,
    );
    let mut validated = validate_render_config(config).unwrap();
    derive_composite_render_plan(&mut validated.scene, None).unwrap();
    let dense = build_dense_activity_report_validated(&synthetic_activity(), &validated).unwrap();

    assert!((validated.scene.fps - (30000.0 / 1001.0)).abs() < 1e-9);
    assert_eq!(validated.scene.update_rate.get(), 1);
    assert_eq!(dense.frame_count, 300);
}

/// When `composite_render_duration` is missing, the planner defaults to
/// remaining video duration after trim start (60s video - 10s trim = 50s).
#[test]
fn test_3_7_render_duration_defaults_to_remaining_video_after_trim() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 0.0,
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 60.0,
            "composite_video_trim_start": 10.0,
            "composite_widget_update_rate": 1
            "#,
    );

    let mut scene = ovrley_core::normalize::validate_scene_config(config.scene.clone()).unwrap();
    derive_composite_render_plan(&mut scene, None).unwrap();

    assert_eq!(scene.end - scene.start, 50.0);
}

/// Trim start >= video duration is an immediate plan-derivation error,
/// not a silent default.
#[test]
fn test_3_8_rejects_impossible_trim() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 0.0,
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 60.0,
            "composite_video_trim_start": 60.0,
            "composite_widget_update_rate": 1
            "#,
    );

    let mut scene = ovrley_core::normalize::validate_scene_config(config.scene.clone()).unwrap();
    let error = derive_composite_render_plan(&mut scene, None).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid configuration: scene.composite_video_trim_start (60) must be less than scene.composite_video_duration (60)"
    );
}

/// Canonical speed value from the shared test-common fixture.
fn explicit_speed_value_json() -> serde_json::Value {
    common::builders::speed_value_json()
}

/// Builds a minimal transparent-render config with one speed value and no
/// composite fields — used to verify the transparent branch is selected.
fn transparent_config(start: f64, end: f64, fps: f64) -> RenderConfig {
    let mut scene = common::builders::scene_json();
    scene["fps"] = serde_json::json!(fps);
    scene["start"] = serde_json::json!(start);
    scene["end"] = serde_json::json!(end);
    let config = serde_json::json!({
        "scene": scene,
        "values": [explicit_speed_value_json()],
        "labels": [],
        "plots": []
    });
    parse_config_json(&config.to_string()).unwrap()
}

/// Builds a composite config from extra scene-level JSON fields injected
/// into a boilerplate `scene` block — keeps individual tests concise.
fn composite_config(extra_scene_fields: &str) -> RenderConfig {
    let json_str = composite_config_json(extra_scene_fields);
    let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    serde_json::from_value(value).unwrap()
}

fn composite_validated_scene(
    extra_scene_fields: &str,
) -> ovrley_core::normalize::ValidatedSceneConfig {
    let config = composite_config(extra_scene_fields);
    ovrley_core::normalize::validate_scene_config(config.scene).unwrap()
}

/// Returns the full JSON template string for a composite config, splicing
/// `extra_scene_fields` into the `scene` block.
fn composite_config_json(extra_scene_fields: &str) -> String {
    let mut scene = common::builders::scene_json();
    scene["ffmpeg"] = serde_json::json!({"codec": "libx264"});
    let extra: serde_json::Value =
        serde_json::from_str(&format!("{{{}}}", extra_scene_fields)).unwrap();
    if let serde_json::Value::Object(map) = &extra {
        for (k, v) in map {
            scene[k] = v.clone();
        }
    }
    serde_json::json!({
        "scene": scene,
        "values": [explicit_speed_value_json()],
        "labels": [],
        "plots": []
    })
    .to_string()
}

/// Parses the shared synthetic activity fixture used by command tests.
/// The fixture uses elapsed seconds as speed values so that speed == time.
fn synthetic_activity() -> ParsedActivity {
    parse_activity_json(&synthetic_activity_json()).unwrap()
}

/// Provides the shared synthetic activity JSON string used by command tests.
fn synthetic_activity_json() -> String {
    r#"{
            "sample_elapsed_seconds":[0.0,600.0],
            "sample_distance_progress":[0.0,1.0],
            "trim_start_seconds":0.0,
            "trim_end_seconds":600.0,
            "speed":[0.0,600.0]
        }"#
    .to_string()
}

fn short_fractional_activity_json() -> String {
    r#"{
            "sample_elapsed_seconds":[0.0,384.324],
            "sample_distance_progress":[0.0,1.0],
            "trim_start_seconds":0.0,
            "trim_end_seconds":384.324,
            "speed":[0.0,384.324]
        }"#
    .to_string()
}

/// Builds an isolated `AppPaths` workspace for the command integration
/// tests, rooted under `target/command_tests` so side effects never land
/// in user-facing directories.
fn test_paths(ws_root: PathBuf) -> AppPaths {
    let git_root = common::test_config::repo_git_root();
    let test_root = ws_root.join("target").join("command_tests");
    AppPaths {
        repo_root: git_root.clone(),
        font_dirs: vec![git_root.join("fonts")],
        debug_render_dir: test_root.join("debug_render"),
        temp_dir: test_root.join("tmp"),
        bundled_templates_dirs: vec![git_root.join("templates")],
        user_templates_dir: test_root.join("templates"),
        downloads_dir: test_root.join("downloads"),
    }
}

/// Polls the controller every 20ms for up to 6 seconds, returning the
/// final progress snapshot once the render reaches `complete` or `error`.
/// If the timeout expires, returns whatever progress exists at that point.
fn wait_for_completed_progress(controller: &RenderController) -> RenderProgress {
    for _ in 0..300 {
        let progress = controller.progress();
        if progress.status == "complete" || progress.status == "error" {
            return progress;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    controller.progress()
}

fn render_output_path(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("ovrley-command-{name}-{}.mp4", std::process::id()))
        .to_string_lossy()
        .into_owned()
}
