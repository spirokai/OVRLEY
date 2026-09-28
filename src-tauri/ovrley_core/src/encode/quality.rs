//! Composite rate control validation and FFmpeg argument construction.

use super::ffmpeg::catalog::CompositeCodecId;
use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

// Controls bitrate spakes in vbr modes
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

pub(crate) fn rate_control_args(codec: CompositeCodecId, quality: EncodingQuality) -> Vec<String> {
    use CompositeCodecId::*;
    let mut args = Vec::new();
    match quality {
        EncodingQuality::Bitrate(mbps) => {
            let mode: &[&str] = match codec {
                NvgpuH264 | NvgpuHevc | NnvgpuH264 | NnvgpuHevc => &["-rc:v", "vbr"],
                QsvH264 | QsvHevc | QsvFullH264 | QsvFullHevc => &["-mbbrc", "1"],
                AmfH264 | AmfHevc => &["-rc", "vbr_peak"],
                VaapiH264 | VaapiHevc => &["-rc_mode", "VBR"],
                SoftwareH264 | SoftwareHevc | MacH264 | MacHevc => &[],
            };
            args.extend(mode.iter().map(|arg| (*arg).to_string()));
            args.extend([
                "-b:v".to_string(),
                format!("{mbps}M"),
                "-maxrate".to_string(),
                format!("{}M", mbps * BITRATE_MAXRATE_MULTIPLIER),
                "-bufsize".to_string(),
                format!("{}M", mbps * BITRATE_BUFSIZE_MULTIPLIER),
            ]);
        }
        EncodingQuality::Quality(value) => {
            let value_arg = value.to_string();
            match codec {
                SoftwareH264 | SoftwareHevc => args.extend(["-crf".to_string(), value_arg]),
                NvgpuH264 | NvgpuHevc | NnvgpuH264 | NnvgpuHevc => {
                    args.extend(["-rc:v", "vbr", "-b:v", "0", "-cq:v"].map(String::from));
                    args.push(value_arg);
                }
                QsvH264 | QsvHevc | QsvFullH264 | QsvFullHevc => {
                    args.extend(["-b:v", "0", "-global_quality"].map(String::from));
                    args.push(value_arg);
                }
                AmfH264 | AmfHevc => {
                    args.extend(["-rc", "cqp", "-qp_i"].map(String::from));
                    args.extend([value_arg.clone(), "-qp_p".to_string(), value_arg.clone()]);
                    if codec == AmfH264 {
                        args.extend(["-qp_b".to_string(), value_arg]);
                    }
                }
                VaapiH264 | VaapiHevc => {
                    args.extend(["-rc_mode", "CQP", "-qp"].map(String::from));
                    args.push(value_arg);
                }
                MacH264 | MacHevc => {
                    // VideoToolbox uses 1–100, increasing with quality. This is
                    // an approximate mapping of our decreasing 1–51 scale.
                    let apple_quality = ((52.0 - f64::from(value)) * 100.0 / 51.0).round();
                    args.extend(["-global_quality".to_string(), apple_quality.to_string()]);
                }
            }
        }
    }
    args
}
