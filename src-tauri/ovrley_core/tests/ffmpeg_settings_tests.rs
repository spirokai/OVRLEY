use ovrley_core::encode::ffmpeg_settings::build_ffmpeg_settings;
use ovrley_core::error::CoreResult;
use serde_json::json;

#[test]
fn prores_ks_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks");
    assert_eq!(settings.pix_fmt, "yuva444p10le");
    assert_eq!(settings.extension, "mov");
    Ok(())
}

#[test]
fn prores_ks_vulkan_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks_vulkan",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks_vulkan");
    assert_eq!(settings.pix_fmt, "vulkan");
    assert_eq!(settings.extension, "mov");
    assert_eq!(settings.hw_init_args.len(), 4);
    assert!(settings.filters.is_some());
    Ok(())
}

#[test]
fn prores_videotoolbox_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_videotoolbox",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_videotoolbox");
    assert_eq!(settings.pix_fmt, "yuva444p10le");
    assert_eq!(settings.extension, "mov");
    Ok(())
}

#[test]
fn qtrle_settings() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "qtrle",
        "loglevel": "error"
    }))?;
    assert_eq!(settings.codec, "qtrle");
    assert_eq!(settings.pix_fmt, "argb");
    assert_eq!(settings.extension, "mov");
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
    Ok(())
}
