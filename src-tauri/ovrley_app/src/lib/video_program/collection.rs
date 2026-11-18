//! Port of `video-collection.js`, `timeline.js` (`buildTimelineProgram`) and `start-window.js`:
//! pure helpers over the clips of the collection.

use super::program::{build_program, Program};
use super::{Clip, ImportedVideo, SecondsRange, VideoEditsMap};
use ovrley_core::media::video_edits::VideoResolution;
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::fps::Fps;
use std::collections::BTreeSet;

/// `VIDEO_START_WINDOW_SECONDS`: how far before the reference's start or after its end a video
/// may start.
pub const VIDEO_START_WINDOW_SECONDS: f64 = 3600.0;

/// `startsOutsideWindow`.
pub fn starts_outside_window(offset_seconds: f64, from: f64, to: f64) -> bool {
    offset_seconds < from - VIDEO_START_WINDOW_SECONDS || offset_seconds > to + VIDEO_START_WINDOW_SECONDS
}

/// `sortVideosByTimeline`: by committed start, then import order.
pub fn sort_clips_by_timeline(clips: &[Clip]) -> Vec<&Clip> {
    let mut sorted: Vec<&Clip> = clips.iter().collect();
    sorted.sort_by(|left, right| {
        left.offset_seconds()
            .partial_cmp(&right.offset_seconds())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.video.import_order.cmp(&right.video.import_order))
    });
    sorted
}

/// `getVideosTimelineBounds`: the interval every clip covers, `None` without clips.
pub fn timeline_bounds(clips: &[Clip]) -> Option<SecondsRange> {
    if clips.is_empty() {
        return None;
    }
    Some(SecondsRange {
        start_second: clips.iter().map(Clip::offset_seconds).fold(f64::INFINITY, f64::min),
        end_second: clips.iter().map(Clip::end_second).fold(f64::NEG_INFINITY, f64::max),
    })
}

/// `joinVideoEdits`. Every video has edits: the slice keeps both maps in step.
pub fn join_video_edits(videos: &[ImportedVideo], edits: &VideoEditsMap) -> Vec<Clip> {
    videos
        .iter()
        .map(|video| Clip { video: video.clone(), edits: edits[&video.id].clone() })
        .collect()
}

/// The transient shift of a dragged clip, or of every clip when `video_id` is `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoDragPreview {
    pub video_id: Option<String>,
    pub delta_seconds: f64,
}

/// `applyVideoDragPreview`.
pub fn apply_video_drag_preview(mut clips: Vec<Clip>, preview: Option<&VideoDragPreview>) -> Vec<Clip> {
    if let Some(preview) = preview {
        for clip in &mut clips {
            if preview.video_id.as_deref().is_none_or(|id| id == clip.id()) {
                clip.edits.offset_seconds += preview.delta_seconds;
            }
        }
    }
    clips
}

/// The clips, program and covered interval shown on the timeline (`buildTimelineProgram`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineProgram {
    pub clips: Vec<Clip>,
    pub program: Program,
    pub bounds: Option<SecondsRange>,
}

/// `buildTimelineProgram`.
pub fn build_timeline_program(videos: &[ImportedVideo], edits: &VideoEditsMap, preview: Option<&VideoDragPreview>, defaults: &TransitionDefaults) -> TimelineProgram {
    let clips = apply_video_drag_preview(join_video_edits(videos, edits), preview);
    let program = build_program(&clips, defaults);
    let bounds = timeline_bounds(&clips);
    TimelineProgram { clips, program, bounds }
}

/// `deriveCompatibility`'s result.
#[derive(Clone, Debug, PartialEq)]
pub struct Compatibility {
    pub reference: Option<(VideoResolution, Fps)>,
    pub incompatible_ids: BTreeSet<String>,
}

/// `deriveCompatibility`: groups clips by display resolution and exact rate. The largest group is
/// compatible; ties go to the group holding the earliest-imported clip.
pub fn derive_compatibility(clips: &[&Clip]) -> Compatibility {
    let mut ordered: Vec<&Clip> = clips.to_vec();
    ordered.sort_by_key(|clip| clip.video.import_order);
    let mut groups: Vec<Vec<&Clip>> = Vec::new();
    for clip in ordered {
        let same = |other: &&Clip| other.video.resolution == clip.video.resolution && other.video.fps == clip.video.fps;
        match groups.iter_mut().find(|group| same(&group[0])) {
            Some(group) => group.push(clip),
            None => groups.push(vec![clip]),
        }
    }
    // `reduce` keeps the first of equally large groups.
    let Some(reference) = groups.iter().fold(None::<&Vec<&Clip>>, |best, group| match best {
        Some(best) if group.len() <= best.len() => Some(best),
        _ => Some(group),
    }) else {
        return Compatibility { reference: None, incompatible_ids: BTreeSet::new() };
    };
    let compatible: BTreeSet<&str> = reference.iter().map(|clip| clip.id()).collect();
    Compatibility {
        reference: Some((reference[0].video.resolution, reference[0].video.fps)),
        incompatible_ids: clips.iter().filter(|clip| !compatible.contains(clip.id())).map(|clip| clip.id().to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    #[test]
    fn starts_are_accepted_up_to_an_hour_around_the_reference() {
        assert!(!starts_outside_window(100.0 - VIDEO_START_WINDOW_SECONDS, 100.0, 500.0));
        assert!(!starts_outside_window(500.0 + VIDEO_START_WINDOW_SECONDS, 100.0, 500.0));
        assert!(!starts_outside_window(300.0, 100.0, 500.0));
        assert!(starts_outside_window(100.0 - VIDEO_START_WINDOW_SECONDS - 0.04, 100.0, 500.0));
        assert!(starts_outside_window(500.0 + VIDEO_START_WINDOW_SECONDS + 0.04, 100.0, 500.0));
    }

    #[test]
    fn a_drag_preview_shifts_one_clip_or_every_clip() {
        let videos = [simple("a", 10.0, 0), simple("b", 10.0, 1)];
        let edits = edits_at(&videos, &[("a", 0.0), ("b", 20.0)]);
        let offsets = |preview: Option<VideoDragPreview>| {
            apply_video_drag_preview(join_video_edits(&videos, &edits), preview.as_ref()).iter().map(Clip::offset_seconds).collect::<Vec<_>>()
        };
        assert_eq!(offsets(None), [0.0, 20.0]);
        assert_eq!(offsets(Some(VideoDragPreview { video_id: Some("b".into()), delta_seconds: -5.0 })), [0.0, 15.0]);
        assert_eq!(offsets(Some(VideoDragPreview { video_id: None, delta_seconds: 2.0 })), [2.0, 22.0]);
        let clips = join_video_edits(&videos, &edits);
        assert_eq!(timeline_bounds(&clips), Some(SecondsRange { start_second: 0.0, end_second: 30.0 }));
        assert_eq!(timeline_bounds(&[]), None);
    }

    #[test]
    fn the_largest_format_group_is_compatible_and_ties_go_to_the_earliest_import() {
        let hd = |id, order| video(TestVideo { id, import_order: order, ..TestVideo::default() });
        let uhd = |id, order| video(TestVideo { id, import_order: order, width: 3840, height: 2160, ..TestVideo::default() });
        let edits = |videos: &[ImportedVideo]| edits_at(videos, &[]);
        let videos = [uhd("a", 0), hd("b", 1), hd("c", 2)];
        let clips = join_video_edits(&videos, &edits(&videos));
        let refs: Vec<&Clip> = clips.iter().collect();
        assert_eq!(derive_compatibility(&refs).incompatible_ids, BTreeSet::from(["a".to_string()]));
        let videos = [hd("b", 1), uhd("a", 0)];
        let clips = join_video_edits(&videos, &edits(&videos));
        let refs: Vec<&Clip> = clips.iter().collect();
        assert_eq!(derive_compatibility(&refs).incompatible_ids, BTreeSet::from(["b".to_string()]));
        assert_eq!(derive_compatibility(&[]).reference, None);
    }
}
