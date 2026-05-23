//! Pure helper logic shared by the composite render pipeline and its tests.
//!
//! This module owns only composite-specific helper functions that are pure or
//! nearly pure: progress math, output verification, stderr trimming, and
//! broken-pipe diagnostic formatting. The composite render loop, ffmpeg
//! process lifecycle, and render-plan orchestration remain in
//! `video_composite_pipeline.rs`.

use std::path::Path;

use crate::encode::video_composite_pipeline::CompositePipelinePlan;
use crate::error::{CoreError, CoreResult};

/// Converts one overlay timestamp into user-facing output-frame progress.
///
/// Composite renders may write fewer overlay frames than final video frames, so
/// progress is based on the source/output FPS rather than the overlay pipe FPS.
pub fn output_progress_for_overlay_time(
    video_local_time: f64,
    plan: &CompositePipelinePlan,
) -> u32 {
    (video_local_time * plan.output_fps.as_f64())
        .round()
        .max(0.0)
        .min(plan.output_frame_count as f64) as u32
}

/// Confirms that FFmpeg finalized a usable output file on success.
///
/// A successful process exit without a non-empty MP4 is treated as a render
/// failure because callers need a playable artifact, not just a clean status.
pub fn verify_successful_composite_output(output_path: &Path) -> CoreResult<()> {
    let metadata = std::fs::metadata(output_path).map_err(|error| CoreError::Io {
        path: output_path.to_path_buf(),
        source: error,
    })?;
    if metadata.len() == 0 {
        return Err(CoreError::Encode(format!(
            "Composite render finished but output file is empty: {}",
            output_path.display()
        )));
    }
    Ok(())
}

/// Returns whether an overlay write error indicates FFmpeg closed the pipe.
///
/// Broken-pipe wording varies by platform, so this uses the common error text
/// and OS error fragment instead of matching a single exact message.
pub fn is_pipe_write_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("failed writing composite overlay frame")
        && (lower.contains("broken pipe")
            || lower.contains("pipe is being closed")
            || lower.contains("os error 32")
            || lower.contains("os error 109")
            || lower.contains("os error 232"))
}

/// Formats a pipe-write failure with FFmpeg status and stderr diagnostics.
///
/// This makes early FFmpeg exits distinguishable from renderer bugs while still
/// preserving the underlying write error and recent encoder output.
pub fn format_pipe_write_failure(
    error: String,
    status: std::process::ExitStatus,
    stderr: &str,
    plan: &CompositePipelinePlan,
) -> String {
    let mut message = format!(
        "{error}. FFmpeg terminated before all overlay frames were written (status {status}) for profile {}.",
        plan.ffmpeg_settings.selected_profile_name
    );
    if let Some(fallback) = &plan.ffmpeg_settings.fallback_profile_name {
        message.push_str(&format!(
            "\nSafe fallback profile available: {fallback}. This explicit experimental render was not silently retried."
        ));
    }
    message.push_str("\nFilter graph:\n");
    message.push_str(&plan.ffmpeg_settings.filter_complex);
    if !stderr.trim().is_empty() {
        message.push_str("\nFFmpeg stderr:\n");
        message.push_str(&stderr_tail(stderr));
    }
    message
}

/// Returns the final part of FFmpeg stderr for concise error messages.
pub fn stderr_tail(stderr: &str) -> String {
    let lines = stderr.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(30);
    lines[start..].join("\n")
}
