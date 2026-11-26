//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw overlay pixels to ffmpeg (RGBA, or `yuva420p` for
//! composite graphs that blend in YUV; see [`overlay`]), and records
//! timing/debug output.
//! Callers submit work through [`crate::jobs::execution::RenderExecutionService`].
//! Job planning fixes timing and settings; [`pipeline`] prepares assets and
//! feeds ordered frames into one supervised FFmpeg process.
//!
//! ## Thread Map
//!
//! | Thread Type | Spawned By | Owns | Shutdown Signal | Joined By |
//! |-------------|------------|------|-----------------|-----------|
//! | Writer | Process owner | ffmpeg stdin | Queue EOF / pipeline shutdown | Process owner |
//! | Frame render worker | `render_frames_parallel` | Skia surface + RGBA buffer | Work queue exhaustion / shared stop flag | Frame coordinator |
//! | Monitor | Process owner | ffmpeg stderr, bounded diagnostic history | ffmpeg exits → stderr EOF | Process owner |
//! | Operation | Render execution service | Reservation and accepted inputs | Completion / cancel / error | Execution service |

pub mod ffmpeg;
pub mod overlay;
pub mod pipeline;
pub mod progress;
pub mod settings;
pub mod timing;
