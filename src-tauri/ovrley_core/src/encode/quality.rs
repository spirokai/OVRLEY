//! Composite rate-control validation and FFmpeg template expansion.

use super::ffmpeg::composite_profiles::CompositeProfile;
use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

const BITRATE_MAXRATE_MULTIPLIER: f64 = 1.5;
const BITRATE_BUFSIZE_MULTIPLIER: f64 = 2.0;

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
                "{videotoolbox_quality}" => {
                    // VideoToolbox quality increases on a 1–100 scale. Preserve
                    // the approximate mapping from our decreasing 1–51 scale.
                    ((52.0 - f64::from(value)) * 100.0 / 51.0)
                        .round()
                        .to_string()
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
