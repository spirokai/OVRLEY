//! Shared composite-pipeline test harness helpers.
//!
//! This module owns only integration-test support for
//! `video_composite_pipeline_tests.rs`: fixture config builders, render
//! harness helpers, ffprobe readers, debug-summary readers, and small
//! argument assertions. It deliberately does not re-implement production
//! pipeline logic; planner behavior, progress math, stderr formatting, and
//! success verification must come from `ovrley_core` production modules.

// This helper module is compiled into multiple integration-test crates via
// `tests/common/mod.rs`, but only the composite test suite uses most items.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::paths::AppPaths;
use ovrley_core::config::{parse_config_json, RenderConfig};
use ovrley_core::debug::RenderProfiler;
use ovrley_core::encode::ffmpeg_composite::{
    build_composite_ffmpeg_settings, CompositeFfmpegBuildRequest, HwAccelInfo,
};
use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::video::RenderController;
use ovrley_core::encode::video_composite_debug::{
    write_composite_timing_summary, CompositeTimingSummaryInput,
};
use ovrley_core::encode::video_composite_pipeline::{
    derive_composite_pipeline_plan, render_composite_video_single, CompositePipelinePlan,
};
use serde_json::Value;

/// Bundles the key artifacts produced by a fixture composite render.
///
/// Tests inspect the controller progress plus the finalized output path and
/// file size without needing to know how the fixture directories were built.
pub struct RenderFixtureResult {
    /// Keeps the full test paths available for assertions that inspect the
    /// render workspace, even if most tests only read the output path.
    #[allow(dead_code)]
    pub paths: AppPaths,
    /// Exposes render progress and cancellation state to the caller.
    pub controller: RenderController,
    /// Points to the finalized composite output file.
    pub output_path: PathBuf,
    /// Stores the final output size so tests can assert non-empty output.
    pub output_size: u64,
}

/// Builds a minimal composite config and delegates plan derivation to the
/// production planner.
///
/// This helper exists only to keep tests concise. All timing and ffmpeg-plan
/// behavior still comes from `derive_composite_pipeline_plan(...)`.
pub fn derive_fixture_composite_plan(
    scene_prefix: &str,
    fps_num: u32,
    fps_den: u32,
    video_duration: f64,
    render_duration: Option<f64>,
    trim_start: Option<f64>,
    update_rate: Option<u32>,
) -> CompositePipelinePlan {
    let config = parse_config_json(&format!(
        r#"{{
            "scene":{{
                "fps":30,
                "start":0,
                "end":10,
                {scene_prefix}
            }},
            "values":[],
            "labels":[],
            "plots":[]
        }}"#
    ))
    .unwrap();
    let paths = AppPaths::from_repo_root(PathBuf::from("."));

    derive_composite_pipeline_plan(
        &paths,
        &config,
        "input.mp4",
        "60M",
        fps_num,
        fps_den,
        video_duration,
        render_duration,
        trim_start,
        update_rate,
    )
    .unwrap()
}

/// Renders the shared composite fixture and returns the output details.
///
/// This is the primary black-box render helper for success-path tests.
pub fn render_fixture_composite(
    video_path: &str,
    fps_num: u32,
    fps_den: u32,
    render_duration: f64,
    update_rate: u32,
    sync_offset: f64,
    width: u32,
    height: u32,
) -> RenderFixtureResult {
    let paths = test_paths();
    render_fixture_composite_with_paths(
        paths,
        RenderController::default(),
        video_path,
        fps_num,
        fps_den,
        render_duration,
        update_rate,
        sync_offset,
        width,
        height,
        "libx264",
    )
    .unwrap()
}

/// Starts a composite fixture render on a background thread.
///
/// Cancellation tests use this to flip the controller flag while rendering is
/// in progress and then inspect the resulting success or error value.
pub fn spawn_fixture_composite_render(
    paths: AppPaths,
    controller: RenderController,
    video_path: &str,
    fps_num: u32,
    fps_den: u32,
    render_duration: f64,
    update_rate: u32,
    sync_offset: f64,
    width: u32,
    height: u32,
    codec: &str,
) -> thread::JoinHandle<Result<String, String>> {
    let video_path = video_path.to_string();
    let codec = codec.to_string();
    thread::spawn(move || {
        render_fixture_composite_with_paths(
            paths,
            controller,
            &video_path,
            fps_num,
            fps_den,
            render_duration,
            update_rate,
            sync_offset,
            width,
            height,
            &codec,
        )
        .map(|result| {
            result
                .output_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
    })
}

/// Renders the shared composite fixture into a caller-provided workspace.
///
/// This keeps path construction reusable for tests that need stable output
/// directories, such as cancellation cleanup or debug-summary assertions.
pub fn render_fixture_composite_with_paths(
    paths: AppPaths,
    controller: RenderController,
    video_path: &str,
    fps_num: u32,
    fps_den: u32,
    render_duration: f64,
    update_rate: u32,
    sync_offset: f64,
    width: u32,
    height: u32,
    codec: &str,
) -> Result<RenderFixtureResult, String> {
    let absolute_video_path = crate::common::test_config::fixtures()
        .join("video")
        .join(video_path.trim_start_matches("tmp/"));
    let absolute_video_path = absolute_video_path.to_string_lossy().to_string();
    let mut config = recent_template_config(width, height);
    config.scene.ffmpeg = serde_json::json!({"codec": codec});
    let source_fps = Fps::new(fps_num, fps_den).unwrap();
    let overlay_fps = source_fps.divided_by(update_rate).unwrap();
    config.scene.start = sync_offset;
    config.scene.end = sync_offset + render_duration;
    config.scene.fps = overlay_fps.as_f64();
    config.scene.composite_video_path = Some(absolute_video_path.clone());
    config.scene.composite_bitrate = Some("20M".to_string());
    config.scene.composite_sync_offset = Some(sync_offset);
    config.scene.composite_video_fps_num = Some(fps_num);
    config.scene.composite_video_fps_den = Some(fps_den);
    config.scene.composite_video_duration = Some(render_duration.max(20.0));
    config.scene.composite_render_duration = Some(render_duration);
    config.scene.composite_video_trim_start = Some(0.0);
    config.scene.composite_widget_update_rate = Some(update_rate);

    let activity = fixture_activity();
    let dense_activity = build_dense_activity_report(&activity, &config).unwrap();
    let filename = render_composite_video_single(
        &paths,
        &config,
        &activity,
        &dense_activity,
        &controller,
        &absolute_video_path,
        "20M",
        sync_offset,
        fps_num,
        fps_den,
        render_duration.max(20.0),
        Some(render_duration),
        Some(0.0),
        Some(update_rate),
    )
    .map_err(|error| error.to_string())?;
    let output_path = paths.downloads_dir.join(filename);
    let output_size = fs::metadata(&output_path)
        .map_err(|error| format!("Failed to read output metadata: {error}"))?
        .len();

    Ok(RenderFixtureResult {
        paths,
        controller,
        output_path,
        output_size,
    })
}

/// Returns the sorted list of composite outputs inside a test workspace.
///
/// Cancellation tests compare the pre- and post-render snapshots to confirm
/// that failed or cancelled renders do not leave extra composite files behind.
pub fn composited_outputs(paths: &AppPaths) -> Vec<String> {
    let mut outputs = fs::read_dir(&paths.downloads_dir)
        .unwrap()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("video_composited_") && name.ends_with(".mp4"))
        .collect::<Vec<_>>();
    outputs.sort();
    outputs
}

/// Returns the latest composite timing-summary path in the debug output tree.
pub fn composite_debug_timing_summary_path(paths: &AppPaths) -> PathBuf {
    let phase_dir = paths.debug_render_dir.join("composite");
    let mut summaries = fs::read_dir(&phase_dir)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", phase_dir.display()))
        .flatten()
        .map(|entry| entry.path().join("timing_summary.json"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    summaries.sort();
    summaries.pop().unwrap_or_else(|| {
        panic!(
            "No composite debug timing summary under {}",
            phase_dir.display()
        )
    })
}

/// Loads the latest composite timing summary as JSON.
pub fn composite_debug_timing_summary(paths: &AppPaths) -> Value {
    let json = fs::read_to_string(composite_debug_timing_summary_path(paths)).unwrap();
    serde_json::from_str(&json).unwrap()
}

/// Writes a deterministic composite timing summary for debug-file assertions.
///
/// The fixed output filename makes it easy for tests to assert directory and
/// JSON fields without having to chase a generated timestamp.
pub fn write_fixture_composite_debug_summary(path_name: &str) -> AppPaths {
    let paths = test_paths_named(path_name);
    let _ = fs::remove_dir_all(paths.debug_render_dir.join("composite"));
    let source_fps = Fps::new(60000, 1001).unwrap();
    let overlay_pipe_fps = Fps::new(30000, 1001).unwrap();
    let ffmpeg_settings = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "libx264",
        bitrate: "20M",
        video_path: PathBuf::from("input.mp4").as_path(),
        video_trim_start: 0.0,
        render_duration: 0.2,
        width: 1920,
        height: 1080,
        source_fps,
        overlay_pipe_fps,
        hwaccel_available: &HwAccelInfo::default(),
    })
    .unwrap();
    let output_path = paths
        .downloads_dir
        .join("video_composited_1778853729503903000.mp4");
    let mut profiler = RenderProfiler::default();
    profiler.record_ms("frame.total", 1.0);
    profiler.record_ms("frame.draw", 0.8);
    profiler.record_ms("ffmpeg.write", 0.5);

    write_composite_timing_summary(CompositeTimingSummaryInput {
        debug_render_dir: &paths.debug_render_dir,
        ffmpeg_settings: &ffmpeg_settings,
        output_path: &output_path,
        source_fps,
        overlay_pipe_fps,
        widget_update_rate: 2,
        render_duration: 0.2,
        overlay_frame_count: 6,
        output_frame_count: 12,
        total_ms: 123.4,
        render_loop_ms: 100.0,
        ffmpeg_finalize_wait_ms: 23.4,
        timings: profiler.summary(),
        codec: "libx264",
        bitrate: "20M",
        input_width: 1920,
        input_height: 1080,
        trim_start: 0.0,
        sync_offset: 600.0,
    })
    .unwrap();
    paths
}

/// Loads the shipped recent-template fixture and overrides the output size.
pub fn recent_template_config(width: u32, height: u32) -> RenderConfig {
    let git_root = crate::common::test_config::repo_git_root();
    let template =
        fs::read_to_string(git_root.join("templates").join("recent-template.json")).unwrap();
    let value: Value = serde_json::from_str(&template).unwrap();
    let mut config: RenderConfig =
        serde_json::from_value(value.get("config").unwrap().clone()).unwrap();
    config.scene.width = Some(width);
    config.scene.height = Some(height);
    config.scene.ffmpeg = serde_json::json!({"codec":"libx264"});
    config
}

/// Builds a minimal end-to-end composite render config for segmented tests.
pub fn composite_test_config(render_duration: f64) -> RenderConfig {
    let json = format!(
        r#"{{
        "scene": {{
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "start": 0.0,
            "end": {duration},
            "ffmpeg": {{
                "codec": "libx264"
            }}
        }},
        "widgets": [],
        "maps": [],
        "athlete": null
    }}"#,
        duration = render_duration
    );
    parse_config_json(&json).unwrap()
}

/// Loads the shared parsed activity fixture used by composite integration tests.
pub fn fixture_activity() -> ParsedActivity {
    let activity = fs::read_to_string(crate::common::test_config::fit_activity_path()).unwrap();
    parse_activity_json(&activity).unwrap()
}

/// Returns the default workspace used by most composite integration tests.
pub fn test_paths() -> AppPaths {
    test_paths_named("phase5_tests")
}

/// Returns an isolated workspace for a named composite integration test.
///
/// Each named workspace gets its own downloads, temp, and debug directories so
/// tests can inspect side effects without clobbering one another.
pub fn test_paths_named(name: &str) -> AppPaths {
    let git_root = crate::common::test_config::repo_git_root();
    let ws_root = crate::common::test_config::workspace_root();
    let test_root = ws_root.join("target").join(name);
    let downloads_dir = test_root.join("downloads");
    let temp_dir = test_root.join("tmp");
    let debug_render_dir = test_root.join("debug_render");
    fs::create_dir_all(&downloads_dir).unwrap();
    fs::create_dir_all(&temp_dir).unwrap();
    fs::create_dir_all(&debug_render_dir).unwrap();
    AppPaths {
        repo_root: git_root.clone(),
        font_dirs: vec![git_root.join("fonts")],
        debug_render_dir,
        temp_dir,
        bundled_templates_dirs: vec![git_root.join("templates")],
        user_templates_dir: test_root.join("templates"),
        downloads_dir,
    }
}

/// Reads the output video stream FPS fields with ffprobe.
pub fn ffprobe_video_rates(output_path: &Path) -> String {
    let ffprobe_path = crate::common::test_config::repo_git_root()
        .join("vendor")
        .join("ffmpeg")
        .join("bin")
        .join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
    let output = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate,avg_frame_rate",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(output_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Reads the output audio codec fields with ffprobe.
pub fn ffprobe_audio_codecs(output_path: &Path) -> String {
    let ffprobe_path = crate::common::test_config::repo_git_root()
        .join("vendor")
        .join("ffmpeg")
        .join("bin")
        .join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
    let output = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(output_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Asserts that an ffmpeg argument vector contains the exact key/value pair.
pub fn assert_argument_pair(args: &[String], key: &str, value: &str) {
    assert!(
        has_argument_pair(args, key, value),
        "missing argument pair {key} {value} in {args:?}"
    );
}

/// Returns whether an ffmpeg argument vector contains the exact key/value pair.
pub fn has_argument_pair(args: &[String], key: &str, value: &str) -> bool {
    args.windows(2)
        .any(|window| window[0] == key && window[1] == value)
}

/// Cancels a running fixture render after a short delay.
///
/// This keeps the cancellation timing policy in one place for tests that need
/// the controller flag set while ffmpeg work is still in flight.
pub fn cancel_after_delay(controller: &RenderController, delay_ms: u64) {
    thread::sleep(std::time::Duration::from_millis(delay_ms));
    controller.cancel_flag().store(true, Ordering::SeqCst);
}
