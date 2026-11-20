//! Composite encoder profile tests.
//!
//! Verifies every canonical composite codec ID owns one command template.
//!
//! ## Type
//! Unit test. Pure data-driven lookup — no ffmpeg or video fixtures.
//!
//! ## Regressions guarded
//! - Profile table entries referencing the wrong typed encoder
//! - New profiles not added to the canonical list
//! - Non-canonical external names bypassing ingress validation

use ovrley_core::encode::ffmpeg::catalog::{CodecSelection, CompositeCodecId, COMPOSITE_CODECS};
use ovrley_core::encode::ffmpeg::composite_profiles::composite_profile;
use serde_json::json;

use crate::common;

#[test]
fn resolves_known_profile_by_id() {
    let profile = composite_profile(CompositeCodecId::SoftwareH264);
    assert_eq!(profile.codec_id, CompositeCodecId::SoftwareH264);
    assert_eq!(
        profile
            .codec_id
            .metadata()
            .encoder_id
            .metadata()
            .ffmpeg_name,
        "libx264"
    );
}

#[test]
fn canonical_external_name_is_typed_at_ingress() {
    let config = common::seam::validate_scene_ffmpeg(json!({ "codec": "h264_nvenc" })).unwrap();
    assert_eq!(
        config.codec,
        CodecSelection::Composite(CompositeCodecId::NvgpuH264)
    );
}

#[test]
fn compatibility_aliases_are_rejected_at_ingress() {
    for name in ["auto", "auto_h264", "software_h264", "nvgpu_h264"] {
        assert!(common::seam::validate_scene_ffmpeg(json!({ "codec": name })).is_err());
    }
}

#[test]
fn unknown_external_profile_returns_error() {
    let result = common::seam::validate_scene_ffmpeg(json!({ "codec": "nonexistent_codec" }));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported"));
}

#[test]
fn all_builtin_profiles_resolve() {
    for metadata in COMPOSITE_CODECS {
        let profile = composite_profile(metadata.id);
        assert_eq!(profile.codec_id, metadata.id);
        assert_eq!(profile.codec_id.metadata().encoder_id, metadata.encoder_id);
    }
}
