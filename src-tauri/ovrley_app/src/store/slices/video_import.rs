//! Port of the player's half of `store/slices/createVideoImportSlice.js` (plan D3): the collection
//! state, the timeline edits, the clip selection, the drag preview and cut removal.
//!
//! The import (`commitVideoCollection`), relinking, the background image, telemetry, placement by
//! recording time and the per-segment overlay switch arrive with Phase 6.
//!
//! Actions that touch other slices (the playhead, the selection) are store functions; each runs
//! as one dispatch, so one undo step.

use crate::features::player::playback::{timeline_minimum, total_playback_duration};
use crate::lib::utils::clamp;
use crate::lib::video_program::collection::{join_video_edits, timeline_bounds, VideoDragPreview};
use crate::lib::video_program::edits::{find_video, segment_before_cut, CollectionDraft};
use crate::lib::video_program::segments::TrimSide;
use crate::lib::video_program::{Clip, ImportedVideo, VideoEditsMap};
use ovrley_core::media::video_edits::{Transition, FIRST_SEGMENT_KEY};
use crate::lib::video_sync::SyncActivity;
use crate::store::Store;
use ovrley_core::encode::ffmpeg::transitions::{transition_defaults, TransitionDefaults};
use std::rc::Rc;

/// A timeline lane: the activity, or a video's clip. Its id is `activity` or `video:<id>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClipLane {
    Activity,
    Video(String),
}

/// `ACTIVITY_LANE_ID`.
pub const ACTIVITY_LANE_ID: &str = "activity";

impl ClipLane {
    /// `videoLaneId` / `ACTIVITY_LANE_ID`.
    pub fn id(&self) -> String {
        match self {
            Self::Activity => ACTIVITY_LANE_ID.into(),
            Self::Video(video_id) => format!("video:{video_id}"),
        }
    }

    /// A lane id as Slint reports it: `activity`, or `video:<id>` (`videoIdFromLaneId`, which
    /// strips only the first prefix).
    pub fn from_id(id: &str) -> Self {
        if id == ACTIVITY_LANE_ID {
            Self::Activity
        } else {
            Self::Video(id.strip_prefix("video:").unwrap_or(id).to_string())
        }
    }

    /// `videoIdFromLaneId`.
    pub fn video_id(&self) -> Option<&str> {
        match self {
            Self::Activity => None,
            Self::Video(video_id) => Some(video_id),
        }
    }
}

/// The clip selected on the timeline (or, from Phase 6, in the video panel), and on a video clip
/// the segment under the pointer (`None` for the whole clip).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedClip {
    pub lane: ClipLane,
    pub segment_key: Option<String>,
}

/// A cut whose removal waits for the user's choice of the merged part's export state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutRemovalRequest {
    pub video_id: String,
    pub cut_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoImportSlice {
    /// Canonical entities, copy-on-write (D4).
    pub imported_videos: Rc<Vec<ImportedVideo>>,
    /// Undoable edits keyed by video id, copy-on-write.
    pub video_edits: Rc<VideoEditsMap>,
    /// The transient shift of the dragged clip, or of every clip; outside the history.
    pub video_drag_preview: Option<VideoDragPreview>,
    pub cut_removal_request: Option<CutRemovalRequest>,
    pub transition_defaults: TransitionDefaults,
    pub selected_clip: Option<SelectedClip>,
    /// `availableCodecs`: detected once, for filmstrip decoding (and the render dialog in Phase 7).
    pub available_codecs: crate::features::video_preview::filmstrips::CodecDetection,
}

impl Default for VideoImportSlice {
    fn default() -> Self {
        Self {
            imported_videos: Rc::default(),
            video_edits: Rc::default(),
            video_drag_preview: None,
            cut_removal_request: None,
            transition_defaults: transition_defaults().clone(),
            selected_clip: None,
            available_codecs: Default::default(),
        }
    }
}

impl VideoImportSlice {
    pub fn has_videos(&self) -> bool {
        !self.imported_videos.is_empty()
    }

    /// The committed clips (`joinVideoEdits`).
    pub fn clips(&self) -> Vec<Clip> {
        join_video_edits(&self.imported_videos, &self.video_edits)
    }

    /// Applies an edit to copies of the collection, placed against `activity`, and keeps them only
    /// when it succeeds.
    pub fn edit_collection<R>(&mut self, activity: Option<&SyncActivity>, edit: impl FnOnce(&mut CollectionDraft) -> Result<R, String>) -> Result<R, String> {
        let mut videos = (*self.imported_videos).clone();
        let mut edits = (*self.video_edits).clone();
        let result = edit(&mut CollectionDraft { videos: &mut videos, edits: &mut edits, activity })?;
        if videos != *self.imported_videos {
            self.imported_videos = Rc::new(videos);
        }
        if edits != *self.video_edits {
            self.video_edits = Rc::new(edits);
        }
        Ok(result)
    }
}

/// Applies an edit to the store's collection, placed against the store's activity.
pub fn edit_collection<R>(store: &mut Store, edit: impl FnOnce(&mut CollectionDraft) -> Result<R, String>) -> Result<R, String> {
    let activity = store.media.sync_activity();
    store.video_import.edit_collection(activity.as_ref(), edit)
}

/// `clampPlayheadToVideos`: moves the playhead by `delta_seconds`, kept on the timeline.
pub fn clamp_playhead_to_videos(store: &mut Store, delta_seconds: f64) {
    let bounds = timeline_bounds(&store.video_import.clips());
    let minimum = timeline_minimum(bounds);
    let total = total_playback_duration(store.media.activity_summary.as_ref().map(|summary| summary.duration_seconds), store.editor.fallback_duration_seconds, bounds);
    store.editor.selected_second = clamp(store.editor.selected_second + delta_seconds, minimum, total);
}

fn selection_on<'a>(store: &'a mut Store, video_id: &str) -> Option<&'a mut SelectedClip> {
    store.video_import.selected_clip.as_mut().filter(|selection| selection.lane.video_id() == Some(video_id))
}

/// `selectClip`: a clip, and on a video clip a segment, or nothing.
pub fn select_clip(store: &mut Store, selection: Option<SelectedClip>) -> Result<(), String> {
    if let Some(video_id) = selection.as_ref().and_then(|selection| selection.lane.video_id()) {
        find_video(&store.video_import.imported_videos, video_id)?;
    }
    store.video_import.selected_clip = selection;
    Ok(())
}

/// `removeImportedVideo`, without the history rebase (`change_video_collection` wraps it, D17).
/// The playhead stays on the timeline; the clip's selection and cut question go with it.
pub fn remove_imported_video(store: &mut Store, video_id: &str) -> Result<(), String> {
    edit_collection(store, |draft| draft.remove_video(video_id))?;
    store.video_import.video_drag_preview = None;
    if store.video_import.cut_removal_request.as_ref().is_some_and(|request| request.video_id == video_id) {
        store.video_import.cut_removal_request = None;
    }
    if selection_on(store, video_id).is_some() {
        store.video_import.selected_clip = None;
    }
    clamp_playhead_to_videos(store, 0.0);
    Ok(())
}

/// `setVideoOffset`: one clip's start by hand. With `compensate_playhead` the playhead moves with
/// the clip, as a dragged clip carries it.
pub fn set_video_offset(store: &mut Store, video_id: &str, seconds: f64, compensate_playhead: bool) -> Result<(), String> {
    let previous = store.video_import.video_edits.get(video_id).map_or(0.0, |edits| edits.offset_seconds);
    edit_collection(store, |draft| draft.set_video_offset(video_id, seconds))?;
    store.video_import.video_drag_preview = None;
    if compensate_playhead {
        clamp_playhead_to_videos(store, seconds - previous);
    }
    Ok(())
}

/// `shiftVideoOffsets`: every clip by the same amount.
pub fn shift_video_offsets(store: &mut Store, delta_seconds: f64, compensate_playhead: bool) -> Result<(), String> {
    edit_collection(store, |draft| draft.shift_video_offsets(delta_seconds))?;
    store.video_import.video_drag_preview = None;
    if compensate_playhead {
        clamp_playhead_to_videos(store, delta_seconds);
    }
    Ok(())
}

/// `cutVideo`. Returns the new cut's id.
pub fn cut_video(store: &mut Store, video_id: &str, frame: i64) -> Result<String, String> {
    edit_collection(store, |draft| draft.cut_video(video_id, frame))
}

/// `trimVideo`: a start trim moves a selected first segment to the part it now starts.
pub fn trim_video(store: &mut Store, video_id: &str, side: TrimSide, frame: i64) -> Result<String, String> {
    let cut_id = edit_collection(store, |draft| draft.trim_video(video_id, side, frame))?;
    if side == TrimSide::Start {
        if let Some(selection) = selection_on(store, video_id).filter(|selection| selection.segment_key.as_deref() == Some(FIRST_SEGMENT_KEY)) {
            selection.segment_key = Some(cut_id.clone());
        }
    }
    Ok(cut_id)
}

/// `moveCut`.
pub fn move_cut(store: &mut Store, video_id: &str, cut_id: &str, frame: i64) -> Result<(), String> {
    edit_collection(store, |draft| draft.move_cut(video_id, cut_id, frame))
}

/// `applyCutRemoval`: removes a cut; a selected segment that goes with it stays selected as the
/// merged segment.
fn apply_cut_removal(store: &mut Store, video_id: &str, cut_id: &str, excluded: bool, skip_overlay: bool) -> Result<(), String> {
    let edits = store.video_import.video_edits.get(video_id).ok_or_else(|| format!("Unknown imported video: {video_id}"))?;
    let merged_key = segment_before_cut(edits, cut_id)?;
    edit_collection(store, |draft| draft.remove_cut(video_id, cut_id, excluded, skip_overlay))?;
    if let Some(selection) = selection_on(store, video_id).filter(|selection| selection.segment_key.as_deref() == Some(cut_id)) {
        selection.segment_key = Some(merged_key);
    }
    Ok(())
}

/// What `request_cut_removal` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutRemoval {
    /// The cut is gone.
    Merged,
    /// The two sides differ: the dialog asks the user (`cut_removal_request`).
    Asked,
}

/// `requestCutRemoval`: sides in the same export state merge at once; otherwise the user chooses.
/// With `keep`, the merged part takes that segment's state and nothing is asked (a first or last
/// cut dropped on its edge keeps the inner part).
pub fn request_cut_removal(store: &mut Store, video_id: &str, cut_id: &str, keep: Option<&str>) -> Result<CutRemoval, String> {
    let edits = store.video_import.video_edits.get(video_id).ok_or_else(|| format!("Unknown imported video: {video_id}"))?;
    let earlier = edits.segments[&segment_before_cut(edits, cut_id)?];
    let state = match keep {
        Some(key) => Some(*edits.segments.get(key).ok_or_else(|| format!("Unknown video segment: {key}"))?),
        None if earlier.excluded == edits.segments[cut_id].excluded => Some(earlier),
        None => None,
    };
    match state {
        Some(state) => {
            apply_cut_removal(store, video_id, cut_id, state.excluded, state.skip_overlay)?;
            Ok(CutRemoval::Merged)
        }
        None => {
            store.video_import.cut_removal_request = Some(CutRemovalRequest { video_id: video_id.into(), cut_id: cut_id.into() });
            Ok(CutRemoval::Asked)
        }
    }
}

/// `resolveCutRemoval`: removes the requested cut, the merged part taking the chosen state.
pub fn resolve_cut_removal(store: &mut Store, excluded: bool) -> Result<(), String> {
    let request = store.video_import.cut_removal_request.clone().ok_or("No cut removal is waiting for a choice")?;
    let edits = &store.video_import.video_edits[&request.video_id];
    let skip_overlay = edits.segments[&segment_before_cut(edits, &request.cut_id)?].skip_overlay;
    apply_cut_removal(store, &request.video_id, &request.cut_id, excluded, skip_overlay)?;
    store.video_import.cut_removal_request = None;
    Ok(())
}

/// `dismissCutRemoval`: keeps the cut.
pub fn dismiss_cut_removal(store: &mut Store) {
    store.video_import.cut_removal_request = None;
}

/// `setVideoSegmentExcluded`.
pub fn set_video_segment_excluded(store: &mut Store, video_id: &str, segment_key: &str, excluded: bool) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_segment_excluded(video_id, segment_key, excluded))
}

/// `setJoinTransition`: overrides the transition of a join (the segment it leads into), or with
/// `None` returns it to the defaults.
pub fn set_join_transition(store: &mut Store, video_id: &str, segment_key: &str, transition: Option<Transition>) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_segment_transition(video_id, segment_key, transition))
}

/// `setVideoDragPreview`.
pub fn set_video_drag_preview(store: &mut Store, preview: Option<VideoDragPreview>) -> Result<(), String> {
    if let Some(preview) = &preview {
        if let Some(video_id) = &preview.video_id {
            find_video(&store.video_import.imported_videos, video_id)?;
        }
        if !preview.delta_seconds.is_finite() {
            return Err("Video drag shift must be a finite number".into());
        }
    }
    store.video_import.video_drag_preview = preview;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::features::undo_redo::undo;
    use crate::lib::video_program::fixtures::*;
    use ovrley_core::media::video_edits::TransitionKind;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use serde_json::json;

    /// A store holding videos at offsets, outside the history.
    pub(crate) fn store_with(videos: Vec<ImportedVideo>, offsets: &[(&str, f64)]) -> Store {
        let mut store = Store::default();
        store.video_import.video_edits = Rc::new(edits_at(&videos, offsets));
        store.video_import.imported_videos = Rc::new(videos);
        store
    }

    fn dispatcher() -> Dispatcher {
        let (parts, _) = fake_services(json!({}));
        Dispatcher::new(store_with(vec![simple("ride", 10.0, 0)], &[]), parts)
    }

    fn edits(dispatcher: &Dispatcher) -> ovrley_core::media::video_edits::VideoEdits {
        dispatcher.with_store(|store| store.video_import.video_edits["ride"].clone())
    }

    fn fade() -> Transition {
        Transition::new(TransitionKind::from_name("fade").unwrap(), 1.0).unwrap()
    }

    #[test]
    fn a_cut_an_exclusion_and_a_transition_are_one_undo_step_each() {
        let dispatcher = dispatcher();
        let cut_id = dispatcher.dispatch_with_result(|store, _| cut_video(store, "ride", 150).unwrap()).unwrap();
        let cut = cut_id.clone();
        dispatcher.dispatch(move |store, _| set_video_segment_excluded(store, "ride", &cut, true).unwrap());
        let cut = cut_id.clone();
        dispatcher.dispatch(move |store, _| set_join_transition(store, "ride", &cut, Some(fade())).unwrap());
        assert_eq!(edits(&dispatcher).segments[&cut_id].transition_in, Some(fade()));
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(edits(&dispatcher).segments[&cut_id].transition_in, None);
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert!(!edits(&dispatcher).segments[&cut_id].excluded);
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert!(edits(&dispatcher).cuts.is_empty());
        assert!(!edits(&dispatcher).segments.contains_key(&cut_id));
    }

    #[test]
    fn lane_ids_name_the_activity_or_a_video() {
        assert_eq!(ClipLane::from_id("activity"), ClipLane::Activity);
        assert_eq!(ClipLane::from_id("video:clip-a").video_id(), Some("clip-a"));
        assert_eq!(ClipLane::from_id("video:video:nested").video_id(), Some("video:nested"));
        assert_eq!(ClipLane::Video("a".into()).id(), "video:a");
    }

    #[test]
    fn a_cut_between_alike_segments_merges_without_asking() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        let cut_id = cut_video(&mut store, "ride", 150).unwrap();
        assert_eq!(request_cut_removal(&mut store, "ride", &cut_id, None), Ok(CutRemoval::Merged));
        assert_eq!(store.video_import.cut_removal_request, None);
        assert!(store.video_import.video_edits["ride"].cuts.is_empty());
        assert_eq!(store.video_import.video_edits["ride"].segments.len(), 1);
    }

    #[test]
    fn a_cut_between_different_states_asks_first_and_applies_the_choice() {
        for choice in [true, false] {
            let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
            let cut_id = cut_video(&mut store, "ride", 150).unwrap();
            set_video_segment_excluded(&mut store, "ride", &cut_id, true).unwrap();
            assert_eq!(request_cut_removal(&mut store, "ride", &cut_id, None), Ok(CutRemoval::Asked));
            assert_eq!(store.video_import.cut_removal_request, Some(CutRemovalRequest { video_id: "ride".into(), cut_id: cut_id.clone() }));
            assert_eq!(store.video_import.video_edits["ride"].cuts.len(), 1);
            resolve_cut_removal(&mut store, choice).unwrap();
            assert_eq!(store.video_import.cut_removal_request, None);
            assert!(store.video_import.video_edits["ride"].cuts.is_empty());
            assert_eq!(store.video_import.video_edits["ride"].segments["start"].excluded, choice);
        }
    }

    #[test]
    fn a_kept_segment_decides_without_asking() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        let cut_id = cut_video(&mut store, "ride", 150).unwrap();
        set_video_segment_excluded(&mut store, "ride", &cut_id, true).unwrap();
        assert_eq!(request_cut_removal(&mut store, "ride", &cut_id, Some(&cut_id)), Ok(CutRemoval::Merged));
        assert!(store.video_import.video_edits["ride"].segments["start"].excluded);
    }

    #[test]
    fn a_start_trim_moves_a_selected_first_segment() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: Some("start".into()) })).unwrap();
        let cut_id = trim_video(&mut store, "ride", TrimSide::Start, 30).unwrap();
        assert_eq!(store.video_import.selected_clip, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: Some(cut_id) }));
        assert!(select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("gone".into()), segment_key: None })).is_err());
    }

    #[test]
    fn a_merged_selected_segment_stays_selected() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        let cut_id = cut_video(&mut store, "ride", 150).unwrap();
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: Some(cut_id.clone()) })).unwrap();
        request_cut_removal(&mut store, "ride", &cut_id, None).unwrap();
        assert_eq!(store.video_import.selected_clip.unwrap().segment_key.as_deref(), Some("start"));
    }

    #[test]
    fn keeping_the_cut_changes_nothing_and_removing_the_video_ends_the_question() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        let cut_id = cut_video(&mut store, "ride", 150).unwrap();
        set_video_segment_excluded(&mut store, "ride", &cut_id, true).unwrap();
        request_cut_removal(&mut store, "ride", &cut_id, None).unwrap();
        dismiss_cut_removal(&mut store);
        assert_eq!(store.video_import.cut_removal_request, None);
        assert_eq!(store.video_import.video_edits["ride"].cuts.len(), 1);
        request_cut_removal(&mut store, "ride", &cut_id, None).unwrap();
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: None })).unwrap();
        remove_imported_video(&mut store, "ride").unwrap();
        assert_eq!((store.video_import.cut_removal_request.clone(), store.video_import.selected_clip.clone()), (None, None));
    }

    #[test]
    fn removing_a_clip_keeps_the_playhead_on_the_timeline() {
        let mut store = store_with(vec![simple("early", 20.0, 0), simple("late", 20.0, 1)], &[("late", 600.0)]);
        store.editor.selected_second = 615.0;
        remove_imported_video(&mut store, "late").unwrap();
        assert_eq!(store.editor.selected_second, 20.0);
    }

    #[test]
    fn a_compensated_offset_carries_the_playhead() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        store.editor.selected_second = 5.0;
        set_video_offset(&mut store, "ride", 3.0, true).unwrap();
        assert_eq!(store.editor.selected_second, 8.0);
        shift_video_offsets(&mut store, -1.0, false).unwrap();
        assert_eq!((store.video_import.video_edits["ride"].offset_seconds, store.editor.selected_second), (2.0, 8.0));
    }

    #[test]
    fn drag_previews_name_known_clips_and_finite_shifts() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        assert!(set_video_drag_preview(&mut store, Some(VideoDragPreview { video_id: Some("gone".into()), delta_seconds: 1.0 })).is_err());
        assert!(set_video_drag_preview(&mut store, Some(VideoDragPreview { video_id: None, delta_seconds: f64::NAN })).is_err());
        set_video_drag_preview(&mut store, Some(VideoDragPreview { video_id: Some("ride".into()), delta_seconds: 2.0 })).unwrap();
        set_video_offset(&mut store, "ride", 1.0, false).unwrap();
        assert_eq!(store.video_import.video_drag_preview, None, "a committed offset ends the preview");
    }
}
