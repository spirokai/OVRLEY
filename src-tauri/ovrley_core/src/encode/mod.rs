//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw RGBA pixels to ffmpeg, and records timing/debug output.
//! Callers submit work through [`crate::render_jobs::execution::RenderExecutionService`].
//! Job planning fixes timing and settings; [`pipeline`] prepares assets and
//! feeds ordered RGBA frames into one supervised FFmpeg process.
//!
//! ## Thread Map
//!
//! | Thread Type | Spawned By | Owns | Shutdown Signal | Joined By |
//! |-------------|------------|------|-----------------|-----------|
//! | Writer | Process owner | ffmpeg stdin | Queue EOF / pipeline shutdown | Process owner |
//! | Frame render worker | `render_frames_parallel` | Skia surface + RGBA buffer | Work queue exhaustion / shared stop flag | Frame coordinator |
//! | Monitor | Process owner | ffmpeg stderr, bounded diagnostic history | ffmpeg exits → stderr EOF | Process owner |
//! | Operation | Render execution service | Reservation and accepted inputs | Completion / cancel / error | Execution service |

/// Encoding diagnostics and timing summaries.
pub mod debug;
/// FFmpeg discovery, capability detection, profiles, and argument builders.
pub mod ffmpeg;
/// Rational frame-rate helpers shared by composite encoding modules.
pub mod fps;
/// Encoding runtime pipelines and their shared frame/process infrastructure.
pub mod pipeline;
/// Canonical composite-render data contract.
pub mod plan;
/// Live render progress estimation helpers.
pub mod progress;
/// Composite quality/bitrate validation and FFmpeg arguments.
pub mod quality;
/// Video-local activity coverage shared by both export modes.
pub mod video_timing;
