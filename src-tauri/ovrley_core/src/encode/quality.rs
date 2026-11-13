//! Composite rate-control validation and FFmpeg template expansion.

use super::ffmpeg::composite_profiles::CompositeProfile;
use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

const BITRATE_MAXRATE_MULTIPLIER: f64 = 1.5;
const BITRATE_BUFSIZE_MULTIPLIER: f64 = 2.0;

/// CRF at the best-quality end of the frontend quality slider
/// (`QUALITY_SLIDER_RANGE.min`); it maps to VideoToolbox's maximum quality.
const VIDEOTOOLBOX_MAX_QUALITY_CRF: f64 = 10.0;
const VIDEOTOOLBOX_MAX_QUALITY: f64 = 100.0;
/// VideoToolbox quality lost per CRF step. Calibrated on 4K camera footage so
/// the default CRFs land on comparable output: H.264 CRF 21 maps to quality 72
/// (VMAF-matched to libx264 CRF 21) and HEVC CRF 26 maps to quality 70 (about
/// the bitrate NVENC produces at CQ 26). The export dialog displays this scale
/// from `VIDEOTOOLBOX_QUALITY_SCALE` in `app/.../data/qualityDefaults.js`;
/// keep both in sync.
const VIDEOTOOLBOX_H264_QUALITY_PER_CRF: f64 = 28.0 / 11.0;
const VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF: f64 = 30.0 / 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

pub fn validate_quality(quality_type: QualityType, value: f64) -> CoreResult<EncodingQuality> {
    match quality_type {
        QualityType::Quality => {
            if !value.is_finite() || value.fract() != 0.0 || !(1.0..=51.0).contains(&value) {
                return Err(CoreError::Config(
                    "qualityValue must be an integer between 1 and 51 for quality mode".into(),
                ));
            }
            Ok(EncodingQuality::Quality(value as u8))
        }
        QualityType::Bitrate => {
            if !value.is_finite() || value <= 0.0 {
                return Err(CoreError::Config(
                    "qualityValue must be a positive finite Mbps value for bitrate mode".into(),
                ));
            }
            Ok(EncodingQuality::Bitrate(value))
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
    fn videotoolbox_slider_best_end_reaches_maximum_quality() {
        assert_eq!(videotoolbox_quality(10, VIDEOTOOLBOX_H264_QUALITY_PER_CRF), 100);
        assert_eq!(videotoolbox_quality(10, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF), 100);
    }

    #[test]
    fn videotoolbox_default_crfs_land_on_calibrated_quality() {
        assert_eq!(videotoolbox_quality(21, VIDEOTOOLBOX_H264_QUALITY_PER_CRF), 72);
        assert_eq!(videotoolbox_quality(26, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF), 70);
    }

    #[test]
    fn videotoolbox_quality_saturates_at_scale_limits() {
        assert_eq!(videotoolbox_quality(1, VIDEOTOOLBOX_H264_QUALITY_PER_CRF), 100);
        assert_eq!(videotoolbox_quality(51, VIDEOTOOLBOX_H264_QUALITY_PER_CRF), 1);
        assert_eq!(videotoolbox_quality(51, VIDEOTOOLBOX_HEVC_QUALITY_PER_CRF), 23);
    }
}
