//! Port of `lib/video-program/timeline.js`'s `selectVideoTimeline` and `hooks/useVideoTimeline.js`
//! (plan D5): the committed clips, program and bounds, the same as displayed during a clip drag,
//! and the clip warnings, cached on the pointers of their inputs.
//!
//! The reconcile step refreshes the cache after every action, before the projections, so every
//! reader after a dispatch sees the current timeline. An action that changes the collection reads
//! the slice itself.

use crate::lib::video_program::collection::{build_timeline_program, TimelineProgram, VideoDragPreview};
use crate::lib::video_program::warnings::{derive_video_warnings, VideoWarnings};
use crate::lib::video_program::{ImportedVideo, VideoEditsMap};
use crate::store::Store;
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use std::rc::Rc;

/// The derived timeline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoTimeline {
    pub committed: Rc<TimelineProgram>,
    /// The committed timeline itself without a drag preview.
    pub displayed: Rc<TimelineProgram>,
    /// `useVideoWarnings`: of the committed clips.
    pub warnings: Rc<VideoWarnings>,
}

#[derive(Clone, Debug)]
struct Inputs {
    videos: Rc<Vec<ImportedVideo>>,
    edits: Rc<VideoEditsMap>,
    defaults: TransitionDefaults,
    preview: Option<VideoDragPreview>,
    /// The external activity's duration (`null` without one), which the warnings read.
    activity_duration_seconds: Option<f64>,
}

/// `memoizeLatest` for the three builds.
#[derive(Clone, Debug, Default)]
pub struct TimelineCache {
    inputs: Option<Inputs>,
    timeline: VideoTimeline,
}

impl TimelineCache {
    pub fn timeline(&self) -> &VideoTimeline {
        &self.timeline
    }
}

fn inputs_of(store: &Store) -> Inputs {
    Inputs {
        videos: Rc::clone(&store.video_import.imported_videos),
        edits: Rc::clone(&store.video_import.video_edits),
        defaults: store.video_import.transition_defaults.clone(),
        preview: store.video_import.video_drag_preview.clone(),
        activity_duration_seconds: store.media.parsed_activity.as_ref().and(store.media.activity_summary.as_ref()).map(|summary| summary.duration_seconds),
    }
}

/// Rebuilds what changed: the committed timeline on the collection or the defaults, the displayed
/// one also on the drag preview, the warnings also on the activity's duration.
pub fn reconcile_timeline(store: &mut Store) {
    let next = inputs_of(store);
    let cache = &mut store.player.timeline;
    let previous = cache.inputs.as_ref();
    let committed_changed =
        previous.is_none_or(|previous| !Rc::ptr_eq(&previous.videos, &next.videos) || !Rc::ptr_eq(&previous.edits, &next.edits) || previous.defaults != next.defaults);
    let displayed_changed = committed_changed || previous.is_some_and(|previous| previous.preview != next.preview);
    let warnings_changed = committed_changed || previous.is_some_and(|previous| previous.activity_duration_seconds != next.activity_duration_seconds);
    if committed_changed {
        cache.timeline.committed = Rc::new(build_timeline_program(&next.videos, &next.edits, None, &next.defaults));
    }
    if displayed_changed {
        cache.timeline.displayed = match &next.preview {
            None => Rc::clone(&cache.timeline.committed),
            Some(preview) => Rc::new(build_timeline_program(&next.videos, &next.edits, Some(preview), &next.defaults)),
        };
    }
    if warnings_changed {
        let committed = &cache.timeline.committed;
        cache.timeline.warnings = Rc::new(derive_video_warnings(&committed.clips, &committed.program, next.activity_duration_seconds));
    }
    cache.inputs = Some(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::video_program::fixtures::simple;
    use crate::store::slices::video_import::tests::store_with;
    use crate::store::slices::video_import::{set_video_drag_preview, set_video_offset};

    #[test]
    fn one_build_per_change_and_the_displayed_timeline_follows_the_drag() {
        let mut store = store_with(vec![simple("a", 10.0, 0), simple("b", 10.0, 1)], &[("b", 30.0)]);
        reconcile_timeline(&mut store);
        let first = Rc::clone(&store.player.timeline.timeline().committed);
        assert!(Rc::ptr_eq(&first, &store.player.timeline.timeline().displayed), "without a drag both are one");
        store.editor.selected_second = 4.0;
        reconcile_timeline(&mut store);
        assert!(Rc::ptr_eq(&first, &store.player.timeline.timeline().committed), "an unrelated change rebuilds nothing");

        set_video_drag_preview(&mut store, Some(VideoDragPreview { video_id: Some("b".into()), delta_seconds: -5.0 })).unwrap();
        reconcile_timeline(&mut store);
        let timeline = store.player.timeline.timeline();
        assert!(Rc::ptr_eq(&first, &timeline.committed));
        assert_eq!(timeline.displayed.clips[1].offset_seconds(), 25.0);

        set_video_offset(&mut store, "b", 25.0, false).unwrap();
        reconcile_timeline(&mut store);
        let timeline = store.player.timeline.timeline();
        assert!(!Rc::ptr_eq(&first, &timeline.committed));
        assert!(Rc::ptr_eq(&timeline.committed, &timeline.displayed));
        assert_eq!(timeline.committed.bounds.unwrap().end_second, 35.0);
        assert!(timeline.warnings["a"].is_empty());
    }
}
