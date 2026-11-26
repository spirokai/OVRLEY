//! Helpers over the clips of the collection: timeline order and bounds, the timeline program,
//! format compatibility, and the per-clip warnings and combined-export issues.

use std::collections::{BTreeMap, BTreeSet};
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::timing::Fps;
use ovrley_core::media::edits::VideoResolution;
use crate::dialogs::filename_from_selected_path;
use crate::i18n::{catalog, interpolate, Locale};
use crate::video::{Clip, ImportedVideo, SecondsRange, VideoEditsMap};
use crate::video::program::{build_program, Program};
use crate::video::segments::is_video_excluded;
use crate::video::sync::SyncWarning;

/// How far before the reference's start or after its end a video
/// may start.
pub const VIDEO_START_WINDOW_SECONDS: f64 = 3600.0;

pub fn starts_outside_window(offset_seconds: f64, from: f64, to: f64) -> bool {
    offset_seconds < from - VIDEO_START_WINDOW_SECONDS || offset_seconds > to + VIDEO_START_WINDOW_SECONDS
}

/// By committed start, then import order.
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

/// The interval every clip covers, `None` without clips.
pub fn timeline_bounds(clips: &[Clip]) -> Option<SecondsRange> {
    if clips.is_empty() {
        return None;
    }
    Some(SecondsRange {
        start_second: clips.iter().map(Clip::offset_seconds).fold(f64::INFINITY, f64::min),
        end_second: clips.iter().map(Clip::end_second).fold(f64::NEG_INFINITY, f64::max),
    })
}

/// Every video joined with its edits; the store keeps both maps in step.
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

/// The clips, program and covered interval shown on the timeline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineProgram {
    pub clips: Vec<Clip>,
    pub program: Program,
    pub bounds: Option<SecondsRange>,
}

pub fn build_timeline_program(videos: &[ImportedVideo], edits: &VideoEditsMap, preview: Option<&VideoDragPreview>, defaults: &TransitionDefaults) -> TimelineProgram {
    let clips = apply_video_drag_preview(join_video_edits(videos, edits), preview);
    let program = build_program(&clips, defaults);
    let bounds = timeline_bounds(&clips);
    TimelineProgram { clips, program, bounds }
}

/// The clips grouped by display resolution and exact rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Compatibility {
    pub reference: Option<(VideoResolution, Fps)>,
    pub incompatible_ids: BTreeSet<String>,
}

/// Groups clips by display resolution and exact rate. The largest group is
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

/// What keeps videos from being exported as one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombinedExportIssueKind {
    Overlap,
    Incompatible,
    Rotation,
}

/// One issue and the videos it concerns, in clip order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombinedExportIssue {
    pub kind: CombinedExportIssueKind,
    pub video_ids: Vec<String>,
}

/// Overlapping videos, videos whose resolution or frame rate differs
/// from the others, and, when the encoder keeps the display rotation, videos rotated differently
/// from the first piece's video. Empty when one file is possible.
pub fn find_combined_export_issues(program: &Program, clips: &[&Clip], keeps_rotation: bool) -> Vec<CombinedExportIssue> {
    let mut issues = Vec::new();
    if !program.overlaps.is_empty() {
        let overlapping: BTreeSet<&str> = program.overlaps.iter().flat_map(|overlap| [overlap.earlier_video_id.as_str(), overlap.later_video_id.as_str()]).collect();
        issues.push(CombinedExportIssue {
            kind: CombinedExportIssueKind::Overlap,
            video_ids: clips.iter().filter(|clip| overlapping.contains(clip.id())).map(|clip| clip.id().to_string()).collect(),
        });
    }
    let incompatible = derive_compatibility(clips).incompatible_ids;
    if !incompatible.is_empty() {
        issues.push(CombinedExportIssue {
            kind: CombinedExportIssueKind::Incompatible,
            video_ids: clips.iter().filter(|clip| incompatible.contains(clip.id())).map(|clip| clip.id().to_string()).collect(),
        });
    }
    if let (true, Some(first)) = (keeps_rotation, program.pieces.first()) {
        let rotation = clips.iter().find(|clip| clip.id() == first.video_id).expect("pieces come from the clips").video.rotation_degrees;
        let rotated: Vec<String> = clips.iter().filter(|clip| clip.video.rotation_degrees != rotation).map(|clip| clip.id().to_string()).collect();
        if !rotated.is_empty() {
            issues.push(CombinedExportIssue { kind: CombinedExportIssueKind::Rotation, video_ids: std::iter::once(first.video_id.clone()).chain(rotated).collect() });
        }
    }
    issues
}

/// One clip warning (`{kind, params}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoWarning {
    /// The file cannot be found.
    Missing,
    /// Another format than the compatible group, which keeps it out of a combined export.
    Incompatible,
    /// It overlaps the clip of this file name.
    Overlap { name: String },
    /// Its synchronization warning.
    Sync(SyncWarning),
    /// Placed without a sync warning, it starts more than an hour from the activity.
    Far,
}

/// Warnings keyed by video id; every clip has an entry.
pub type VideoWarnings = BTreeMap<String, Vec<VideoWarning>>;

fn clip_name(clip: &Clip) -> String {
    filename_from_selected_path(&clip.video.path).unwrap_or_default().to_string()
}

/// The warnings of every clip. A clip excluded from export does not take part in the format comparison.
pub fn derive_video_warnings(clips: &[Clip], program: &Program, activity_duration_seconds: Option<f64>) -> VideoWarnings {
    let exported: Vec<&Clip> = clips.iter().filter(|clip| !is_video_excluded(clip)).collect();
    let incompatible = derive_compatibility(&exported).incompatible_ids;
    let names: BTreeMap<&str, String> = clips.iter().map(|clip| (clip.id(), clip_name(clip))).collect();
    let mut warnings: VideoWarnings = clips.iter().map(|clip| (clip.id().to_string(), Vec::new())).collect();
    for clip in clips {
        let entry = warnings.get_mut(clip.id()).expect("every clip has an entry");
        if clip.video.missing {
            entry.push(VideoWarning::Missing);
        }
        if incompatible.contains(clip.id()) {
            entry.push(VideoWarning::Incompatible);
        }
    }
    // One warning per overlapping pair; two clips may share a file name.
    let mut warned = BTreeSet::new();
    for overlap in &program.overlaps {
        for (video_id, other_id) in [(&overlap.earlier_video_id, &overlap.later_video_id), (&overlap.later_video_id, &overlap.earlier_video_id)] {
            if warned.insert((video_id.clone(), other_id.clone())) {
                warnings.get_mut(video_id.as_str()).expect("overlaps name clips").push(VideoWarning::Overlap { name: names[other_id.as_str()].clone() });
            }
        }
    }
    for clip in clips {
        let entry = warnings.get_mut(clip.id()).expect("every clip has an entry");
        match (&clip.video.sync_warning, activity_duration_seconds) {
            (Some(warning), _) => entry.push(VideoWarning::Sync(*warning)),
            (None, Some(duration)) if starts_outside_window(clip.offset_seconds(), 0.0, duration) => entry.push(VideoWarning::Far),
            _ => {}
        }
    }
    warnings
}

pub fn format_video_warning(warning: &VideoWarning, locale: Locale) -> String {
    let text = |key: &str, default: &'static str| catalog().t_or(locale, key, default).to_string();
    match warning {
        VideoWarning::Sync(warning) => warning.message(locale),
        VideoWarning::Missing => text("video-preview.warningMissing", "Video file not found. Locate it or remove the video."),
        VideoWarning::Incompatible => text("video-preview.warningIncompatible", "Different resolution or frame rate than the other videos"),
        VideoWarning::Overlap { name } => interpolate(&text("video-preview.warningOverlap", "Overlaps {{name}}"), &[("name", name)]),
        VideoWarning::Far => text("video-preview.warningFarFromActivity", "Starts more than an hour before or after the activity"),
    }
}

/// An issue with the file names of its videos.
pub fn format_combined_export_issue(issue: &super::collection::CombinedExportIssue, clips: &[Clip], locale: Locale) -> String {
    use super::collection::CombinedExportIssueKind;
    let names: Vec<String> = clips.iter().filter(|clip| issue.video_ids.iter().any(|id| id == clip.id())).map(clip_name).collect();
    let (key, default) = match issue.kind {
        CombinedExportIssueKind::Overlap => ("render-video.combinedOverlap", "Videos overlap ({{names}}). Move a video or exclude the overlapping part from export."),
        CombinedExportIssueKind::Incompatible => ("render-video.combinedIncompatible", "Videos differ in resolution or frame rate ({{names}}). Remove them or export separate files."),
        CombinedExportIssueKind::Rotation => (
            "render-video.combinedRotation",
            "Videos are rotated differently ({{names}}), which this encoder cannot join into one file. Export separate files.",
        ),
    };
    interpolate(catalog().t_or(locale, key, default), &[("names", &names.join(", "))])
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;
    use super::super::collection::build_timeline_program;
    use super::super::ImportedVideo;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;

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

    fn named(id: &'static str, path: &'static str, duration_seconds: f64, import_order: u32) -> ImportedVideo {
        video(TestVideo { id, path, duration_seconds, import_order, ..TestVideo::default() })
    }

    fn warnings_for(videos: &[ImportedVideo], offsets: &[(&str, f64)], activity: Option<f64>) -> VideoWarnings {
        let timeline = build_timeline_program(videos, &edits_at(videos, offsets), None, transition_defaults());
        derive_video_warnings(&timeline.clips, &timeline.program, activity)
    }

    #[test]
    fn lists_incompatible_formats_overlaps_on_both_clips_and_sync_warnings() {
        let mut outside = named("outside", "/videos/outside.mp4", 60.0, 2);
        outside.sync_warning = Some(SyncWarning::OutsideActivity);
        let b = video(TestVideo { id: "b", path: "/videos/b.mp4", duration_seconds: 30.0, fps: (25, 1), import_order: 1, ..TestVideo::default() });
        let warnings = warnings_for(&[named("a", "/videos/a.mp4", 30.0, 0), b, outside], &[("a", 0.0), ("b", 20.0), ("outside", 200.0)], None);
        assert_eq!(warnings["a"], [VideoWarning::Overlap { name: "b.mp4".into() }]);
        assert_eq!(warnings["b"], [VideoWarning::Incompatible, VideoWarning::Overlap { name: "a.mp4".into() }]);
        assert_eq!(warnings["outside"], [VideoWarning::Sync(SyncWarning::OutsideActivity)]);
    }

    #[test]
    fn a_clip_overlapping_two_videos_of_one_name_warns_about_each() {
        let videos = [named("long", "/videos/long.mp4", 60.0, 0), named("day1", "/day1/ride.mp4", 20.0, 1), named("day2", "/day2/ride.mp4", 20.0, 2)];
        let warnings = warnings_for(&videos, &[("long", 0.0), ("day1", 10.0), ("day2", 40.0)], None);
        assert_eq!(warnings["long"], [VideoWarning::Overlap { name: "ride.mp4".into() }, VideoWarning::Overlap { name: "ride.mp4".into() }]);
    }

    #[test]
    fn a_clip_without_a_sync_warning_far_from_the_activity_is_warned() {
        let clip = |id: &'static str, order| named(id, "/videos/c.mp4", 0.5, order);
        let mut synced = clip("synced", 4);
        synced.sync_warning = Some(SyncWarning::OutsideActivity);
        let videos = [clip("early", 0), clip("edgeBefore", 1), clip("edgeAfter", 2), clip("late", 3), synced];
        let offsets = [("early", -3601.0), ("edgeBefore", -3600.0), ("edgeAfter", 4600.0), ("late", 4601.0), ("synced", -5000.0)];
        let warnings = warnings_for(&videos, &offsets, Some(1000.0));
        assert_eq!(warnings["early"], [VideoWarning::Far]);
        assert!(warnings["edgeBefore"].is_empty() && warnings["edgeAfter"].is_empty());
        assert_eq!(warnings["late"], [VideoWarning::Far]);
        assert_eq!(warnings["synced"], [VideoWarning::Sync(SyncWarning::OutsideActivity)]);
        assert!(warnings_for(&videos, &offsets, None)["early"].is_empty(), "nothing to be far from without an activity");
    }

    #[test]
    fn an_excluded_video_is_no_incompatible_format() {
        let other = video(TestVideo { id: "other", path: "/videos/other.mp4", duration_seconds: 30.0, fps: (25, 1), import_order: 2, ..TestVideo::default() });
        let videos = [named("a", "/videos/a.mp4", 30.0, 0), named("b", "/videos/b.mp4", 30.0, 1), other];
        let mut edits = edits_at(&videos, &[("b", 100.0), ("other", 200.0)]);
        edits.get_mut("other").unwrap().segments.get_mut("start").unwrap().excluded = true;
        let timeline = build_timeline_program(&videos, &edits, None, transition_defaults());
        assert!(derive_video_warnings(&timeline.clips, &timeline.program, None)["other"].is_empty());
    }

    #[test]
    fn warnings_read_in_the_language() {
        assert_eq!(format_video_warning(&VideoWarning::Overlap { name: "a.mp4".into() }, crate::i18n::DEFAULT_LOCALE), "Overlaps a.mp4");
        assert_eq!(format_video_warning(&VideoWarning::Sync(SyncWarning::OutsideActivity), crate::i18n::DEFAULT_LOCALE), "Video is outside the activity");
    }
}
