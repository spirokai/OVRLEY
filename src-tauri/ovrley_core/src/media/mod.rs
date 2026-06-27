//! Source media probing and embedded telemetry extraction.
//!
//! This module reads imported source media. It does not own rendering, output
//! encoding, or FFmpeg encode pipelines.

/// MP4 embedded telemetry and telemetry-first metadata extraction.
pub mod mp4_telemetry;
/// Shared source video metadata contract.
pub mod source_video_metadata;
/// MP4/MOV metadata extraction via ffprobe.
pub mod video_probe;

pub use source_video_metadata::{Resolution, SourceVideoMetadata};
