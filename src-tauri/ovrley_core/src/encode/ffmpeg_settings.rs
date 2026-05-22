//! FFmpeg codec settings resolution.
//!
//! Owns: codec argument derivation from user-facing ffmpeg config values.
//! Does not own: ffmpeg binary discovery, process spawning, pipeline execution.
//!
//! Allowed dependencies: serde_json, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.

use serde_json::Value;

use crate::error::{CoreError, CoreResult};

/// Fully resolved ffmpeg settings for one render.
#[derive(Clone, Debug)]
pub struct FfmpegSettings {
    /// Logical codec requested by the template.
    pub codec: String,
    /// ffmpeg loglevel passed to `-loglevel`.
    pub loglevel: String,
    /// Output pixel format passed after codec args.
    pub pix_fmt: String,
    /// Codec-specific output args appended before output path.
    pub output_args: Vec<String>,
    /// Public output file extension.
    pub extension: String,
    /// Optional explicit muxer/container passed with `-f`.
    pub muxer: Option<String>,
    /// Hardware-device setup args required before input declaration.
    pub hw_init_args: Vec<String>,
    /// Optional video filter graph, such as upload to Vulkan.
    pub filters: Option<String>,
}

/// Builds validated ffmpeg settings from `scene.ffmpeg`.
///
/// Supported codecs are alpha-preserving formats suitable for overlay exports.
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
    let container_override = object
        .and_then(|map| map.get("container"))
        .and_then(Value::as_str)
        .map(str::to_string);

    match codec.as_str() {
        "prores_ks" => {
            let pix_fmt = object
                .and_then(|map| map.get("pix_fmt"))
                .and_then(Value::as_str)
                .unwrap_or("yuva444p10le")
                .to_string();
            let mut output_args = vec!["-c:v".to_string(), "prores_ks".to_string()];
            append_ffmpeg_option(
                &mut output_args,
                "-threads",
                object.and_then(|map| map.get("threads")),
            );
            if !output_args.iter().any(|value| value == "-threads") {
                output_args.push("-threads".to_string());
                output_args.push("0".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-profile:v",
                object.and_then(|map| map.get("prores_profile")),
            );
            if !output_args.iter().any(|value| value == "-profile:v") {
                output_args.push("-profile:v".to_string());
                output_args.push("4444".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-qscale:v",
                object.and_then(|map| map.get("qscale")),
            );
            if !output_args.iter().any(|value| value == "-qscale:v") {
                output_args.push("-qscale:v".to_string());
                output_args.push("4".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-bits_per_mb",
                object.and_then(|map| map.get("bits_per_mb")),
            );
            append_ffmpeg_option(
                &mut output_args,
                "-qscale:v",
                object.and_then(|map| map.get("qscale")),
            );
            if !output_args.iter().any(|value| value == "-qscale:v") {
                output_args.push("-qscale:v".to_string());
                output_args.push("4".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-mbs_per_slice",
                object.and_then(|map| map.get("mbs_per_slice")),
            );
            append_ffmpeg_option(
                &mut output_args,
                "-vendor",
                object.and_then(|map| map.get("vendor")),
            );
            append_ffmpeg_option(
                &mut output_args,
                "-alpha_bits",
                object.and_then(|map| map.get("alpha_bits")),
            );
            append_extra_output_args(&mut output_args, ffmpeg_config);
            Ok(FfmpegSettings {
                codec,
                loglevel,
                pix_fmt,
                output_args,
                extension: container_override
                    .clone()
                    .unwrap_or_else(|| "mov".to_string()),
                muxer: container_override,
                hw_init_args: Vec::new(),
                filters: None,
            })
        }
        "prores_ks_vulkan" => {
            let pix_fmt = "vulkan".to_string();
            let mut output_args = vec!["-c:v".to_string(), "prores_ks_vulkan".to_string()];
            append_ffmpeg_option(
                &mut output_args,
                "-profile:v",
                object.and_then(|map| map.get("prores_profile")),
            );
            if !output_args.iter().any(|value| value == "-profile:v") {
                output_args.push("-profile:v".to_string());
                output_args.push("4".to_string()); // Default to 4444 for alpha parity
            }
            append_ffmpeg_option(
                &mut output_args,
                "-bits_per_mb",
                object.and_then(|map| map.get("bits_per_mb")),
            );
            append_ffmpeg_option(
                &mut output_args,
                "-mbs_per_slice",
                object.and_then(|map| map.get("mbs_per_slice")),
            );
            if !output_args.iter().any(|value| value == "-mbs_per_slice") {
                output_args.push("-mbs_per_slice".to_string());
                output_args.push("4".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-vendor",
                object.and_then(|map| map.get("vendor")),
            );
            if !output_args.iter().any(|value| value == "-vendor") {
                output_args.push("-vendor".to_string());
                output_args.push("apl0".to_string());
            }
            append_ffmpeg_option(
                &mut output_args,
                "-alpha_bits",
                object.and_then(|map| map.get("alpha_bits")),
            );
            if !output_args.iter().any(|value| value == "-alpha_bits") {
                output_args.push("-alpha_bits".to_string());
                output_args.push("16".to_string());
            }
            append_extra_output_args(&mut output_args, ffmpeg_config);

            Ok(FfmpegSettings {
                codec,
                loglevel,
                pix_fmt,
                output_args,
                extension: container_override
                    .clone()
                    .unwrap_or_else(|| "mov".to_string()),
                muxer: container_override,
                hw_init_args: vec![
                    "-init_hw_device".to_string(),
                    "vulkan=vk".to_string(),
                    "-filter_hw_device".to_string(),
                    "vk".to_string(),
                ],
                filters: Some("format=yuva444p10le,hwupload".to_string()),
            })
        }
        "prores_videotoolbox" => {
            let pix_fmt = object
                .and_then(|map| map.get("pix_fmt"))
                .and_then(Value::as_str)
                .unwrap_or("yuva444p10le")
                .to_string();
            let mut output_args = vec!["-c:v".to_string(), "prores_videotoolbox".to_string()];
            append_ffmpeg_option(
                &mut output_args,
                "-profile:v",
                object.and_then(|map| map.get("prores_profile")),
            );
            if !output_args.iter().any(|value| value == "-profile:v") {
                output_args.push("-profile:v".to_string());
                output_args.push("4".to_string()); // Default to 4444 for parity with other ProRes paths
            }
            append_extra_output_args(&mut output_args, ffmpeg_config);

            Ok(FfmpegSettings {
                codec,
                loglevel,
                pix_fmt,
                output_args,
                extension: container_override
                    .clone()
                    .unwrap_or_else(|| "mov".to_string()),
                muxer: container_override,
                hw_init_args: Vec::new(),
                filters: None,
            })
        }
        "qtrle" => {
            let pix_fmt = object
                .and_then(|map| map.get("pix_fmt"))
                .and_then(Value::as_str)
                .unwrap_or("argb")
                .to_string();
            let mut output_args = vec!["-c:v".to_string(), "qtrle".to_string()];
            append_extra_output_args(&mut output_args, ffmpeg_config);

            Ok(FfmpegSettings {
                codec,
                loglevel,
                pix_fmt,
                output_args,
                extension: container_override
                    .clone()
                    .unwrap_or_else(|| "mov".to_string()),
                muxer: container_override,
                hw_init_args: Vec::new(),
                filters: None,
            })
        }
        other => Err(CoreError::Encode(format!(
            "Unsupported scene.ffmpeg.codec '{other}'. Supported codecs are prores_ks, prores_ks_vulkan, prores_videotoolbox, and qtrle."
        ))),
    }
}

// Appends one scalar ffmpeg option from template JSON when present.
fn append_ffmpeg_option(args: &mut Vec<String>, flag: &str, value: Option<&Value>) {
    // Accept primitive JSON values only. Complex values are ignored so malformed
    // template extras cannot produce surprising command-line fragments.
    let Some(value) = value else {
        return;
    };
    match value {
        Value::Null => {}
        Value::String(text) if text.is_empty() => {}
        Value::String(text) => {
            args.push(flag.to_string());
            args.push(text.clone());
        }
        Value::Number(number) => {
            args.push(flag.to_string());
            args.push(number.to_string());
        }
        Value::Bool(boolean) => {
            args.push(flag.to_string());
            args.push(if *boolean { "1" } else { "0" }.to_string());
        }
        _ => {}
    }
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
