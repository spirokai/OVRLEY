//! Core rendering and encoding library for OVRLEY.
//!
//! This crate is the Rust backend used by the Tauri shell. It owns the
//! production data path from frontend JSON payloads through activity trimming,
//! per-frame interpolation, Skia overlay rendering, ffmpeg encoding, progress
//! tracking, and debug artifact generation.
//!
//! Public modules are intentionally grouped by responsibility so the Tauri
//! command layer can stay thin while the testable business logic remains here.

/// Activity JSON contracts plus trim and interpolation utilities.
pub mod activity;
/// Backend-facing command helpers used by the Tauri application layer.
pub mod commands;
/// Template and scene configuration contracts.
pub mod config;
/// Progress and timing diagnostics shared by render and encode code.
pub mod debug;
/// Video encoding and ffmpeg integration.
pub mod encode;
/// Structured error types and result alias used by all core modules.
pub mod error;
/// Application path configuration and resolution.
pub mod paths;
/// Shared interpolation utilities used by activity and render modules.
pub mod interpolation;
/// Shared Ramer-Douglas-Peucker line simplification.
pub mod rdp;
/// Skia-based overlay rendering.
pub mod render;
/// Cross-cutting domain types (MetricKind, etc.) shared by config, render, and activity.
pub mod types;

pub use error::{CoreError, CoreResult};
pub use types::MetricKind;
