//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw RGBA pixels to ffmpeg, and records timing/debug output.
//! The public surface is intentionally small: callers start renders through the
//! controller in [`video`], while the pipeline module contains the single-pass
//! frame producer/ffmpeg consumer implementation.

/// ffmpeg codec and hardware-acceleration detection.
pub mod codec_detect;
/// ffmpeg discovery and codec argument construction.
pub mod ffmpeg;
/// FFmpeg argument construction for MP4 compositing mode.
pub mod ffmpeg_composite;
/// Rational frame-rate helpers shared by composite encoding modules.
pub mod fps;
/// Render controller and public video render orchestration.
pub mod video;
/// Composite MP4 render pipeline used by the composite render entry point.
pub(crate) mod video_composite_pipeline;
/// Debug summaries, sample-frame exports, and segment stitching helpers.
mod video_debug;
/// Single-render video pipeline used by normal and segmented renders.
pub(crate) mod video_pipeline;
/// Video metadata extraction via ffprobe.
pub mod video_probe;
