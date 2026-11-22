//! Port of `video-segments.js`: the segments of a clip, and where cuts and trims can go.
//! `fitCutsToFrameCount` arrives with relinking in Phase 6.

use super::Clip;
use ovrley_core::media::video_edits::{SegmentEdits, VideoEdits, FIRST_SEGMENT_KEY};

/// One segment of a clip in source order (`listVideoSegments`).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub key: String,
    pub start_frame: i64,
    pub end_frame: i64,
    pub edits: SegmentEdits,
}

/// `listVideoSegments`.
pub fn list_video_segments(clip: &Clip) -> Vec<Segment> {
    let starts: Vec<(&str, i64)> = std::iter::once((FIRST_SEGMENT_KEY, 0)).chain(clip.cuts().iter().map(|cut| (cut.id.as_str(), cut.frame))).collect();
    starts
        .iter()
        .enumerate()
        .map(|(index, (key, frame))| Segment {
            key: key.to_string(),
            start_frame: *frame,
            end_frame: starts.get(index + 1).map_or(clip.frame_count(), |next| next.1),
            edits: *clip.segment(key),
        })
        .collect()
}

/// `segmentJoinKey`: the key of the join into a segment, unique across the program.
pub fn segment_join_key(video_id: &str, segment_key: &str) -> String {
    format!("{video_id}:{segment_key}")
}

/// `isVideoExcluded`: nothing of the clip is exported.
pub fn is_video_excluded(clip: &Clip) -> bool {
    clip.edits.segments.values().all(|segment| segment.excluded)
}

/// `resolveCutFrame`: the frame a cut at a clip-local second would use, strictly inside the clip
/// and not on a cut.
pub fn resolve_cut_frame(clip: &Clip, source_second: f64) -> Option<i64> {
    let frame = clip.fps().seconds_to_frames(source_second);
    if frame <= 0 || frame >= clip.frame_count() || clip.cuts().iter().any(|cut| cut.frame == frame) {
        return None;
    }
    Some(frame)
}

/// Which end of a clip a trim applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TrimSide {
    Start,
    End,
}

/// `planVideoTrim`'s edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrimPlan {
    /// A new trim (`trimVideo`).
    Trim,
    /// Moves the end's trim cut (`moveCut`).
    Move { cut_id: String },
}

/// `planVideoTrim`: an untrimmed end gets a new trim; a trimmed end moves its trim cut (the first
/// cut for the start, the last for the end). A trim never passes another cut.
pub fn plan_video_trim(clip: &Clip, side: TrimSide, frame: i64) -> Option<TrimPlan> {
    let cuts = clip.cuts();
    let frame_count = clip.frame_count();
    if frame <= 0 || frame >= frame_count {
        return None;
    }
    let last_key = cuts.last().map_or(FIRST_SEGMENT_KEY, |cut| cut.id.as_str());
    let outer = match side {
        TrimSide::Start => FIRST_SEGMENT_KEY,
        TrimSide::End => last_key,
    };
    if !clip.segment(outer).excluded {
        let inside = match side {
            TrimSide::Start => frame < cuts.first().map_or(frame_count, |cut| cut.frame),
            TrimSide::End => frame > cuts.last().map_or(0, |cut| cut.frame),
        };
        return inside.then_some(TrimPlan::Trim);
    }
    let (trim_cut, inside) = match side {
        TrimSide::Start => (cuts.first()?, frame < cuts.get(1).map_or(frame_count, |cut| cut.frame)),
        TrimSide::End => (cuts.last()?, frame > cuts.len().checked_sub(2).map_or(0, |index| cuts[index].frame)),
    };
    (inside && frame != trim_cut.frame).then(|| TrimPlan::Move { cut_id: trim_cut.id.clone() })
}

/// `findSegmentAt`: the segment showing at a clip-local second; the clip's end belongs to its
/// last segment.
pub fn find_segment_at(clip: &Clip, source_second: f64) -> Segment {
    let frame = clip.fps().seconds_to_frames(source_second).min(clip.frame_count() - 1);
    list_video_segments(clip)
        .into_iter()
        .find(|segment| frame < segment.end_frame)
        .expect("the last segment ends at the clip's frame count")
}

/// `resolveEditClip`: the selected clip when it covers the second, else the clip the preview shows
/// there, else the earliest clip covering it.
pub fn resolve_edit_clip<'a>(clips: &'a [Clip], preview_video_id: Option<&str>, selected_video_id: Option<&str>, second: f64) -> Option<&'a Clip> {
    let covering: Vec<&Clip> = clips.iter().filter(|clip| clip.offset_seconds() <= second && second <= clip.end_second()).collect();
    let by_id = |id: Option<&str>| id.and_then(|id| covering.iter().copied().find(|clip| clip.id() == id));
    by_id(selected_video_id).or_else(|| by_id(preview_video_id)).or_else(|| {
        covering.iter().copied().min_by(|left, right| {
            left.offset_seconds()
                .partial_cmp(&right.offset_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.video.import_order.cmp(&right.video.import_order))
        })
    })
}

/// `fitCutsToFrameCount`: the edits fitted to a source file of `frame_count` frames: cuts at or
/// past its end are dropped with the edits of the segments they start.
pub fn fit_cuts_to_frame_count(edits: &VideoEdits, frame_count: i64) -> VideoEdits {
    let cuts: Vec<_> = edits.cuts.iter().filter(|cut| cut.frame < frame_count).cloned().collect();
    let segments = edits
        .segments
        .iter()
        .filter(|(key, _)| key.as_str() == FIRST_SEGMENT_KEY || cuts.iter().any(|cut| &cut.id == *key))
        .map(|(key, segment)| (key.clone(), *segment))
        .collect();
    VideoEdits { cuts, segments, ..edits.clone() }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    fn ride(cuts: &[(String, i64, SegmentEdits)], start: SegmentEdits) -> Clip {
        clip(simple("ride", 10.0, 0), cut_edits(cuts, start))
    }

    fn keys(clip: &Clip) -> Vec<(String, i64, i64, bool)> {
        list_video_segments(clip).into_iter().map(|segment| (segment.key, segment.start_frame, segment.end_frame, segment.edits.excluded)).collect()
    }

    #[test]
    fn a_video_without_cuts_is_one_segment() {
        let whole = ride(&[], SegmentEdits::INCLUDED);
        assert_eq!(keys(&whole), [("start".to_string(), 0, 300, false)]);
        assert!(!is_video_excluded(&whole));
        assert!(is_video_excluded(&ride(&[], excluded())));
    }

    #[test]
    fn cuts_split_a_video_into_segments_keyed_by_the_cut_they_start_at() {
        let video = ride(&[cut("c1", 100, excluded()), cut("c2", 200, SegmentEdits::INCLUDED)], SegmentEdits::INCLUDED);
        assert_eq!(
            keys(&video),
            [("start".to_string(), 0, 100, false), ("c1".to_string(), 100, 200, true), ("c2".to_string(), 200, 300, false)]
        );
        assert!(!is_video_excluded(&video));
    }

    #[test]
    fn finds_the_segment_at_a_second_the_last_one_at_the_end() {
        let video = ride(&[cut("c1", 150, SegmentEdits::INCLUDED)], SegmentEdits::INCLUDED);
        assert_eq!(find_segment_at(&video, 4.9).key, "start");
        assert_eq!(find_segment_at(&video, 5.0).key, "c1");
        assert_eq!(find_segment_at(&video, 10.0).key, "c1");
    }

    #[test]
    fn a_cut_goes_strictly_inside_and_not_on_another_cut() {
        let video = ride(&[cut("c1", 150, SegmentEdits::INCLUDED)], SegmentEdits::INCLUDED);
        assert_eq!(resolve_cut_frame(&video, 2.0), Some(60));
        assert_eq!(resolve_cut_frame(&video, 0.0), None);
        assert_eq!(resolve_cut_frame(&video, 10.0), None);
        assert_eq!(resolve_cut_frame(&video, 5.0), None);
    }

    #[test]
    fn commands_apply_to_the_selected_clip_else_the_previewed_else_the_earliest() {
        let videos = [simple("a", 20.0, 0), simple("b", 20.0, 1)];
        let clips = crate::lib::video_program::collection::join_video_edits(&videos, &edits_at(&videos, &[("b", 10.0)]));
        let id = |clip: Option<&Clip>| clip.map(|clip| clip.id().to_string());
        assert_eq!(id(resolve_edit_clip(&clips, Some("a"), Some("b"), 15.0)).as_deref(), Some("b"));
        assert_eq!(id(resolve_edit_clip(&clips, Some("b"), None, 15.0)).as_deref(), Some("b"));
        assert_eq!(id(resolve_edit_clip(&clips, None, None, 15.0)).as_deref(), Some("a"));
        assert_eq!(id(resolve_edit_clip(&clips, Some("a"), Some("b"), 5.0)).as_deref(), Some("a"), "a selected clip not covering the second gives way");
        assert_eq!(resolve_edit_clip(&clips, None, None, 40.0), None);
    }

    #[test]
    fn an_untrimmed_end_gets_a_new_trim_up_to_the_nearest_cut() {
        let whole = ride(&[], SegmentEdits::INCLUDED);
        assert_eq!(plan_video_trim(&whole, TrimSide::Start, 90), Some(TrimPlan::Trim));
        assert_eq!(plan_video_trim(&whole, TrimSide::End, 90), Some(TrimPlan::Trim));
        let cut_once = ride(&[cut("c1", 150, SegmentEdits::INCLUDED)], SegmentEdits::INCLUDED);
        assert_eq!(plan_video_trim(&cut_once, TrimSide::Start, 149), Some(TrimPlan::Trim));
        assert_eq!(plan_video_trim(&cut_once, TrimSide::Start, 200), None);
        assert_eq!(plan_video_trim(&cut_once, TrimSide::End, 100), None);
    }

    #[test]
    fn a_trimmed_end_moves_its_trim_cut_without_passing_the_next_cut() {
        let trimmed = ride(&[cut("trim", 30, SegmentEdits::INCLUDED), cut("c1", 150, SegmentEdits::INCLUDED), cut("tail", 270, excluded())], excluded());
        let moved = |cut_id: &str| Some(TrimPlan::Move { cut_id: cut_id.into() });
        assert_eq!(plan_video_trim(&trimmed, TrimSide::Start, 10), moved("trim"));
        assert_eq!(plan_video_trim(&trimmed, TrimSide::Start, 120), moved("trim"));
        assert_eq!(plan_video_trim(&trimmed, TrimSide::Start, 150), None);
        assert_eq!(plan_video_trim(&trimmed, TrimSide::Start, 30), None);
        assert_eq!(plan_video_trim(&trimmed, TrimSide::End, 200), moved("tail"));
        assert_eq!(plan_video_trim(&trimmed, TrimSide::End, 100), None);
    }

    #[test]
    fn an_edge_frame_or_a_wholly_excluded_video_trims_nothing() {
        assert_eq!(plan_video_trim(&ride(&[], SegmentEdits::INCLUDED), TrimSide::Start, 0), None);
        assert_eq!(plan_video_trim(&ride(&[], SegmentEdits::INCLUDED), TrimSide::End, 300), None);
        assert_eq!(plan_video_trim(&ride(&[], excluded()), TrimSide::Start, 90), None);
    }

    #[test]
    fn fitting_drops_cuts_at_or_past_the_end_with_their_segments() {
        let edits = cut_edits(&[cut("a", 100, SegmentEdits::INCLUDED), cut("b", 200, excluded()), cut("c", 300, SegmentEdits::INCLUDED)], excluded());
        let fitted = fit_cuts_to_frame_count(&edits, 200);
        assert_eq!(fitted.cuts.iter().map(|cut| cut.id.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(fitted.segments.keys().map(String::as_str).collect::<Vec<_>>(), ["a", "start"]);
        assert!(fitted.segments["start"].excluded);
        assert_eq!(fit_cuts_to_frame_count(&edits, 301), edits);
    }
}
