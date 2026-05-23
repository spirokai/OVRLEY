//! Editable FFmpeg command templates for transparent-overlay encoder profiles.
//!
//! Owns: static default-profile data for transparent overlay exports, including
//!       profile lookup and FFmpeg command fragments.
//! Does not own: JSON parsing, container overrides, or final `FfmpegSettings`
//!       assembly. Those remain in [`crate::encode::ffmpeg_settings`].

use super::codec_catalog::transparent_codec;

/// One fully expanded transparent profile ready for builder assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransparentProfile {
    pub name: &'static str,
    pub codec: &'static str,
    pub input_args: &'static [&'static str],
    pub filter_complex: Option<&'static str>,
    pub output_args: &'static [&'static str],
}

const BUILTIN_PROFILES: &[TransparentProfile] = &[
    TransparentProfile {
        name: "prores_ks",
        codec: "prores_ks",
        input_args: &[],
        filter_complex: None,
        output_args: &[
            "-c:v",
            "prores_ks",
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
        name: "prores_ks_vulkan",
        codec: "prores_ks_vulkan",
        input_args: &["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"],
        filter_complex: Some("format=yuva444p10le,hwupload"),
        output_args: &[
            "-c:v",
            "prores_ks_vulkan",
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
        name: "prores_videotoolbox",
        codec: "prores_videotoolbox",
        input_args: &[],
        filter_complex: None,
        output_args: &[
            "-c:v",
            "prores_videotoolbox",
            "-profile:v",
            "4",
            "-f",
            "mov",
            "-pix_fmt",
            "yuva444p10le",
        ],
    },
    TransparentProfile {
        name: "qtrle",
        codec: "qtrle",
        input_args: &[],
        filter_complex: None,
        output_args: &["-c:v", "qtrle", "-f", "mov", "-pix_fmt", "argb"],
    },
];

/// Resolves and expands one transparent encoder profile.
///
/// The lookup accepts any alias owned by the canonical codec catalog, then
/// returns the canonical static profile entry for the settings builder.
pub fn transparent_profile(name_or_codec: &str) -> Option<&'static TransparentProfile> {
    let normalized = transparent_codec(name_or_codec)
        .map(|metadata| metadata.codec_name)
        .unwrap_or(name_or_codec);

    BUILTIN_PROFILES
        .iter()
        .find(|profile| profile.name == normalized || profile.codec == normalized)
}
