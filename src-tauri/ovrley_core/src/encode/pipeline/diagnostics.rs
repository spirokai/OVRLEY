//! One stderr reader for progress and bounded FFmpeg diagnostics.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::ChildStderr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::encode::ffmpeg::composite::CompositeEncoding;
use crate::error::{CoreError, CoreResult};
use std::path::Path;

const STDERR_LINE_LIMIT: usize = 200;

#[derive(Default)]
pub(crate) struct EncoderMonitor {
    encoded_frames: AtomicU32,
    lines: Mutex<VecDeque<String>>,
}

impl EncoderMonitor {
    pub(crate) fn encoded_frames(&self) -> u32 {
        self.encoded_frames.load(Ordering::Relaxed)
    }

    pub(crate) fn stderr(&self) -> String {
        self.lines
            .lock()
            .expect("FFmpeg stderr mutex poisoned")
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn read(&self, stderr: ChildStderr) {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(frame) = parse_frame(&line) {
                self.encoded_frames.store(frame, Ordering::Relaxed);
            }
            let mut lines = self.lines.lock().expect("FFmpeg stderr mutex poisoned");
            if lines.len() == STDERR_LINE_LIMIT {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }
}

fn parse_frame(line: &str) -> Option<u32> {
    let start = line.find("frame=")? + "frame=".len();
    line[start..]
        .trim_start()
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_padded_ffmpeg_progress_without_confusing_other_logs() {
        assert_eq!(parse_frame("frame=  123 fps=30"), Some(123));
        assert_eq!(parse_frame("frame=456"), Some(456));
        assert_eq!(parse_frame("frame=unknown"), None);
        assert_eq!(parse_frame("video:123kB audio:0kB"), None);
    }
}

/// Confirms that FFmpeg finalized a usable output file on success.
///
/// A successful process exit without a non-empty video is treated as a render
/// failure because callers need a playable artifact, not just a clean status.
pub fn verify_successful_output(output_path: &Path) -> CoreResult<()> {
    let metadata = std::fs::metadata(output_path).map_err(|error| CoreError::Io {
        path: output_path.to_path_buf(),
        source: error,
    })?;
    if metadata.len() == 0 {
        return Err(CoreError::Encode(format!(
            "Render finished but output file is empty: {}",
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
    plan: &CompositeEncoding,
) -> String {
    let mut message = format!(
        "{error}. FFmpeg terminated before all overlay frames were written (status {status}) for profile {}.",
        plan.ffmpeg_settings.codec_id.metadata().profile_name
    );
    if let Some(fallback) = plan.ffmpeg_settings.codec_id.metadata().fallback_profile {
        message.push_str(&format!(
            "\nSafe fallback profile available: {}. This explicit experimental render was not silently retried.",
            fallback.metadata().profile_name
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
