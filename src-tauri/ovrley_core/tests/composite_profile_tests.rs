use ovrley_core::encode::ffmpeg_composite_profiles::composite_profile_template;

#[test]
fn resolves_known_profile_by_name() {
    let profile = composite_profile_template("software_h264").unwrap();
    assert_eq!(profile.codec, "libx264");
}

#[test]
fn resolves_by_codec_alias() {
    let profile = composite_profile_template("h264_nvenc").unwrap();
    assert_eq!(profile.codec, "h264_nvenc");
    assert!(!profile.output_args.is_empty());
}

#[test]
fn unknown_profile_returns_error() {
    let result = composite_profile_template("nonexistent_codec");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unknown composite profile"));
}

#[test]
fn all_builtin_profiles_resolve() {
    let names = [
        "software_h264",
        "software_hevc",
        "nvgpu_h264",
        "nvgpu_hevc",
        "nnvgpu_h264",
        "nnvgpu_hevc",
        "qsv_h264",
        "qsv_hevc",
        "qsv_full_h264",
        "qsv_full_hevc",
        "mac_h264",
        "mac_hevc",
        "vaapi_h264",
        "vaapi_hevc",
        "amf_h264",
        "amf_hevc",
    ];
    for name in names {
        let profile = composite_profile_template(name)
            .unwrap_or_else(|e| panic!("Failed to resolve '{}': {}", name, e));
        assert!(
            !profile.output_args.is_empty(),
            "Profile '{}' has no output args",
            name
        );
    }
}
