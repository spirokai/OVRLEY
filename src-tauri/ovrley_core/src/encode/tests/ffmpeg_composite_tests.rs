use super::*;
use std::path::Path;

fn settings(source_fps: Fps, overlay_pipe_fps: Fps, trim_start: f64) -> CompositeFfmpegSettings {
    build_composite_ffmpeg_settings(
        "libx264",
        "60M",
        Path::new("test.mp4"),
        trim_start,
        10.0,
        3840,
        2160,
        source_fps,
        overlay_pipe_fps,
        &HwAccelInfo::default(),
    )
    .unwrap()
}

#[test]
fn test_2_1_builds_command_for_29_97_fps_source_without_rounding() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
    assert_argument_pair(&built.output_args, "-r", "30000/1001");
    assert!(!built.input_1_args.iter().any(|arg| arg == "30"));
    assert!(!built.output_args.iter().any(|arg| arg == "30"));
}

#[test]
fn test_2_2_preserves_source_fps_with_lower_overlay_update_rate() {
    let built = settings(
        Fps::new(60000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
    assert_argument_pair(&built.output_args, "-r", "60000/1001");
}

#[test]
fn test_2_3_sync_offset_is_not_used_as_seek_argument() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert!(!has_argument_pair(&built.input_0_args, "-ss", "300"));
}

#[test]
fn test_2_4_trim_seek_appears_only_for_video_trim_start() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        10.0,
    );

    assert_argument_pair(&built.input_0_args, "-ss", "10");
    assert_argument_pair(&built.input_0_args, "-t", "10");
    assert_argument_pair(&built.input_0_args, "-i", "test.mp4");
}

#[test]
fn test_2_5_rawvideo_pipe_input_has_expected_shape() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_eq!(
        built.input_1_args,
        vec![
            "-thread_queue_size",
            "512",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            "3840x2160",
            "-r",
            "30000/1001",
            "-i",
            "pipe:0"
        ]
    );
}

#[test]
fn test_2_6_filter_graph_labels_and_maps_output() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert!(built.filter_complex.contains("[out]"));
    assert_argument_pair(&built.output_args, "-map", "[out]");
}

#[test]
fn test_2_7_optional_audio_map_and_copy_are_present() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-map", "0:a?");
    assert_argument_pair(&built.output_args, "-c:a", "copy");
}

#[test]
fn test_2_8_float_fps_fallback_can_feed_rational_builder_args() {
    let fps = Fps::from_f64_fallback(29.97).unwrap();
    let built = settings(fps, fps, 0.0);

    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
}

#[test]
fn validates_zero_direct_fps_values() {
    let error = build_composite_ffmpeg_settings(
        "libx264",
        "60M",
        Path::new("test.mp4"),
        0.0,
        10.0,
        3840,
        2160,
        Fps { num: 0, den: 1 },
        Fps::new(30000, 1001).unwrap(),
        &HwAccelInfo::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        "Composite source FPS numerator must be greater than zero"
    );
}

fn assert_argument_pair(args: &[String], key: &str, value: &str) {
    assert!(
        has_argument_pair(args, key, value),
        "missing argument pair {key} {value} in {args:?}"
    );
}

fn has_argument_pair(args: &[String], key: &str, value: &str) -> bool {
    args.windows(2)
        .any(|window| window[0] == key && window[1] == value)
}
