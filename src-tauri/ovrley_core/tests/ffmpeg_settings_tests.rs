//! Transparent-overlay FFmpeg settings tests.
//!
//! Verifies `build_ffmpeg_settings` produces correct pixel formats,
//! codec selection, container extensions, output-arg passthrough, and
//! Vulkan hardware initialization for transparent render codec paths
//! (prores_ks, prores_ks_vulkan, prores_videotoolbox, qtrle).
//!
//! ## Type
//! Unit test. No ffmpeg subprocess — exercises the settings builder with
//! controlled JSON input.
//!
//! ## Regressions guarded
//! - ProRes variants using wrong pixel formats (alpha channel loss)
//! - Vulkan path missing hardware init args
//! - Unknown codecs panicking instead of returning errors
//! - Custom container overrides ignored

use ovrley_core::encode::ffmpeg_settings::build_ffmpeg_settings;
use ovrley_core::error::CoreResult;
use serde_json::json;

mod common;

use common::composite::{assert_argument_pair, has_argument_pair};

#[test]
fn prores_ks_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks");
    assert_eq!(settings.extension, "mov");
    assert_argument_pair(&settings.output_args, "-pix_fmt", "yuva444p10le");
    Ok(())
}

#[test]
fn prores_ks_vulkan_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks_vulkan",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks_vulkan");
    assert_eq!(settings.extension, "mov");
    assert_eq!(settings.input_args.len(), 4);
    assert!(settings.filter_complex.is_some());
    assert_argument_pair(&settings.output_args, "-pix_fmt", "vulkan");
    Ok(())
}

#[test]
fn prores_videotoolbox_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_videotoolbox",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_videotoolbox");
    assert_eq!(settings.extension, "mov");
    assert_argument_pair(&settings.output_args, "-pix_fmt", "yuva444p10le");
    Ok(())
}

#[test]
fn qtrle_settings() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "qtrle",
        "loglevel": "error"
    }))?;
    assert_eq!(settings.codec, "qtrle");
    assert_eq!(settings.extension, "mov");
    assert_argument_pair(&settings.output_args, "-pix_fmt", "argb");
    Ok(())
}

#[test]
fn unknown_codec_errors() {
    let result = build_ffmpeg_settings(&json!({
        "codec": "nonexistent_codec",
        "loglevel": "info"
    }));
    assert!(result.is_err());
}

#[test]
fn output_args_passthrough() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info",
        "output_args": ["-color_range", "2"]
    }))?;
    assert!(settings.output_args.contains(&"-color_range".to_string()));
    Ok(())
}

#[test]
fn default_codec_is_prores_ks() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks");
    Ok(())
}

#[test]
fn custom_container_override() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info",
        "container": "mkv"
    }))?;
    assert_eq!(settings.extension, "mkv");
    assert_argument_pair(&settings.output_args, "-f", "mkv");
    Ok(())
}

#[test]
fn prores_ks_defaults_are_applied_once() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info"
    }))?;
    assert_argument_pair(&settings.output_args, "-threads", "0");
    assert_argument_pair(&settings.output_args, "-profile:v", "4444");
    assert_argument_pair(&settings.output_args, "-qscale:v", "5");
    assert_eq!(count_flag(&settings.output_args, "-qscale:v"), 1);
    Ok(())
}

#[test]
fn pix_fmt_override_rewrites_output_arg_when_allowed() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info",
        "pix_fmt": "yuva444p12le"
    }))?;
    assert_argument_pair(&settings.output_args, "-pix_fmt", "yuva444p12le");
    assert!(!has_argument_pair(
        &settings.output_args,
        "-pix_fmt",
        "yuva444p10le"
    ));
    Ok(())
}

#[test]
fn vulkan_pix_fmt_override_is_ignored() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks_vulkan",
        "loglevel": "info",
        "pix_fmt": "yuva444p10le"
    }))?;
    assert_argument_pair(&settings.output_args, "-pix_fmt", "vulkan");
    assert!(!has_argument_pair(
        &settings.output_args,
        "-pix_fmt",
        "yuva444p10le"
    ));
    Ok(())
}

#[test]
fn profile_specific_json_knobs_do_not_override_catalog_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info",
        "threads": 3,
        "prores_profile": "hq",
        "qscale": 7,
        "bits_per_mb": 8000,
        "vendor": "ap10",
        "alpha_bits": 8
    }))?;
    assert_argument_pair(&settings.output_args, "-threads", "0");
    assert_argument_pair(&settings.output_args, "-profile:v", "4444");
    assert_argument_pair(&settings.output_args, "-qscale:v", "5");
    assert_eq!(count_flag(&settings.output_args, "-qscale:v"), 1);
    assert!(!has_argument_pair(&settings.output_args, "-threads", "3"));
    assert!(!has_argument_pair(
        &settings.output_args,
        "-profile:v",
        "hq"
    ));
    assert!(!has_argument_pair(&settings.output_args, "-qscale:v", "7"));
    assert!(!settings.output_args.iter().any(|arg| arg == "-bits_per_mb"));
    assert!(!settings.output_args.iter().any(|arg| arg == "ap10"));
    assert!(!settings.output_args.iter().any(|arg| arg == "8"));
    Ok(())
}

fn count_flag(args: &[String], key: &str) -> usize {
    args.iter().filter(|arg| arg.as_str() == key).count()
}
