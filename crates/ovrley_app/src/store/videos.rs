//! The video collection state: its commit and relink, the background image, placement by
//! recording time, the timeline edits, the clip selection, the drag preview, cut removal, the
//! transition defaults and embedded telemetry.
//!
//! Actions that touch other state (the playhead, the selection) are store functions; each runs
//! as one dispatch, so one undo step.

use std::rc::Rc;
use ovrley_core::encode::ffmpeg::transitions::{transition_defaults, TransitionDefaults};
use ovrley_core::media::edits::{FIRST_SEGMENT_KEY, TimezoneMode, Transition};
use crate::features::player::playback::{timeline_minimum, total_playback_duration};
use crate::features::sync::manual::{clear_landmarks, clear_video_sync_for_activity, fit_landmarks_to_relinked_video};
use crate::store::Store;
use crate::video::{Clip, ImportedVideo, TelemetryStatus, VideoEditsMap, VideoTelemetry};
use crate::video::collection::{join_video_edits, timeline_bounds, VideoDragPreview};
use crate::video::edits::{find_video, segment_before_cut, CollectionDraft, CreationTimeSource};
use crate::video::segments::{fit_cuts_to_frame_count, TrimSide};
use crate::video::sync::SyncActivity;

/// A timeline lane: the activity, or a video's clip. Its id is `activity` or `video:<id>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClipLane {
    Activity,
    Video(String),
}

pub const ACTIVITY_LANE_ID: &str = "activity";

impl ClipLane {

    pub fn id(&self) -> String {
        match self {
            Self::Activity => ACTIVITY_LANE_ID.into(),
            Self::Video(video_id) => format!("video:{video_id}"),
        }
    }

    /// A lane id as Slint reports it: `activity`, or `video:<id>` (only the first prefix is
    /// stripped).
    pub fn from_id(id: &str) -> Self {
        if id == ACTIVITY_LANE_ID {
            Self::Activity
        } else {
            Self::Video(id.strip_prefix("video:").unwrap_or(id).to_string())
        }
    }

    pub fn video_id(&self) -> Option<&str> {
        match self {
            Self::Activity => None,
            Self::Video(video_id) => Some(video_id),
        }
    }
}

/// The clip selected on the timeline or in the video panel, and on a video clip
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
pub struct VideoImportState {
    /// Canonical entities, copy-on-write.
    pub imported_videos: Rc<Vec<ImportedVideo>>,
    /// Undoable edits keyed by video id, copy-on-write.
    pub video_edits: Rc<VideoEditsMap>,
    /// The transient shift of the dragged clip, or of every clip; outside the history.
    pub video_drag_preview: Option<VideoDragPreview>,
    pub cut_removal_request: Option<CutRemovalRequest>,
    pub transition_defaults: TransitionDefaults,
    pub selected_clip: Option<SelectedClip>,
    /// The debug background image, which replaces the videos.
    pub imported_background_image_path: Option<String>,
    /// Detected once, for filmstrip decoding and the render dialog.
    pub available_codecs: crate::features::player::filmstrips::CodecDetection,
}

impl Default for VideoImportState {
    fn default() -> Self {
        Self {
            imported_videos: Rc::default(),
            video_edits: Rc::default(),
            video_drag_preview: None,
            cut_removal_request: None,
            transition_defaults: transition_defaults().clone(),
            selected_clip: None,
            imported_background_image_path: None,
            available_codecs: Default::default(),
        }
    }
}

impl VideoImportState {
    pub fn has_videos(&self) -> bool {
        !self.imported_videos.is_empty()
    }

    /// The committed clips.
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

/// Moves the playhead by `delta_seconds`, kept on the timeline.
pub fn clamp_playhead_to_videos(store: &mut Store, delta_seconds: f64) {
    let bounds = timeline_bounds(&store.video_import.clips());
    let minimum = timeline_minimum(bounds);
    let total = total_playback_duration(store.media.activity_summary.as_ref().map(|summary| summary.duration_seconds), store.editor.fallback_duration_seconds, bounds);
    store.editor.selected_second = (store.editor.selected_second + delta_seconds).max(minimum).min(total);
}

fn selection_on<'a>(store: &'a mut Store, video_id: &str) -> Option<&'a mut SelectedClip> {
    store.video_import.selected_clip.as_mut().filter(|selection| selection.lane.video_id() == Some(video_id))
}

/// A clip, and on a video clip a segment, or nothing.
pub fn select_clip(store: &mut Store, selection: Option<SelectedClip>) -> Result<(), String> {
    if let Some(video_id) = selection.as_ref().and_then(|selection| selection.lane.video_id()) {
        find_video(&store.video_import.imported_videos, video_id)?;
    }
    store.video_import.selected_clip = selection;
    Ok(())
}

/// Selects a whole video clip and, with the playhead outside it, pauses the playhead at the clip's
/// start, so the preview shows the video that was picked.
pub fn select_clip_revealing_playhead(store: &mut Store, video_id: &str) -> Result<(), String> {
    select_clip(store, Some(SelectedClip { lane: ClipLane::Video(video_id.to_string()), segment_key: None }))?;
    let offset = store.video_import.video_edits[video_id].offset_seconds;
    let duration = find_video(&store.video_import.imported_videos, video_id)?.duration_seconds;
    let second = store.editor.selected_second;
    if second < offset || second >= offset + duration {
        store.editor.commit_preview_scrub(offset);
    }
    Ok(())
}

/// Removes a video, without the history rebase (`change_video_collection` wraps it).
/// The playhead stays on the timeline; the clip's selection and cut question go with it, and so do
/// the manual sync's landmarks when they were marked in it.
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
    if store.manual_video_sync.video_id.as_deref() == Some(video_id) {
        clear_landmarks(store);
    }
    Ok(())
}

/// Replaces the collection, without the history rebase (`change_video_collection` wraps it): the
/// collection replaces the current one. The background image, the drag preview, the cut question
/// and a selection of a video that left go; the playhead stays on the timeline; the manual sync's
/// landmarks go when their video left.
pub fn commit_video_collection(store: &mut Store, videos: Vec<ImportedVideo>, edits: VideoEditsMap) {
    let slice = &mut store.video_import;
    slice.imported_videos = Rc::new(videos);
    slice.video_edits = Rc::new(edits);
    slice.imported_background_image_path = None;
    slice.video_drag_preview = None;
    slice.cut_removal_request = None;
    if let Some(video_id) = slice.selected_clip.as_ref().and_then(|selection| selection.lane.video_id()) {
        if find_video(&slice.imported_videos, video_id).is_err() {
            slice.selected_clip = None;
        }
    }
    clamp_playhead_to_videos(store, 0.0);
    let left = store.manual_video_sync.video_id.as_deref().is_some_and(|video_id| find_video(&store.video_import.imported_videos, video_id).is_err());
    if left {
        clear_landmarks(store);
    }
}

/// Relinks a missing video, without the history rebase: a found file replaces it. Its cuts are
/// fitted to the file's frames; manual-sync landmarks past a shorter file's end (all of them gone
/// clears the manual sync) and a selected segment that no longer exists go; the playhead stays on
/// the timeline.
pub fn relink_video(store: &mut Store, video: ImportedVideo) -> Result<(), String> {
    let slice = &mut store.video_import;
    let index = slice.imported_videos.iter().position(|candidate| candidate.id == video.id).filter(|index| slice.imported_videos[*index].missing).ok_or_else(|| format!("Video is not missing: {}", video.id))?;
    let fitted = fit_cuts_to_frame_count(&slice.video_edits[&video.id], video.frame_count);
    fit_landmarks_to_relinked_video(store, &video.id, video.duration_seconds);
    let slice = &mut store.video_import;
    if let Some(selection) = slice.selected_clip.as_mut().filter(|selection| selection.lane.video_id() == Some(video.id.as_str())) {
        if selection.segment_key.as_ref().is_some_and(|key| !fitted.segments.contains_key(key)) {
            selection.segment_key = None;
        }
    }
    Rc::make_mut(&mut slice.video_edits).insert(video.id.clone(), fitted);
    Rc::make_mut(&mut slice.imported_videos)[index] = video;
    slice.video_drag_preview = None;
    clamp_playhead_to_videos(store, 0.0);
    Ok(())
}

fn clear_collection(store: &mut Store, background_image: Option<String>) {
    let slice = &mut store.video_import;
    slice.imported_videos = Rc::default();
    slice.video_edits = Rc::default();
    slice.imported_background_image_path = background_image;
    slice.video_drag_preview = None;
    slice.cut_removal_request = None;
    slice.selected_clip = None;
    clear_landmarks(store);
}

/// Clears the videos, without the history rebase.
pub fn clear_imported_videos(store: &mut Store) {
    clear_collection(store, None);
}

/// Sets the background image, without the history rebase: the image replaces the videos.
pub fn set_imported_background_image(store: &mut Store, path: String) {
    clear_collection(store, Some(path));
}

/// Validated; not in the history.
pub fn set_transition_defaults(store: &mut Store, defaults: TransitionDefaults) -> Result<(), String> {
    defaults.validate()?;
    store.video_import.transition_defaults = defaults;
    Ok(())
}

/// Every automatic clip by its detected time.
pub fn compute_video_sync(store: &mut Store) -> Result<(), String> {
    edit_collection(store, |draft| draft.place_automatic_videos())
}

/// Every clip, also the ones placed by hand.
pub fn sync_all_videos(store: &mut Store) -> Result<(), String> {
    edit_collection(store, |draft| draft.sync_videos())
}

/// "Sync the video"; the playhead stays on the timeline.
pub fn reset_video_offset(store: &mut Store, video_id: &str) -> Result<(), String> {
    edit_collection(store, |draft| draft.reset_video_offset(video_id))?;
    store.video_import.video_drag_preview = None;
    clamp_playhead_to_videos(store, 0.0);
    Ok(())
}

/// Every clip's timezone reading.
pub fn set_video_sync_timezone_mode(store: &mut Store, mode: TimezoneMode) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_videos_timezone_mode(mode))
}

pub fn set_video_timezone_mode(store: &mut Store, video_id: &str, mode: TimezoneMode) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_video_timezone_mode(video_id, mode))
}

/// The clip's recording-time source. The entity's time changes too, but only the offset is in
/// the history.
pub fn set_video_creation_time_source(store: &mut Store, video_id: &str, source: CreationTimeSource) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_video_creation_time_source(video_id, source))
}

pub fn set_video_creation_times_from_filenames(store: &mut Store) -> Result<(), String> {
    edit_collection(store, |draft| draft.apply_filename_creation_times())
}

pub fn reset_video_creation_times(store: &mut Store) -> Result<(), String> {
    edit_collection(store, |draft| draft.reset_creation_times())
}

pub fn set_video_segment_skip_overlay(store: &mut Store, video_id: &str, segment_key: &str, skip_overlay: bool) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_segment_skip_overlay(video_id, segment_key, skip_overlay))
}

/// A settled result, outside the history. A video removed meanwhile is
/// skipped (`false`). A ready result without an external activity sizes the course plots.
pub fn set_video_telemetry(store: &mut Store, video_id: &str, telemetry: VideoTelemetry) -> Result<bool, String> {
    if telemetry.status() == TelemetryStatus::Pending {
        return Err("Video telemetry results must be settled".into());
    }
    let Some(index) = store.video_import.imported_videos.iter().position(|video| video.id == video_id) else {
        return Ok(false);
    };
    store.history.set_mode(crate::features::history::HistoryMode::Skip);
    if let (Some(activity), None) = (telemetry.activity(), &store.media.parsed_activity) {
        crate::features::shell::apply_course_plot_dimensions(store, &activity.activity.sample_course_points);
    }
    Rc::make_mut(&mut store.video_import.imported_videos)[index].telemetry = telemetry;
    Ok(true)
}

/// The placement part of changing the activity: the detected location goes, and the
/// videos are placed against the new activity (or between themselves).
pub fn place_videos_for_activity(store: &mut Store) {
    clear_video_sync_for_activity(store);
    if let Err(error) = compute_video_sync(store) {
        log::error!("video placement: {error}");
    }
}

/// One clip's start by hand. With `compensate_playhead` the playhead moves with
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

/// Every clip by the same amount.
pub fn shift_video_offsets(store: &mut Store, delta_seconds: f64, compensate_playhead: bool) -> Result<(), String> {
    edit_collection(store, |draft| draft.shift_video_offsets(delta_seconds))?;
    store.video_import.video_drag_preview = None;
    if compensate_playhead {
        clamp_playhead_to_videos(store, delta_seconds);
    }
    Ok(())
}

/// Returns the new cut's id.
pub fn cut_video(store: &mut Store, video_id: &str, frame: i64) -> Result<String, String> {
    edit_collection(store, |draft| draft.cut_video(video_id, frame))
}

/// A start trim moves a selected first segment to the part it now starts.
pub fn trim_video(store: &mut Store, video_id: &str, side: TrimSide, frame: i64) -> Result<String, String> {
    let cut_id = edit_collection(store, |draft| draft.trim_video(video_id, side, frame))?;
    if side == TrimSide::Start {
        if let Some(selection) = selection_on(store, video_id).filter(|selection| selection.segment_key.as_deref() == Some(FIRST_SEGMENT_KEY)) {
            selection.segment_key = Some(cut_id.clone());
        }
    }
    Ok(cut_id)
}

pub fn move_cut(store: &mut Store, video_id: &str, cut_id: &str, frame: i64) -> Result<(), String> {
    edit_collection(store, |draft| draft.move_cut(video_id, cut_id, frame))
}

/// Removes a cut; a selected segment that goes with it stays selected as the
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

/// Sides in the same export state merge at once; otherwise the user chooses.
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
    if let Some(state) = state {
        apply_cut_removal(store, video_id, cut_id, state.excluded, state.skip_overlay)?;
        Ok(CutRemoval::Merged)
    } else {
        store.video_import.cut_removal_request = Some(CutRemovalRequest { video_id: video_id.into(), cut_id: cut_id.into() });
        Ok(CutRemoval::Asked)
    }
}

/// Removes the requested cut, the merged part taking the chosen state.
pub fn resolve_cut_removal(store: &mut Store, excluded: bool) -> Result<(), String> {
    let request = store.video_import.cut_removal_request.clone().ok_or("No cut removal is waiting for a choice")?;
    let edits = &store.video_import.video_edits[&request.video_id];
    let skip_overlay = edits.segments[&segment_before_cut(edits, &request.cut_id)?].skip_overlay;
    apply_cut_removal(store, &request.video_id, &request.cut_id, excluded, skip_overlay)?;
    store.video_import.cut_removal_request = None;
    Ok(())
}

/// Keeps the cut.
pub fn dismiss_cut_removal(store: &mut Store) {
    store.video_import.cut_removal_request = None;
}

pub fn set_video_segment_excluded(store: &mut Store, video_id: &str, segment_key: &str, excluded: bool) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_segment_excluded(video_id, segment_key, excluded))
}

/// Overrides the transition of a join (the segment it leads into), or with
/// `None` returns it to the defaults.
pub fn set_join_transition(store: &mut Store, video_id: &str, segment_key: &str, transition: Option<Transition>) -> Result<(), String> {
    edit_collection(store, |draft| draft.set_segment_transition(video_id, segment_key, transition))
}

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
    use crate::features::history::undo;
    use crate::video::fixtures::*;
    use ovrley_core::media::edits::TransitionKind;
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

    fn edits(dispatcher: &Dispatcher) -> ovrley_core::media::edits::VideoEdits {
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

    /// Imports videos as the import dialog does: placed in a draft of the collection, then committed.
    fn import_videos(store: &mut Store, videos: Vec<ImportedVideo>) {
        let activity = store.media.sync_activity();
        let mut all = (*store.video_import.imported_videos).clone();
        let mut edits = (*store.video_import.video_edits).clone();
        CollectionDraft { videos: &mut all, edits: &mut edits, activity: activity.as_ref() }.add_videos(videos).unwrap();
        commit_video_collection(store, all, edits);
    }

    fn timed(id: &'static str, path: &'static str, creation_time: &str, import_order: u32) -> ImportedVideo {
        let mut video = video(TestVideo { id, path, duration_seconds: 20.0, import_order, ..TestVideo::default() });
        video.creation_time = Some(creation_time.into());
        video.time_source = Some("ffprobe".into());
        video
    }

    #[test]
    fn removing_the_clip_that_holds_the_manual_sync_clears_its_landmarks() {
        use ovrley_core::media::sync::LandmarkKind;
        let mut store = Store::default();
        import_videos(&mut store, vec![timed("early", "/videos/early.mp4", "2026-07-18T08:00:00Z", 0)]);
        import_videos(&mut store, vec![timed("late", "/videos/late.mp4", "2026-07-18T08:10:00Z", 1)]);
        store.editor.selected_second = 615.0;
        crate::features::sync::manual::add_landmark(&mut store, "late", LandmarkKind::Stop, 2.0).unwrap();
        remove_imported_video(&mut store, "early").unwrap();
        assert_eq!(store.manual_video_sync.video_id.as_deref(), Some("late"), "another clip leaves the sync alone");
        remove_imported_video(&mut store, "late").unwrap();
        assert_eq!((store.manual_video_sync.video_id.clone(), store.manual_video_sync.landmarks.len()), (None, 0));
        assert!(store.video_import.imported_videos.is_empty());
    }

    #[test]
    fn selecting_a_clip_with_reveal_pauses_the_playhead_at_its_start_only_from_outside() {
        let mut store = store_with(vec![simple("a", 10.0, 0), simple("b", 10.0, 1)], &[]);
        set_video_offset(&mut store, "b", 20.0, false).unwrap();
        store.editor.selected_second = 5.0;
        select_clip_revealing_playhead(&mut store, "b").unwrap();
        assert_eq!(store.editor.selected_second, 20.0);
        assert_eq!(store.video_import.selected_clip, Some(SelectedClip { lane: ClipLane::Video("b".into()), segment_key: None }));
        store.editor.selected_second = 25.0;
        select_clip_revealing_playhead(&mut store, "b").unwrap();
        assert_eq!(store.editor.selected_second, 25.0, "a playhead inside stays");
        assert!(select_clip_revealing_playhead(&mut store, "gone").is_err());
    }

    #[test]
    fn settled_telemetry_stays_on_its_clip_and_late_results_are_discarded() {
        let mut store = Store::default();
        import_videos(&mut store, vec![simple("ride", 10.0, 0)]);
        let activity = crate::store::media::ImportedActivity { activity: std::sync::Arc::new(Default::default()), preview_bytes: std::sync::Arc::new(Vec::new()) };
        assert_eq!(set_video_telemetry(&mut store, "ride", VideoTelemetry::ready(activity.clone())), Ok(true));
        assert_eq!(store.video_import.imported_videos[0].telemetry, VideoTelemetry::ready(activity));
        assert!(store.media.parsed_activity.is_none());
        clear_imported_videos(&mut store);
        assert_eq!(set_video_telemetry(&mut store, "ride", VideoTelemetry::without_activity(TelemetryStatus::Absent).unwrap()), Ok(false));
        assert!(set_video_telemetry(&mut store, "ride", VideoTelemetry::pending()).unwrap_err().contains("settled"));
    }

    #[test]
    fn undo_restores_offsets_of_remaining_clips_and_never_resurrects_removed_ones() {
        let (parts, _) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, _| crate::features::history::change_video_collection(store, &[], |store| import_videos(store, vec![simple("kept", 60.0, 0), video(TestVideo { id: "removed", path: "/videos/removed.mp4", import_order: 1, ..TestVideo::default() })])));
        dispatcher.dispatch(|store, _| set_video_offset(store, "kept", 5.0, false).unwrap());
        dispatcher.dispatch(|store, _| set_video_offset(store, "removed", 50.0, false).unwrap());
        dispatcher.dispatch(|store, _| {
            crate::features::history::change_video_collection(store, &[], |store| {
                remove_imported_video(store, "removed").unwrap();
                import_videos(store, vec![video(TestVideo { id: "added", path: "/videos/added.mp4", import_order: 2, ..TestVideo::default() })]);
                set_video_offset(store, "kept", 7.0, false).unwrap();
            });
        });
        let offsets = |dispatcher: &Dispatcher| dispatcher.with_store(|store| store.video_import.video_edits.iter().map(|(id, edits)| (id.clone(), edits.offset_seconds)).collect::<Vec<_>>());
        // The added clip has no recording time, so it went after the last clip.
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(dispatcher.with_store(|store| store.video_import.imported_videos.iter().map(|video| video.id.clone()).collect::<Vec<_>>()), ["kept", "added"]);
        assert_eq!(offsets(&dispatcher), [("added".to_string(), 65.0), ("kept".to_string(), 5.0)]);
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(offsets(&dispatcher), [("added".to_string(), 65.0), ("kept".to_string(), 0.0)]);
    }

    #[test]
    fn a_commit_clears_the_background_image_and_a_selection_of_a_video_that_left() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        set_imported_background_image(&mut store, "/bg.png".into());
        assert!(store.video_import.imported_videos.is_empty());
        assert_eq!(store.video_import.imported_background_image_path.as_deref(), Some("/bg.png"));
        import_videos(&mut store, vec![simple("a", 10.0, 0)]);
        assert_eq!(store.video_import.imported_background_image_path, None);
        crate::features::sync::manual::add_landmark(&mut store, "a", ovrley_core::media::sync::LandmarkKind::Stop, 1.0).unwrap();
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("a".into()), segment_key: None })).unwrap();
        commit_video_collection(&mut store, Vec::new(), VideoEditsMap::new());
        assert_eq!((store.video_import.selected_clip.clone(), store.manual_video_sync.video_id.clone()), (None, None));
        assert!(store.manual_video_sync.landmarks.is_empty(), "the landmarks left with their video");
    }

    #[test]
    fn relinking_fits_the_cuts_drops_landmarks_past_the_end_and_a_lost_selected_segment() {
        use ovrley_core::media::sync::Landmark;
        let mut gone = simple("ride", 10.0, 0);
        gone.missing = true;
        let mut store = store_with(vec![gone], &[]);
        let cut_id = cut_video(&mut store, "ride", 250).unwrap();
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: Some(cut_id.clone()) })).unwrap();
        store.manual_video_sync.video_id = Some("ride".into());
        store.manual_video_sync.landmarks = vec![Landmark::Stop { id: "in".into(), video_second: 4.0 }, Landmark::Stop { id: "out".into(), video_second: 7.0 }];
        assert!(relink_video(&mut store, simple("other", 5.0, 0)).unwrap_err().contains("not missing"));
        relink_video(&mut store, simple("ride", 5.0, 0)).unwrap();
        let edits = &store.video_import.video_edits["ride"];
        assert!(edits.cuts.is_empty() && !edits.segments.contains_key(&cut_id));
        assert_eq!(store.manual_video_sync.landmarks.iter().map(|landmark| landmark.id()).collect::<Vec<_>>(), ["in"]);
        assert_eq!(store.video_import.selected_clip.as_ref().unwrap().segment_key, None);
        assert!(!store.video_import.imported_videos[0].missing);
        assert!(relink_video(&mut store, simple("ride", 5.0, 0)).is_err(), "only a missing video relinks");
    }

    #[test]
    fn relinking_a_file_shorter_than_every_landmark_clears_the_manual_sync() {
        use ovrley_core::media::sync::Landmark;
        let mut gone = simple("ride", 10.0, 0);
        gone.missing = true;
        let mut store = store_with(vec![gone], &[]);
        store.manual_video_sync.video_id = Some("ride".into());
        store.manual_video_sync.landmarks = vec![Landmark::Stop { id: "out".into(), video_second: 7.0 }];
        relink_video(&mut store, simple("ride", 5.0, 0)).unwrap();
        assert_eq!((store.manual_video_sync.video_id.clone(), store.manual_video_sync.landmarks.len()), (None, 0));
    }

    #[test]
    fn placement_edits_are_one_undo_step_each_and_defaults_are_not_recorded() {
        let (parts, _) = fake_services(json!({}));
        let mut store = Store::default();
        let mut early = video(TestVideo { id: "early", path: "/videos/GX_20260718_080000.mp4", ..TestVideo::default() });
        early.creation_time = Some("2026-07-18T07:00:00Z".into());
        early.time_source = Some("ffprobe".into());
        import_videos(&mut store, vec![early, simple("late", 10.0, 1)]);
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(|store, _| set_video_offset(store, "early", 30.0, false).unwrap());
        dispatcher.dispatch(|store, _| reset_video_offset(store, "early").unwrap());
        dispatcher.dispatch(|store, _| set_video_creation_time_source(store, "early", CreationTimeSource::Filename).unwrap());
        let defaults = TransitionDefaults { duration_seconds: 2.0, ..transition_defaults().clone() };
        dispatcher.dispatch(move |store, _| set_transition_defaults(store, defaults).unwrap());
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2, "the offset and the reset; the time source keeps the offset and defaults are not recorded");
        assert!(dispatcher.dispatch_with_result(|store, _| set_transition_defaults(store, TransitionDefaults { duration_seconds: 0.0, ..transition_defaults().clone() })).unwrap().is_err());
    }

}
