//! Port of `video-warnings.js`: per-clip warnings shown on the timeline (and the video panel in
//! Phase 6).

use super::collection::{derive_compatibility, starts_outside_window};
use super::program::Program;
use super::segments::is_video_excluded;
use super::Clip;
use crate::i18n::{catalog, interpolate, Locale};
use crate::lib::video_sync::SyncWarning;
use crate::lib::file_dialog::filename_from_selected_path;
use std::collections::{BTreeMap, BTreeSet};

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

/// `deriveVideoWarnings`. A clip excluded from export does not take part in the format comparison.
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

/// `formatVideoWarning`.
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

/// `formatCombinedExportIssue`: an issue with the file names of its videos.
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
    use super::super::collection::build_timeline_program;
    use super::super::fixtures::*;
    use super::super::ImportedVideo;
    use super::*;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;

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
