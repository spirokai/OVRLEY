//! FFmpeg codec settings resolution.
//!
//! Owns: codec argument derivation from user-facing ffmpeg config values.
//! Does not own: ffmpeg binary discovery, process spawning, pipeline execution.
//!
//! Allowed dependencies: serde_json, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.

use crate::encode::ffmpeg::catalog::{CodecSelection, TransparentCodecId};
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedFfmpegConfig;
use crate::render::FrameSize;
use std::path::Path;

/// Fully resolved ffmpeg settings for one render.
#[derive(Clone, Debug)]
pub struct FfmpegSettings {
    pub codec_id: TransparentCodecId,
    /// ffmpeg loglevel passed to `-loglevel`.
    pub loglevel: String,
    /// Input-side hardware-device setup args required before rawvideo input.
    pub input_args: Vec<String>,
    /// Optional filter chain applied between rawvideo input and encode output.
    pub filter_complex: Option<String>,
    /// Codec-specific output args appended before output path.
    pub output_args: Vec<String>,
    /// Public output file extension.
    pub extension: String,
}

impl FfmpegSettings {
    /// Produces the final argv at the FFmpeg process boundary.
    pub(crate) fn command_args(
        &self,
        output_path: &Path,
        frame_size: FrameSize,
        fps: &str,
        input_pix_fmt: &str,
    ) -> Vec<String> {
        let mut args = vec!["-loglevel".to_string(), self.loglevel.clone()];
        args.extend(self.input_args.iter().cloned());
        args.extend([
            "-f".to_string(),
            "rawvideo".to_string(),
            "-s".to_string(),
            format!("{}x{}", frame_size.width, frame_size.height),
            "-pix_fmt".to_string(),
            input_pix_fmt.to_string(),
            "-r".to_string(),
            fps.to_string(),
            "-i".to_string(),
            "-".to_string(),
        ]);
        if let Some(filter) = &self.filter_complex {
            args.extend(["-vf".to_string(), filter.clone()]);
        }
        args.extend(self.output_args.iter().cloned());
        args.extend(["-y".to_string(), output_path.to_string_lossy().into_owned()]);
        args
    }
}

/// Builds validated ffmpeg settings from `scene.ffmpeg`.
///
/// Supported codecs are alpha-preserving formats suitable for overlay exports.
/// Profile-specific FFmpeg defaults come from the transparent profile catalog.
/// The ingress validator ignores non-profile extension keys; `output_args`
/// remains the explicit escape hatch for advanced users.
pub fn build_ffmpeg_settings(ffmpeg_config: &ValidatedFfmpegConfig) -> CoreResult<FfmpegSettings> {
    let codec_id = match ffmpeg_config.codec {
        CodecSelection::Transparent(codec_id) => codec_id,
        CodecSelection::Composite(codec_id) => {
            return Err(CoreError::Encode(format!(
                "Composite codec '{}' cannot be used for a transparent render",
                codec_id.metadata().profile_name
            )))
        }
    };
    let profile = transparent_profile(codec_id);
    let encoder_name = codec_id.metadata().encoder_id.metadata().ffmpeg_name;
    let mut output_args = vec!["-c:v".to_string(), encoder_name.to_string()];
    output_args.extend(
        profile
            .output_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>(),
    );
    if let Some(container) = &ffmpeg_config.container {
        replace_arg_pair_value(&mut output_args, "-f", container);
    }
    if codec_id != TransparentCodecId::ProresKsVulkan {
        if let Some(pix_fmt) = ffmpeg_config.pix_fmt.as_deref() {
            replace_arg_pair_value(&mut output_args, "-pix_fmt", pix_fmt);
        }
    }
    output_args.extend(ffmpeg_config.output_args.iter().cloned());
    if codec_id == TransparentCodecId::ProresKsVulkan
        && !output_args.iter().any(|value| value == "-async_depth")
    {
        output_args.extend(["-async_depth".to_string(), "4".to_string()]);
    }

    Ok(FfmpegSettings {
        codec_id,
        loglevel: ffmpeg_config.loglevel.clone(),
        input_args: profile
            .input_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
        filter_complex: profile.filter_complex.map(str::to_string),
        output_args,
        extension: ffmpeg_config
            .container
            .clone()
            .unwrap_or_else(|| "mov".to_string()),
    })
}

/// Replaces one flag/value pair or appends it if the pair does not exist yet.
fn replace_arg_pair_value(args: &mut Vec<String>, flag: &str, value: &str) {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args[index + 1] = value.to_string();
        return;
    }

    args.push(flag.to_string());
    args.push(value.to_string());
}
