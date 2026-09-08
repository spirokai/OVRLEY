//! Composite FFmpeg command construction tests.
//!
//! Verifies `build_composite_ffmpeg_settings` produces correct argument
//! arrays for every codec path: software (libx264, libx265), hardware
//! (NVENC, QSV, AMF, VideoToolbox), automatic fallback, and full-CUDA/QSV
//! filter stacks. Covers FPS preservation with rational values, trim/seeking,
//! filtered audio encoding, filter-graph labeling, bitrate overrides, and clear errors
//! for unavailable encoders.
//!
//! ## Type
//! Unit test. No subprocesses — builds FFmpeg args and inspects the
//! resulting argument arrays.
//!
//! ## Regressions guarded
//! - Rational FPS values rounded to integers in ffmpeg args
//! - Composite trim using coarse input seeking with an exact filter-side boundary
//! - Filter graph labels breaking output mapping
//! - Hardware encoder fallback paths silently degrading
//! - Bitrate overrides ignored for specific profiles
//! - Full-CUDA/QSV paths crashing when filters are unavailable

mod common;

use common::composite::{assert_argument_pair, has_argument_pair};
use ovrley_core::encode::composite::CompositeRenderPlan;
use ovrley_core::encode::ffmpeg::catalog::CompositeCodecId;
use ovrley_core::encode::ffmpeg::composite::{
    build_composite_ffmpeg_settings, CompositeFfmpegSettings,
};
use ovrley_core::encode::fps::Fps;
use ovrley_core::encode::pipeline::composite_plan::derive_composite_render_plan;
use ovrley_core::normalize::{validate_scene_config, SceneConfig};
use ovrley_core::render::FrameSize;
use serde_json::json;

/// Builds composite FFmpeg settings with default libx264 codec for quick tests
/// that only care about FPS/timing/trim behavior, not codec selection.
fn settings(source_fps: Fps, overlay_pipe_fps: Fps, trim_start: f64) -> CompositeFfmpegSettings {
    settings_for_codec("libx264", "60M", source_fps, overlay_pipe_fps, trim_start)
}

/// Builds composite FFmpeg settings with an explicit codec, bitrate, and
/// hardware-acceleration info — used for codec-path and hardware tests.
fn settings_for_codec(
    codec: &str,
    bitrate: &str,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    trim_start: f64,
) -> CompositeFfmpegSettings {
    settings_for_codec_with_rotation(
        codec,
        bitrate,
        source_fps,
        overlay_pipe_fps,
        trim_start,
        None,
    )
}

fn settings_for_codec_with_rotation(
    codec: &str,
    bitrate: &str,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    trim_start: f64,
    rotation_degrees: Option<i32>,
) -> CompositeFfmpegSettings {
    let render = render_plan(codec, bitrate, source_fps, overlay_pipe_fps, trim_start);
    build_composite_ffmpeg_settings(
        &render,
        FrameSize {
            width: 3840,
            height: 2160,
        },
        true,
        rotation_degrees,
    )
    .unwrap()
}

/// Builds composite settings for tests that vary resolution-specific input args.
fn settings_for_dimensions(width: u32, height: u32) -> CompositeFfmpegSettings {
    let fps = Fps::new(30000, 1001).unwrap();
    let render = render_plan("libx264", "60M", fps, fps, 0.0);
    build_composite_ffmpeg_settings(&render, FrameSize { width, height }, true, None).unwrap()
}

fn cuda_settings_for_dimensions(codec: &str, width: u32, height: u32) -> CompositeFfmpegSettings {
    let fps = Fps::new(30000, 1001).unwrap();
    let render = render_plan(codec, "60M", fps, fps, 0.0);
    build_composite_ffmpeg_settings(&render, FrameSize { width, height }, true, None).unwrap()
}

fn render_plan(
    codec: &str,
    bitrate: &str,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    trim_start: f64,
) -> CompositeRenderPlan {
    let mut scene: SceneConfig =
        serde_json::from_value(common::seam::explicit_scene_json()).unwrap();
    scene.ffmpeg = json!({"codec": codec});
    scene.composite_video_path = Some("test.mp4".to_string());
    scene.composite_bitrate = Some(bitrate.to_string());
    scene.composite_sync_offset = Some(0.0);
    let (fps_num, fps_den) = source_fps.components();
    scene.composite_video_fps_num = Some(fps_num);
    scene.composite_video_fps_den = Some(fps_den);
    scene.composite_video_duration = Some(trim_start + 10.0);
    scene.composite_render_duration = Some(10.0);
    scene.composite_video_trim_start = Some(trim_start);
    scene.composite_widget_update_rate =
        Some((source_fps.as_f64() / overlay_pipe_fps.as_f64()).round() as u32);
    let mut scene = validate_scene_config(scene).unwrap();
    if codec == "qsv_full_h264" {
        scene.ffmpeg.qsv_full_init_args = vec![
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
    }
    derive_composite_render_plan(&mut scene, None).unwrap()
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
fn test_2_4_video_trim_uses_coarse_seek_and_filter_side_cut_for_video_and_audio() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        10.0,
    );

    assert_argument_pair(&built.input_0_args, "-ss", "5");
    assert_argument_pair(&built.input_0_args, "-i", "test.mp4");
    assert_argument_pair(&built.input_2_args, "-ss", "5");
    assert_argument_pair(&built.input_2_args, "-i", "test.mp4");
    assert!(!has_argument_pair(&built.input_2_args, "-t", "10"));
    assert!(built
        .filter_complex
        .contains("trim=start=5:end=15,setpts=PTS-STARTPTS,"));
    assert!(built
        .filter_complex
        .contains("[2:a]atrim=start=5:duration=10,asetpts=N/SR/TB[aout]"));
}

#[test]
fn test_2_4a_short_trim_keeps_zero_input_seek_and_exact_filter_offset() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        3.0,
    );

    assert!(!built.input_0_args.iter().any(|arg| arg == "-ss"));
    assert!(!built.input_2_args.iter().any(|arg| arg == "-ss"));
    assert!(built
        .filter_complex
        .contains("trim=start=3:end=13,setpts=PTS-STARTPTS,"));
    assert!(built
        .filter_complex
        .contains("[2:a]atrim=start=3:duration=10,asetpts=N/SR/TB[aout]"));
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
            "16",
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
fn test_2_5a_rawvideo_pipe_queue_size_scales_with_resolution() {
    for (width, height, expected_queue_size) in [
        (1280, 720, "64"),
        (1920, 1080, "64"),
        (2560, 1440, "16"),
        (3840, 2160, "16"),
        (5120, 2880, "4"),
        (7680, 4320, "4"),
    ] {
        let built = settings_for_dimensions(width, height);
        assert_argument_pair(
            &built.input_1_args,
            "-thread_queue_size",
            expected_queue_size,
        );
    }
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
fn test_2_7_filtered_audio_map_and_encoding_are_present() {
    let built = settings(
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-map", "[aout]");
    assert_argument_pair(&built.output_args, "-c:a", "aac");
    assert_argument_pair(&built.output_args, "-b:a", "192k");
}

#[test]
fn test_2_7b_source_without_audio_omits_audio_input_and_filter_graph() {
    let render = render_plan(
        "libx264",
        "60M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );
    let built = build_composite_ffmpeg_settings(
        &render,
        FrameSize {
            width: 3840,
            height: 2160,
        },
        false,
        None,
    )
    .unwrap();

    assert!(built.input_2_args.is_empty());
    assert!(!built.filter_complex.contains("[2:a]"));
    assert!(!built.output_args.iter().any(|arg| arg == "[aout]"));
    assert!(!built.output_args.iter().any(|arg| arg == "-c:a"));
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
    assert!(!built.output_args.iter().any(|arg| arg == "-shortest"));
}

#[test]
fn test_2_8_float_fps_metadata_can_feed_rational_builder_args() {
    let fps = Fps::from_f64_metadata(29.97).unwrap();
    let built = settings(fps, fps, 0.0);

    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
}

#[test]
fn rejects_zero_fps_at_construction() {
    assert!(Fps::new(0, 1).is_err());
}

#[test]
fn test_8_1_software_h264_profile_uses_cpu_overlay_and_libx264() {
    let built = settings_for_codec(
        "libx264",
        "20M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-c:v", "libx264");
    assert_argument_pair(&built.output_args, "-b:v", "20M");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_2_software_h265_profile_uses_cpu_overlay_and_libx265() {
    let built = settings_for_codec(
        "libx265",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-c:v", "libx265");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert_argument_pair(&built.output_args, "-pix_fmt", "yuv420p10le");
    assert_argument_pair(&built.output_args, "-profile:v", "main10");
}

#[test]
fn test_8_3_nvenc_h264_simple_path_uses_cpu_overlay_when_available() {
    let built = settings_for_codec(
        "h264_nvenc",
        "60M",
        Fps::new(60000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_nvenc");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert_argument_pair(&built.input_1_args, "-r", "30000/1001");
    assert_argument_pair(&built.output_args, "-r", "60000/1001");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(!built.filter_complex.contains("overlay_cuda"));
}

#[test]
fn test_8_5_videotoolbox_h264_simple_path_when_available() {
    let built = settings_for_codec(
        "h264_videotoolbox",
        "10M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_videotoolbox");
    assert_argument_pair(&built.output_args, "-b:v", "10M");
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_7_qsv_h264_simple_path_when_available() {
    let built = settings_for_codec(
        "h264_qsv",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );

    assert_argument_pair(&built.output_args, "-c:v", "h264_qsv");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(built.filter_complex.contains("format=yuv420p[out]"));
}

#[test]
fn test_8_7b_amf_h264_simple_path_when_available() {
    let built = settings_for_codec(
        "h264_amf",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );

    assert_eq!(built.codec_id, CompositeCodecId::AmfH264);
    assert_argument_pair(&built.output_args, "-c:v", "h264_amf");
    assert_argument_pair(&built.output_args, "-b:v", "60M");
    assert_argument_pair(&built.input_0_args, "-init_hw_device", "d3d11va=dx");
    assert_argument_pair(&built.input_0_args, "-filter_hw_device", "dx");
    assert!(built.filter_complex.contains("overlay=0:0"));
    assert!(built.filter_complex.contains("format=nv12,hwupload[out]"));
}

#[test]
fn test_8_9_bitrate_override_is_respected_for_every_profile() {
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
        );
        let high = settings_for_codec(
            codec,
            "60M",
            Fps::new(30, 1).unwrap(),
            Fps::new(30, 1).unwrap(),
            0.0,
        );

        assert_argument_pair(&low.output_args, "-b:v", "10M");
        assert_argument_pair(&high.output_args, "-b:v", "60M");
    }
}

#[test]
/// Verifies the full-CUDA path (nnvgpu_h264) produces a complete filter stack:
/// scale_cuda → overlay_cuda with hwupload on the overlay input. Also checks
/// the fallback profile is recorded as `nvgpu_h264`.
fn test_9_2_cuda_h264_full_profile_uses_overlay_cuda_when_available() {
    let built = settings_for_codec(
        "nnvgpu_h264",
        "60M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_eq!(built.codec_id, CompositeCodecId::NnvgpuH264);
    assert_eq!(
        built.codec_id.metadata().fallback_profile,
        Some(CompositeCodecId::NvgpuH264)
    );
    assert_argument_pair(&built.input_0_args, "-init_hw_device", "cuda=cuda");
    assert_argument_pair(&built.input_0_args, "-filter_hw_device", "cuda");
    assert_argument_pair(&built.input_0_args, "-hwaccel", "cuda");
    assert_argument_pair(&built.input_0_args, "-hwaccel_output_format", "cuda");
    assert!(built
        .filter_complex
        .contains("scale_cuda=w=3840:h=2160:format=yuv420p"));
    assert!(built.filter_complex.contains("overlay_cuda"));
    assert_argument_pair(&built.output_args, "-c:v", "h264_nvenc");
    assert_argument_pair(
        &built.output_args,
        "-bsf:v",
        "h264_metadata=crop_right=0:crop_bottom=16",
    );
}

#[test]
fn test_9_2_1_cuda_h264_crops_32_pixel_frame_overhang_on_each_axis() {
    let landscape = cuda_settings_for_dimensions("nnvgpu_h264", 1920, 1080);
    let portrait = cuda_settings_for_dimensions("nnvgpu_h264", 1080, 1920);
    let both_axes = cuda_settings_for_dimensions("nnvgpu_h264", 1918, 1078);
    let aligned = cuda_settings_for_dimensions("nnvgpu_h264", 1920, 1920);

    assert_argument_pair(
        &landscape.output_args,
        "-bsf:v",
        "h264_metadata=crop_right=0:crop_bottom=8",
    );
    assert_argument_pair(
        &portrait.output_args,
        "-bsf:v",
        "h264_metadata=crop_right=8:crop_bottom=0",
    );
    assert_argument_pair(
        &both_axes.output_args,
        "-bsf:v",
        "h264_metadata=crop_right=2:crop_bottom=10",
    );
    assert!(!aligned.output_args.iter().any(|arg| arg == "-bsf:v"));
}

#[test]
/// Verifies the full-CUDA HEVC path (nnvgpu_hevc) produces the CUDA filter
/// stack with hevc_nvenc as the output codec. Fallback profile must be
/// recorded as `nvgpu_hevc`.
fn test_9_3_cuda_hevc_full_profile_uses_overlay_cuda_when_available() {
    let built = settings_for_codec(
        "nnvgpu_hevc",
        "60M",
        Fps::new(30000, 1001).unwrap(),
        Fps::new(30000, 1001).unwrap(),
        0.0,
    );

    assert_eq!(built.codec_id, CompositeCodecId::NnvgpuHevc);
    assert_eq!(
        built.codec_id.metadata().fallback_profile,
        Some(CompositeCodecId::NvgpuHevc)
    );
    assert!(built
        .filter_complex
        .contains("scale_cuda=w=3840:h=2160:format=yuv420p"));
    assert!(built.filter_complex.contains("overlay_cuda"));
    assert_argument_pair(&built.output_args, "-c:v", "hevc_nvenc");
    assert_argument_pair(
        &built.output_args,
        "-bsf:v",
        "hevc_metadata=width=3840:height=2160",
    );
}

#[test]
fn test_9_3_1_cuda_hevc_uses_display_oriented_output_dimensions() {
    let landscape = cuda_settings_for_dimensions("nnvgpu_hevc", 1920, 1080);
    let portrait = cuda_settings_for_dimensions("nnvgpu_hevc", 1080, 1920);

    assert_argument_pair(
        &landscape.output_args,
        "-bsf:v",
        "hevc_metadata=width=1920:height=1080",
    );
    assert_argument_pair(
        &portrait.output_args,
        "-bsf:v",
        "hevc_metadata=width=1080:height=1920",
    );
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
    let built = settings_for_codec_with_rotation(
        "qsv_full_h264",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
        Some(0),
    );

    assert_eq!(built.codec_id, CompositeCodecId::QsvFullH264);
    assert_eq!(
        built.codec_id.metadata().fallback_profile,
        Some(CompositeCodecId::QsvH264)
    );
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
    assert_argument_pair(&built.input_0_args, "-noautorotate", "-i");
    assert!(!has_argument_pair(
        &built.output_args,
        "-metadata:s:v:0",
        "rotate=0"
    ));
    assert_argument_pair(&built.output_args, "-c:v", "h264_qsv");
}

#[test]
fn qsv_full_rotated_sources_rotate_only_the_rgba_overlay() {
    let cases = [
        (
            90,
            "scale_qsv=w=2160:h=3840:format=nv12[main_hw]",
            "transpose=1,",
        ),
        (
            180,
            "scale_qsv=w=3840:h=2160:format=nv12[main_hw]",
            "hflip,vflip,",
        ),
        (
            270,
            "scale_qsv=w=2160:h=3840:format=nv12[main_hw]",
            "transpose=2,",
        ),
    ];

    for (rotation, main_scale, overlay_rotation) in cases {
        let built = settings_for_codec_with_rotation(
            "qsv_full_h264",
            "60M",
            Fps::new(30, 1).unwrap(),
            Fps::new(30, 1).unwrap(),
            0.0,
            Some(rotation),
        );

        assert_argument_pair(&built.input_0_args, "-noautorotate", "-i");
        assert!(built.filter_complex.contains(main_scale));
        assert!(built.filter_complex.contains(&format!(
            "[1:v]setpts=PTS-STARTPTS,{overlay_rotation}hwupload=extra_hw_frames=64[overlay_hw]"
        )));
        assert!(!built.filter_complex.contains("vpp_qsv"));
        assert!(!built
            .filter_complex
            .contains("sidedata=mode=delete:type=DISPLAYMATRIX"));
        assert!(!has_argument_pair(
            &built.output_args,
            "-metadata:s:v:0",
            "rotate=0"
        ));
    }

    let render = render_plan(
        "qsv_full_h264",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );
    let error = build_composite_ffmpeg_settings(
        &render,
        FrameSize {
            width: 3840,
            height: 2160,
        },
        true,
        Some(45),
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("Unsupported source rotation 45 degrees"));
}

#[test]
fn test_9_7_safe_codec_names_do_not_select_experimental_profiles() {
    let nvenc = settings_for_codec(
        "h264_nvenc",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );
    let qsv = settings_for_codec(
        "h264_qsv",
        "60M",
        Fps::new(30, 1).unwrap(),
        Fps::new(30, 1).unwrap(),
        0.0,
    );

    assert_eq!(nvenc.codec_id, CompositeCodecId::NvgpuH264);
    assert_eq!(qsv.codec_id, CompositeCodecId::QsvH264);
    assert!(!nvenc.filter_complex.contains("overlay_cuda"));
    assert!(!qsv.filter_complex.contains("overlay_qsv"));
}
