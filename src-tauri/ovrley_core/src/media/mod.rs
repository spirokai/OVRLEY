//! Source media probing and embedded telemetry extraction.
//!
//! This module reads imported source media. It does not own rendering, output
//! encoding, or FFmpeg encode pipelines.

/// DJI Action AC004 metadata fallback extraction.
pub mod dji_ac004;
/// MP4 embedded telemetry and telemetry-first metadata extraction.
pub mod mp4_telemetry;
/// Intermediate telemetry sample shape and payload checks.
pub mod native_sample;
/// Shared source video metadata contract.
pub mod source_video_metadata;
/// Shared math utilities for telemetry processing.
pub mod telemetry_math;
/// Shared time conversion and media-relative timestamp helpers.
pub mod time;
/// MP4/MOV metadata extraction via ffprobe.
pub mod video_probe;

pub use source_video_metadata::{Resolution, SourceVideoMetadata};
