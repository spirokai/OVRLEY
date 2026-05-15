use super::*;
use std::path::Path;

fn settings(source_fps: Fps, overlay_pipe_fps: Fps, trim_start: f64) -> CompositeFfmpegSettings {
    settings_for_codec(
        "libx264",
        "60M",
        source_fps,
        overlay_pipe_fps,
        trim_start,
        &HwAccelInfo::default(),
    )
}

fn settings_for_codec(
    codec: &str,
    bitrate: &str,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    trim_start: f64,
    hwaccel: &HwAccelInfo,
) -> CompositeFfmpegSettings {
    build_composite_ffmpeg_settings(
        codec,
        bitrate,
        Path::new("test.mp4"),
        trim_start,
        10.0,
        3840,
        2160,
        source_fps,
        overlay_pipe_fps,
        hwaccel,
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

#[test]
fn test_8_1_software_h264_profile_uses_cpu_overlay_and_libx264() {
    let built = settings_for_codec(
        "libx264",
        "20M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
        &HwAccelInfo::default(),
    );

    assert_argument_pair(&built.output_args, "-c:v", "libx264");
    assert_argument_pair(&built.output_args, "-b:v", "20M");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(built.hw_init_args.is_empty());
}

#[test]
fn test_8_2_software_h265_profile_uses_cpu_overlay_and_libx265() {
    let built = settings_for_codec(
        "libx265",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &HwAccelInfo::default(),
    );

    assert_argument_pair(&built.output_args, "-c:v", "libx265");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_3_nvenc_h264_simple_path_uses_cpu_overlay_when_available() {
    let hwaccel = HwAccelInfo {
        h264_nvenc_available: true,
        nvenc_available: true,
        ..HwAccelInfo::default()
    };
    let built = settings_for_codec(
        "h264_nvenc",
        "60M",
        Fps::new(60000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_nvenc");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
    assert_argument_pair(&built.output_args, "-r", "60000/1001");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(!built.filter_complex.contains("overlay_cuda"));
}

#[test]
fn test_8_4_nvenc_hevc_unavailable_fails_clearly() {
    let error = build_composite_ffmpeg_settings(
        "hevc_nvenc",
        "60M",
        Path::new("test.mp4"),
        0.0,
        10.0,
        3840,
        2160,
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        &HwAccelInfo::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        "Requested hardware encoder hevc_nvenc is unavailable."
    );
}

#[test]
fn test_8_5_videotoolbox_h264_simple_path_when_available() {
    let hwaccel = HwAccelInfo {
        h264_videotoolbox_available: true,
        videotoolbox_available: true,
        ..HwAccelInfo::default()
    };
    let built = settings_for_codec(
        "h264_videotoolbox",
        "10M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_videotoolbox");
    assert_argument_pair(&built.output_args, "-b:v", "10M");
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_6_videotoolbox_hevc_unavailable_fails_clearly() {
    let error = build_composite_ffmpeg_settings(
        "hevc_videotoolbox",
        "60M",
        Path::new("test.mp4"),
        0.0,
        10.0,
        3840,
        2160,
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        &HwAccelInfo::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        "Requested hardware encoder hevc_videotoolbox is unavailable."
    );
}

#[test]
fn test_8_7_qsv_h264_simple_path_when_available() {
    let hwaccel = HwAccelInfo {
        h264_qsv_available: true,
        qsv_available: true,
        ..HwAccelInfo::default()
    };
    let built = settings_for_codec(
        "h264_qsv",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_qsv");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert!(built.filter_complex.contains("overlay=0:0"));
}

#[test]
fn test_8_8_automatic_h264_uses_software_fallback() {
    let built = settings_for_codec(
        "auto_h264",
        "10M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &HwAccelInfo::default(),
    );

    assert_argument_pair(&built.output_args, "-c:v", "libx264");
    assert_argument_pair(&built.output_args, "-b:v", "10M");
}

#[test]
fn test_8_9_bitrate_override_is_respected_for_every_profile() {
    let hwaccel = HwAccelInfo {
        h264_nvenc_available: true,
        h264_qsv_available: true,
        h264_videotoolbox_available: true,
        nvenc_available: true,
        qsv_available: true,
        videotoolbox_available: true,
        ..HwAccelInfo::default()
    };

    for codec in [
        "libx264",
        "libx265",
        "h264_nvenc",
        "h264_qsv",
        "h264_videotoolbox",
    ] {
        let low = settings_for_codec(
            codec,
            "10M",
            Fps::new(30, 1).unwrap(),
            Fps::new(30, 1).unwrap(),
            0.0,
            &hwaccel,
        );
        let high = settings_for_codec(
            codec,
            "60M",
            Fps::new(30, 1).unwrap(),
            Fps::new(30, 1).unwrap(),
            0.0,
            &hwaccel,
        );

        assert_argument_pair(&low.output_args, "-b:v", "10M");
        assert_argument_pair(&high.output_args, "-b:v", "60M");
    }
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
