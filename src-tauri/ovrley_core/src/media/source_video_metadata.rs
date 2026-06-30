//! Shared metadata contract for imported source videos.
//!
//! Both ffprobe and telemetry-parser populate this shape so the command layer
//! can merge source metadata without converting between probe-specific types.

use serde::{Deserialize, Serialize};

/// Imported source video metadata returned to the frontend after import.
///
/// All fields except `path` and `has_audio` may be `None` if the active probe
/// could not report them. `sync_time` is the canonical sync hint for aligning
/// source video with activity telemetry. `creation_time` is retained as a
/// transition field for existing frontend code and as provenance for ffprobe
/// fallback timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceVideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>,
    pub sync_time: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub bit_rate: Option<String>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
    pub camera_type: Option<String>,
    pub camera_model: Option<String>,
}

/// Width and height in pixels as reported by source video probes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}
