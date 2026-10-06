//! Video encoding subsystem.
//!
//! The encoder receives already-densified activity data and rendered Skia
//! frames, streams raw RGBA pixels to ffmpeg, and records timing/debug output.
//! The public surface is intentionally small: callers start renders through the
//! controller through [`pipeline`], while the pipeline modules feed ordered output from
//! profile-sized frame-worker pools into one ffmpeg process.
//!
//! ## Thread Map
//!
//! | Thread Type | Spawned By | Owns | Shutdown Signal | Joined By |
//! |-------------|------------|------|-----------------|-----------|
//! | Writer | Transparent/composite pipeline | ffmpeg stdin | Channel sender dropped (EOF) | Spawning function |
//! | Frame render worker | `render_frames_parallel` | Skia surface + RGBA buffer | Work queue exhaustion / shared stop flag | Frame coordinator |
//! | Monitor (transparent) | Transparent pipeline | ffmpeg stderr, `Arc<AtomicU32>` | ffmpeg exits → stderr EOF | Spawning function |
//! | Monitor (composite) | Composite pipeline | ffmpeg stderr, `Arc<Mutex<Vec>>` | ffmpeg exits → stderr EOF | Spawning function |
//! | Command dispatch | `backend_render` / composite render dispatcher | Full render call | Completion / cancel / error (updates controller) | Fire-and-forget |

/// Canonical composite-render data contract.
pub mod composite;
/// Encoding diagnostics and timing summaries.
pub mod debug;
/// FFmpeg discovery, capability detection, profiles, and argument builders.
pub mod ffmpeg;
/// Rational frame-rate helpers shared by composite encoding modules.
pub mod fps;
/// Encoding runtime pipelines and their shared frame/process infrastructure.
pub mod pipeline;
/// Live render progress estimation helpers.
pub mod progress;
/// Composite quality/bitrate validation and FFmpeg arguments.
pub mod quality;
/// Video-local activity coverage shared by both export modes.
pub mod video_timing;
