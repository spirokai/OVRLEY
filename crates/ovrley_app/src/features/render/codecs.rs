//! The render dialog's codec grid.
//!
//! Core's catalog owns the codecs and their availability (`CodecSelection`, `AvailableCodecs`);
//! this module only adds what the dialog shows: the output formats, the accelerations, the grid
//! between them and a codec name, and the quality and bitrate defaults. The codec stays a string,
//! resolved through core's catalog where a rule needs it.

use ovrley_core::encode::ffmpeg::catalog::{CodecSelection, CompositeCodecId};
use ovrley_core::encode::ffmpeg::detect::AvailableCodecs;
use ovrley_core::encode::settings::{videotoolbox_display_quality, EncodingQuality};
use ovrley_core::encode::settings::{H264_QUALITY_DEFAULT, HEVC_QUALITY_DEFAULT, QUALITY_SLIDER_MAX, QUALITY_SLIDER_MIN};

/// The platform the accelerations are offered on, read once from the build target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformOs {
    Windows,
    Macos,
    Linux,
}

impl PlatformOs {
    /// The platform the app was built for.
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::Macos,
            _ => Self::Linux,
        }
    }
}

/// Which group of the format select a format belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatGroup {
    Transparent,
    Mp4,
}

/// `OUTPUT_FORMATS`, in the select's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Prores,
    Qtrle,
    H264,
    Hevc,
}

/// The accelerations, in the select's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Acceleration {
    NvidiaCuda,
    Nvidia,
    Amd,
    QsvFull,
    Videotoolbox,
    Vaapi,
    Cpu,
    VulkanProres,
}

impl OutputFormat {
    pub const ALL: [Self; 4] = [Self::Prores, Self::Qtrle, Self::H264, Self::Hevc];

    /// The select item's value.
    pub fn value(self) -> &'static str {
        match self {
            Self::Prores => "prores",
            Self::Qtrle => "qtrle",
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }

    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.value() == value)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Prores => "ProRes",
            Self::Qtrle => "QT RLE",
            Self::H264 => "H.264",
            Self::Hevc => "H.265 / HEVC",
        }
    }

    pub fn group(self) -> FormatGroup {
        match self {
            Self::Prores | Self::Qtrle => FormatGroup::Transparent,
            Self::H264 | Self::Hevc => FormatGroup::Mp4,
        }
    }

    /// `format.codecs[acceleration]`: the codec name of a format and acceleration, if the grid
    /// has one.
    pub fn codec(self, acceleration: Acceleration) -> Option<&'static str> {
        use Acceleration as A;
        Some(match (self, acceleration) {
            (Self::Prores, A::Cpu) => "prores_ks",
            (Self::Prores, A::Videotoolbox) => "prores_videotoolbox",
            (Self::Prores, A::VulkanProres) => "prores_ks_vulkan",
            (Self::Qtrle, A::Cpu) => "qtrle",
            (Self::H264, A::Cpu) => "libx264",
            (Self::H264, A::Nvidia) => "h264_nvenc",
            (Self::H264, A::NvidiaCuda) => "nnvgpu_h264",
            (Self::H264, A::QsvFull) => "qsv_full_h264",
            (Self::H264, A::Amd) => "h264_amf",
            (Self::H264, A::Videotoolbox) => "h264_videotoolbox",
            (Self::H264, A::Vaapi) => "h264_vaapi",
            (Self::Hevc, A::Cpu) => "libx265",
            (Self::Hevc, A::Nvidia) => "hevc_nvenc",
            (Self::Hevc, A::NvidiaCuda) => "nnvgpu_hevc",
            (Self::Hevc, A::QsvFull) => "qsv_full_hevc",
            (Self::Hevc, A::Amd) => "hevc_amf",
            (Self::Hevc, A::Videotoolbox) => "hevc_videotoolbox",
            (Self::Hevc, A::Vaapi) => "hevc_vaapi",
            _ => return None,
        })
    }
}

impl Acceleration {
    pub const ALL: [Self; 8] = [
        Self::NvidiaCuda,
        Self::Nvidia,
        Self::Amd,
        Self::QsvFull,
        Self::Videotoolbox,
        Self::Vaapi,
        Self::Cpu,
        Self::VulkanProres,
    ];

    /// The select item's value.
    pub fn value(self) -> &'static str {
        match self {
            Self::NvidiaCuda => "nvidia_cuda",
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::QsvFull => "qsv_full",
            Self::Videotoolbox => "videotoolbox",
            Self::Vaapi => "vaapi",
            Self::Cpu => "cpu",
            Self::VulkanProres => "vulkan_prores",
        }
    }

    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|acceleration| acceleration.value() == value)
    }

    /// The select item's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::NvidiaCuda => "NVIDIA GPU | CUDA",
            Self::Nvidia => "NVIDIA GPU",
            Self::Amd => "AMD GPU",
            Self::QsvFull => "Intel QSV",
            Self::Videotoolbox => "Apple VideoToolbox",
            Self::Vaapi => "VAAPI",
            Self::Cpu => "CPU",
            Self::VulkanProres => "Vulkan",
        }
    }

    /// AMF is Direct3D 11 only, so Windows; Linux reaches
    /// Intel and AMD through VAAPI.
    pub fn visible_on(self, platform: PlatformOs) -> bool {
        use PlatformOs::{Windows, Linux, Macos};
        match self {
            Self::NvidiaCuda | Self::Nvidia => matches!(platform, Windows | Linux),
            Self::Amd | Self::QsvFull => platform == Windows,
            Self::Videotoolbox => platform == Macos,
            Self::Vaapi => platform == Linux,
            Self::Cpu | Self::VulkanProres => true,
        }
    }
}

/// The format and acceleration a codec name sits at, `None` for a name
/// outside the grid (a hand-edited project).
pub fn codec_selection(codec: &str) -> Option<(OutputFormat, Acceleration)> {
    OutputFormat::ALL
        .into_iter()
        .flat_map(|format| {
            Acceleration::ALL
                .into_iter()
                .map(move |acceleration| (format, acceleration))
        })
        .find(|(format, acceleration)| format.codec(*acceleration) == Some(codec))
}

/// A composite codec in core's catalog.
pub fn is_mp4_codec(codec: &str) -> bool {
    matches!(CodecSelection::from_external_name(codec), Some(CodecSelection::Composite(_)))
}

/// The codec needs the detected QSV filter initialization.
pub fn is_qsv_full_codec(codec: &str) -> bool {
    matches!(CompositeCodecId::from_external_name(codec), Some(CompositeCodecId::QsvFullH264 | CompositeCodecId::QsvFullHevc))
}

/// Whether a format's acceleration is available, through core's catalog rules. Nothing is
/// available until codec detection ends.
pub fn is_acceleration_available(
    format: OutputFormat,
    acceleration: Acceleration,
    codecs: Option<&AvailableCodecs>,
) -> bool {
    let (Some(codec), Some(codecs)) = (format.codec(acceleration), codecs) else {
        return false;
    };
    match CodecSelection::from_external_name(codec).expect("the dialog's grid holds catalog names")
    {
        CodecSelection::Transparent(id) => codecs.has_transparent_codec(id),
        CodecSelection::Composite(id) => codecs.has_composite_codec(id),
    }
}

/// One acceleration of a format as the select lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccelerationOption {
    pub acceleration: Acceleration,
    pub available: bool,
}

/// The format's accelerations shown on this platform.
pub fn visible_accelerations(
    format: OutputFormat,
    platform: PlatformOs,
    codecs: Option<&AvailableCodecs>,
) -> Vec<AccelerationOption> {
    Acceleration::ALL
        .into_iter()
        .filter(|acceleration| {
            format.codec(*acceleration).is_some() && acceleration.visible_on(platform)
        })
        .map(|acceleration| AccelerationOption {
            acceleration,
            available: is_acceleration_available(format, acceleration, codecs),
        })
        .collect()
}

pub fn first_available_acceleration(
    format: OutputFormat,
    platform: PlatformOs,
    codecs: Option<&AvailableCodecs>,
) -> Option<Acceleration> {
    visible_accelerations(format, platform, codecs)
        .into_iter()
        .find(|option| option.available)
        .map(|option| option.acceleration)
}

/// Transparent formats always are; an MP4 format needs one usable
/// acceleration.
pub fn is_format_available(
    format: OutputFormat,
    platform: PlatformOs,
    codecs: Option<&AvailableCodecs>,
) -> bool {
    format.group() == FormatGroup::Transparent
        || first_available_acceleration(format, platform, codecs).is_some()
}

pub fn first_available_mp4_codec(
    platform: PlatformOs,
    codecs: Option<&AvailableCodecs>,
) -> Option<&'static str> {
    OutputFormat::ALL
        .into_iter()
        .filter(|format| format.group() == FormatGroup::Mp4)
        .find_map(|format| {
            first_available_acceleration(format, platform, codecs)
                .and_then(|acceleration| format.codec(acceleration))
        })
}

/// The default CRF of an MP4 format.
pub fn default_quality(format: OutputFormat) -> u8 {
    if format == OutputFormat::Hevc {
        HEVC_QUALITY_DEFAULT
    } else {
        H264_QUALITY_DEFAULT
    }
}

/// The slider runs from worse to better, CRF the other way.
pub fn invert_quality_value(value: f64) -> f64 {
    f64::from(QUALITY_SLIDER_MIN) + f64::from(QUALITY_SLIDER_MAX) - value
}

/// The value on the encoder's own scale: VideoToolbox's 1–100 quality
/// from core's scale, CRF for the others, or the bitrate.
pub fn quality_value_label(codec: &str, quality: EncodingQuality) -> String {
    match quality {
        EncodingQuality::Bitrate(mbps) => format!("{mbps} Mbps"),
        EncodingQuality::Quality(crf) => match CompositeCodecId::from_external_name(codec)
            .and_then(|id| videotoolbox_display_quality(id, crf))
        {
            Some(quality) => format!("Quality {quality}"),
            None => format!("CRF {crf}"),
        },
    }
}

/// One row of `BITRATE_BINS`: the most pixels it covers and its rates in Mbps (`hfr` above 30 fps).
struct BitrateBin {
    max_pixels: u64,
    h264: u32,
    h265: u32,
    h264_hfr: u32,
    h265_hfr: u32,
}

/// `BITRATE_BINS` (1080p, 1440p, 4K), then `BITRATE_FALLBACK`.
const BITRATE_BINS: [BitrateBin; 3] = [
    BitrateBin {
        max_pixels: 2_073_600,
        h264: 20,
        h265: 20,
        h264_hfr: 35,
        h265_hfr: 35,
    },
    BitrateBin {
        max_pixels: 3_686_400,
        h264: 40,
        h265: 40,
        h264_hfr: 70,
        h265_hfr: 70,
    },
    BitrateBin {
        max_pixels: 8_294_400,
        h264: 60,
        h265: 60,
        h264_hfr: 100,
        h265_hfr: 100,
    },
];
const BITRATE_FALLBACK: BitrateBin = BitrateBin {
    max_pixels: u64::MAX,
    h264: 80,
    h265: 80,
    h264_hfr: 100,
    h265_hfr: 100,
};

/// The recommended Mbps for the output size, rate and MP4 codec; HEVC
/// encoders take the HEVC column.
pub fn default_bitrate(width: u32, height: u32, fps: f64, codec: CompositeCodecId) -> u32 {
    let pixels = u64::from(width) * u64::from(height);
    let hevc = codec.metadata().encoder_id.is_hevc();
    let hfr = fps > 30.0;
    let bin = BITRATE_BINS.iter().find(|bin| pixels <= bin.max_pixels).unwrap_or(&BITRATE_FALLBACK);
    match (hevc, hfr) {
        (true, true) => bin.h265_hfr,
        (true, false) => bin.h265,
        (false, true) => bin.h264_hfr,
        (false, false) => bin.h264,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(set: impl FnOnce(&mut AvailableCodecs)) -> AvailableCodecs {
        let mut codecs = AvailableCodecs::default();
        set(&mut codecs);
        codecs
    }

    // Zero pixels has no bins.
    #[test]
    fn default_bitrates_follow_the_bins() {
        use CompositeCodecId::{MacHevc, NvgpuHevc, SoftwareH264, SoftwareHevc};
        assert_eq!(default_bitrate(1920, 1080, 30.0, SoftwareH264), 20);
        assert_eq!(default_bitrate(1920, 1080, 60.0, SoftwareH264), 35);
        assert_eq!(default_bitrate(1920, 1080, 30.0, SoftwareHevc), 20);
        assert_eq!(default_bitrate(1920, 1080, 60.0, MacHevc), 35);
        assert_eq!(default_bitrate(3840, 2160, 30.0, SoftwareH264), 60);
        assert_eq!(default_bitrate(3840, 2160, 60.0, SoftwareH264), 100);
        assert_eq!(default_bitrate(2560, 1440, 30.0, SoftwareH264), 40);
        assert_eq!(default_bitrate(7680, 4320, 30.0, SoftwareH264), 80, "beyond every bin");
        assert_eq!(default_bitrate(7680, 4320, 60.0, NvgpuHevc), 100);
        assert_eq!(default_bitrate(0, 0, 30.0, SoftwareH264), 20);
        assert_eq!(default_bitrate(1920, 1080, 30.0, SoftwareH264), 20, "30 fps is not high frame rate");
        assert_eq!(default_bitrate(1920, 1080, 31.0, SoftwareH264), 35);
        assert_eq!(default_bitrate(1920, 1080, 30000.0 / 1001.0, SoftwareH264), 20);
    }

    #[test]
    fn quality_labels_use_the_encoders_scale() {
        assert_eq!(
            quality_value_label("libx264", EncodingQuality::Quality(21)),
            "CRF 21"
        );
        assert_eq!(
            quality_value_label("hevc_nvenc", EncodingQuality::Quality(26)),
            "CRF 26"
        );
        assert_eq!(
            quality_value_label("h264_videotoolbox", EncodingQuality::Quality(10)),
            "Quality 100"
        );
        assert_eq!(
            quality_value_label("hevc_videotoolbox", EncodingQuality::Quality(10)),
            "Quality 100"
        );
        assert_eq!(
            quality_value_label("h264_videotoolbox", EncodingQuality::Quality(21)),
            "Quality 72"
        );
        assert_eq!(
            quality_value_label("hevc_videotoolbox", EncodingQuality::Quality(26)),
            "Quality 70"
        );
        assert_eq!(
            quality_value_label("h264_videotoolbox", EncodingQuality::Quality(18)),
            "Quality 80"
        );
        assert_eq!(
            quality_value_label("hevc_videotoolbox", EncodingQuality::Quality(18)),
            "Quality 85"
        );
        assert_eq!(
            quality_value_label("h264_videotoolbox", EncodingQuality::Quality(1)),
            "Quality 100"
        );
        assert_eq!(
            quality_value_label("h264_videotoolbox", EncodingQuality::Quality(51)),
            "Quality 1"
        );
        assert_eq!(
            quality_value_label("hevc_videotoolbox", EncodingQuality::Bitrate(40.0)),
            "40 Mbps"
        );
        assert_eq!(
            quality_value_label("libx264", EncodingQuality::Bitrate(7.5)),
            "7.5 Mbps"
        );
    }

    /// Every codec the grid offers is a catalog name.
    #[test]
    fn every_grid_codec_is_in_cores_catalog() {
        for format in OutputFormat::ALL {
            for acceleration in Acceleration::ALL {
                if let Some(codec) = format.codec(acceleration) {
                    assert!(
                        CodecSelection::from_external_name(codec).is_some(),
                        "{codec}"
                    );
                    assert_eq!(codec_selection(codec), Some((format, acceleration)));
                }
            }
        }
        assert_eq!(
            codec_selection("h264_qsv"),
            None,
            "QSV regular is not offered"
        );
        assert!(
            is_mp4_codec("hevc_vaapi")
                && is_mp4_codec("nnvgpu_h264")
                && !is_mp4_codec("prores_ks")
                && !is_mp4_codec("x264_custom")
        );
    }

    /// Availability follows the flags detection sets together.
    #[test]
    fn availability_follows_the_detected_flags() {
        let none = detected(|_| {});
        assert!(
            !is_acceleration_available(OutputFormat::Prores, Acceleration::Cpu, None),
            "nothing before detection ends"
        );
        assert!(!is_acceleration_available(
            OutputFormat::Prores,
            Acceleration::Cpu,
            Some(&none)
        ));
        let prores = detected(|c| c.prores_ks = true);
        assert!(is_acceleration_available(
            OutputFormat::Prores,
            Acceleration::Cpu,
            Some(&prores)
        ));
        let vt = detected(|c| {
            c.prores_videotoolbox = true;
            c.h264_videotoolbox = true;
            c.videotoolbox = true;
        });
        assert!(is_acceleration_available(
            OutputFormat::Prores,
            Acceleration::Videotoolbox,
            Some(&vt)
        ));
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::Videotoolbox,
            Some(&vt)
        ));
        assert!(!is_acceleration_available(
            OutputFormat::Hevc,
            Acceleration::Videotoolbox,
            Some(&vt)
        ));
        let nvenc = detected(|c| {
            c.h264_nvenc = true;
            c.nvgpu = true;
        });
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::Nvidia,
            Some(&nvenc)
        ));
        assert!(
            !is_acceleration_available(OutputFormat::H264, Acceleration::NvidiaCuda, Some(&nvenc)),
            "CUDA needs the filter stack"
        );
        let cuda = detected(|c| {
            c.h264_nvenc = true;
            c.nnvgpu = true;
        });
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::NvidiaCuda,
            Some(&cuda)
        ));
        let qsv = detected(|c| c.h264_qsv = true);
        assert!(!is_acceleration_available(
            OutputFormat::H264,
            Acceleration::QsvFull,
            Some(&qsv)
        ));
        let qsv_full = detected(|c| {
            c.h264_qsv = true;
            c.qsv_full = true;
        });
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::QsvFull,
            Some(&qsv_full)
        ));
        let vaapi = detected(|c| c.hevc_vaapi = true);
        assert!(!is_acceleration_available(
            OutputFormat::Hevc,
            Acceleration::Vaapi,
            Some(&vaapi)
        ));
        let vaapi_full = detected(|c| {
            c.hevc_vaapi = true;
            c.vaapi_full = true;
        });
        assert!(is_acceleration_available(
            OutputFormat::Hevc,
            Acceleration::Vaapi,
            Some(&vaapi_full)
        ));
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::Amd,
            Some(&detected(|c| c.h264_amf = true))
        ));
        // Core's one difference: software MP4 is always available once detection ended.
        assert!(is_acceleration_available(
            OutputFormat::H264,
            Acceleration::Cpu,
            Some(&none)
        ));
    }

    #[test]
    fn the_platform_decides_which_accelerations_show() {
        let shown = |format, platform| {
            visible_accelerations(format, platform, None)
                .into_iter()
                .map(|option| option.acceleration)
                .collect::<Vec<_>>()
        };
        use Acceleration::*;
        assert_eq!(
            shown(OutputFormat::H264, PlatformOs::Macos),
            [Videotoolbox, Cpu]
        );
        assert_eq!(
            shown(OutputFormat::H264, PlatformOs::Windows),
            [NvidiaCuda, Nvidia, Amd, QsvFull, Cpu]
        );
        assert_eq!(
            shown(OutputFormat::Hevc, PlatformOs::Linux),
            [NvidiaCuda, Nvidia, Vaapi, Cpu]
        );
        assert_eq!(
            shown(OutputFormat::Prores, PlatformOs::Macos),
            [Videotoolbox, Cpu, VulkanProres]
        );
        assert_eq!(shown(OutputFormat::Qtrle, PlatformOs::Windows), [Cpu]);
    }

    #[test]
    fn formats_and_the_first_mp4_codec() {
        let software = AvailableCodecs::default();
        assert!(
            is_format_available(OutputFormat::Prores, PlatformOs::Macos, None),
            "transparent formats always are"
        );
        assert!(!is_format_available(
            OutputFormat::H264,
            PlatformOs::Macos,
            None
        ));
        assert_eq!(first_available_mp4_codec(PlatformOs::Macos, None), None);
        assert_eq!(
            first_available_mp4_codec(PlatformOs::Macos, Some(&software)),
            Some("libx264")
        );
        let vt = detected(|c| c.h264_videotoolbox = true);
        assert_eq!(
            first_available_mp4_codec(PlatformOs::Macos, Some(&vt)),
            Some("h264_videotoolbox")
        );
        assert_eq!(
            first_available_acceleration(OutputFormat::Hevc, PlatformOs::Macos, Some(&vt)),
            Some(Acceleration::Cpu)
        );
    }

    #[test]
    fn quality_defaults_and_the_inverted_slider() {
        assert_eq!(default_quality(OutputFormat::H264), 21);
        assert_eq!(default_quality(OutputFormat::Hevc), 26);
        assert_eq!(invert_quality_value(10.0), 35.0);
        assert_eq!(invert_quality_value(21.0), 24.0);
        assert!(is_qsv_full_codec("qsv_full_hevc") && !is_qsv_full_codec("h264_qsv"));
    }
}
