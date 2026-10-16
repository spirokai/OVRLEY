//! Shared activity coverage on a video-local, rational frame clock.

use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};

/// The covered portion of an output window. Frames outside it have no activity
/// samples and render rasters only. A video that does not overlap the activity
/// covers no frames; its window is then the whole activity, which no frame samples.
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
        if !activity_end.is_finite() || activity_end <= 0.0 {
            return Err(CoreError::Config(format!(
                "Activity end must be finite and greater than zero: {activity_end}"
            )));
        }
        let start = offset.max(0.0);
        let end = (offset + duration).min(activity_end);
        if end <= start {
            return Ok(Self {
                start: 0.0,
                end: activity_end,
                blank_leading_frame_count: total_frames,
                frame_count: 0,
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_video_outside_the_activity_covers_no_frames() {
        let fps = Fps::new(30, 1).unwrap();
        for offset in [-30.0, 120.0, 200.0] {
            let coverage = ActivityCoverage::for_video(30.0, offset, 120.0, fps, 900).unwrap();
            assert_eq!(
                coverage,
                ActivityCoverage {
                    start: 0.0,
                    end: 120.0,
                    blank_leading_frame_count: 900,
                    frame_count: 0
                }
            );
            assert!(coverage.frame_timeline(fps, offset).is_empty());
        }
        assert!(ActivityCoverage::for_video(30.0, 0.0, 0.0, fps, 900).is_err());
    }

    #[test]
    fn partial_overlap_leaves_leading_and_trailing_frames_uncovered() {
        let fps = Fps::new(30, 1).unwrap();
        let coverage = ActivityCoverage::for_video(30.0, -5.0, 20.0, fps, 900).unwrap();
        assert_eq!((coverage.start, coverage.end), (0.0, 20.0));
        assert_eq!(coverage.blank_leading_frame_count, 150);
        assert_eq!(coverage.frame_count, 600);
    }
}
