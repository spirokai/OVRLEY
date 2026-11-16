//! Port of `features/player/utils/playerTiming.js`: the durations of the timeline and where
//! playback starts. The playback engine (`usePlaybackEngine`) joins in this module.

use crate::lib::video_program::program::{resolve_playback_start, PlaybackSource, PlaybackStart, Program};
use crate::lib::video_program::SecondsRange;

/// `snapTimelineSecondToFrame`: a timeline second on the nearest frame of a video whose frame
/// zero sits at `origin_second`.
pub fn snap_timeline_second_to_frame(second: f64, fps: f64, origin_second: f64) -> f64 {
    origin_second + crate::js::round((second - origin_second) * fps) / fps
}

/// `resolvePlaybackSource`: without video playback the timeline clock plays from the playhead.
pub fn resolve_playback_source(should_use_video_playback: bool, playhead_second: f64, program: &Program) -> PlaybackStart {
    if !should_use_video_playback {
        return PlaybackStart { source: PlaybackSource::Timeline, second: playhead_second };
    }
    resolve_playback_start(program, playhead_second)
}

/// `getTimelineMinimum`: the first second media occupies, never after 0.
pub fn timeline_minimum(video_bounds: Option<SecondsRange>) -> f64 {
    video_bounds.map_or(0.0, |bounds| bounds.start_second.min(0.0))
}

/// `getTotalPlaybackDuration`: the largest of the activity, the fallback duration (only without
/// activity and without videos) and the videos' end.
pub fn total_playback_duration(activity_duration_seconds: Option<f64>, fallback_duration_seconds: f64, video_bounds: Option<SecondsRange>) -> f64 {
    let activity = activity_duration_seconds.unwrap_or(0.0);
    let video_end = video_bounds.map_or(0.0, |bounds| bounds.end_second);
    let content = if activity > 0.0 {
        activity
    } else if video_bounds.is_some() {
        0.0
    } else {
        fallback_duration_seconds
    };
    content.max(video_end).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::video_program::collection::build_timeline_program;
    use crate::lib::video_program::fixtures::*;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;

    fn bounds(start_second: f64, end_second: f64) -> Option<SecondsRange> {
        Some(SecondsRange { start_second, end_second })
    }

    #[test]
    fn frames_snap_from_their_origin() {
        assert!((snap_timeline_second_to_frame(12.38, 30.0, 5.25) - 12.383_333_333_3).abs() < 1e-9);
    }

    #[test]
    fn the_total_duration_reaches_the_videos_end_and_ignores_the_fallback_with_videos() {
        assert_eq!(total_playback_duration(Some(12.0), 9.0, bounds(10.0, 16.0)), 16.0);
        assert_eq!(total_playback_duration(Some(0.0), 73.0, bounds(0.0, 12.509)), 12.509);
        assert_eq!(total_playback_duration(None, 73.0, None), 73.0);
        assert_eq!(total_playback_duration(Some(25.0), 0.0, bounds(-5.0, 25.0)), 25.0);
    }

    #[test]
    fn the_minimum_is_a_negative_video_start() {
        assert_eq!(timeline_minimum(bounds(-5.0, 25.0)), -5.0);
        assert_eq!(timeline_minimum(bounds(3.0, 25.0)), 0.0);
        assert_eq!(timeline_minimum(None), 0.0);
    }

    #[test]
    fn video_playback_is_scoped_to_the_video_window() {
        let program = |offset| {
            let videos = [simple("video-1", 4.0, 0)];
            build_timeline_program(&videos, &edits_at(&videos, &[("video-1", offset)]), None, transition_defaults()).program
        };
        let (shifted, negative) = (program(5.0), program(-5.0));
        let source = |program: &Program, at, video| resolve_playback_source(video, at, program);
        let start = |source, second| PlaybackStart { source, second };
        assert_eq!(source(&shifted, 4.99, true), start(PlaybackSource::Timeline, 4.99));
        assert_eq!(source(&shifted, 5.0, true), start(PlaybackSource::Video, 5.0));
        assert_eq!(source(&shifted, 8.99, true), start(PlaybackSource::Video, 8.99));
        assert_eq!(source(&shifted, 9.0, true), start(PlaybackSource::Timeline, 9.0));
        assert_eq!(source(&shifted, 6.0, false), start(PlaybackSource::Timeline, 6.0));
        assert_eq!(source(&negative, -4.99, true), start(PlaybackSource::Video, -4.99));
        assert_eq!(source(&negative, -1.0, true), start(PlaybackSource::Timeline, -1.0));
    }
}
