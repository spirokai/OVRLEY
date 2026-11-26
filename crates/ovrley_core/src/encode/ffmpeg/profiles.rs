//! Editable FFmpeg command templates of the composite and transparent encoder profiles.
//!
//! Profiles are data: static input, filter and output fragments, including quality and bitrate
//! templates. The argument builders expand render-specific values.

use super::catalog::CompositeCodecId;
use super::catalog::TransparentCodecId;

/// Profile-specific FFmpeg templates for composite encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeProfile {
    pub codec_id: CompositeCodecId,
    pub cpu_cores_per_frame_worker: usize,
    pub input_args: &'static [&'static str],
    pub filter_complex: Option<&'static str>,
    pub output_args: &'static [&'static str],
    /// Uses `{quality}`, `{videotoolbox_h264_quality}` or
    /// `{videotoolbox_hevc_quality}` value placeholders.
    pub quality_args: &'static [&'static str],
    /// Uses `{bitrate}`, `{maxrate}`, and `{bufsize}` values in Mbps.
    pub bitrate_args: &'static [&'static str],
}

// CPU overlay graphs crop every atlas slot (`{atlas_slot_crops}` labels the
// crops) and chain one overlay per slot onto `[base]` at the slot's scene
// position (`{atlas_slot_overlays}` leaves the last overlay unlabelled).
const SOFTWARE_H264_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS,{atlas_slot_crops};\
{atlas_slot_overlays},format=yuv420p[out]";

const SOFTWARE_HEVC_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS,{atlas_slot_crops};\
{atlas_slot_overlays}[out]";

// VideoToolbox graphs overlay on the CPU like the software graphs and hand the
// encoder NV12, its native layout. Left unpinned, FFmpeg negotiates packed
// 4:4:4 `ayuv` for HEVC, converting every frame on the CPU only for
// VideoToolbox to subsample it again (and failing outright for full range).
const VIDEOTOOLBOX_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS,{atlas_slot_crops};\
{atlas_slot_overlays},format=nv12[out]";

// Hardware overlay graphs crop every atlas slot on the CPU, upload each crop
// and chain one hardware overlay per slot onto `[base]` the same way. QSV full
// overlay places each slot on its coded main video.
const VAAPI_FILTER: &str =
    "[0:v]{base_video_filters}scale_vaapi=w={width}:h={height}:format=nv12[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,{atlas_slot_crops};\
{atlas_slot_overlays}[out]";

// The filter graph uploads each overlay slot with `hwupload`, which needs a device
// of its own; `-hwaccel vaapi` alone only gives the decoder one.
const VAAPI_INPUT_ARGS: &[&str] = &[
    "-init_hw_device",
    "vaapi=va",
    "-filter_hw_device",
    "va",
    "-hwaccel",
    "vaapi",
    "-hwaccel_output_format",
    "vaapi",
];

const AMF_D3D11_INPUT_ARGS: &[&str] = &["-init_hw_device", "d3d11va=dx", "-filter_hw_device", "dx"];

const AMF_D3D11_FILTER: &str = "[0:v]{base_video_filters}null[base];\
[1:v]setpts=PTS-STARTPTS,{atlas_slot_crops};\
{atlas_slot_overlays},format=nv12,hwupload[out]";

const CUDA_H264_FILTER: &str =
    "[0:v]{base_video_filters}scale_cuda=w={width}:h={height}:format=yuv420p[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,{atlas_slot_crops};\
{atlas_slot_overlays}[out]";

const CUDA_HEVC_FILTER: &str =
    "[0:v]{base_video_filters}scale_cuda=w={width}:h={height}:format=yuv420p[base];\
[1:v]setpts=PTS-STARTPTS,format=yuva420p,{atlas_slot_crops};\
{atlas_slot_overlays}[out]";

const QSV_FULL_FILTER: &str =
    "[0:v]{base_video_filters}scale_qsv=w={main_width}:h={main_height}:format=nv12[base];\
[1:v]setpts=PTS-STARTPTS,{atlas_slot_crops};\
{atlas_slot_overlays}[out]";

const COMPOSITE_PROFILES: &[CompositeProfile] = &[
    CompositeProfile {
        codec_id: CompositeCodecId::SoftwareH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &["-preset", "veryfast"],
        quality_args: &["-crf", "{quality}"],
        bitrate_args: &[
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
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
        quality_args: &["-crf", "{quality}"],
        bitrate_args: &[
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NvgpuH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &[
            "-bf:v",
            "3",
            "-profile:v",
            "high",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
        ],
        quality_args: &["-rc:v", "vbr", "-b:v", "0", "-maxrate", "200M", "-cq:v", "{quality}"],
        bitrate_args: &[
            "-rc:v",
            "vbr",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::NvgpuHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &[
            "-bf:v",
            "3",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
            "-tag:v",
            "hvc1",
        ],
        quality_args: &["-rc:v", "vbr", "-b:v", "0", "-maxrate", "200M", "-cq:v", "{quality}"],
        bitrate_args: &[
            "-rc:v",
            "vbr",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
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
            "-bf:v",
            "3",
            "-profile:v",
            "main",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
        ],
        quality_args: &["-rc:v", "vbr", "-b:v", "0", "-maxrate", "200M", "-cq:v", "{quality}"],
        bitrate_args: &[
            "-rc:v",
            "vbr",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
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
            "-bf:v",
            "3",
            "-spatial-aq",
            "true",
            "-temporal-aq",
            "true",
            "-tag:v",
            "hvc1",
        ],
        quality_args: &["-rc:v", "vbr", "-b:v", "0", "-maxrate", "200M", "-cq:v", "{quality}"],
        bitrate_args: &[
            "-rc:v",
            "vbr",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_H264_FILTER),
        output_args: &[],
        quality_args: &["-b:v", "0", "-global_quality", "{quality}"],
        bitrate_args: &[
            "-mbbrc",
            "1",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(SOFTWARE_HEVC_FILTER),
        output_args: &["-tag:v", "hvc1"],
        quality_args: &["-b:v", "0", "-global_quality", "{quality}"],
        bitrate_args: &[
            "-mbbrc",
            "1",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvFullH264,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &[],
        quality_args: &["-b:v", "0", "-global_quality", "{quality}"],
        bitrate_args: &[
            "-mbbrc",
            "1",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::QsvFullHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: Some(QSV_FULL_FILTER),
        output_args: &["-tag:v", "hvc1"],
        quality_args: &["-b:v", "0", "-global_quality", "{quality}"],
        bitrate_args: &[
            "-mbbrc",
            "1",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::MacH264,
        cpu_cores_per_frame_worker: 0,
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(VIDEOTOOLBOX_FILTER),
        output_args: &["-prio_speed", "1"],
        quality_args: &["-global_quality", "{videotoolbox_h264_quality}"],
        bitrate_args: &[
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::MacHevc,
        cpu_cores_per_frame_worker: 0,
        input_args: &["-hwaccel", "videotoolbox"],
        filter_complex: Some(VIDEOTOOLBOX_FILTER),
        // Speed priority roughly doubles 4K media-engine HEVC throughput.
        output_args: &["-tag:v", "hvc1", "-prio_speed", "1"],
        quality_args: &["-global_quality", "{videotoolbox_hevc_quality}"],
        bitrate_args: &[
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::VaapiH264,
        cpu_cores_per_frame_worker: 4,
        input_args: VAAPI_INPUT_ARGS,
        filter_complex: Some(VAAPI_FILTER),
        output_args: &[],
        quality_args: &["-rc_mode", "CQP", "-qp", "{quality}"],
        bitrate_args: &[
            "-rc_mode",
            "VBR",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::VaapiHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: VAAPI_INPUT_ARGS,
        filter_complex: Some(VAAPI_FILTER),
        output_args: &["-tag:v", "hvc1", "-compression_level", "7", "-async_depth", "4"],
        quality_args: &["-rc_mode", "CQP", "-qp", "{quality}"],
        bitrate_args: &[
            "-rc_mode",
            "VBR",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::AmfH264,
        cpu_cores_per_frame_worker: 4,
        input_args: AMF_D3D11_INPUT_ARGS,
        filter_complex: Some(AMF_D3D11_FILTER),
        output_args: &[],
        quality_args: &[
            "-rc",
            "cqp",
            "-qp_i",
            "{quality}",
            "-qp_p",
            "{quality}",
            "-qp_b",
            "{quality}",
        ],
        bitrate_args: &[
            "-rc",
            "vbr_peak",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
    CompositeProfile {
        codec_id: CompositeCodecId::AmfHevc,
        cpu_cores_per_frame_worker: 4,
        input_args: AMF_D3D11_INPUT_ARGS,
        filter_complex: Some(AMF_D3D11_FILTER),
        output_args: &["-tag:v", "hvc1"],
        quality_args: &["-rc", "cqp", "-qp_i", "{quality}", "-qp_p", "{quality}"],
        bitrate_args: &[
            "-rc",
            "vbr_peak",
            "-b:v",
            "{bitrate}",
            "-maxrate",
            "{maxrate}",
            "-bufsize",
            "{bufsize}",
        ],
    },
];

/// Returns the command templates owned by a validated composite codec ID.
pub fn composite_profile(codec_id: CompositeCodecId) -> &'static CompositeProfile {
    COMPOSITE_PROFILES
        .iter()
        .find(|profile| profile.codec_id == codec_id)
        .expect("composite profile catalog is exhaustive")
}

/// Selects the BT.709 RGB-to-YUV matrix around the Vulkan conversion.
pub const PRORES_KS_VULKAN_FILTER: &str = "setparams=colorspace=bt709,hwupload,scale_vulkan=format=yuva444p10le:out_range=tv,setparams=colorspace=bt709";

/// One fully expanded transparent profile ready for builder assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransparentProfile {
    pub codec_id: TransparentCodecId,
    /// Logical CPU cores reserved for each frame-render worker. Zero forces one worker.
    pub cpu_cores_per_frame_worker: usize,
    pub input_args: &'static [&'static str],
    pub filter_complex: Option<&'static str>,
    pub output_args: &'static [&'static str],
}

const TRANSPARENT_PROFILES: &[TransparentProfile] = &[
    TransparentProfile {
        codec_id: TransparentCodecId::ProresKs,
        cpu_cores_per_frame_worker: 0,
        input_args: &[],
        filter_complex: None,
        output_args: &[
            "-threads",
            "0",
            "-profile:v",
            "4444",
            "-qscale:v",
            "10",
            "-f",
            "mov",
            "-pix_fmt",
            "yuva444p10le",
        ],
    },
    TransparentProfile {
        codec_id: TransparentCodecId::ProresKsVulkan,
        cpu_cores_per_frame_worker: 4,
        input_args: &["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"],
        filter_complex: Some(PRORES_KS_VULKAN_FILTER),
        output_args: &[
            "-profile:v",
            "4",
            "-q:v",
            "10",
            "-mbs_per_slice",
            "8",
            "-vendor",
            "apl0",
            "-alpha_bits",
            "8",
            "-f",
            "mov",
            "-pix_fmt",
            "vulkan",
        ],
    },
    TransparentProfile {
        codec_id: TransparentCodecId::ProresVideotoolbox,
        // The media engine encodes; FFmpeg's CPU work is the atlas xstack and pixel conversion.
        cpu_cores_per_frame_worker: 4,
        input_args: &[],
        filter_complex: None,
        // The encoder does not accept yuva444p10le; FFmpeg would substitute
        // ayuv64le and convert every frame on the CPU. BGRA leaves only a
        // channel swizzle and lets VideoToolbox convert to 4444 itself.
        output_args: &["-profile:v", "4", "-f", "mov", "-pix_fmt", "bgra"],
    },
    TransparentProfile {
        codec_id: TransparentCodecId::Qtrle,
        cpu_cores_per_frame_worker: 6,
        input_args: &[],
        filter_complex: None,
        output_args: &["-f", "mov", "-pix_fmt", "argb"],
    },
];

/// Resolves and expands one transparent encoder profile.
///
/// Returns the canonical static profile entry for a validated codec ID.
pub fn transparent_profile(codec_id: TransparentCodecId) -> &'static TransparentProfile {
    TRANSPARENT_PROFILES
        .iter()
        .find(|profile| profile.codec_id == codec_id)
        .expect("transparent profile catalog is exhaustive")
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
        for profile in COMPOSITE_PROFILES {
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
    fn nvidia_profiles_enable_temporal_aq() {
        for codec_id in [
            CompositeCodecId::NvgpuH264,
            CompositeCodecId::NvgpuHevc,
            CompositeCodecId::NnvgpuH264,
            CompositeCodecId::NnvgpuHevc,
        ] {
            assert_output_arg_pair(codec_id, "-temporal-aq", "true");
        }
    }

    #[test]
    fn videotoolbox_hevc_prioritizes_speed() {
        assert_output_arg_pair(CompositeCodecId::MacHevc, "-prio_speed", "1");
    }

    #[test]
    fn no_composite_profile_requests_cbr() {
        for profile in COMPOSITE_PROFILES {
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

    #[test]
    fn profiles_define_frame_worker_cpu_costs() {
        let costs = TRANSPARENT_PROFILES
            .iter()
            .map(|profile| {
                (
                    profile.codec_id.metadata().profile_name,
                    profile.cpu_cores_per_frame_worker,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            costs,
            vec![
                ("prores_ks", 0),
                ("prores_ks_vulkan", 4),
                ("prores_videotoolbox", 4),
                ("qtrle", 6),
            ]
        );
    }
}
