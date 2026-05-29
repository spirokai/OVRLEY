//! Fixture-driven rendered frame, transparent video, and composite video
//! baseline suite.
//!
//! Normal compare run:
//! `cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`
//!
//! Baseline record/update run:
//! `OVRLEY_RECORD_BASELINES=1 cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`

mod common;
#[path = "common/render_baseline_support.rs"]
mod render_baseline_support;

use anyhow::Result;

/// Renders preview PNGs from fixture activity/config inputs and compares them to
/// committed pixel baselines.
#[test]
#[ignore = "baseline tests run manually only: cargo test --test render_baseline_suite -- --ignored"]
fn rendered_frames_match_baselines() -> Result<()> {
    let _guard = render_baseline_support::suite_lock()
        .lock()
        .expect("render baseline suite lock poisoned");
    render_baseline_support::run_frame_cases()
}

/// Renders short transparent overlay videos and compares selected decoded
/// output frames to committed baselines.
#[test]
#[ignore = "baseline tests run manually only: cargo test --test render_baseline_suite -- --ignored"]
fn transparent_videos_match_baselines() -> Result<()> {
    let _guard = render_baseline_support::suite_lock()
        .lock()
        .expect("render baseline suite lock poisoned");
    render_baseline_support::run_transparent_video_cases()
}

/// Renders short composite MP4 outputs against the source fixture video and
/// compares selected decoded output frames to committed baselines.
#[test]
#[ignore = "baseline tests run manually only: cargo test --test render_baseline_suite -- --ignored"]
fn composite_videos_match_baselines() -> Result<()> {
    let _guard = render_baseline_support::suite_lock()
        .lock()
        .expect("render baseline suite lock poisoned");
    render_baseline_support::run_composite_video_cases()
}
