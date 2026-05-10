//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw RGBA pixels to ffmpeg, and records timing/debug output.
//! The public surface is intentionally small: callers start renders through the
//! controller in [`video`], while the pipeline module contains the single-pass
//! frame producer/ffmpeg consumer implementation.

/// ffmpeg discovery and codec argument construction.
pub mod ffmpeg;
/// Render controller and public video render orchestration.
pub mod video;
/// Debug summaries, sample-frame exports, and segment stitching helpers.
mod video_debug;
/// Single-render video pipeline used by normal and segmented renders.
pub(crate) mod video_pipeline;
