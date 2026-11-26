//! OVRLEY's core without a UI: activity import and preparation, render config validation,
//! Skia overlay rendering, FFmpeg encoding, render jobs and their diagnostics.
#![recursion_limit = "256"]

pub mod activity;
#[doc(hidden)]
pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod encode;
pub mod error;
pub mod fonts;
pub mod import;
pub mod jobs;
pub mod media;
pub mod output;
pub mod paths;
pub mod raster;
pub mod render;
pub mod template;
