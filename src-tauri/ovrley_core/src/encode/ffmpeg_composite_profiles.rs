//! Editable FFmpeg command templates for MP4 composite encoder profiles.
//!
//! Profiles are intentionally data-shaped: static input, filter, and output
//! fragments live here, while render-specific values such as bitrate, duration,
//! FPS, dimensions, and output path are injected by the composite builder.

use super::ffmpeg_composite::CompositeProfile;

/// Static command fragments for one composite FFmpeg profile.
///
/// This table type keeps profile-specific flags easy to scan and edit without
/// mixing them with render-time values generated for each export.
struct CompositeProfileTemplate {
    name: &'static str,
    codec: &'static str,
    input_args: &'static [&'static str],
    filter_complex: Option<&'static str>,
    output_args: &'static [&'static str],
}

const SOFTWARE_FILTER: &str = "[0:v]setpts=PTS-STARTPTS,scale={width}:{height}[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]";

const VAAPI_FILTER: &str = "[0:v]setpts=PTS-STARTPTS,scale={width}:{height}[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=nv12,hwupload[out]";

const CUDA_FILTER: &str = "[0:v]setpts=PTS-STARTPTS,scale_cuda=format=yuv420p[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];\
[base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]";

const QSV_FULL_FILTER: &str = "[0:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=64[main_hw];\
[1:v]setpts=PTS-STARTPTS,format=bgra,hwupload=extra_hw_frames=64[overlay_hw];\
[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12[out]";

const BUILTIN_PROFILES: &[CompositeProfileTemplate] = &[
    CompositeProfileTemplate {
        name: "software_h264",
        codec: "libx264",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "libx264"],
    },
    CompositeProfileTemplate {
        name: "software_hevc",
        codec: "libx265",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "libx265"],
    },
    CompositeProfileTemplate {
        name: "nvgpu_h264",
        codec: "h264_nvenc",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &[
            "-c:v",
            "h264_nvenc",
            "-rc:v",
            "cbr",
            "-bf:v",
            "3",
            "-profile:v",
            "high",
            "-spatial-aq",
            "true",
        ],
    },
    CompositeProfileTemplate {
        name: "nvgpu_hevc",
        codec: "hevc_nvenc",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &[
            "-c:v",
            "hevc_nvenc",
            "-rc:v",
            "cbr",
            "-bf:v",
            "3",
            "-profile:v",
            "main",
            "-spatial-aq",
            "true",
        ],
    },
    CompositeProfileTemplate {
        name: "nnvgpu_h264",
        codec: "h264_nvenc",
        input_args: &["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
        filter_complex: Some(CUDA_FILTER),
        output_args: &[
            "-c:v",
            "h264_nvenc",
            "-rc:v",
            "cbr",
            "-bf:v",
            "3",
            "-profile:v",
            "main",
            "-spatial-aq",
            "true",
        ],
    },
    CompositeProfileTemplate {
        name: "nnvgpu_hevc",
        codec: "hevc_nvenc",
        input_args: &["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
        filter_complex: Some(CUDA_FILTER),
        output_args: &[
            "-c:v",
            "hevc_nvenc",
            "-rc:v",
            "cbr",
            "-bf:v",
            "3",
            "-profile:v",
            "main",
            "-spatial-aq",
            "true",
        ],
    },
    CompositeProfileTemplate {
        name: "qsv_h264",
        codec: "h264_qsv",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "h264_qsv"],
    },
    CompositeProfileTemplate {
        name: "qsv_hevc",
        codec: "hevc_qsv",
        input_args: &[],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "hevc_qsv"],
    },
    CompositeProfileTemplate {
        name: "qsv_full_h264",
        codec: "h264_qsv",
        input_args: &[
            "-init_hw_device",
            "qsv=hw",
            "-hwaccel",
            "qsv",
            "-hwaccel_output_format",
            "qsv",
        ],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &["-c:v", "h264_qsv", "-global_quality", "25"],
    },
    CompositeProfileTemplate {
        name: "qsv_full_hevc",
        codec: "hevc_qsv",
        input_args: &[
            "-init_hw_device",
            "qsv=hw",
            "-hwaccel",
            "qsv",
            "-hwaccel_output_format",
            "qsv",
        ],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &["-c:v", "hevc_qsv", "-global_quality", "25"],
    },
    CompositeProfileTemplate {
        name: "mac_h264",
        codec: "h264_videotoolbox",
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "h264_videotoolbox", "-q:v", "60"],
    },
    CompositeProfileTemplate {
        name: "mac_hevc",
        codec: "hevc_videotoolbox",
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(SOFTWARE_FILTER),
        output_args: &["-c:v", "hevc_videotoolbox", "-q:v", "60"],
    },
    CompositeProfileTemplate {
        name: "vaapi_h264",
        codec: "h264_vaapi",
        input_args: &[],
        filter_complex: Some(VAAPI_FILTER),
        output_args: &["-c:v", "h264_vaapi"],
    },
    CompositeProfileTemplate {
        name: "vaapi_hevc",
        codec: "hevc_vaapi",
        input_args: &[],
        filter_complex: Some(VAAPI_FILTER),
        output_args: &["-c:v", "hevc_vaapi"],
    },
];

/// Returns the catalog entry for a named composite profile or codec.
///
/// Callers may pass either profile names such as `nvgpu_h264` or encoder codec
/// names such as `h264_nvenc`; codec names resolve to the safe CPU-overlay path.
pub(crate) fn composite_profile_template(name_or_codec: &str) -> Option<CompositeProfile> {
    let normalized = match name_or_codec {
        "auto" | "auto_h264" => "software_h264",
        "auto_hevc" | "auto_h265" => "software_hevc",
        "libx264" => "software_h264",
        "libx265" => "software_hevc",
        "h264_nvenc" => "nvgpu_h264",
        "hevc_nvenc" => "nvgpu_hevc",
        "h264_qsv" => "qsv_h264",
        "hevc_qsv" => "qsv_hevc",
        "h264_videotoolbox" => "mac_h264",
        "hevc_videotoolbox" => "mac_hevc",
        "h264_vaapi" => "vaapi_h264",
        "hevc_vaapi" => "vaapi_hevc",
        other => other,
    };

    BUILTIN_PROFILES
        .iter()
        .find(|profile| profile.name == normalized)
        .map(expand_template)
}

/// Expands one static profile template into owned FFmpeg argument fragments.
///
/// The returned profile deliberately excludes bitrate, FPS, output path, and
/// raw-overlay pipe arguments because those are derived for each render.
fn expand_template(template: &CompositeProfileTemplate) -> CompositeProfile {
    CompositeProfile {
        name: template.name,
        codec: template.codec,
        input_args: template
            .input_args
            .iter()
            .map(|arg| arg.to_string())
            .collect(),
        filter_complex: template.filter_complex.map(str::to_string),
        output_args: template
            .output_args
            .iter()
            .map(|arg| arg.to_string())
            .collect(),
    }
}
