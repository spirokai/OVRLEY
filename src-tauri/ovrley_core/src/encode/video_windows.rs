//! Pure time-window helpers for segmented video rendering.
//!
//! This module owns only deterministic range splitting for render
//! orchestration. It does not spawn threads, talk to ffmpeg, or mutate render
//! state. Production code uses these helpers to derive stable transparent and
//! composite segment windows before the segmented orchestration layer runs.

use crate::config::RenderConfig;
use crate::encode::fps::Fps;

/// Output-frame window range for one parallel composite segment.
///
/// Parallel composite rendering splits the total output frame range into
/// roughly equal non-overlapping windows. Each segment runs an independent
/// ffmpeg process responsible for exactly the frames in
/// `[output_start_frame, output_end_frame)`. The video-time equivalents are
/// derived from the source FPS so each ffmpeg invocation can seek and trim
/// correctly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositeSegmentWindow {
    /// First frame index this segment must produce (inclusive).
    pub output_start_frame: u32,
    /// Frame index one past the last frame (exclusive).
    pub output_end_frame: u32,
    /// Source-video seek position in seconds for this segment.
    pub video_start_seconds: f64,
    /// Duration in seconds this segment's ffmpeg should process.
    pub render_duration_seconds: f64,
}

/// Divides total output frames into roughly equal windows for segmented rendering.
///
/// When the segment count exceeds the frame count, it is clamped so each
/// segment produces at least one frame. The remainder from integer division is
/// distributed one frame at a time across the first N segments so early
/// segments may be one frame longer than later ones.
///
/// Returns an empty `Vec` when `total_output_frames` is zero.
pub fn composite_output_frame_windows(
    total_output_frames: u32,
    render_duration: f64,
    source_fps: Fps,
    segment_count: usize,
) -> Vec<CompositeSegmentWindow> {
    if total_output_frames == 0 {
        return Vec::new();
    }

    let actual_segment_count = segment_count.min(total_output_frames as usize).max(1);
    let base_frames = total_output_frames / actual_segment_count as u32;
    let extra_segments = total_output_frames % actual_segment_count as u32;
    let frame_seconds = source_fps.den as f64 / source_fps.num as f64;
    let mut output_start_frame = 0u32;
    let mut windows = Vec::with_capacity(actual_segment_count);

    for index in 0..actual_segment_count {
        let segment_frames = base_frames + u32::from((index as u32) < extra_segments);
        let output_end_frame = output_start_frame + segment_frames;
        let video_start_seconds = output_start_frame as f64 * frame_seconds;
        let segment_end_seconds = if index == actual_segment_count - 1 {
            render_duration
        } else {
            output_end_frame as f64 * frame_seconds
        };

        windows.push(CompositeSegmentWindow {
            output_start_frame,
            output_end_frame,
            video_start_seconds,
            render_duration_seconds: (segment_end_seconds - video_start_seconds).max(0.0),
        });
        output_start_frame = output_end_frame;
    }

    windows
}

/// Returns the scene duration when start and end are exact integer seconds.
pub(crate) fn integer_second_duration(config: &RenderConfig) -> Option<u32> {
    // Stitching expects clean second boundaries so duplicated or missing frames
    // are not introduced at segment joins.
    let start = config.scene.start.round();
    let end = config.scene.end.round();
    if (config.scene.start - start).abs() > 1e-9 || (config.scene.end - end).abs() > 1e-9 {
        return None;
    }
    if end <= start {
        return None;
    }
    Some((end - start) as u32)
}

/// Splits an integer-second scene into balanced contiguous render windows.
pub(crate) fn integer_second_windows(
    config: &RenderConfig,
    total_seconds: u32,
    segment_count: usize,
) -> Vec<(f64, f64)> {
    let actual_segment_count = segment_count.min(total_seconds as usize).max(1);
    let base_seconds = total_seconds / actual_segment_count as u32;
    let extra_segments = total_seconds % actual_segment_count as u32;
    let mut cursor = config.scene.start.round();
    let mut windows = Vec::with_capacity(actual_segment_count);

    for index in 0..actual_segment_count {
        let segment_seconds = base_seconds + u32::from((index as u32) < extra_segments);
        let next_cursor = cursor + f64::from(segment_seconds);
        windows.push((cursor, next_cursor));
        cursor = next_cursor;
    }

    windows
}
