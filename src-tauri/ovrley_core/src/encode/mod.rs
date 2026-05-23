//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw RGBA pixels to ffmpeg, and records timing/debug output.
//! The public surface is intentionally small: callers start renders through the
//! controller in [`video`], while the pipeline module contains the single-pass
//! frame producer/ffmpeg consumer implementation.
//!
//! ## Thread Map
//!
//! | Thread Type | Spawned By | Owns | Shutdown Signal | Joined By |
//! |-------------|------------|------|-----------------|-----------|
//! | Writer | `render_video_single` / `render_composite_video_single` | ffmpeg stdin | Channel sender dropped (EOF) | Spawning function |
//! | Monitor (transparent) | `render_video_single` | ffmpeg stderr, `Arc<AtomicU32>` | ffmpeg exits → stderr EOF | Spawning function |
//! | Monitor (composite) | `render_composite_video_single` | ffmpeg stderr, `Arc<Mutex<Vec>>` | ffmpeg exits → stderr EOF | Spawning function |
//! | Segment render worker | `render_video_segmented` / `render_composite_video_segmented` | Per-segment ffmpeg + buffer pool | Child-controller cancel flag | Aggregator loop |
//! | Parallel render worker | `run_parallel_renders` | Independent config + ffmpeg | Work queue exhaustion | `run_parallel_renders` |
//! | Command dispatch | `backend_render` / composite render dispatcher | Full render call | Completion / cancel / error (updates controller) | Fire-and-forget |

/// Canonical codec/profile catalog shared by detection and FFmpeg builders.
pub mod codec_catalog;
/// ffmpeg codec and hardware-acceleration detection.
pub mod codec_detect;
/// ffmpeg discovery and codec argument construction.
pub mod ffmpeg;
/// FFmpeg argument construction for MP4 compositing mode.
pub mod ffmpeg_composite;
/// Editable FFmpeg command templates for composite encoder profiles.
pub mod ffmpeg_composite_profiles;
/// FFmpeg codec settings resolution (separated from binary discovery).
pub mod ffmpeg_settings;
/// Editable FFmpeg command templates for transparent overlay encoder profiles.
pub mod ffmpeg_transparent_profiles;
/// Rational frame-rate helpers shared by composite encoding modules.
pub mod fps;
/// Shared internal queueing and timing helpers for encode pipelines.
pub(crate) mod pipeline_shared;
/// Live render progress estimation helpers.
pub mod progress; // test seam
/// Render controller and public video render orchestration.
pub mod video;
/// Composite-only debug summaries for MP4 compositing diagnostics.
pub mod video_composite_debug; // test seam
/// Composite MP4 render pipeline used by the composite render entry point.
pub mod video_composite_pipeline; // test seam
/// Pure composite helper logic shared by production code and integration tests.
#[doc(hidden)]
pub mod video_composite_support;
/// Debug summaries, sample-frame exports, and segment stitching helpers.
mod video_debug;
/// Internal benchmark and multi-render orchestration helpers.
pub(crate) mod video_parallel;
/// Single-render video pipeline used by normal and segmented renders.
pub(crate) mod video_pipeline;
/// Video metadata extraction via ffprobe.
pub mod video_probe;
/// Internal segmented render orchestration for transparent and composite paths.
pub(crate) mod video_segmented;
/// Internal time-window helpers for segmented video orchestration.
pub(crate) mod video_windows;
