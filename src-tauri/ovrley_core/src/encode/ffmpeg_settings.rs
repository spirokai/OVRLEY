//! FFmpeg codec settings resolution.
//!
//! Owns: codec argument derivation from user-facing ffmpeg config values.
//! Does not own: ffmpeg binary discovery, process spawning, pipeline execution.
//!
//! Allowed dependencies: serde_json, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.

use crate::encode::ffmpeg_transparent_profiles::{transparent_profile, TransparentProfile};
use serde_json::Value;

use crate::error::{CoreError, CoreResult};

/// Fully resolved ffmpeg settings for one render.
#[derive(Clone, Debug)]
pub struct FfmpegSettings {
    /// Logical codec requested by the template.
    pub codec: String,
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

/// Builds validated ffmpeg settings from `scene.ffmpeg`.
///
/// Supported codecs are alpha-preserving formats suitable for overlay exports.
/// Profile-specific FFmpeg defaults come from the transparent profile catalog.
/// Unknown keys are ignored except `output_args`, which appends raw extra args
/// for advanced users.
pub fn build_ffmpeg_settings(ffmpeg_config: &Value) -> CoreResult<FfmpegSettings> {
    let object = ffmpeg_config.as_object();
    let codec = object
        .and_then(|map| map.get("codec"))
        .and_then(Value::as_str)
        .unwrap_or("prores_ks")
        .to_string();
    let loglevel = object
        .and_then(|map| map.get("loglevel"))
        .and_then(Value::as_str)
        .unwrap_or("info")
        .to_string();
    let profile = transparent_profile(&codec).ok_or_else(|| {
        CoreError::Encode(format!(
            "Unsupported scene.ffmpeg.codec '{codec}'. Supported codecs are prores_ks, prores_ks_vulkan, prores_videotoolbox, and qtrle."
        ))
    })?;
    let mut output_args = profile
        .output_args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    if let Some(container) = object
        .and_then(|map| map.get("container"))
        .and_then(Value::as_str)
    {
        replace_arg_pair_value(&mut output_args, "-f", container);
    }
    if let Some(pix_fmt) = resolved_output_pix_fmt(object, &profile) {
        replace_arg_pair_value(&mut output_args, "-pix_fmt", &pix_fmt);
    }
    append_extra_output_args(&mut output_args, ffmpeg_config);

    Ok(FfmpegSettings {
        codec: profile.codec.to_string(),
        loglevel,
        input_args: profile
            .input_args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
        filter_complex: profile.filter_complex.map(str::to_string),
        output_args,
        extension: object
            .and_then(|map| map.get("container"))
            .and_then(Value::as_str)
            .unwrap_or("mov")
            .to_string(),
    })
}

/// Resolves a supported output pixel-format override for one transparent profile.
fn resolved_output_pix_fmt(
    object: Option<&serde_json::Map<String, Value>>,
    profile: &TransparentProfile,
) -> Option<String> {
    (!matches!(profile.name, "prores_ks_vulkan"))
        .then(|| {
            object
                .and_then(|map| map.get("pix_fmt"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .flatten()
}

/// Replaces one flag/value pair or appends it if the pair does not exist yet.
fn replace_arg_pair_value(args: &mut Vec<String>, flag: &str, value: &str) {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        if let Some(existing) = args.get_mut(index + 1) {
            *existing = value.to_string();
            return;
        }
    }

    args.push(flag.to_string());
    args.push(value.to_string());
}

// Appends user-provided extra ffmpeg output arguments from `scene.ffmpeg.output_args`.
fn append_extra_output_args(args: &mut Vec<String>, ffmpeg_config: &Value) {
    // `output_args` is an explicit escape hatch, but still limited to primitive
    // values that can be represented safely as separate argv entries.
    let extra_args = ffmpeg_config
        .as_object()
        .and_then(|map| map.get("output_args"))
        .and_then(Value::as_array);
    if let Some(extra_args) = extra_args {
        args.extend(extra_args.iter().filter_map(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(boolean) => Some(if *boolean { "1" } else { "0" }.to_string()),
            _ => None,
        }));
    }
}
