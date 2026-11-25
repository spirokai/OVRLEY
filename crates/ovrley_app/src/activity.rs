//! The activity window a render keeps, and the activity that drives widgets at a timeline
//! second.
//!
//! An external activity is authoritative for the whole timeline and starts at timeline second 0.
//! Without it, the clip the preview shows supplies its own embedded telemetry in clip-local time;
//! telemetry is never borrowed from another clip.

use ovrley_core::activity::trim::ActivityTrim;
use crate::store::media::{activity_duration_seconds, ActivitySummary, AvailableMetric, ImportedActivity};
use crate::video::Clip;
use crate::video::program::{resolve_preview_target, Program};

/// The shortest trimmed activity.
pub const MIN_ACTIVITY_TRIM_SECONDS: f64 = 1.0;

/// The rendered activity window: the trim, else the whole activity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityWindow {
    pub start_second: f64,
    pub end_second: f64,
    pub is_trimmed: bool,
}

impl ActivityWindow {
    pub fn new(duration_seconds: f64, trim: Option<ActivityTrim>) -> Self {
        match trim {
            None => Self { start_second: 0.0, end_second: duration_seconds, is_trimmed: false },
            Some(trim) => Self { start_second: trim.start_second, end_second: trim.end_second, is_trimmed: true },
        }
    }
}

/// Which edge of the trim moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimEdge {
    Start,
    End,
}

/// Moves one edge, keeping it inside the activity and the edges [`MIN_ACTIVITY_TRIM_SECONDS`]
/// apart. A trim covering the whole activity is `None`.
pub fn trim_activity_edge(trim: Option<ActivityTrim>, duration_seconds: f64, edge: TrimEdge, second: f64) -> Option<ActivityTrim> {
    let window = ActivityWindow::new(duration_seconds, trim);
    let next = match edge {
        TrimEdge::Start => ActivityTrim {
            start_second: second.max(0.0).min((window.end_second - MIN_ACTIVITY_TRIM_SECONDS).max(0.0)),
            end_second: window.end_second,
        },
        TrimEdge::End => ActivityTrim {
            start_second: window.start_second,
            end_second: second.max(duration_seconds.min(window.start_second + MIN_ACTIVITY_TRIM_SECONDS)).min(duration_seconds),
        },
    };
    if next.start_second <= 0.0 && next.end_second >= duration_seconds {
        None
    } else {
        Some(next)
    }
}

/// The activity that drives widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityContext {
    /// `None` while the clip's telemetry is pending, absent or failed.
    pub activity: Option<ImportedActivity>,
    pub summary: Option<ActivitySummary>,
    /// The timeline second of activity second 0.
    pub origin_second: f64,
    /// The clip whose telemetry it is; `None` for the external activity.
    pub video_id: Option<String>,
}

/// The external activity from origin 0, else the telemetry of `clip` (the clip the preview shows)
/// from the clip's start. `None` without activity and clip.
pub fn resolve_activity_context(parsed_activity: Option<&ImportedActivity>, activity_summary: Option<&ActivitySummary>, clip: Option<&Clip>) -> Option<ActivityContext> {
    if let Some(activity) = parsed_activity {
        return Some(ActivityContext { activity: Some(activity.clone()), summary: activity_summary.cloned(), origin_second: 0.0, video_id: None });
    }
    let clip = clip?;
    let activity = clip.video.telemetry.activity().cloned();
    Some(ActivityContext {
        summary: activity.as_ref().map(|imported| ActivitySummary::of(&imported.activity)),
        activity,
        origin_second: clip.offset_seconds(),
        video_id: Some(clip.id().to_string()),
    })
}

/// Whether activity data covers a timeline second. With an external
/// activity the second lies inside it; without one, inside the telemetry of the clip playing
/// there. Without activity or clips (template design) everything is covered. A part whose overlay
/// is switched off is never covered, as in the render.
pub fn has_activity_coverage(parsed_activity: Option<&ImportedActivity>, activity_summary: Option<&ActivitySummary>, program: &Program, clips: &[Clip], second: f64) -> bool {
    let target = if clips.is_empty() { None } else { resolve_preview_target(program, second) };
    if target.as_ref().is_some_and(|target| (target.is_inside || target.is_excluded) && target.piece.skip_overlay) {
        return false;
    }
    if parsed_activity.is_some() {
        let duration = activity_summary.map_or(0.0, |summary| summary.duration_seconds);
        return second >= 0.0 && second <= duration;
    }
    if clips.is_empty() {
        return true;
    }
    let Some(target) = target.filter(|target| target.is_inside) else { return false };
    let clip = clips.iter().find(|clip| clip.id() == target.video_id).expect("the program's clips");
    let Some(activity) = clip.video.telemetry.activity() else { return false };
    let clip_second = second - clip.offset_seconds();
    clip_second >= 0.0 && clip_second <= activity_duration_seconds(&activity.activity)
}

/// The external activity's metrics, or the union of every clip's
/// telemetry metrics, first one per attribute.
pub fn resolve_available_metrics(activity_summary: Option<&ActivitySummary>, clips: &[Clip]) -> Vec<AvailableMetric> {
    if let Some(summary) = activity_summary {
        return summary.available_metrics.clone();
    }
    let mut metrics: Vec<AvailableMetric> = Vec::new();
    for activity in clips.iter().filter_map(|clip| clip.video.telemetry.activity()) {
        for metric in ActivitySummary::of(&activity.activity).available_metrics {
            if !metrics.iter().any(|known| known.attribute == metric.attribute) {
                metrics.push(metric);
            }
        }
    }
    metrics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::collection::build_timeline_program;
    use crate::video::fixtures::*;
    use crate::video::{TelemetryStatus, VideoTelemetry};
    use ovrley_core::activity::schema::ParsedActivity;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;
    use std::sync::Arc;

    fn telemetry(duration: f64) -> ImportedActivity {
        let mut activity = ParsedActivity { trim_end_seconds: duration, ..Default::default() };
        activity.extra.insert("valid_attributes".into(), serde_json::json!(["speed"]));
        activity.extra.insert("coverage".into(), serde_json::json!({"speed": {"source": "direct"}}));
        ImportedActivity { activity: Arc::new(activity), preview_bytes: Arc::new(Vec::new()) }
    }

    fn summary(duration_seconds: f64) -> ActivitySummary {
        ActivitySummary { duration_seconds, ..ActivitySummary::of(&ParsedActivity::default()) }
    }

    #[test]
    fn an_external_activity_covers_exactly_its_duration() {
        let videos = [simple("video-1", 60.0, 0)];
        let timeline = build_timeline_program(&videos, &edits_at(&videos, &[("video-1", -10.0)]), None, transition_defaults());
        let external = telemetry(30.0);
        let covered = |second| has_activity_coverage(Some(&external), Some(&summary(30.0)), &timeline.program, &timeline.clips, second);
        assert_eq!([covered(-5.0), covered(0.0), covered(30.0), covered(31.0)], [false, true, true, false]);
    }

    #[test]
    fn without_an_external_activity_the_clip_covers_its_telemetry_time() {
        let mut with = simple("with", 30.0, 0);
        with.telemetry = VideoTelemetry::ready(telemetry(20.0));
        let mut without = simple("without", 30.0, 1);
        without.telemetry = VideoTelemetry::without_activity(TelemetryStatus::Absent).unwrap();
        let videos = [with, without];
        let timeline = build_timeline_program(&videos, &edits_at(&videos, &[("without", 40.0)]), None, transition_defaults());
        let covered = |second| has_activity_coverage(None, None, &timeline.program, &timeline.clips, second);
        assert!(covered(10.0));
        assert!(!covered(25.0), "past the telemetry end of its clip");
        assert!(!covered(35.0), "gap");
        assert!(!covered(50.0), "clip without telemetry");
        assert!(has_activity_coverage(None, None, &Program::default(), &[], 5.0), "template design is covered");
    }

    #[test]
    fn a_part_with_its_overlay_off_is_never_covered() {
        let mut video = simple("video-1", 60.0, 0);
        video.telemetry = VideoTelemetry::ready(telemetry(20.0));
        let videos = [video];
        let mut edits = edits_at(&videos, &[("video-1", 10.0)]);
        edits.get_mut("video-1").unwrap().segments.get_mut("start").unwrap().skip_overlay = true;
        let timeline = build_timeline_program(&videos, &edits, None, transition_defaults());
        let external = telemetry(100.0);
        let covered_external = |second| has_activity_coverage(Some(&external), Some(&summary(100.0)), &timeline.program, &timeline.clips, second);
        assert!(covered_external(5.0), "the gap before the part stays covered");
        assert!(!covered_external(20.0));
        assert!(!has_activity_coverage(None, None, &timeline.program, &timeline.clips, 20.0));
    }

    #[test]
    fn the_context_is_the_external_activity_else_the_clip_telemetry() {
        let external = telemetry(120.0);
        let context = resolve_activity_context(Some(&external), Some(&summary(120.0)), None).unwrap();
        assert_eq!((context.origin_second, context.video_id), (0.0, None));
        let mut video = simple("video-1", 60.0, 0);
        video.telemetry = VideoTelemetry::ready(telemetry(120.0));
        let clip_with = clip(video, ovrley_core::media::video_edits::VideoEdits { offset_seconds: 300.0, ..Default::default() });
        let context = resolve_activity_context(None, None, Some(&clip_with)).unwrap();
        assert_eq!((context.origin_second, context.video_id.as_deref()), (300.0, Some("video-1")));
        assert_eq!(context.summary.unwrap().available_metrics.len(), 1);
        let pending = clip(simple("p", 10.0, 0), Default::default());
        let context = resolve_activity_context(None, None, Some(&pending)).unwrap();
        assert_eq!((context.activity, context.summary), (None, None));
        assert_eq!(resolve_activity_context(None, None, None), None);
    }

    fn trim(start_second: f64, end_second: f64) -> Option<ActivityTrim> {
        Some(ActivityTrim { start_second, end_second })
    }

    #[test]
    fn the_window_is_the_whole_activity_without_a_trim() {
        assert_eq!(ActivityWindow::new(60.0, None), ActivityWindow { start_second: 0.0, end_second: 60.0, is_trimmed: false });
        assert_eq!(ActivityWindow::new(60.0, trim(5.0, 40.0)), ActivityWindow { start_second: 5.0, end_second: 40.0, is_trimmed: true });
    }

    #[test]
    fn an_edge_moves_inside_the_activity() {
        assert_eq!(trim_activity_edge(None, 60.0, TrimEdge::Start, 10.0), trim(10.0, 60.0));
        assert_eq!(trim_activity_edge(None, 60.0, TrimEdge::End, 45.0), trim(0.0, 45.0));
        assert_eq!(trim_activity_edge(trim(10.0, 60.0), 60.0, TrimEdge::Start, -5.0), None, "back to the whole activity");
        assert_eq!(trim_activity_edge(trim(0.0, 45.0), 60.0, TrimEdge::End, 90.0), None);
    }

    #[test]
    fn edges_stay_one_second_apart() {
        assert_eq!(trim_activity_edge(trim(10.0, 20.0), 60.0, TrimEdge::Start, 25.0), trim(19.0, 20.0));
        assert_eq!(trim_activity_edge(trim(10.0, 20.0), 60.0, TrimEdge::End, 5.0), trim(10.0, 11.0));
    }

    #[test]
    fn a_short_activity_keeps_its_edges_inside() {
        assert_eq!(trim_activity_edge(None, 0.5, TrimEdge::Start, 0.4), None, "no room for a trim");
        assert_eq!(trim_activity_edge(trim(0.2, 0.5), 0.5, TrimEdge::End, 0.1), trim(0.2, 0.5));
    }

    #[test]
    fn the_end_stays_after_the_start() {
        assert_eq!(trim_activity_edge(trim(30.0, 60.0), 60.0, TrimEdge::End, 10.0), trim(30.0, 31.0));
    }
}
