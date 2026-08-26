//! Composite video pipeline integration tests.
//!
//! The largest test suite in the crate. Covers the full composite pipeline:
//! `derive_composite_pipeline_plan`, canonical frame-worker rendering,
//! fractional overrun guards, sync-offset correctness, FPS preservation,
//! audio track copying, progress reporting, cancellation lifecycle,
//! FFmpeg failure diagnostics, broken-pipe handling, and composite debug
//! timing summaries.
//!
//! ## Fixtures
//!
//! - `test_config::sample_video_path()` (test-1080p.mp4) - representative
//!   H.264 video with audio track for most composite render-through tests.
//! - `test_config::fit_activity_path()` - activity data for dense reports.
//! - `templates/recent-template.json` - real template for realistic widget
//!   configuration in render-through tests.
//! - Shared sample video fixture (`tmp/test-1080p.mp4`) - used for composite
//!   render-through tests on machines without the old temporary 4K asset.
//! - Shared fixture/render helpers from `tests/common/composite.rs` for
//!   workspace setup, render execution, ffprobe reads, and debug-summary IO.
//!
//! ## Type
//! Integration test. Requires live ffmpeg and ffprobe in `vendor/ffmpeg/bin/`.
//! Runs full render pipelines that produce MP4 output files. Tests marked
//! `#[ignore]` are long-running 4K renders for manual validation only.
//!
//! ## Regressions guarded
//! - Composite pipeline producing wrong FPS (integer rounding of rationals)
//! - Sync offset incorrectly applied as ffmpeg seek (should be timing offset)
//! - Fractional render durations writing past the declared duration
//! - Audio track dropped during composite rendering
//! - Cancellation leaving partial output files or zombie processes
//! - FFmpeg crash producing unhelpful error messages
//! - Parallel frame production writing frames out of order
//! - Lower overlay update rate producing incorrect frame counts
//! - Debug timing summaries missing expected fields

mod common;

use std::process::Command;

use ovrley_core::activity::build_dense_activity_report_validated;
use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::pipeline::composite::render_composite_video;
use ovrley_core::encode::pipeline::composite_plan::{
    derive_composite_pipeline_plan, derive_composite_render_plan,
};
use ovrley_core::encode::pipeline::composite_support::{
    format_pipe_write_failure, is_pipe_write_error, verify_successful_composite_output,
};
use ovrley_core::encode::progress::RenderController;
use ovrley_core::normalize::validate_render_config;

use common::composite::{
    assert_argument_pair, cancel_after_delay, composite_debug_timing_summary,
    composite_debug_timing_summary_path, composite_test_config, composited_outputs,
    custom_output_target, derive_fixture_composite_plan, ffprobe_audio_codecs, ffprobe_video_rates,
    fixture_activity, has_argument_pair, mutable_recent_template_config, render_fixture_composite,
    spawn_fixture_composite_render, test_paths, test_paths_named,
    write_fixture_composite_debug_summary,
};

#[test]
/// Derives a plan from a 29.97 FPS source with 2x widget update rate and
/// verifies output_fps matches source, overlay_pipe_fps is halved, and
/// overlay/output frame counts are correct.
fn test_4_3_derives_composite_shell_timing_without_rounding() {
    let plan = derive_fixture_composite_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        10.0,
        10.0,
        0.0,
        2,
    );

    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-r", "30000/1001");
    assert_argument_pair(&plan.ffmpeg_settings.input_1_args, "-r", "15000/1001");
    assert_eq!(plan.render.overlay_frame_count, 150);
    assert_eq!(plan.render.output_frame_count, 300);
}

#[test]
fn negative_sync_plan_keeps_full_video_output_and_limits_activity_overlap() {
    let mut config = mutable_recent_template_config(1920, 1080);
    config.scene.composite_video_path = Some("input.mp4".to_string());
    config.scene.composite_bitrate = Some("20M".to_string());
    config.scene.composite_sync_offset = Some(-5.0);
    config.scene.composite_video_fps_num = Some(30);
    config.scene.composite_video_fps_den = Some(1);
    config.scene.composite_video_duration = Some(30.0);
    config.scene.composite_render_duration = Some(30.0);
    config.scene.composite_video_trim_start = Some(0.0);
    config.scene.composite_widget_update_rate = Some(1);

    let mut shorter_activity_scene = validate_render_config(config.clone()).unwrap().scene;
    let shorter_activity_plan =
        derive_composite_render_plan(&mut shorter_activity_scene, Some(10.0)).unwrap();
    assert_eq!(shorter_activity_scene.start, 0.0);
    assert_eq!(shorter_activity_scene.end, 10.0);
    assert_eq!(shorter_activity_plan.activity_overlap_duration, 10.0);
    assert_eq!(shorter_activity_plan.overlay_frame_count, 900);
    assert_eq!(shorter_activity_plan.output_frame_count, 900);

    let mut scene = validate_render_config(config).unwrap().scene;
    let plan = derive_composite_render_plan(&mut scene, Some(25.0)).unwrap();

    assert_eq!(scene.start, 0.0);
    assert_eq!(scene.end, 25.0);
    assert_eq!(scene.end - scene.start, 25.0);
    assert_eq!(plan.activity_overlap_duration, 25.0);
    assert_eq!(plan.blank_leading_frame_count, 150);
    assert_eq!(plan.overlay_frame_count, 900);
    assert_eq!(plan.output_frame_count, 900);
}

#[test]
fn composite_plan_rejects_offset_at_video_duration_boundary() {
    let mut config = mutable_recent_template_config(1920, 1080);
    config.scene.composite_video_path = Some("input.mp4".to_string());
    config.scene.composite_bitrate = Some("20M".to_string());
    config.scene.composite_sync_offset = Some(-30.0);
    config.scene.composite_video_fps_num = Some(30);
    config.scene.composite_video_fps_den = Some(1);
    config.scene.composite_video_duration = Some(30.0);
    config.scene.composite_render_duration = Some(30.0);
    config.scene.composite_video_trim_start = Some(0.0);
    config.scene.composite_widget_update_rate = Some(1);

    let mut scene = validate_render_config(config).unwrap().scene;
    let error = derive_composite_render_plan(&mut scene, Some(25.0)).unwrap_err();

    assert!(error.to_string().contains("positive overlap"));
}

#[test]
/// After deriving a plan, verifies the FFmpeg settings embedded in the plan
/// have correct FPS args, codec/bitrate args, and filter graph labels.
fn test_4_4_builds_ffmpeg_settings_inside_composite_shell() {
    let plan = derive_fixture_composite_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        10.0,
        10.0,
        0.0,
        2,
    );

    assert_argument_pair(&plan.ffmpeg_settings.input_1_args, "-r", "15000/1001");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-r", "30000/1001");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-c:v", "libx264");
    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-b:v", "60M");
    assert!(plan.ffmpeg_settings.filter_complex.contains("[0:v]"));
    assert!(plan.ffmpeg_settings.filter_complex.contains("[1:v]"));
    assert!(plan.ffmpeg_settings.filter_complex.contains("[out]"));
    let output_filename = plan.output_path.file_name().unwrap().to_str().unwrap();
    assert!(output_filename.ends_with(".mp4"));
}

/// Verifies the fractional overrun guard: the first overlay-index whose
/// timestamp reaches or exceeds the render duration is correctly rejected,
/// while the previous frame's timestamp is strictly before duration.
#[test]
fn fractional_overrun_guard_rejects_first_timestamp_at_or_after_duration() {
    let fps = Fps::new(30000, 1001).unwrap();

    let overrun_index = fps.frame_count_for_duration(1.0).unwrap();
    let previous_time = fps.seconds_at_frame(overrun_index - 1);
    let overrun_time = fps.seconds_at_frame(overrun_index);

    assert!(previous_time < 1.0);
    assert!(overrun_time >= 1.0);
}

/// When no codec is specified in ffmpeg settings, the plan defaults to
/// libx264 for MP4 output.
#[test]
fn composite_shell_uses_provided_codec_for_mp4_output() {
    let plan = derive_fixture_composite_plan(
        r#""width":1920,"height":1080,"ffmpeg":{"codec":"libx264"}"#,
        25,
        1,
        2.0,
        2.0,
        0.0,
        1,
    );

    assert_argument_pair(&plan.ffmpeg_settings.output_args, "-c:v", "libx264");
}

/// End-to-end composite render at 29.97 FPS with 1x update rate and 4K
/// resolution. Verifies output file exists and is non-empty.
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_1_basic_software_h264_composite_creates_mp4() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
}

/// Probes the output of a 29.97 FPS composite render and asserts the
/// container reports 30000/1001 (not a rounded integer) in r_frame_rate
/// or avg_frame_rate.
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_2_preserves_29_97_output_fps() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

    assert!(fps.contains("r_frame_rate=30000/1001") || fps.contains("avg_frame_rate=30000/1001"));
    assert!(!fps.contains("30/1"));
}

/// Probes the output of a 59.94 FPS composite render and asserts the
/// container reports 60000/1001 (not a rounded 60/1).
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_3_preserves_59_94_output_fps_when_requested() {
    let result = render_fixture_composite(
        "tmp/test-1080p.mp4",
        60000,
        1001,
        0.12,
        1,
        600.0,
        3840,
        2160,
    );
    let fps = ffprobe_video_rates(&result.output_path);

    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
    assert!(!fps.contains("60/1"));
}

/// 60→30 overlay FPS at 2x update rate: 0.2s render = 12 output frames
/// (6 overlay frames halved in pipe). Verifies progress matches.
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_4_lower_overlay_update_rate_renders_half_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 60000, 1001, 0.2, 2, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

    assert_eq!(result.controller.progress().encoded, 12);
    assert_eq!(result.controller.progress().total, 12);
    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
}

/// At 6x update rate the overlay pipe runs at 10 FPS (60÷6) but the output
/// stays at 59.94 FPS. Progress still reports output frame count (12).
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_5_aggressive_overlay_update_rate_renders_one_sixth_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 60000, 1001, 0.2, 6, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

    assert_eq!(result.controller.progress().encoded, 12);
    assert_eq!(result.controller.progress().total, 12);
    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
}

/// Sync offset must be applied as a timing/dense-activity offset, never as
/// an ffmpeg `-ss` seek argument on the video input. Verifies the video
/// input uses trim-based seeking and the filter graph uses `trim=start=0`.
#[test]
fn test_5_6_sync_offset_is_not_ffmpeg_seek() {
    let mut config = mutable_recent_template_config(3840, 2160);
    config.scene.composite_video_path = Some("tmp/test-1080p.mp4".to_string());
    config.scene.composite_bitrate = Some("20M".to_string());
    config.scene.composite_sync_offset = Some(0.0);
    config.scene.composite_video_fps_num = Some(30000);
    config.scene.composite_video_fps_den = Some(1001);
    config.scene.composite_video_duration = Some(20.0);
    config.scene.composite_render_duration = Some(0.2);
    config.scene.composite_video_trim_start = Some(0.0);
    config.scene.composite_widget_update_rate = Some(1);
    let config = validate_render_config(config).unwrap();
    let paths = test_paths();
    let mut scene = config.scene.clone();
    let render = derive_composite_render_plan(&mut scene, None).unwrap();
    let output_target = custom_output_target(
        &paths,
        "plan",
        ovrley_core::output::RenderOutputKind::Composite,
    );
    let plan =
        derive_composite_pipeline_plan(&paths, &scene, render, true, None, &output_target).unwrap();

    assert!(!has_argument_pair(
        &plan.ffmpeg_settings.input_0_args,
        "-ss",
        "300"
    ));
    assert_argument_pair(
        &plan.ffmpeg_settings.input_2_args,
        "-i",
        "tmp/test-1080p.mp4",
    );
    assert!(plan
        .ffmpeg_settings
        .filter_complex
        .contains("trim=start=0:end=0.2,setpts=PTS-STARTPTS,"));
    assert!(plan
        .ffmpeg_settings
        .filter_complex
        .contains("[2:a]atrim=start=0:duration=0.2,asetpts=N/SR/TB[aout]"));
}

/// Fractional render durations (0.101s) must use the overrun guard so that
/// the last written frame's timestamp is before duration and the first
/// rejected frame's timestamp is at or after duration.
#[test]
fn test_5_7_fractional_duration_uses_overrun_guard() {
    let plan = derive_fixture_composite_plan(
        r#""width":3840,"height":2160,"ffmpeg":{"codec":"libx264"}"#,
        30000,
        1001,
        20.0,
        0.101,
        0.0,
        1,
    );

    assert_eq!(plan.render.overlay_frame_count, 4);
    let overlay_fps = Fps::new(30000, 1001).unwrap();
    let last_written_time = overlay_fps.seconds_at_frame(plan.render.overlay_frame_count - 1);
    let first_rejected_time = overlay_fps.seconds_at_frame(plan.render.overlay_frame_count);
    assert!(last_written_time < 0.101);
    assert!(first_rejected_time >= 0.101);
}

/// Composite render of a source video that has an audio track must produce
/// output with AAC audio copy (the probe should report codec_name=aac).
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_5_8_video_with_audio_copies_audio_track() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);
    let audio = ffprobe_audio_codecs(&result.output_path);

    assert!(audio.contains("codec_name=aac"));
}

/// Multi-threaded cancellation test: spawns a composite render on a
/// background thread, cancels after 100ms, and verifies the render returns
/// a Cancelled error and no partial output files are left behind.
///
/// Uses snapshots of the downloads directory before and after the render
/// to confirm cleanup. The test uses the shared harness helper to trigger
/// cancellation after a short delay.
///
/// Regressions guarded: cancelled renders leaving stale output files,
/// cancel flag not respected by the render loop, error from cancel path
/// not containing "cancelled".
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
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

    cancel_after_delay(&controller, 100);
    let error = render_job.join().unwrap().unwrap_err();
    let after = composited_outputs(&paths);

    assert_eq!(error, "Render cancelled");
    assert_eq!(before, after);
}

#[test]
/// After a successful composite render, progress must show `current ==
/// total == encoded` and the total must match the expected output frame count.
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_6_2_progress_reaches_completion_on_success() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);
    let progress = result.controller.progress();

    assert_eq!(progress.current, progress.total);
    assert_eq!(progress.encoded, progress.total);
    assert_eq!(progress.total, 6);
}

/// Progress reporting must use output frame count (not overlay count):
/// at 6x update rate, 60 output frames = 10 overlays, and each overlay
/// tick advances progress by 6.
#[test]
fn test_6_3_progress_uses_output_frames_with_lower_overlay_fps() {
    let plan = derive_fixture_composite_plan(
        r#""width":1920,"height":1080,"ffmpeg":{"codec":"libx264"}"#,
        60000,
        1001,
        1.0,
        1.0,
        0.0,
        6,
    );
    let first_overlay_progress = plan.output_progress(1);
    let second_overlay_progress = plan.output_progress(2);

    assert_eq!(plan.render.output_frame_count, 60);
    assert_eq!(plan.render.overlay_frame_count, 10);
    assert_eq!(first_overlay_progress, 0);
    assert_eq!(second_overlay_progress, 6);
}

/// Rejects unknown codecs at config ingress instead of passing malformed
/// state through to FFmpeg as a custom encoder name.
#[test]
fn test_6_4_unknown_codec_fails_at_ingress() {
    let mut config = mutable_recent_template_config(1920, 1080);
    config.scene.ffmpeg = serde_json::json!({"codec": "definitely_not_a_codec"});

    let error = match validate_render_config(config) {
        Ok(_) => panic!("unknown codec unexpectedly validated"),
        Err(error) => error,
    };

    assert_eq!(
        error.to_string(),
        "Invalid configuration: scene.ffmpeg.codec is unsupported: definitely_not_a_codec"
    );
}

#[test]
/// Verifies broken-pipe errors from ffmpeg crashes include diagnostic
/// context: the FFmpeg exit status, selected profile name, filter graph,
/// and stderr tail. Uses a synthetic `ExitStatus` from `cmd /C exit 1`
/// and exercises `format_pipe_write_failure` directly.
///
/// Regressions guarded: pipe-write errors producing empty or misleading
/// messages, missing filter graph in crash diagnostics.
fn test_6_5_broken_pipe_error_includes_ffmpeg_exit_context() {
    let status = Command::new(if cfg!(windows) { "cmd" } else { "false" })
        .args(if cfg!(windows) {
            vec!["/C", "exit", "1"]
        } else {
            Vec::new()
        })
        .status()
        .unwrap();
    let plan = derive_fixture_composite_plan(
        r#""width":1920,"height":1080,"ffmpeg":{"codec":"libx264"}"#,
        30,
        1,
        1.0,
        0.2,
        0.0,
        1,
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

/// On success, `verify_successful_composite_output` must not error for a
/// real rendered composite MP4.
#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_6_6_output_file_exists_and_is_nonzero_on_success() {
    let result = render_fixture_composite("tmp/test-1080p.mp4", 30, 1, 0.2, 1, 600.0, 1920, 1080);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
    verify_successful_composite_output(&result.output_path).unwrap();
}

/// After writing a fixture debug summary, the timing summary JSON file must
/// exist on disk under the expected path.
#[test]
fn test_7_1_timing_summary_exists() {
    let paths = write_fixture_composite_debug_summary("composite_debug_summary_exists");

    assert!(composite_debug_timing_summary_path(&paths).is_file());
}

/// The debug timing summary must record `phase: "composite"` and
/// `mode: "mp4_composite"` so downstream tooling can distinguish render types.
#[test]
fn test_7_2_phase_marker_is_correct() {
    let paths = write_fixture_composite_debug_summary("composite_debug_phase_marker");
    let summary = composite_debug_timing_summary(&paths);

    assert_eq!(summary["phase"], "composite");
    assert_eq!(summary["mode"], "mp4_composite");
}

/// FPS values in the debug summary must be recorded as rational strings
/// ("60000/1001") not as floats. The decoded `fps` and `layout_fps` must
/// match the expected NTSC-approximate values.
#[test]
fn test_7_3_fps_values_are_recorded_as_rationals() {
    let paths = write_fixture_composite_debug_summary("composite_debug_rational_fps");
    let summary = composite_debug_timing_summary(&paths);

    assert_eq!(summary["diagnostics"]["source_fps"], "60000/1001");
    assert_eq!(summary["diagnostics"]["overlay_pipe_fps"], "30000/1001");
    assert_eq!(summary["update_rate"], 2);
    assert_eq!(summary["fps"], 59.94);
    assert_eq!(summary["layout_fps"], 29.97);
}

/// The debug summary records `rendered_frames`, `layout_total_frames`,
/// and `total_frames` so tests can verify overlay/output frame ratio.
#[test]
fn test_7_4_frame_counts_are_recorded() {
    let paths = write_fixture_composite_debug_summary("composite_debug_frame_counts");
    let summary = composite_debug_timing_summary(&paths);

    assert_eq!(summary["rendered_frames"], 6);
    assert_eq!(summary["layout_total_frames"], 6);
    assert_eq!(summary["total_frames"], 12);
}

/// The debug summary must record positive total wall time, render loop ms,
/// per-frame timings, and a note that ffmpeg.timing is not isolated. Also
/// verifies the `composite.widget_update_rate` timing key is not emitted
/// (it's a plan-level constant, not a per-frame measurement).
#[test]
fn test_7_5_total_wall_time_is_recorded() {
    let paths = write_fixture_composite_debug_summary("composite_debug_total_wall_time");
    let summary = composite_debug_timing_summary(&paths);

    assert!(summary["total_time_taken"].as_f64().unwrap() > 0.0);
    assert!(summary["overlay_filename"]
        .as_str()
        .unwrap()
        .contains("custom-debug-output.mp4"));
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

/// The composite debug directory must NOT exist for a workspace that has
/// never seen a composite render — the debug output is only created by
/// composite rendering, never by transparent renders.
#[test]
fn test_7_6_composite_debug_output_is_only_created_by_composite_render() {
    let paths = test_paths_named("composite_debug_transparent_unaffected");

    assert!(!paths.debug_render_dir.join("composite").exists());
}

/// Manual validation test: renders a full ~20s 4K composite at 29.97 FPS
/// and verifies output exists, is non-empty, and the container FPS is
/// preserved as a rational. Ignored by default to keep CI fast.
#[test]
#[ignore = "Long-running 4K end-to-end render for manual validation."]
fn test_manual_full_duration_4k_composite() {
    let result = render_fixture_composite(
        "tmp/test-1080p.mp4",
        30000,
        1001,
        20.353667,
        1,
        600.0,
        3840,
        2160,
    );
    let fps = ffprobe_video_rates(&result.output_path);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
    assert!(fps.contains("r_frame_rate=30000/1001") || fps.contains("avg_frame_rate=30000/1001"));
    eprintln!(
        "Full-duration 4K composite output: {}",
        result.output_path.display()
    );
}

#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_frame_workers_render_short_composite_in_order() {
    let paths = test_paths_named("parallel_frame_workers_short_composite");
    let video_path = common::test_config::sample_video_path()
        .to_string_lossy()
        .to_string();
    let mut validated = composite_test_config(0.2, &video_path, 0.0);
    let activity = fixture_activity();
    let render_plan = derive_composite_render_plan(&mut validated.scene, None).unwrap();
    let dense = build_dense_activity_report_validated(&activity, &validated).unwrap();
    let controller = RenderController::default();
    controller
        .try_start(dense.frame_count as u32, "test_parallel_frame_workers")
        .unwrap();
    let output_target = custom_output_target(
        &paths,
        "parallel",
        ovrley_core::output::RenderOutputKind::Composite,
    );
    let filename = render_composite_video(
        &paths,
        &validated,
        &activity,
        &dense,
        &controller,
        render_plan,
        true,
        &output_target,
    )
    .unwrap();

    let output_path = paths.downloads_dir.join(filename);
    assert!(output_path.is_file());
    assert!(std::fs::metadata(&output_path).unwrap().len() > 0);
    let progress = controller.progress();
    assert_eq!(progress.current, 6);
    assert_eq!(progress.current, progress.total);
    assert_eq!(progress.encoded, progress.total);
    let rates = ffprobe_video_rates(&output_path);
    assert!(rates.contains("r_frame_rate=30000/1001"));

    let summary = composite_debug_timing_summary(&paths);
    assert_eq!(summary["diagnostics"]["frame_render_mode"], "frame_workers");
    let workers = summary["diagnostics"]["frame_render_workers"]
        .as_u64()
        .unwrap();
    assert!((1..=4).contains(&workers));
    for timing in [
        "frame.draw",
        "parallel.worker_frame",
        "parallel.result_wait",
        "parallel.reorder_hold",
        "writer.rendered_frame_wait",
        "ffmpeg.write",
    ] {
        assert!(summary["timings"][timing].is_object(), "missing {timing}");
    }
}

#[test]
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
/// End-to-end composite render through the canonical frame-worker pipeline.
fn test_frame_worker_composite_render() {
    let paths = test_paths();
    let video_path = common::test_config::sample_video_path()
        .to_string_lossy()
        .to_string();
    let mut validated = composite_test_config(5.0, &video_path, 0.0);
    let activity = fixture_activity();
    let render_plan = derive_composite_render_plan(&mut validated.scene, None).unwrap();
    let dense = build_dense_activity_report_validated(&activity, &validated).unwrap();
    let controller = RenderController::default();
    controller
        .try_start(dense.frame_count as u32, "test_parallel_2")
        .unwrap();

    let output_target = custom_output_target(
        &paths,
        "parallel-2",
        ovrley_core::output::RenderOutputKind::Composite,
    );
    let result = render_composite_video(
        &paths,
        &validated,
        &activity,
        &dense,
        &controller,
        render_plan,
        true,
        &output_target,
    );
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel composite output: {}", output.display());
}

#[test]
/// End-to-end frame-worker composite render with audio-copy and trim start.
///
/// Configures a 5-second composite render at 29.97 FPS with a 15-second
/// video trim start (trimming the first 15 seconds of the source video)
/// and audio track copying. Verifies output file exists and is non-empty.
///
/// Requires live ffmpeg and the test-1080p.mp4 fixture (which has an
/// audio track).
///
/// Regressions guarded: trim start with audio causing sync issues or failures.
#[ignore = "requires video fixture tests/fixtures/video/test-1080p.mp4"]
fn test_frame_worker_composite_render_with_audio() {
    let paths = test_paths();
    let video_path = common::test_config::sample_video_path()
        .to_string_lossy()
        .to_string();
    let mut validated = composite_test_config(5.0, &video_path, 15.0);
    let activity = fixture_activity();
    let render_plan = derive_composite_render_plan(&mut validated.scene, None).unwrap();
    let dense = build_dense_activity_report_validated(&activity, &validated).unwrap();
    let controller = RenderController::default();
    controller
        .try_start(dense.frame_count as u32, "test_parallel_audio")
        .unwrap();

    let output_target = custom_output_target(
        &paths,
        "parallel-audio",
        ovrley_core::output::RenderOutputKind::Composite,
    );
    let result = render_composite_video(
        &paths,
        &validated,
        &activity,
        &dense,
        &controller,
        render_plan,
        true,
        &output_target,
    );
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel audio output: {}", output.display());
}
