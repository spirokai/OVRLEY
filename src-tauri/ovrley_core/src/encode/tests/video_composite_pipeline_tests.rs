use super::*;
use crate::activity::{build_dense_activity_report, parse_activity_json};
use crate::config::{parse_config_json, RenderConfig};
use crate::encode::video::RenderController;
use crate::encode::video_composite_debug::{
    write_composite_timing_summary, CompositeTimingSummaryInput,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

#[test]
fn test_4_3_derives_composite_shell_timing_without_rounding() {
    let plan = phase4_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        10.0,
        Some(10.0),
        Some(0.0),
        Some(2),
    );

    assert_eq!(plan.source_fps, Fps::new(30000, 1001).unwrap());
    assert_eq!(plan.output_fps, Fps::new(30000, 1001).unwrap());
    assert_eq!(plan.overlay_pipe_fps, Fps::new(15000, 1001).unwrap());
    assert_eq!(plan.overlay_frame_count, 150);
    assert_eq!(plan.output_frame_count, 300);
}

#[test]
fn test_4_4_builds_ffmpeg_settings_inside_composite_shell() {
    let plan = phase4_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        10.0,
        Some(10.0),
        Some(0.0),
        Some(2),
    );

    assert_argument_pair(&plan.ffmpeg_settings.input_1_args, "-r", "15000/1001");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-r", "30000/1001");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-c:v", "libx264");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-b:v", "60M");
    assert!(plan.ffmpeg_settings.filter_complex.contains("[0:v]"));
    assert!(plan.ffmpeg_settings.filter_complex.contains("[1:v]"));
    assert!(plan.ffmpeg_settings.filter_complex.contains("[out]"));
    assert!(plan.output_filename.starts_with("video_composited_"));
    assert!(plan.output_filename.ends_with(".mp4"));
}

#[test]
fn fractional_overrun_guard_rejects_first_timestamp_at_or_after_duration() {
    let fps = Fps::new(30000, 1001).unwrap();

    let overrun_index = first_fractional_overrun_overlay_index(1.0, fps);
    let previous_time = (overrun_index - 1) as f64 / fps.as_f64();
    let overrun_time = overrun_index as f64 / fps.as_f64();

    assert!(previous_time < 1.0);
    assert!(overrun_time >= 1.0);
}

#[test]
fn composite_shell_defaults_to_libx264_for_mp4_output() {
    let plan = phase4_plan(
        r#""width":1920,"height":1080,"ffmpeg":{}"#,
        25,
        1,
        2.0,
        Some(2.0),
        Some(0.0),
        None,
    );

    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-c:v", "libx264");
}

#[test]
fn test_5_1_basic_software_h264_composite_creates_mp4() {
    let result =
        render_fixture_composite("tmp/test-4k.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
}

#[test]
fn test_5_2_preserves_29_97_output_fps() {
    let result =
        render_fixture_composite("tmp/test-4k.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.paths.repo_root, &result.output_path);

    assert!(fps.contains("r_frame_rate=30000/1001") || fps.contains("avg_frame_rate=30000/1001"));
    assert!(!fps.contains("30/1"));
}

#[test]
fn test_5_3_preserves_59_94_output_fps_when_requested() {
    let result =
        render_fixture_composite("tmp/test-4k.mp4", 60000, 1001, 0.12, 1, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.paths.repo_root, &result.output_path);

    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
    assert!(!fps.contains("60/1"));
}

#[test]
fn test_5_4_lower_overlay_update_rate_renders_half_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-4k.mp4", 60000, 1001, 0.2, 2, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.paths.repo_root, &result.output_path);

    assert_eq!(result.controller.progress().encoded, 12);
    assert_eq!(result.controller.progress().total, 12);
    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
}

#[test]
fn test_5_5_aggressive_overlay_update_rate_renders_one_sixth_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-4k.mp4", 60000, 1001, 0.2, 6, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.paths.repo_root, &result.output_path);

    assert_eq!(result.controller.progress().encoded, 12);
    assert_eq!(result.controller.progress().total, 12);
    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
}

#[test]
fn test_5_6_sync_offset_is_not_ffmpeg_seek() {
    let config = recent_template_config(3840, 2160);
    let paths = test_paths();
    let plan = derive_composite_pipeline_plan(
        &paths,
        &config,
        "tmp/test-4k.mp4",
        "20M",
        30000,
        1001,
        20.0,
        Some(0.2),
        Some(0.0),
        Some(1),
    )
    .unwrap();

    assert!(!has_argument_pair(
        &plan.ffmpeg_settings.input_0_args,
        "-ss",
        "300"
    ));
    assert_argument_pair(&plan.ffmpeg_settings.input_2_args, "-t", "0.2");
    assert!(plan
        .ffmpeg_settings
        .filter_complex
        .contains("trim=start=0:end=0.2,setpts=PTS-STARTPTS,"));
}

#[test]
fn test_5_7_fractional_duration_uses_overrun_guard() {
    let plan = phase4_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        20.0,
        Some(0.101),
        Some(0.0),
        Some(1),
    );

    assert_eq!(plan.overlay_frame_count, 4);
    assert_eq!(expected_guarded_overlay_frame_count(&plan), 4);
    let last_written_time =
        (expected_guarded_overlay_frame_count(&plan) - 1) as f64 / plan.overlay_pipe_fps.as_f64();
    let first_rejected_time =
        expected_guarded_overlay_frame_count(&plan) as f64 / plan.overlay_pipe_fps.as_f64();
    assert!(last_written_time < plan.render_duration);
    assert!(first_rejected_time >= plan.render_duration);
}

#[test]
fn test_5_8_video_with_audio_copies_audio_track() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);
    let audio = ffprobe_audio_codecs(&result.paths.repo_root, &result.output_path);

    assert!(audio.contains("codec_name=aac"));
}

#[test]
fn test_5_9_invalid_dense_frame_range_fails_clearly() {
    let mut config = recent_template_config(1920, 1080);
    config.scene.start = 0.0;
    config.scene.end = 1.0;
    config.scene.fps = 30.0;
    let dense_activity = build_dense_activity_report(&fixture_activity(), &config).unwrap();
    let paths = test_paths();
    let plan = derive_composite_pipeline_plan(
        &paths,
        &config,
        "tmp/test-1080p.mp4",
        "20M",
        30,
        1,
        20.0,
        Some(0.2),
        Some(0.0),
        Some(1),
    )
    .unwrap();

    let error = dense_frame_index_for_overlay(&config, &dense_activity, &plan, 600.0).unwrap_err();

    assert!(error.contains("outside dense activity range"));
}

#[test]
fn test_6_1_cancel_mid_render_stops_and_cleans_partial_output() {
    let paths = test_paths_named("phase6_cancel");
    let before = composited_outputs(&paths);
    let controller = RenderController::default();
    let render_job = spawn_fixture_composite_render(
        paths.clone(),
        controller.clone(),
        "tmp/test-1080p.mp4",
        30,
        1,
        5.0,
        1,
        600.0,
        1920,
        1080,
        "libx264",
    );

    thread::sleep(Duration::from_millis(100));
    controller.cancel_flag().store(true, Ordering::SeqCst);
    let error = render_job.join().unwrap().unwrap_err();
    let after = composited_outputs(&paths);

    assert!(error.to_lowercase().contains("cancelled"));
    assert_eq!(before, after);
}

#[test]
fn test_6_2_progress_reaches_completion_on_success() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);
    let progress = result.controller.progress();

    assert_eq!(progress.current, progress.total);
    assert_eq!(progress.encoded, progress.total);
    assert_eq!(progress.total, 6);
}

#[test]
fn test_6_3_progress_uses_output_frames_with_lower_overlay_fps() {
    let plan = phase4_plan(
        r#""width":1920,"height":1080,"ffmpeg":{"codec":"libx264"}"#,
        60000,
        1001,
        1.0,
        Some(1.0),
        Some(0.0),
        Some(6),
    );
    let first_overlay_progress = output_progress_for_overlay_time(0.0, &plan);
    let second_overlay_progress =
        output_progress_for_overlay_time(1.0 / plan.overlay_pipe_fps.as_f64(), &plan);

    assert_eq!(plan.output_frame_count, 60);
    assert_eq!(plan.overlay_frame_count, 10);
    assert_eq!(first_overlay_progress, 0);
    assert_eq!(second_overlay_progress, 6);
}

#[test]
fn test_6_4_ffmpeg_failure_reports_stderr() {
    let error = spawn_fixture_composite_render(
        test_paths(),
        RenderController::default(),
        "tmp/test-1080p.mp4",
        30,
        1,
        0.2,
        1,
        600.0,
        1920,
        1080,
        "definitely_not_a_codec",
    )
    .join()
    .unwrap()
    .unwrap_err();

    assert!(error.contains("FFmpeg stderr"));
    assert!(
        error.contains("Unknown encoder")
            || error.contains("Error selecting an encoder")
            || error.contains("terminated before all overlay frames")
    );
}

#[test]
fn test_6_5_broken_pipe_error_includes_ffmpeg_exit_context() {
    let status = Command::new(if cfg!(windows) { "cmd" } else { "false" })
        .args(if cfg!(windows) {
            vec!["/C", "exit", "1"]
        } else {
            Vec::new()
        })
        .status()
        .unwrap();
    let plan = phase4_plan(
        r#""width":1920,"height":1080,"ffmpeg":{"codec":"libx264"}"#,
        30,
        1,
        1.0,
        Some(0.2),
        Some(0.0),
        Some(1),
    );
    let message = format_pipe_write_failure(
        "Failed writing composite overlay frame: Broken pipe".to_string(),
        status,
        "filter graph failed\nUnknown filter",
        &plan,
    );

    assert!(is_pipe_write_error(
        "Failed writing composite overlay frame: Broken pipe"
    ));
    assert!(message.contains("FFmpeg terminated before all overlay frames were written"));
    assert!(message.contains("profile software_h264"));
    assert!(message.contains("Filter graph"));
    assert!(message.contains("FFmpeg stderr"));
    assert!(message.contains("Unknown filter"));
}

#[test]
fn test_6_6_output_file_exists_and_is_nonzero_on_success() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
    verify_successful_composite_output(&result.output_path).unwrap();
}

#[test]
fn test_7_1_timing_summary_exists() {
    let paths = write_fixture_phase7_summary("phase7_summary_exists");

    assert!(phase7_timing_summary_path(&paths).is_file());
    assert!(phase7_timing_summary_path(&paths)
        .parent()
        .unwrap()
        .ends_with("1778853729503903000"));
}

#[test]
fn test_7_2_phase_marker_is_correct() {
    let paths = write_fixture_phase7_summary("phase7_phase_marker");
    let summary = phase7_timing_summary(&paths);

    assert_eq!(summary["phase"], "phase_7");
    assert_eq!(summary["mode"], "mp4_composite");
}

#[test]
fn test_7_3_fps_values_are_recorded_as_rationals() {
    let paths = write_fixture_phase7_summary("phase7_rational_fps");
    let summary = phase7_timing_summary(&paths);

    assert_eq!(summary["diagnostics"]["source_fps"], "60000/1001");
    assert_eq!(summary["diagnostics"]["overlay_pipe_fps"], "30000/1001");
    assert_eq!(summary["update_rate"], 2);
    assert_eq!(summary["fps"], 59.94);
    assert_eq!(summary["layout_fps"], 29.97);
}

#[test]
fn test_7_4_frame_counts_are_recorded() {
    let paths = write_fixture_phase7_summary("phase7_frame_counts");
    let summary = phase7_timing_summary(&paths);

    assert_eq!(summary["rendered_frames"], 6);
    assert_eq!(summary["layout_total_frames"], 6);
    assert_eq!(summary["total_frames"], 12);
}

#[test]
fn test_7_5_total_wall_time_is_recorded() {
    let paths = write_fixture_phase7_summary("phase7_total_wall_time");
    let summary = phase7_timing_summary(&paths);

    assert!(summary["total_time_taken"].as_f64().unwrap() > 0.0);
    assert!(summary["overlay_filename"]
        .as_str()
        .unwrap()
        .contains("video_composited_1778853729503903000.mp4"));
    assert!(summary["diagnostics"]["render_loop_ms"].as_f64().unwrap() > 0.0);
    assert!(
        summary["diagnostics"]["ffmpeg_finalize_wait_ms"]
            .as_f64()
            .unwrap()
            >= 0.0
    );
    assert!(summary["timings"]["frame.total"]["count"].as_u64().unwrap() > 0);
    assert!(
        summary["timings"]["ffmpeg.write"]["count"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        summary["diagnostics"]["ffmpeg_timing_note"],
        "FFmpeg decode/filter/encode timings are not isolated; ffmpeg.write measures stdin write/backpressure time."
    );
    assert!(summary["performance"]["ffmpeg_decode_filter_encode_note"]
        .as_str()
        .unwrap()
        .contains("cannot be exactly separated"));
    assert!(summary["timings"]
        .get("composite.widget_update_rate")
        .is_none());
}

#[test]
fn test_7_6_phase_7_output_is_only_created_by_composite_render() {
    let paths = test_paths_named("phase7_transparent_unaffected");

    assert!(!paths.debug_render_dir.join("phase_7").exists());
}

#[test]
#[ignore = "Long-running 4K end-to-end render for manual validation."]
fn test_manual_full_duration_4k_composite() {
    let result = render_fixture_composite(
        "tmp/test-4k.mp4",
        30000,
        1001,
        20.353667,
        1,
        600.0,
        3840,
        2160,
    );
    let fps = ffprobe_video_rates(&result.paths.repo_root, &result.output_path);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
    assert!(fps.contains("r_frame_rate=30000/1001") || fps.contains("avg_frame_rate=30000/1001"));
    eprintln!(
        "Full-duration 4K composite output: {}",
        result.output_path.display()
    );
}

fn phase4_plan(
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

struct RenderFixtureResult {
    paths: AppPaths,
    controller: RenderController,
    output_path: PathBuf,
    output_size: u64,
}

fn render_fixture_composite(
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

fn spawn_fixture_composite_render(
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

fn render_fixture_composite_with_paths(
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
    let absolute_video_path = repo_root().join(video_path);
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
    )?;
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

fn composited_outputs(paths: &AppPaths) -> Vec<String> {
    let mut outputs = fs::read_dir(&paths.downloads_dir)
        .unwrap()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("video_composited_") && name.ends_with(".mp4"))
        .collect::<Vec<_>>();
    outputs.sort();
    outputs
}

fn phase7_timing_summary_path(paths: &AppPaths) -> PathBuf {
    let phase_dir = paths.debug_render_dir.join("phase_7");
    let mut summaries = fs::read_dir(&phase_dir)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", phase_dir.display()))
        .flatten()
        .map(|entry| entry.path().join("timing_summary.json"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    summaries.sort();
    summaries
        .pop()
        .unwrap_or_else(|| panic!("No Phase 7 timing summary under {}", phase_dir.display()))
}

fn phase7_timing_summary(paths: &AppPaths) -> Value {
    let json = fs::read_to_string(phase7_timing_summary_path(paths)).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn write_fixture_phase7_summary(path_name: &str) -> AppPaths {
    let paths = test_paths_named(path_name);
    let _ = fs::remove_dir_all(paths.debug_render_dir.join("phase_7"));
    let source_fps = Fps::new(60000, 1001).unwrap();
    let overlay_pipe_fps = Fps::new(30000, 1001).unwrap();
    let ffmpeg_settings = build_composite_ffmpeg_settings(
        "libx264",
        "20M",
        PathBuf::from("input.mp4").as_path(),
        0.0,
        0.2,
        1920,
        1080,
        source_fps,
        overlay_pipe_fps,
        &HwAccelInfo::default(),
    )
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

fn recent_template_config(width: u32, height: u32) -> RenderConfig {
    let repo_root = repo_root();
    let template =
        fs::read_to_string(repo_root.join("templates").join("recent-template.json")).unwrap();
    let value: Value = serde_json::from_str(&template).unwrap();
    let mut config: RenderConfig =
        serde_json::from_value(value.get("config").unwrap().clone()).unwrap();
    config.scene.width = Some(width);
    config.scene.height = Some(height);
    config.scene.ffmpeg = serde_json::json!({"codec":"libx264"});
    config
}

#[test]
fn test_parallel_composite_render_2_segments() {
    let paths = test_paths();
    let config = composite_test_config(5.0);
    let activity = fixture_activity();
    let dense = build_dense_activity_report(&activity, &config).unwrap();
    let controller = RenderController::default();
    controller.try_start(dense.frame_count as u32, "test_parallel_2").unwrap();

    let result = crate::encode::video::render_composite_video(
        &paths, &config, &activity, &dense, &controller,
        "D:\\Downloads\\video sample.mp4", "10M",
        0.0, 30000, 1001, 35.0,
        Some(5.0), Some(0.0), Some(1),
    );
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel composite output: {}", output.display());
}

#[test]
fn test_parallel_composite_render_with_audio() {
    let paths = test_paths();
    let config = composite_test_config(5.0);
    let activity = fixture_activity();
    let dense = build_dense_activity_report(&activity, &config).unwrap();
    let controller = RenderController::default();
    controller.try_start(dense.frame_count as u32, "test_parallel_audio").unwrap();

    let result = crate::encode::video::render_composite_video(
        &paths, &config, &activity, &dense, &controller,
        "D:\\Downloads\\video sample.mp4", "10M",
        0.0, 30000, 1001, 35.0,
        Some(5.0), Some(15.0), Some(1),
    );
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel audio output: {}", output.display());
}

fn composite_test_config(render_duration: f64) -> RenderConfig {
    let json = format!(
        r#"{{
        "scene": {{
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "start": 0.0,
            "end": {duration},
            "ffmpeg": {{
                "codec": "qsv_full_h264",
                "qsv_full_init_args": [
                    "-init_hw_device", "dxva2=dx",
                    "-init_hw_device", "qsv=qs@dx",
                    "-filter_hw_device", "qs",
                    "-hwaccel", "qsv",
                    "-hwaccel_output_format", "qsv"
                ]
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

fn fixture_activity() -> ParsedActivity {
    let activity = fs::read_to_string(
        repo_root()
            .join("debug")
            .join("activities")
            .join("Test_FIT-parse-debug.json"),
    )
    .unwrap();
    parse_activity_json(&activity).unwrap()
}

fn test_paths() -> AppPaths {
    test_paths_named("phase5_tests")
}

fn test_paths_named(name: &str) -> AppPaths {
    let repo_root = repo_root();
    let test_root = repo_root.join("src-tauri").join("target").join(name);
    let downloads_dir = test_root.join("downloads");
    let temp_dir = test_root.join("tmp");
    let debug_render_dir = test_root.join("debug_render");
    fs::create_dir_all(&downloads_dir).unwrap();
    fs::create_dir_all(&temp_dir).unwrap();
    fs::create_dir_all(&debug_render_dir).unwrap();
    AppPaths {
        repo_root: repo_root.clone(),
        font_dirs: vec![repo_root.join("fonts")],
        debug_render_dir,
        temp_dir,
        bundled_templates_dirs: vec![repo_root.join("templates")],
        user_templates_dir: test_root.join("templates"),
        downloads_dir,
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ffprobe_video_rates(repo_root: &std::path::Path, output_path: &std::path::Path) -> String {
    let ffprobe_path = repo_root
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

fn ffprobe_audio_codecs(repo_root: &std::path::Path, output_path: &std::path::Path) -> String {
    let ffprobe_path = repo_root
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

fn assert_argument_pair(args: &[String], key: &str, value: &str) {
    assert!(
        args.windows(2)
            .any(|window| window[0] == key && window[1] == value),
        "missing argument pair {key} {value} in {args:?}"
    );
}

fn has_argument_pair(args: &[String], key: &str, value: &str) -> bool {
    args.windows(2)
        .any(|window| window[0] == key && window[1] == value)
}
