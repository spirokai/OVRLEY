//! Shared activity coverage on a video-local, rational frame clock.

use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};

/// The covered portion of an output window. Padding has no activity samples.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityCoverage {
    pub start: f64,
    pub end: f64,
    pub blank_leading_frame_count: u64,
    pub frame_count: u64,
}

impl ActivityCoverage {
    pub fn for_video(
        duration: f64,
        offset: f64,
        activity_end: f64,
        fps: Fps,
        total_frames: u64,
    ) -> CoreResult<Self> {
        let start = offset.max(0.0);
        let end = (offset + duration).min(activity_end);
        if end <= start {
            return Err(CoreError::Config(format!(
                "Video range [{offset}, {}] must have positive overlap with activity [0, {activity_end}]",
                offset + duration
            )));
        }
        let mut first = fps
            .frame_count_for_duration((start - offset).max(0.0))?
            .min(total_frames);
        // Duration rounding tolerates container jitter, but a frame preceding
        // actual activity coverage must still be blank.
        if first < total_frames && offset + fps.seconds_at_frame(first) < start {
            first += 1;
        }
        let mut last = fps
            .frame_count_for_duration(end - offset)?
            .min(total_frames);
        if last > first && offset + fps.seconds_at_frame(last - 1) >= end {
            last -= 1;
        }
        Ok(Self {
            start,
            end,
            blank_leading_frame_count: first,
            frame_count: last.saturating_sub(first),
        })
    }

    /// Scene-relative timestamps retain the fractional phase of the source
    /// clock: the first covered frame need not fall at activity time zero.
    pub fn frame_timeline(&self, fps: Fps, offset: f64) -> Vec<f64> {
        (0..self.frame_count)
            .map(|index| {
                offset + fps.seconds_at_frame(self.blank_leading_frame_count + index) - self.start
            })
            .collect()
    }
}
