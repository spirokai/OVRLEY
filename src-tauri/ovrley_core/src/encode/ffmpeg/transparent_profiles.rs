//! Editable FFmpeg command templates for transparent-overlay encoder profiles.
//!
//! Owns: static default-profile data for transparent overlay exports, including
//!       profile lookup and FFmpeg command fragments.
//! Does not own: JSON parsing, container overrides, or final `FfmpegSettings`
//!       assembly. Those remain in [`crate::encode::ffmpeg_settings`].

use super::catalog::TransparentCodecId;

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

const BUILTIN_PROFILES: &[TransparentProfile] = &[
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
            "5",
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
        output_args: &["-profile:v", "4", "-f", "mov", "-pix_fmt", "yuva444p10le"],
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
    BUILTIN_PROFILES
        .iter()
        .find(|profile| profile.codec_id == codec_id)
        .expect("transparent profile catalog is exhaustive")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_define_frame_worker_cpu_costs() {
        let costs = BUILTIN_PROFILES
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
