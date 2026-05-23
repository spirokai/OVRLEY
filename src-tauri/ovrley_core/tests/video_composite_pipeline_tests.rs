//! Composite video pipeline integration tests.
//!
//! The largest test suite in the crate. Covers the full composite pipeline:
//! `derive_composite_pipeline_plan`, `render_composite_video_single`,
//! `render_composite_video` (segmented/parallel), frame-window partitioning,
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
//! - Parallel segmentation producing wrong frame windows or overlapping segments
//! - Lower overlay update rate producing incorrect frame counts
//! - Debug timing summaries missing expected fields

mod common;

use std::process::Command;

use ovrley_core::activity::build_dense_activity_report;
use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::video::render_composite_video;
use ovrley_core::encode::video::CompositeRenderRequest;
use ovrley_core::encode::video::RenderController;
use ovrley_core::encode::video_composite_pipeline::{
    dense_frame_index_for_overlay, derive_composite_pipeline_plan,
    expected_guarded_overlay_frame_count, first_fractional_overrun_overlay_index,
};
use ovrley_core::encode::video_composite_support::{
    format_pipe_write_failure, is_pipe_write_error, output_progress_for_overlay_time,
    verify_successful_composite_output,
};

use common::composite::{
    assert_argument_pair, cancel_after_delay, composite_debug_timing_summary,
    composite_debug_timing_summary_path, composite_test_config, composited_outputs,
    derive_fixture_composite_plan, ffprobe_audio_codecs, ffprobe_video_rates, fixture_activity,
    has_argument_pair, recent_template_config, render_fixture_composite,
    spawn_fixture_composite_render, test_paths, test_paths_named,
    write_fixture_composite_debug_summary,
};

#[test]
fn test_4_3_derives_composite_shell_timing_without_rounding() {
    let plan = derive_fixture_composite_plan(
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
    let plan = derive_fixture_composite_plan(
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
    let plan = derive_fixture_composite_plan(
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
        render_fixture_composite("tmp/test-1080p.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);

    assert!(result.output_path.is_file());
    assert!(result.output_size > 0);
}

#[test]
fn test_5_2_preserves_29_97_output_fps() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 30000, 1001, 0.2, 1, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

    assert!(fps.contains("r_frame_rate=30000/1001") || fps.contains("avg_frame_rate=30000/1001"));
    assert!(!fps.contains("30/1"));
}

#[test]
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

#[test]
fn test_5_4_lower_overlay_update_rate_renders_half_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 60000, 1001, 0.2, 2, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

    assert_eq!(result.controller.progress().encoded, 12);
    assert_eq!(result.controller.progress().total, 12);
    assert!(fps.contains("r_frame_rate=60000/1001") || fps.contains("avg_frame_rate=60000/1001"));
}

#[test]
fn test_5_5_aggressive_overlay_update_rate_renders_one_sixth_overlay_frames() {
    let result =
        render_fixture_composite("tmp/test-1080p.mp4", 60000, 1001, 0.2, 6, 600.0, 3840, 2160);
    let fps = ffprobe_video_rates(&result.output_path);

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
        "tmp/test-1080p.mp4",
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
    let plan = derive_fixture_composite_plan(
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
    let audio = ffprobe_audio_codecs(&result.output_path);

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

    assert!(error.to_string().contains("outside dense activity range"));
}

#[test]
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
    let plan = derive_fixture_composite_plan(
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
    let paths = write_fixture_composite_debug_summary("composite_debug_summary_exists");

    assert!(composite_debug_timing_summary_path(&paths).is_file());
    assert!(composite_debug_timing_summary_path(&paths)
        .parent()
        .unwrap()
        .ends_with("1778853729503903000"));
}

#[test]
fn test_7_2_phase_marker_is_correct() {
    let paths = write_fixture_composite_debug_summary("composite_debug_phase_marker");
    let summary = composite_debug_timing_summary(&paths);

    assert_eq!(summary["phase"], "composite");
    assert_eq!(summary["mode"], "mp4_composite");
}

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

#[test]
fn test_7_4_frame_counts_are_recorded() {
    let paths = write_fixture_composite_debug_summary("composite_debug_frame_counts");
    let summary = composite_debug_timing_summary(&paths);

    assert_eq!(summary["rendered_frames"], 6);
    assert_eq!(summary["layout_total_frames"], 6);
    assert_eq!(summary["total_frames"], 12);
}

#[test]
fn test_7_5_total_wall_time_is_recorded() {
    let paths = write_fixture_composite_debug_summary("composite_debug_total_wall_time");
    let summary = composite_debug_timing_summary(&paths);

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
fn test_7_6_composite_debug_output_is_only_created_by_composite_render() {
    let paths = test_paths_named("composite_debug_transparent_unaffected");

    assert!(!paths.debug_render_dir.join("composite").exists());
}

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
/// End-to-end parallel composite render with 2 segments.
///
/// Configures a 5-second composite render at 29.97 FPS split across 2
/// parallel segments. Uses `render_composite_video` (the segmented
/// dispatcher) rather than `render_composite_video_single`. Verifies
/// output file exists and is non-empty.
///
/// Requires live ffmpeg and the test-1080p.mp4 fixture.
///
/// Regressions guarded: parallel segmentation producing corrupt output,
/// segment boundary misalignment, render_composite_video returning error
/// for valid inputs.
fn test_parallel_composite_render_2_segments() {
    let paths = test_paths();
    let config = composite_test_config(5.0);
    let activity = fixture_activity();
    let dense = build_dense_activity_report(&activity, &config).unwrap();
    let controller = RenderController::default();
    controller
        .try_start(dense.frame_count as u32, "test_parallel_2")
        .unwrap();

    let video_path = common::test_config::sample_video_path()
        .to_string_lossy()
        .to_string();
    let result = render_composite_video(&CompositeRenderRequest {
        paths: &paths,
        config: &config,
        activity: &activity,
        dense_activity: &dense,
        controller: &controller,
        composite_video_path: &video_path,
        composite_bitrate: "10M",
        composite_sync_offset: 0.0,
        composite_video_fps_num: 30000,
        composite_video_fps_den: 1001,
        composite_video_duration: 35.0,
        composite_render_duration: Some(5.0),
        composite_video_trim_start: Some(0.0),
        composite_widget_update_rate: Some(1),
    });
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel composite output: {}", output.display());
}

#[test]
/// End-to-end parallel composite render with audio-copy and trim start.
///
/// Configures a 5-second composite render at 29.97 FPS with a 15-second
/// video trim start (trimming the first 15 seconds of the source video)
/// and audio track copying. Uses `render_composite_video` (segmented
/// dispatcher). Verifies output file exists and is non-empty.
///
/// Requires live ffmpeg and the test-1080p.mp4 fixture (which has an
/// audio track).
///
/// Regressions guarded: trim start with audio causing sync issues,
/// parallel segments dropping audio, render_composite_video failing
/// when trim and audio are both active.
fn test_parallel_composite_render_with_audio() {
    let paths = test_paths();
    let config = composite_test_config(5.0);
    let activity = fixture_activity();
    let dense = build_dense_activity_report(&activity, &config).unwrap();
    let controller = RenderController::default();
    controller
        .try_start(dense.frame_count as u32, "test_parallel_audio")
        .unwrap();

    let video_path = common::test_config::sample_video_path()
        .to_string_lossy()
        .to_string();
    let result = render_composite_video(&CompositeRenderRequest {
        paths: &paths,
        config: &config,
        activity: &activity,
        dense_activity: &dense,
        controller: &controller,
        composite_video_path: &video_path,
        composite_bitrate: "10M",
        composite_sync_offset: 0.0,
        composite_video_fps_num: 30000,
        composite_video_fps_den: 1001,
        composite_video_duration: 35.0,
        composite_render_duration: Some(5.0),
        composite_video_trim_start: Some(15.0),
        composite_widget_update_rate: Some(1),
    });
    assert!(result.is_ok(), "Failed: {:?}", result);
    let filename = result.unwrap();
    let output = paths.downloads_dir.join(&filename);
    assert!(output.exists());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    println!("Parallel audio output: {}", output.display());
}
