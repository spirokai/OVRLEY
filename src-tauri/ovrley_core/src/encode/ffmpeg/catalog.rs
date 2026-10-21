//! Canonical codec and profile catalog for encoder-facing FFmpeg choices.
//!
//! Owns: typed codec/profile identifiers, external names, overlay/filter-stack
//!       classification, and availability-rule metadata shared by detection and
//!       FFmpeg-setting builders.
//! Does not own: subprocess probing, frontend wire serialization, or final
//!       FFmpeg argument assembly.
//!
//! The catalog is intentionally data-shaped. Each entry describes one canonical
//! transparent codec or composite profile used throughout normalization,
//! capability detection, and FFmpeg command construction.

/// Canonical identifiers for unique FFmpeg encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EncoderId {
    ProresKs,
    ProresKsVulkan,
    ProresVideotoolbox,
    Qtrle,
    Libx264,
    Libx265,
    H264Nvenc,
    HevcNvenc,
    H264Qsv,
    HevcQsv,
    H264Videotoolbox,
    HevcVideotoolbox,
    H264Vaapi,
    HevcVaapi,
    H264Amf,
    HevcAmf,
}

impl EncoderId {
    /// Returns the catalog metadata for this encoder.
    pub fn metadata(self) -> &'static EncoderMetadata {
        ENCODERS
            .iter()
            .find(|metadata| metadata.id == self)
            .expect("encoder catalog is exhaustive")
    }
}

/// Declares how detection must exercise an encoder at the FFmpeg boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    NullSource,
    TransparentProfile(TransparentCodecId),
    VaapiDevice,
}

/// Static metadata for one unique FFmpeg encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncoderMetadata {
    pub id: EncoderId,
    pub ffmpeg_name: &'static str,
    pub probe_kind: ProbeKind,
}

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
        TRANSPARENT_CODECS
            .iter()
            .find(|metadata| metadata.id == self)
            .expect("transparent codec catalog is exhaustive")
    }

    /// Resolves the canonical external codec name at config ingress.
    pub fn from_external_name(name: &str) -> Option<Self> {
        TRANSPARENT_CODECS
            .iter()
            .find(|metadata| metadata.external_name == name)
            .map(|metadata| metadata.id)
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
    pub profile_name: &'static str,
    pub encoder_id: EncoderId,
    pub external_name: &'static str,
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

/// Canonical codec selection produced once from the external scene config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecSelection {
    Transparent(TransparentCodecId),
    Composite(CompositeCodecId),
}

impl CodecSelection {
    /// Translates one canonical external codec name into the internal typed model.
    pub fn from_external_name(name: &str) -> Option<Self> {
        TransparentCodecId::from_external_name(name)
            .map(Self::Transparent)
            .or_else(|| CompositeCodecId::from_external_name(name).map(Self::Composite))
    }
}

impl CompositeCodecId {
    /// Returns the catalog metadata for this composite profile.
    pub fn metadata(self) -> &'static CompositeCodecMetadata {
        COMPOSITE_CODECS
            .iter()
            .find(|metadata| metadata.id == self)
            .expect("composite codec catalog is exhaustive")
    }

    /// Resolves the canonical external profile name at config ingress.
    pub fn from_external_name(name: &str) -> Option<Self> {
        COMPOSITE_CODECS
            .iter()
            .find(|metadata| metadata.external_name == name)
            .map(|metadata| metadata.id)
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
    pub encoder_id: EncoderId,
    pub external_name: &'static str,
    pub filter_stack_kind: CompositeFilterStackKind,
    pub fallback_profile: Option<CompositeCodecId>,
    pub availability_rule: CompositeAvailabilityRule,
}

pub const ENCODERS: &[EncoderMetadata] = &[
    EncoderMetadata {
        id: EncoderId::ProresKs,
        ffmpeg_name: "prores_ks",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::ProresKsVulkan,
        ffmpeg_name: "prores_ks_vulkan",
        probe_kind: ProbeKind::TransparentProfile(TransparentCodecId::ProresKsVulkan),
    },
    EncoderMetadata {
        id: EncoderId::ProresVideotoolbox,
        ffmpeg_name: "prores_videotoolbox",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::Qtrle,
        ffmpeg_name: "qtrle",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::Libx264,
        ffmpeg_name: "libx264",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::Libx265,
        ffmpeg_name: "libx265",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::H264Nvenc,
        ffmpeg_name: "h264_nvenc",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::HevcNvenc,
        ffmpeg_name: "hevc_nvenc",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::H264Qsv,
        ffmpeg_name: "h264_qsv",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::HevcQsv,
        ffmpeg_name: "hevc_qsv",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::H264Videotoolbox,
        ffmpeg_name: "h264_videotoolbox",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::HevcVideotoolbox,
        ffmpeg_name: "hevc_videotoolbox",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::H264Vaapi,
        ffmpeg_name: "h264_vaapi",
        probe_kind: ProbeKind::VaapiDevice,
    },
    EncoderMetadata {
        id: EncoderId::HevcVaapi,
        ffmpeg_name: "hevc_vaapi",
        probe_kind: ProbeKind::VaapiDevice,
    },
    EncoderMetadata {
        id: EncoderId::H264Amf,
        ffmpeg_name: "h264_amf",
        probe_kind: ProbeKind::NullSource,
    },
    EncoderMetadata {
        id: EncoderId::HevcAmf,
        ffmpeg_name: "hevc_amf",
        probe_kind: ProbeKind::NullSource,
    },
];

pub const TRANSPARENT_CODECS: &[TransparentCodecMetadata] = &[
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresKs,
        profile_name: "prores_ks",
        encoder_id: EncoderId::ProresKs,
        external_name: "prores_ks",
        availability_rule: TransparentAvailabilityRule::ProresKs,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresKsVulkan,
        profile_name: "prores_ks_vulkan",
        encoder_id: EncoderId::ProresKsVulkan,
        external_name: "prores_ks_vulkan",
        availability_rule: TransparentAvailabilityRule::ProresKsVulkan,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::ProresVideotoolbox,
        profile_name: "prores_videotoolbox",
        encoder_id: EncoderId::ProresVideotoolbox,
        external_name: "prores_videotoolbox",
        availability_rule: TransparentAvailabilityRule::ProresVideotoolbox,
    },
    TransparentCodecMetadata {
        id: TransparentCodecId::Qtrle,
        profile_name: "qtrle",
        encoder_id: EncoderId::Qtrle,
        external_name: "qtrle",
        availability_rule: TransparentAvailabilityRule::Qtrle,
    },
];

pub const COMPOSITE_CODECS: &[CompositeCodecMetadata] = &[
    CompositeCodecMetadata {
        id: CompositeCodecId::SoftwareH264,
        profile_name: "software_h264",
        encoder_id: EncoderId::Libx264,
        external_name: "libx264",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::Always,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::SoftwareHevc,
        profile_name: "software_hevc",
        encoder_id: EncoderId::Libx265,
        external_name: "libx265",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::Always,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NvgpuH264,
        profile_name: "nvgpu_h264",
        encoder_id: EncoderId::H264Nvenc,
        external_name: "h264_nvenc",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::H264Nvenc,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NvgpuHevc,
        profile_name: "nvgpu_hevc",
        encoder_id: EncoderId::HevcNvenc,
        external_name: "hevc_nvenc",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::HevcNvenc,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NnvgpuH264,
        profile_name: "nnvgpu_h264",
        encoder_id: EncoderId::H264Nvenc,
        external_name: "nnvgpu_h264",
        filter_stack_kind: CompositeFilterStackKind::CudaOverlay,
        fallback_profile: Some(CompositeCodecId::NvgpuH264),
        availability_rule: CompositeAvailabilityRule::H264NvencWithCudaFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::NnvgpuHevc,
        profile_name: "nnvgpu_hevc",
        encoder_id: EncoderId::HevcNvenc,
        external_name: "nnvgpu_hevc",
        filter_stack_kind: CompositeFilterStackKind::CudaOverlay,
        fallback_profile: Some(CompositeCodecId::NvgpuHevc),
        availability_rule: CompositeAvailabilityRule::HevcNvencWithCudaFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvH264,
        profile_name: "qsv_h264",
        encoder_id: EncoderId::H264Qsv,
        external_name: "h264_qsv",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::H264Qsv,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvHevc,
        profile_name: "qsv_hevc",
        encoder_id: EncoderId::HevcQsv,
        external_name: "hevc_qsv",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::HevcQsv,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvFullH264,
        profile_name: "qsv_full_h264",
        encoder_id: EncoderId::H264Qsv,
        external_name: "qsv_full_h264",
        filter_stack_kind: CompositeFilterStackKind::QsvFullOverlay,
        fallback_profile: Some(CompositeCodecId::QsvH264),
        availability_rule: CompositeAvailabilityRule::H264QsvWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::QsvFullHevc,
        profile_name: "qsv_full_hevc",
        encoder_id: EncoderId::HevcQsv,
        external_name: "qsv_full_hevc",
        filter_stack_kind: CompositeFilterStackKind::QsvFullOverlay,
        fallback_profile: Some(CompositeCodecId::QsvHevc),
        availability_rule: CompositeAvailabilityRule::HevcQsvWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::MacH264,
        profile_name: "mac_h264",
        encoder_id: EncoderId::H264Videotoolbox,
        external_name: "h264_videotoolbox",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::H264Videotoolbox,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::MacHevc,
        profile_name: "mac_hevc",
        encoder_id: EncoderId::HevcVideotoolbox,
        external_name: "hevc_videotoolbox",
        filter_stack_kind: CompositeFilterStackKind::SoftwareOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::HevcVideotoolbox,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::VaapiH264,
        profile_name: "vaapi_h264",
        encoder_id: EncoderId::H264Vaapi,
        external_name: "h264_vaapi",
        filter_stack_kind: CompositeFilterStackKind::VaapiOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::H264VaapiWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::VaapiHevc,
        profile_name: "vaapi_hevc",
        encoder_id: EncoderId::HevcVaapi,
        external_name: "hevc_vaapi",
        filter_stack_kind: CompositeFilterStackKind::VaapiOverlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::HevcVaapiWithFullFilters,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::AmfH264,
        profile_name: "amf_h264",
        encoder_id: EncoderId::H264Amf,
        external_name: "h264_amf",
        filter_stack_kind: CompositeFilterStackKind::AmfD3d11Overlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::H264Amf,
    },
    CompositeCodecMetadata {
        id: CompositeCodecId::AmfHevc,
        profile_name: "amf_hevc",
        encoder_id: EncoderId::HevcAmf,
        external_name: "hevc_amf",
        filter_stack_kind: CompositeFilterStackKind::AmfD3d11Overlay,
        fallback_profile: None,
        availability_rule: CompositeAvailabilityRule::HevcAmf,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn encoder_catalog_has_unique_ids_and_ffmpeg_names() {
        let ids = ENCODERS
            .iter()
            .map(|encoder| encoder.id)
            .collect::<BTreeSet<_>>();
        let names = ENCODERS
            .iter()
            .map(|encoder| encoder.ffmpeg_name)
            .collect::<BTreeSet<_>>();

        assert_eq!(ids.len(), ENCODERS.len());
        assert_eq!(names.len(), ENCODERS.len());
    }

    #[test]
    fn every_profile_references_a_catalog_encoder() {
        for encoder_id in TRANSPARENT_CODECS
            .iter()
            .map(|profile| profile.encoder_id)
            .chain(COMPOSITE_CODECS.iter().map(|profile| profile.encoder_id))
        {
            assert!(ENCODERS.iter().any(|encoder| encoder.id == encoder_id));
        }
    }
}
