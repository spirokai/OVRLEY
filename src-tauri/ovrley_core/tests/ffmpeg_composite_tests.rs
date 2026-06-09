//! Composite FFmpeg command construction tests.
//!
//! Verifies `build_composite_ffmpeg_settings` produces correct argument
//! arrays for every codec path: software (libx264, libx265), hardware
//! (NVENC, QSV, AMF, VideoToolbox), automatic fallback, and full-CUDA/QSV
//! filter stacks. Covers FPS preservation with rational values, trim/seeking,
//! audio copy, filter-graph labeling, bitrate overrides, and clear errors
//! for unavailable encoders.
//!
//! ## Type
//! Unit test. No subprocesses — builds FFmpeg args and inspects the
//! resulting argument arrays.
//!
//! ## Regressions guarded
//! - Rational FPS values rounded to integers in ffmpeg args
//! - Composite trim using `-ss` on video input instead of filter-side
//! - Filter graph labels breaking output mapping
//! - Hardware encoder fallback paths silently degrading
//! - Bitrate overrides ignored for specific profiles
//! - Full-CUDA/QSV paths crashing when filters are unavailable

use std::path::Path;

mod common;

use common::composite::{assert_argument_pair, has_argument_pair};
use ovrley_core::encode::codec_detect::AvailableCodecs;
use ovrley_core::encode::ffmpeg_composite::{
    build_composite_ffmpeg_settings, CompositeFfmpegBuildRequest, CompositeFfmpegSettings,
    HwAccelInfo,
};
use ovrley_core::encode::fps::Fps;

/// Builds composite FFmpeg settings with default libx264 codec for quick tests
/// that only care about FPS/timing/trim behavior, not codec selection.
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

/// Builds composite FFmpeg settings with an explicit codec, bitrate, and
/// hardware-acceleration info — used for codec-path and hardware tests.
fn settings_for_codec(
    codec: &str,
    bitrate: &str,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    trim_start: f64,
    hwaccel: &HwAccelInfo,
) -> CompositeFfmpegSettings {
    build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: codec,
        bitrate,
        video_path: Path::new("test.mp4"),
        video_trim_start: trim_start,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps,
        overlay_pipe_fps,
        hwaccel_available: hwaccel,
    })
    .unwrap()
}

/// Wraps a canonical codec snapshot in the composite hardware-info shell so
/// that tests can selectively mark individual codecs as available while
/// keeping all other fields at their defaults.
fn hwaccel_with_available_codecs(available_codecs: AvailableCodecs) -> HwAccelInfo {
    HwAccelInfo {
        available_codecs,
        ..HwAccelInfo::default()
    }
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
fn test_2_4_video_trim_uses_filter_side_cut_and_audio_seek_input() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        10.0,
    );

    assert_argument_pair(&built.input_0_args, "-i", "test.mp4");
    assert_argument_pair(&built.input_2_args, "-ss", "10");
    assert_argument_pair(&built.input_2_args, "-t", "10");
    assert_argument_pair(&built.input_2_args, "-i", "test.mp4");
    assert!(built
        .filter_complex
        .contains("trim=start=10:end=20,setpts=PTS-STARTPTS,"));
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

    assert_argument_pair(&built.output_args, "-map", "2:a?");
    assert_argument_pair(&built.output_args, "-c:a", "copy");
}

#[test]
fn test_2_7a_video_trim_is_filter_side_even_without_input_seek() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert!(!has_argument_pair(&built.input_0_args, "-ss", "0"));
    assert!(built
        .filter_complex
        .contains("trim=start=0:end=10,setpts=PTS-STARTPTS,"));
    assert!(built.output_args.iter().any(|arg| arg == "-shortest"));
}

#[test]
fn test_2_8_float_fps_fallback_can_feed_rational_builder_args() {
    let fps = Fps::from_f64_fallback(29.97).unwrap();
    let built = settings(fps, fps, 0.0);

    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
}

#[test]
fn validates_zero_direct_fps_values() {
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "libx264",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps { num: 0, den: 1 },
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &HwAccelInfo::default(),
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Encoding error: Composite source FPS numerator must be greater than zero"
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
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
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
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert_argument_pair(&built.output_args, "-pix_fmt", "yuv420p10le");
    assert_argument_pair(&built.output_args, "-profile:v", "main10");
}

#[test]
fn test_8_3_nvenc_h264_simple_path_uses_cpu_overlay_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_nvenc: true,
        nvgpu: true,
        ..AvailableCodecs::default()
    });
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
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "hevc_nvenc",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30000, 1001).unwrap(),
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &HwAccelInfo::default(),
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Encoding error: Requested hardware encoder hevc_nvenc is unavailable."
    );
}

#[test]
fn test_8_5_videotoolbox_h264_simple_path_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_videotoolbox: true,
        videotoolbox: true,
        ..AvailableCodecs::default()
    });
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
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "hevc_videotoolbox",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30000, 1001).unwrap(),
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &HwAccelInfo::default(),
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Encoding error: Requested hardware encoder hevc_videotoolbox is unavailable."
    );
}

#[test]
fn test_8_7_qsv_h264_simple_path_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_qsv: true,
        qsv: true,
        ..AvailableCodecs::default()
    });
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
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_7b_amf_h264_simple_path_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_amf: true,
        ..AvailableCodecs::default()
    });
    let built = settings_for_codec(
        "h264_amf",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_eq!(built.selected_profile_name, "amf_h264");
    assert_argument_pair(&built.output_args, "-c:v", "h264_amf");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert_argument_pair(&built.input_0_args, "-init_hw_device", "d3d11va=dx");
    assert_argument_pair(&built.input_0_args, "-filter_hw_device", "dx");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(built.filter_complex.contains("format=nv12,hwupload[out]"));
}

#[test]
fn test_8_7c_amf_hevc_unavailable_fails_clearly() {
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "hevc_amf",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30000, 1001).unwrap(),
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &HwAccelInfo::default(),
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Encoding error: Requested hardware encoder hevc_amf is unavailable."
    );
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
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_nvenc: true,
        h264_qsv: true,
        h264_amf: true,
        h264_videotoolbox: true,
        nvgpu: true,
        qsv: true,
        videotoolbox: true,
        ..AvailableCodecs::default()
    });

    for codec in [
        "libx264",
        "libx265",
        "h264_nvenc",
        "h264_qsv",
        "h264_amf",
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

#[test]
fn test_9_1_cuda_full_profile_requires_cuda_filter_stack() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_nvenc: true,
        nvgpu: true,
        nnvgpu: false,
        ..AvailableCodecs::default()
    });
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "nnvgpu_h264",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30000, 1001).unwrap(),
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &hwaccel,
    })
    .unwrap_err();

    assert!(error.to_string().contains("overlay_cuda"));
    assert!(error.to_string().contains("scale_cuda"));
    assert!(error.to_string().contains("hwupload"));
}

#[test]
/// Verifies the full-CUDA path (nnvgpu_h264) produces a complete filter stack:
/// scale_cuda → overlay_cuda with hwupload on the overlay input. Also checks
/// the fallback profile is recorded as `nvgpu_h264`.
fn test_9_2_cuda_h264_full_profile_uses_overlay_cuda_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_nvenc: true,
        nvgpu: true,
        nnvgpu: true,
        ..AvailableCodecs::default()
    });
    let built = settings_for_codec(
        "nnvgpu_h264",
        "60M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_eq!(built.selected_profile_name, "nnvgpu_h264");
    assert_eq!(built.fallback_profile_name.as_deref(), Some("nvgpu_h264"));
    assert_argument_pair(&built.input_0_args, "-init_hw_device", "cuda=cuda");
    assert_argument_pair(&built.input_0_args, "-filter_hw_device", "cuda");
    assert_argument_pair(&built.input_0_args, "-hwaccel", "cuda");
    assert_argument_pair(&built.input_0_args, "-hwaccel_output_format", "cuda");
    assert!(built.filter_complex.contains("scale_cuda=format=yuv420p"));
    assert!(built.filter_complex.contains("overlay_cuda"));
    assert_argument_pair(&built.output_args, "-c:v", "h264_nvenc");
}

#[test]
/// Verifies the full-CUDA HEVC path (nnvgpu_hevc) produces the CUDA filter
/// stack with hevc_nvenc as the output codec. Fallback profile must be
/// recorded as `nvgpu_hevc`.
fn test_9_3_cuda_hevc_full_profile_uses_overlay_cuda_when_available() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        hevc_nvenc: true,
        nvgpu: true,
        nnvgpu: true,
        ..AvailableCodecs::default()
    });
    let built = settings_for_codec(
        "nnvgpu_hevc",
        "60M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_eq!(built.selected_profile_name, "nnvgpu_hevc");
    assert_eq!(built.fallback_profile_name.as_deref(), Some("nvgpu_hevc"));
    assert!(built.filter_complex.contains("scale_cuda=format=yuv420p"));
    assert!(built.filter_complex.contains("overlay_cuda"));
    assert_argument_pair(&built.output_args, "-c:v", "hevc_nvenc");
}

#[test]
fn test_9_5_qsv_full_profile_requires_qsv_filter_stack() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_qsv: true,
        qsv: true,
        qsv_full: false,
        ..AvailableCodecs::default()
    });
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "qsv_full_h264",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30000, 1001).unwrap(),
        overlay_pipe_fps: Fps::new(30000, 1001).unwrap(),
        hwaccel_available: &hwaccel,
    })
    .unwrap_err();

    assert!(error.to_string().contains("overlay_qsv"));
    assert!(error.to_string().contains("scale_qsv"));
    assert!(error.to_string().contains("hwupload"));
}

#[test]
/// Full-QSV path (qsv_full_h264) with all filters available exercises the
/// complete QSV filter stack: scale_qsv for scaling → hwupload for overlay
/// input → overlay_qsv for composite. Verifies no hwdownload (stays on GPU)
/// and that detected init args are used verbatim.
fn test_9_6_qsv_full_profile_uses_overlay_qsv_when_available() {
    let detected_args = vec![
        "-init_hw_device".to_string(),
        "dxva2=dx".to_string(),
        "-init_hw_device".to_string(),
        "qsv=qs@dx".to_string(),
        "-filter_hw_device".to_string(),
        "qs".to_string(),
        "-hwaccel".to_string(),
        "qsv".to_string(),
        "-hwaccel_output_format".to_string(),
        "qsv".to_string(),
    ];
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_qsv: true,
        qsv: true,
        qsv_full: true,
        qsv_full_init_args: detected_args.clone(),
        ..AvailableCodecs::default()
    });
    let built = settings_for_codec(
        "qsv_full_h264",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_eq!(built.selected_profile_name, "qsv_full_h264");
    assert_eq!(built.fallback_profile_name.as_deref(), Some("qsv_h264"));
    assert!(built.input_0_args.starts_with(&detected_args));
    assert_argument_pair(&built.input_0_args, "-hwaccel", "qsv");
    assert_argument_pair(&built.input_0_args, "-hwaccel_output_format", "qsv");
    assert!(built
        .filter_complex
        .contains("scale_qsv=w=3840:h=2160:format=nv12[main_hw]"));
    assert!(built
        .filter_complex
        .contains("[1:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=64[overlay_hw]"));
    assert!(built.filter_complex.contains("overlay_qsv"));
    assert!(!built.filter_complex.contains("hwdownload"));
    assert_argument_pair(&built.output_args, "-c:v", "h264_qsv");
}

#[test]
/// When `qsv_full_init_args` is empty, the full-QSV path must fail with a
/// clear error about missing hardware-device init args rather than silently
/// producing broken args.
fn test_9_6_qsv_full_profile_requires_detected_init_args() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_qsv: true,
        qsv: true,
        qsv_full: true,
        qsv_full_init_args: Vec::new(),
        ..AvailableCodecs::default()
    });
    let error = build_composite_ffmpeg_settings(&CompositeFfmpegBuildRequest {
        codec_name: "qsv_full_h264",
        bitrate: "60M",
        video_path: Path::new("test.mp4"),
        video_trim_start: 0.0,
        render_duration: 10.0,
        width: 3840,
        height: 2160,
        source_fps: Fps::new(30, 1).unwrap(),
        overlay_pipe_fps: Fps::new(30, 1).unwrap(),
        hwaccel_available: &hwaccel,
    })
    .unwrap_err();

    assert!(error.to_string().contains("QSV hardware-device init args"));
}

#[test]
/// When `qsv_full_init_args` is populated (e.g., from a prior `ffmpeg -init_hw_device` probe),
/// the full-QSV profile must use those exact init args as the input_0 prefix.
fn test_9_6_qsv_full_profile_uses_detected_init_args_when_available() {
    let detected_args = vec![
        "-init_hw_device".to_string(),
        "d3d11va=dx:1".to_string(),
        "-init_hw_device".to_string(),
        "qsv=qs@dx".to_string(),
        "-filter_hw_device".to_string(),
        "qs".to_string(),
        "-hwaccel".to_string(),
        "qsv".to_string(),
        "-hwaccel_output_format".to_string(),
        "qsv".to_string(),
    ];
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_qsv: true,
        qsv: true,
        qsv_full: true,
        qsv_full_init_args: detected_args.clone(),
        ..AvailableCodecs::default()
    });
    let built = settings_for_codec(
        "qsv_full_h264",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert!(built.input_0_args.starts_with(&detected_args));
}

#[test]
fn test_9_7_safe_codec_names_do_not_select_experimental_profiles() {
    let hwaccel = hwaccel_with_available_codecs(AvailableCodecs {
        h264_nvenc: true,
        h264_qsv: true,
        nvgpu: true,
        qsv: true,
        nnvgpu: true,
        qsv_full: true,
        ..AvailableCodecs::default()
    });

    let nvenc = settings_for_codec(
        "h264_nvenc",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );
    let qsv = settings_for_codec(
        "h264_qsv",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        &hwaccel,
    );

    assert_eq!(nvenc.selected_profile_name, "nvgpu_h264");
    assert_eq!(qsv.selected_profile_name, "qsv_h264");
    assert!(!nvenc.filter_complex.contains("overlay_cuda"));
    assert!(!qsv.filter_complex.contains("overlay_qsv"));
}

