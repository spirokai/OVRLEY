//! Editable FFmpeg command templates for MP4 composite encoder profiles.
//!
//! Profiles are intentionally data-shaped: static input, filter, and output
//! fragments live here, while render-specific values such as bitrate, duration,
//! FPS, dimensions, trim filters, and output path are injected by the
//! composite builder.

use super::catalog::CompositeCodecId;
use super::composite::CompositeProfile;

const SOFTWARE_H264_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]";

const SOFTWARE_HEVC_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1[out]";

const VAAPI_FILTER: &str =
    "[0:v]{base_video_filters}scale_vaapi=w={width}:h={height}:format=nv12[main_hw];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[overlay_hw];\
[main_hw][overlay_hw]overlay_vaapi=x=0:y=0:eof_action=repeat:shortest=1[out]";

const AMF_D3D11_INPUT_ARGS: &[&str] = &["-init_hw_device", "d3d11va=dx", "-filter_hw_device", "dx"];

const AMF_D3D11_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=nv12,hwupload[out]";

const CUDA_H264_FILTER: &str =
    "[0:v]{base_video_filters}scale_cuda=w={width}:h={height}:format=yuv420p[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];\
[base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]";

const CUDA_HEVC_FILTER: &str =
    "[0:v]{base_video_filters}scale_cuda=w={width}:h={height}:format=yuv420p[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];\
[base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]";

const QSV_FULL_FILTER: &str =
    "[0:v]{base_video_filters}scale_qsv=w={main_width}:h={main_height}:format=nv12[main_hw];\
[1:v]setpts=PTS-STARTPTS,{qsv_overlay_cpu_rotation_filter}hwupload=extra_hw_frames=64[overlay_hw];\
[main_hw][overlay_hw]overlay_qsv=x=0:y=0[out]";

const BUILTIN_PROFILES: &[CompositeProfile] = &[
    CompositeProfile {
        codec_id: CompositeCodecId::SoftwareH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &["-preset", "veryfast"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::SoftwareHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &[
            "-pix_fmt",
            "yuv420p10le",
            "-profile:v",
            "main10",
            "-preset",
            "veryfast",
            "-tag:v",
            "hvc1",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NvgpuH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &[
            "-rc:v",
            "vbr",
            "-bf:v",
            "3",
            "-profile:v",
            "high",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NvgpuHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &[
            "-rc:v",
            "vbr",
            "-bf:v",
            "3",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
            "-tag:v",
            "hvc1",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NnvgpuH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[
            "-init_hw_device",
            "cuda=cuda",
            "-filter_hw_device",
            "cuda",
            "-hwaccel",
            "cuda",
            "-hwaccel_output_format",
            "cuda",
        ],
        filter_complex: Some(CUDA_H264_FILTER),
        output_args: &[
            "-rc:v",
            "vbr",
            "-bf:v",
            "3",
            "-profile:v",
            "main",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NnvgpuHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[
            "-init_hw_device",
            "cuda=cuda",
            "-filter_hw_device",
            "cuda",
            "-hwaccel",
            "cuda",
            "-hwaccel_output_format",
            "cuda",
        ],
        filter_complex: Some(CUDA_HEVC_FILTER),
        output_args: &[
            "-rc:v",
            "vbr",
            "-bf:v",
            "3",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
            "-tag:v",
            "hvc1",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &["-mbbrc", "1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &["-mbbrc", "1", "-tag:v", "hvc1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvFullH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &["-mbbrc", "1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvFullHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &["-mbbrc", "1", "-tag:v", "hvc1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::MacH264,
        cpu_cores_per_frame_worker: 0,
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &[],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::MacHevc,
        cpu_cores_per_frame_worker: 0,
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &["-tag:v", "hvc1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::VaapiH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &["-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi"],
        filter_complex: Some(VAAPI_FILTER),
        output_args: &["-rc_mode", "VBR"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::VaapiHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &["-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi"],
        filter_complex: Some(VAAPI_FILTER),
        output_args: &["-rc_mode", "VBR", "-tag:v", "hvc1"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::AmfH264,
        cpu_cores_per_frame_worker: 4,
        input_args: AMF_D3D11_INPUT_ARGS,
        filter_complex: Some(AMF_D3D11_FILTER),
        output_args: &["-rc", "vbr_peak"],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::AmfHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: AMF_D3D11_INPUT_ARGS,
        filter_complex: Some(AMF_D3D11_FILTER),
        output_args: &["-rc", "vbr_peak", "-tag:v", "hvc1"],
    },
];

/// Expands the command template owned by a validated composite codec ID.
pub fn composite_profile(codec_id: CompositeCodecId) -> &'static CompositeProfile {
    BUILTIN_PROFILES
        .iter()
        .find(|profile| profile.codec_id == codec_id)
        .expect("composite profile catalog is exhaustive")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_output_arg_pair(codec_id: CompositeCodecId, flag: &str, value: &str) {
        let output_args = composite_profile(codec_id).output_args;
        assert!(
            output_args.windows(2).any(|pair| pair == [flag, value]),
            "missing {flag} {value} for {}: {output_args:?}",
            codec_id.metadata().profile_name
        );
    }

    #[test]
    fn videotoolbox_profiles_force_one_worker_and_all_others_reserve_four_cores() {
        for profile in BUILTIN_PROFILES {
            let expected = if profile
                .codec_id
                .metadata()
                .encoder_id
                .metadata()
                .ffmpeg_name
                .ends_with("_videotoolbox")
            {
                0
            } else {
                4
            };
            assert_eq!(
                profile.cpu_cores_per_frame_worker,
                expected,
                "unexpected frame-worker CPU cost for {}",
                profile.codec_id.metadata().profile_name
            );
        }
    }

    #[test]
    fn hardware_profiles_with_explicit_rate_control_use_vbr() {
        for codec_id in [
            CompositeCodecId::NvgpuH264,
            CompositeCodecId::NvgpuHevc,
            CompositeCodecId::NnvgpuH264,
            CompositeCodecId::NnvgpuHevc,
        ] {
            assert_output_arg_pair(codec_id, "-rc:v", "vbr");
            assert_output_arg_pair(codec_id, "-temporal-aq", "true");
        }

        for codec_id in [CompositeCodecId::VaapiH264, CompositeCodecId::VaapiHevc] {
            assert_output_arg_pair(codec_id, "-rc_mode", "VBR");
        }

        for codec_id in [CompositeCodecId::AmfH264, CompositeCodecId::AmfHevc] {
            assert_output_arg_pair(codec_id, "-rc", "vbr_peak");
        }
    }

    #[test]
    fn qsv_profiles_enable_macroblock_level_bitrate_control() {
        for codec_id in [
            CompositeCodecId::QsvH264,
            CompositeCodecId::QsvHevc,
            CompositeCodecId::QsvFullH264,
            CompositeCodecId::QsvFullHevc,
        ] {
            assert_output_arg_pair(codec_id, "-mbbrc", "1");
        }
    }

    #[test]
    fn no_composite_profile_requests_cbr() {
        for profile in BUILTIN_PROFILES {
            assert!(
                !profile
                    .output_args
                    .iter()
                    .any(|arg| arg.eq_ignore_ascii_case("cbr")),
                "{} still requests CBR",
                profile.codec_id.metadata().profile_name
            );
        }
    }
}
