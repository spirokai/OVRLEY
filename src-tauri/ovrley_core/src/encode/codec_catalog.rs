//! Canonical codec and profile catalog for encoder-facing FFmpeg choices.
//!
//! Owns: typed codec/profile identifiers, alias tables, overlay/filter-stack
//!       classification, and availability-rule metadata shared by detection and
//!       FFmpeg-setting builders.
//! Does not own: subprocess probing, frontend wire serialization, or final
//!       FFmpeg argument assembly.
//!
//! The catalog is intentionally data-shaped. Each entry describes one canonical
//! transparent codec or composite profile, while callers keep their existing
//! public data models and only consult this module for normalization and
//! capability rules.

/// Canonical transparent-overlay codec identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransparentCodecId {
    ProresKs,
    ProresKsVulkan,
    ProresVideotoolbox,
    Qtrle,
}

impl TransparentCodecId {
    /// Returns the catalog metadata for this transparent codec.
    pub fn metadata(self) -> &'static TransparentCodecMetadata {
        transparent_codecs()
            .iter()
            .find(|metadata| metadata.id == self)
            .expect("transparent codec catalog is exhaustive")
    }

    /// Resolves a transparent codec from any accepted alias.
    pub fn from_alias(alias: &str) -> Option<Self> {
        transparent_codec(alias).map(|metadata| metadata.id)
    }
}

/// Availability rules for transparent codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransparentAvailabilityRule {
    ProresKs,
    ProresKsVulkan,
    ProresVideotoolbox,
    Qtrle,
}

/// Static metadata for one transparent codec entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransparentCodecMetadata {
    pub id: TransparentCodecId,
    pub codec_name: &'static str,
    pub accepted_aliases: &'static [&'static str],
    pub availability_rule: TransparentAvailabilityRule,
}

/// Canonical composite encoder profile identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeCodecId {
    SoftwareH264,
    SoftwareHevc,
    NvgpuH264,
    NvgpuHevc,
    NnvgpuH264,
    NnvgpuHevc,
    QsvH264,
    QsvHevc,
    QsvFullH264,
    QsvFullHevc,
    MacH264,
    MacHevc,
    VaapiH264,
    VaapiHevc,
    AmfH264,
    AmfHevc,
}

impl CompositeCodecId {
    /// Returns the catalog metadata for this composite profile.
    pub fn metadata(self) -> &'static CompositeCodecMetadata {
        composite_codecs()
            .iter()
            .find(|metadata| metadata.id == self)
            .expect("composite codec catalog is exhaustive")
    }

    /// Resolves a composite profile from any accepted alias.
    pub fn from_alias(alias: &str) -> Option<Self> {
        composite_codec(alias).map(|metadata| metadata.id)
    }
}

/// Filter-stack families used by composite encoder profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeFilterStackKind {
    SoftwareOverlay,
    VaapiOverlay,
    AmfD3d11Overlay,
    CudaOverlay,
    QsvFullOverlay,
}

/// Availability rules for composite encoder profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeAvailabilityRule {
    Always,
    H264Nvenc,
    HevcNvenc,
    H264Qsv,
    HevcQsv,
    H264Amf,
    HevcAmf,
    H264Videotoolbox,
    HevcVideotoolbox,
    H264VaapiWithFullFilters,
    HevcVaapiWithFullFilters,
    H264NvencWithCudaFilters,
    HevcNvencWithCudaFilters,
    H264QsvWithFullFilters,
    HevcQsvWithFullFilters,
}

/// Static metadata for one composite profile entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompositeCodecMetadata {
    pub id: CompositeCodecId,
    pub profile_name: &'static str,
    pub ffmpeg_codec_name: &'static str,
    pub accepted_aliases: &'static [&'static str],
    pub filter_stack_kind: CompositeFilterStackKind,
    pub availability_rule: CompositeAvailabilityRule,
}

const SOFTWARE_H264_ALIASES: &[&str] = &["software_h264", "auto", "auto_h264", "libx264"];
const SOFTWARE_HEVC_ALIASES: &[&str] = &["software_hevc", "auto_hevc", "auto_h265", "libx265"];
const NVGPU_H264_ALIASES: &[&str] = &["nvgpu_h264", "h264_nvenc"];
const NVGPU_HEVC_ALIASES: &[&str] = &["nvgpu_hevc", "hevc_nvenc"];
const NNVGPU_H264_ALIASES: &[&str] = &["nnvgpu_h264"];
const NNVGPU_HEVC_ALIASES: &[&str] = &["nnvgpu_hevc"];
const QSV_H264_ALIASES: &[&str] = &["qsv_h264", "h264_qsv"];
const QSV_HEVC_ALIASES: &[&str] = &["qsv_hevc", "hevc_qsv"];
const QSV_FULL_H264_ALIASES: &[&str] = &["qsv_full_h264"];
const QSV_FULL_HEVC_ALIASES: &[&str] = &["qsv_full_hevc"];
const MAC_H264_ALIASES: &[&str] = &["mac_h264", "h264_videotoolbox"];
const MAC_HEVC_ALIASES: &[&str] = &["mac_hevc", "hevc_videotoolbox"];
const VAAPI_H264_ALIASES: &[&str] = &["vaapi_h264", "h264_vaapi"];
const VAAPI_HEVC_ALIASES: &[&str] = &["vaapi_hevc", "hevc_vaapi"];
const AMF_H264_ALIASES: &[&str] = &["amf_h264", "h264_amf"];
const AMF_HEVC_ALIASES: &[&str] = &["amf_hevc", "hevc_amf"];

const TRANSPARENT_CODECS: &[TransparentCodecMetadata] = &[
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresKs,
        codec_name: "prores_ks",
        accepted_aliases: &["prores_ks"],
        availability_rule: TransparentAvailabilityRule::ProresKs,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresKsVulkan,
        codec_name: "prores_ks_vulkan",
        accepted_aliases: &["prores_ks_vulkan"],
        availability_rule: TransparentAvailabilityRule::ProresKsVulkan,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresVideotoolbox,
        codec_name: "prores_videotoolbox",
        accepted_aliases: &["prores_videotoolbox"],
        availability_rule: TransparentAvailabilityRule::ProresVideotoolbox,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::Qtrle,
        codec_name: "qtrle",
        accepted_aliases: &["qtrle"],
        availability_rule: TransparentAvailabilityRule::Qtrle,
    },
];

const COMPOSITE_CODECS: &[CompositeCodecMetadata] = &[
    CompositeCodecMetadata {
        id: CompositeCodecId::SoftwareH264,
        profile_name: "software_h264",
        ffmpeg_codec_name: "libx264",
        accepted_aliases: SOFTWARE_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::Always,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::SoftwareHevc,
        profile_name: "software_hevc",
        ffmpeg_codec_name: "libx265",
        accepted_aliases: SOFTWARE_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::Always,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NvgpuH264,
        profile_name: "nvgpu_h264",
        ffmpeg_codec_name: "h264_nvenc",
        accepted_aliases: NVGPU_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::H264Nvenc,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NvgpuHevc,
        profile_name: "nvgpu_hevc",
        ffmpeg_codec_name: "hevc_nvenc",
        accepted_aliases: NVGPU_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::HevcNvenc,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NnvgpuH264,
        profile_name: "nnvgpu_h264",
        ffmpeg_codec_name: "h264_nvenc",
        accepted_aliases: NNVGPU_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::CudaOverlay,
        availability_rule: CompositeAvailabilityRule::H264NvencWithCudaFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NnvgpuHevc,
        profile_name: "nnvgpu_hevc",
        ffmpeg_codec_name: "hevc_nvenc",
        accepted_aliases: NNVGPU_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::CudaOverlay,
        availability_rule: CompositeAvailabilityRule::HevcNvencWithCudaFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvH264,
        profile_name: "qsv_h264",
        ffmpeg_codec_name: "h264_qsv",
        accepted_aliases: QSV_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::H264Qsv,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvHevc,
        profile_name: "qsv_hevc",
        ffmpeg_codec_name: "hevc_qsv",
        accepted_aliases: QSV_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::HevcQsv,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvFullH264,
        profile_name: "qsv_full_h264",
        ffmpeg_codec_name: "h264_qsv",
        accepted_aliases: QSV_FULL_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::QsvFullOverlay,
        availability_rule: CompositeAvailabilityRule::H264QsvWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvFullHevc,
        profile_name: "qsv_full_hevc",
        ffmpeg_codec_name: "hevc_qsv",
        accepted_aliases: QSV_FULL_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::QsvFullOverlay,
        availability_rule: CompositeAvailabilityRule::HevcQsvWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::MacH264,
        profile_name: "mac_h264",
        ffmpeg_codec_name: "h264_videotoolbox",
        accepted_aliases: MAC_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::H264Videotoolbox,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::MacHevc,
        profile_name: "mac_hevc",
        ffmpeg_codec_name: "hevc_videotoolbox",
        accepted_aliases: MAC_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        availability_rule: CompositeAvailabilityRule::HevcVideotoolbox,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::VaapiH264,
        profile_name: "vaapi_h264",
        ffmpeg_codec_name: "h264_vaapi",
        accepted_aliases: VAAPI_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::VaapiOverlay,
        availability_rule: CompositeAvailabilityRule::H264VaapiWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::VaapiHevc,
        profile_name: "vaapi_hevc",
        ffmpeg_codec_name: "hevc_vaapi",
        accepted_aliases: VAAPI_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::VaapiOverlay,
        availability_rule: CompositeAvailabilityRule::HevcVaapiWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::AmfH264,
        profile_name: "amf_h264",
        ffmpeg_codec_name: "h264_amf",
        accepted_aliases: AMF_H264_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::AmfD3d11Overlay,
        availability_rule: CompositeAvailabilityRule::H264Amf,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::AmfHevc,
        profile_name: "amf_hevc",
        ffmpeg_codec_name: "hevc_amf",
        accepted_aliases: AMF_HEVC_ALIASES,
        filter_stack_kind: CompositeFilterStackKind::AmfD3d11Overlay,
        availability_rule: CompositeAvailabilityRule::HevcAmf,
    },
];

/// Returns the full transparent codec catalog.
pub fn transparent_codecs() -> &'static [TransparentCodecMetadata] {
    TRANSPARENT_CODECS
}

/// Resolves a transparent codec entry from any accepted alias.
pub fn transparent_codec(alias: &str) -> Option<&'static TransparentCodecMetadata> {
    transparent_codecs()
        .iter()
        .find(|metadata| alias_matches(metadata.accepted_aliases, alias))
}

/// Returns the full composite profile catalog.
pub fn composite_codecs() -> &'static [CompositeCodecMetadata] {
    COMPOSITE_CODECS
}

/// Resolves a composite profile entry from any accepted alias.
pub fn composite_codec(alias: &str) -> Option<&'static CompositeCodecMetadata> {
    composite_codecs()
        .iter()
        .find(|metadata| alias_matches(metadata.accepted_aliases, alias))
}

/// Checks whether a normalized alias is present in a catalog entry.
fn alias_matches(accepted_aliases: &[&str], candidate: &str) -> bool {
    accepted_aliases.iter().any(|alias| *alias == candidate)
}
