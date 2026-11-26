//! Render settings the editor holds and projects save, and composite rate control: validated
//! [`EncodingQuality`] and its FFmpeg arguments.

use serde::{Deserialize, Serialize};
use super::ffmpeg::profiles::CompositeProfile;
use crate::error::{CoreError, CoreResult};

/// The default CRF of new projects and codec changes.
pub const H264_QUALITY_DEFAULT: u8 = 21;
pub const HEVC_QUALITY_DEFAULT: u8 = 26;
/// The CRFs the quality slider offers, best first.
pub const QUALITY_SLIDER_MIN: u8 = 10;
pub const QUALITY_SLIDER_MAX: u8 = 35;

/// One combined file, or one file per video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Combined,
    Separate,
}

/// The overlay over the video, or the overlay alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ExportMode {
    Transparent,
    Composite,
}

/// The export settings the editor holds and projects save.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSettings {
    pub output_mode: OutputMode,
    pub fps: f64,
    pub widget_update_rate: u32,
    pub export_mode: ExportMode,
    pub codec: String,
    pub quality: EncodingQuality,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            output_mode: OutputMode::Combined,
            fps: 30.0,
            widget_update_rate: 1,
            export_mode: ExportMode::Composite,
            codec: "prores_ks".into(),
            quality: EncodingQuality::Quality(H264_QUALITY_DEFAULT),
        }
    }
}

impl RenderSettings {
    /// Checks the numbers and the codec; the enums and the quality were validated when built.
    pub fn validate(&self) -> CoreResult<()> {
        if !(self.fps.is_finite() && self.fps > 0.0) {
            return Err(CoreError::Config("Render FPS must be a positive finite number".into()));
        }
        if self.widget_update_rate == 0 {
            return Err(CoreError::Config("Widget update rate must be a positive integer".into()));
        }
        if self.codec.trim().is_empty() {
            return Err(CoreError::Config("Render codec is required".into()));
        }
        Ok(())
    }

    /// Sets both, or neither when the result is invalid.
    pub fn set_fps_and_update_rate(&mut self, fps: f64, widget_update_rate: u32) -> CoreResult<()> {
        let next = Self { fps, widget_update_rate, ..self.clone() };
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_widget_update_rate(&mut self, widget_update_rate: u32) -> CoreResult<()> {
        self.set_fps_and_update_rate(self.fps, widget_update_rate)
    }
}

const BITRATE_MAXRATE_MULTIPLIER: f64 = 1.5;
const BITRATE_BUFSIZE_MULTIPLIER: f64 = 2.0;

/// CRF at the best-quality end of the quality slider; it maps to VideoToolbox's maximum quality.
const VIDEOTOOLBOX_MAX_QUALITY_CRF: f64 = QUALITY_SLIDER_MIN as f64;
const VIDEOTOOLBOX_MAX_QUALITY: f64 = 100.0;
/// VideoToolbox quality lost per CRF step. Calibrated on 4K camera footage so
/// the default CRFs land on comparable output: H.264 CRF 21 maps to quality 72
/// (VMAF-matched to libx264 CRF 21) and HEVC CRF 26 maps to quality 70 (about
/// the bitrate NVENC produces at CQ 26). The export dialog shows this scale through
/// [`videotoolbox_display_quality`].
const VIDEOTOOLBOX_H264_QUALITY_PER_CRF: f64 = 28.0 / 11.0;
const VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF: f64 = 30.0 / 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum QualityType {
    Quality,
    Bitrate,
}

/// Validated rate control consumed by the encoder without further coercion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(
    tag = "qualityType",
    content = "qualityValue",
    rename_all = "lowercase"
)]
pub enum EncodingQuality {
    Quality(u8),
    Bitrate(f64),
}

impl EncodingQuality {
    /// Validates a rate control: a whole CRF in `1..=51`, or a positive bitrate in Mbps.
    pub fn try_new(quality_type: QualityType, value: f64) -> CoreResult<Self> {
        match quality_type {
            QualityType::Quality => {
                if !value.is_finite() || value.fract() != 0.0 || !(1.0..=51.0).contains(&value) {
                    return Err(CoreError::Config(
                        "qualityValue must be an integer between 1 and 51 for quality mode".into(),
                    ));
                }
                Ok(Self::Quality(value as u8))
            }
            QualityType::Bitrate => {
                if !value.is_finite() || value <= 0.0 {
                    return Err(CoreError::Config(
                        "qualityValue must be a positive finite Mbps value for bitrate mode".into(),
                    ));
                }
                Ok(Self::Bitrate(value))
            }
        }
    }
}

/// Maps a CRF-scale value onto VideoToolbox's increasing 1–100 quality scale.
///
/// The slider's best CRF reaches quality 100; CRFs beyond either end of the
/// VideoToolbox scale saturate at its limits.
fn videotoolbox_quality(crf: u8, quality_per_crf: f64) -> u8 {
    let quality = VIDEOTOOLBOX_MAX_QUALITY
        - (f64::from(crf) - VIDEOTOOLBOX_MAX_QUALITY_CRF) * quality_per_crf;
    quality.round().clamp(1.0, VIDEOTOOLBOX_MAX_QUALITY) as u8
}

/// The 1–100 quality a VideoToolbox composite codec receives for `crf`, or `None` for an encoder
/// that takes the CRF itself. The export dialog labels its quality value with it.
pub fn videotoolbox_display_quality(
    codec: super::ffmpeg::catalog::CompositeCodecId,
    crf: u8,
) -> Option<u8> {
    use super::ffmpeg::catalog::CompositeCodecId;
    match codec {
        CompositeCodecId::MacH264 => {
            Some(videotoolbox_quality(crf, VIDEOTOOLBOX_H264_QUALITY_PER_CRF))
        }
        CompositeCodecId::MacHevc => {
            Some(videotoolbox_quality(crf, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF))
        }
        _ => None,
    }
}

/// Selects the validated setting's template and expands its value placeholders.
pub(crate) fn rate_control_args(
    profile: &CompositeProfile,
    quality: EncodingQuality,
) -> Vec<String> {
    match quality {
        EncodingQuality::Quality(value) => profile
            .quality_args
            .iter()
            .map(|arg| match *arg {
                "{quality}" => value.to_string(),
                "{videotoolbox_h264_quality}" => {
                    videotoolbox_quality(value, VIDEOTOOLBOX_H264_QUALITY_PER_CRF).to_string()
                }
                "{videotoolbox_hevc_quality}" => {
                    videotoolbox_quality(value, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF).to_string()
                }
                arg => arg.to_string(),
            })
            .collect(),
        EncodingQuality::Bitrate(mbps) => profile
            .bitrate_args
            .iter()
            .map(|arg| match *arg {
                "{bitrate}" => format!("{mbps}M"),
                "{maxrate}" => format!("{}M", mbps * BITRATE_MAXRATE_MULTIPLIER),
                "{bufsize}" => format!("{}M", mbps * BITRATE_BUFSIZE_MULTIPLIER),
                arg => arg.to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid_and_bad_settings_fail() {
        assert!(RenderSettings::default().validate().is_ok());
        assert!(RenderSettings { fps: 0.0, ..RenderSettings::default() }.validate().is_err());
        assert!(RenderSettings { widget_update_rate: 0, ..RenderSettings::default() }.validate().is_err());
        assert!(RenderSettings { codec: " ".into(), ..RenderSettings::default() }.validate().is_err());
        assert!(EncodingQuality::try_new(QualityType::Quality, 52.0).is_err());
        assert_eq!(EncodingQuality::try_new(QualityType::Bitrate, 8.5).unwrap(), EncodingQuality::Bitrate(8.5));
    }

    #[test]
    fn setters_validate_before_they_change_anything() {
        let mut settings = RenderSettings::default();
        settings.set_fps_and_update_rate(60.0, 2).unwrap();
        assert_eq!((settings.fps, settings.widget_update_rate), (60.0, 2));
        assert!(settings.set_widget_update_rate(0).is_err());
        assert!(settings.set_fps_and_update_rate(f64::NAN, 1).is_err());
        assert_eq!((settings.fps, settings.widget_update_rate), (60.0, 2));
        settings.set_widget_update_rate(6).unwrap();
        assert_eq!(settings.widget_update_rate, 6);
    }

    #[test]
    fn videotoolbox_slider_best_end_reaches_maximum_quality() {
        assert_eq!(
            videotoolbox_quality(10, VIDEOTOOLBOX_H264_QUALITY_PER_CRF),
            100
        );
        assert_eq!(
            videotoolbox_quality(10, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF),
            100
        );
    }

    #[test]
    fn videotoolbox_default_crfs_land_on_calibrated_quality() {
        assert_eq!(
            videotoolbox_quality(21, VIDEOTOOLBOX_H264_QUALITY_PER_CRF),
            72
        );
        assert_eq!(
            videotoolbox_quality(26, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF),
            70
        );
    }

    #[test]
    fn videotoolbox_quality_saturates_at_scale_limits() {
        assert_eq!(
            videotoolbox_quality(1, VIDEOTOOLBOX_H264_QUALITY_PER_CRF),
            100
        );
        assert_eq!(
            videotoolbox_quality(51, VIDEOTOOLBOX_H264_QUALITY_PER_CRF),
            1
        );
        assert_eq!(
            videotoolbox_quality(51, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF),
            23
        );
    }
}
