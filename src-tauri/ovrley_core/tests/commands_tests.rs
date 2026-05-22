mod common;

use std::path::PathBuf;

use serde_json::Value;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::commands::{backend_render, is_composite_render, AppPaths};
use ovrley_core::config::parse_config_json;
use ovrley_core::config::RenderConfig;
use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::video::RenderController;
use ovrley_core::encode::video_composite_pipeline::{
    apply_composite_scene_timing, derive_composite_render_plan,
};

#[test]
fn test_3_1_transparent_render_branch_keeps_original_dense_timing() {
    let config = transparent_config(5.0, 15.0, 30.0);
    let activity = synthetic_activity();

    assert!(!is_composite_render(&config));
    let dense = build_dense_activity_report(&activity, &config).unwrap();

    assert_eq!(dense.frame_count, 300);
    assert_eq!(dense.series.speed.first().copied().flatten(), Some(5.0));
    assert_eq!(dense.frame_elapsed_seconds.first().copied(), Some(0.0));
}

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
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 10.0
                "#,
        ),
        &synthetic_activity_json(),
    )
    .unwrap();

    assert_eq!(result.get("started").and_then(Value::as_bool), Some(true));
    assert!(controller.progress().total > 0);
}

#[test]
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
                "width": 3840,
                "height": 2160,
                "composite_video_path": "{}",
                "composite_bitrate": "60M",
                "composite_sync_offset": 300.0,
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 0.2,
                "composite_widget_update_rate": 2
                "#,
            video_path.to_string_lossy().replace('\\', "\\\\")
        )),
        &synthetic_activity_json(),
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
        .starts_with("video_composited_"));
}

#[test]
fn test_3_3_missing_bitrate_validation() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0
            "#,
    );

    let error = derive_composite_render_plan(&config).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid configuration: scene.composite_bitrate required for composite render"
    );
}

#[test]
fn test_3_4_missing_fps_validation() {
    let missing_num = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0
            "#,
    );
    let missing_den = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_video_fps_num": 30000,
            "composite_video_duration": 20.0
            "#,
    );

    assert_eq!(
        derive_composite_render_plan(&missing_num)
            .unwrap_err()
            .to_string(),
        "Invalid configuration: scene.composite_video_fps_num required for composite render"
    );
    assert_eq!(
        derive_composite_render_plan(&missing_den)
            .unwrap_err()
            .to_string(),
        "Invalid configuration: scene.composite_video_fps_den required for composite render"
    );
}

#[test]
fn test_3_5_dense_report_timing_for_sync_offset() {
    let mut config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_sync_offset": 300.0,
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_render_duration": 10.0,
            "composite_widget_update_rate": 1
            "#,
    );
    let plan = derive_composite_render_plan(&config).unwrap();
    apply_composite_scene_timing(&mut config, &plan);

    let dense = build_dense_activity_report(&synthetic_activity(), &config).unwrap();

    assert_eq!(config.scene.start, 300.0);
    assert_eq!(config.scene.end, 310.0);
    assert!((config.scene.fps - (30000.0 / 1001.0)).abs() < 1e-9);
    assert_eq!(dense.series.speed.first().copied().flatten(), Some(300.0));
}

#[test]
fn test_3_6_dense_report_timing_for_lower_overlay_update_rate() {
    let mut config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_video_fps_num": 60000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 20.0,
            "composite_render_duration": 10.0,
            "composite_widget_update_rate": 2
            "#,
    );
    let plan = derive_composite_render_plan(&config).unwrap();
    apply_composite_scene_timing(&mut config, &plan);

    let dense = build_dense_activity_report(&synthetic_activity(), &config).unwrap();

    assert_eq!(plan.overlay_pipe_fps, Fps::new(30000, 1001).unwrap());
    assert!((config.scene.fps - (30000.0 / 1001.0)).abs() < 1e-9);
    assert_eq!(config.widget_update_rate(), 1);
    assert_eq!(dense.frame_count, 300);
}

#[test]
fn test_3_7_render_duration_defaults_to_remaining_video_after_trim() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 60.0,
            "composite_video_trim_start": 10.0
            "#,
    );

    let plan = derive_composite_render_plan(&config).unwrap();

    assert_eq!(plan.render_duration, 50.0);
}

#[test]
fn test_3_8_rejects_impossible_trim() {
    let config = composite_config(
        r#"
            "composite_video_path": "input.mp4",
            "composite_bitrate": "60M",
            "composite_video_fps_num": 30000,
            "composite_video_fps_den": 1001,
            "composite_video_duration": 60.0,
            "composite_video_trim_start": 60.0
            "#,
    );

    let error = derive_composite_render_plan(&config).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid configuration: scene.composite_video_trim_start (60) must be less than scene.composite_video_duration (60)"
    );
}

fn transparent_config(start: f64, end: f64, fps: f64) -> RenderConfig {
    parse_config_json(&format!(
        r#"{{
                "scene":{{"fps":{fps},"start":{start},"end":{end},"ffmpeg":{{}}}},
                "values":[{{"value":"speed","x":0,"y":0}}],
                "labels":[],
                "plots":[]
            }}"#
    ))
    .unwrap()
}

fn composite_config(extra_scene_fields: &str) -> RenderConfig {
    parse_config_json(&composite_config_json(extra_scene_fields)).unwrap()
}

fn composite_config_json(extra_scene_fields: &str) -> String {
    format!(
        r#"{{
                "scene":{{
                    "fps":30,
                    "start":0,
                    "end":10,
                    "ffmpeg":{{}},
                    {extra_scene_fields}
                }},
                "values":[{{"value":"speed","x":0,"y":0}}],
                "labels":[],
                "plots":[]
            }}"#
    )
}

fn synthetic_activity() -> ParsedActivity {
    parse_activity_json(&synthetic_activity_json()).unwrap()
}

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
