//! Cut, exclude and trim at the playhead. The target clip is the selected one, else the one the
//! preview shows, else the earliest under the playhead; exclusion applies to the selected segment,
//! else the segment at the playhead.

use crate::features::player::playback::player_timing;
use crate::store::Store;
use crate::store::videos::{cut_video, move_cut, set_video_segment_excluded, trim_video};
use crate::video::Clip;
use crate::video::program::resolve_preview_target;
use crate::video::segments::{find_segment_at, list_video_segments, plan_video_trim, resolve_cut_frame, resolve_edit_clip, Segment, TrimPlan, TrimSide};

/// The clip commands at the playhead apply to and the clip-local second.
pub fn resolve_edit_target(store: &Store) -> Option<(&Clip, f64)> {
    let committed = &store.player.timeline.timeline().committed;
    let second = player_timing(store).clamped_playhead;
    let preview = resolve_preview_target(&committed.program, second).map(|target| target.video_id);
    let selected = store.video_import.selected_clip.as_ref().and_then(|selection| selection.lane.video_id());
    let clip = resolve_edit_clip(&committed.clips, preview, selected, second)?;
    Some((clip, second - clip.offset_seconds()))
}

/// The selected clip's selected segment, if it still exists.
pub fn resolve_selected_segment(store: &Store) -> Option<(&Clip, Segment)> {
    let selection = store.video_import.selected_clip.as_ref()?;
    let key = selection.segment_key.as_deref()?;
    let clip = store.player.timeline.timeline().committed.clips.iter().find(|clip| Some(clip.id()) == selection.lane.video_id())?;
    let segment = list_video_segments(clip).into_iter().find(|segment| segment.key == key)?;
    Some((clip, segment))
}

fn can_trim(store: &Store, side: TrimSide) -> bool {
    resolve_edit_target(store).is_some_and(|(clip, second)| plan_video_trim(clip, side, clip.fps().seconds_to_frames(second)).is_some())
}

/// The toolbar's state of the commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoEditState {
    pub can_cut: bool,
    /// The export state of the segment exclusion applies to; `None` without one.
    pub excluded: Option<bool>,
    pub can_trim_start: bool,
    pub can_trim_end: bool,
}

pub fn video_edit_state(store: &Store) -> VideoEditState {
    let target = resolve_edit_target(store);
    VideoEditState {
        can_cut: target.is_some_and(|(clip, second)| resolve_cut_frame(clip, second).is_some()),
        excluded: resolve_selected_segment(store)
            .map(|(_, segment)| segment.edits.excluded)
            .or_else(|| target.map(|(clip, second)| find_segment_at(clip, second).edits.excluded)),
        can_trim_start: can_trim(store, TrimSide::Start),
        can_trim_end: can_trim(store, TrimSide::End),
    }
}

/// Cuts the target clip at the playhead frame, unless that frame is an edge or a cut.
pub fn cut_at_playhead(store: &mut Store) -> bool {
    let Some((clip, second)) = resolve_edit_target(store) else { return false };
    let Some(frame) = resolve_cut_frame(clip, second) else { return false };
    let video_id = clip.id().to_string();
    cut_video(store, &video_id, frame).is_ok()
}

/// Toggles the exclusion of the selected segment, else of the segment at the playhead.
pub fn toggle_excluded_at_playhead(store: &mut Store) -> bool {
    let target = resolve_selected_segment(store)
        .map(|(clip, segment)| (clip.id().to_string(), segment))
        .or_else(|| resolve_edit_target(store).map(|(clip, second)| (clip.id().to_string(), find_segment_at(clip, second))));
    let Some((video_id, segment)) = target else { return false };
    set_video_segment_excluded(store, &video_id, &segment.key, !segment.edits.excluded).is_ok()
}

/// Trims an end of the target clip at the playhead.
pub fn trim_at_playhead(store: &mut Store, side: TrimSide) -> bool {
    let Some((clip, second)) = resolve_edit_target(store) else { return false };
    let (video_id, frame) = (clip.id().to_string(), clip.fps().seconds_to_frames(second));
    trim_clip(store, &video_id, side, frame).is_ok()
}

/// Trims an end of a clip at a frame: a new trim, or the end's trim moved.
///
/// # Errors
///
/// When the clip is not on the timeline or the frame cannot trim that end.
pub fn trim_clip(store: &mut Store, video_id: &str, side: TrimSide, frame: i64) -> Result<(), String> {
    let clips = &store.player.timeline.timeline().committed.clips;
    let clip = clips.iter().find(|clip| clip.id() == video_id).ok_or_else(|| format!("There is no video {video_id}."))?;
    let plan = plan_video_trim(clip, side, frame).ok_or("The clip cannot be trimmed there: the frame is on the clip's edge or past a cut.")?;
    match plan {
        TrimPlan::Trim => trim_video(store, video_id, side, frame).map(drop),
        TrimPlan::Move { cut_id } => move_cut(store, video_id, &cut_id, frame),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::player::reconcile_player;
    use crate::video::fixtures::simple;
    use crate::store::videos::{select_clip, tests::store_with, ClipLane, SelectedClip};

    fn store() -> Store {
        let mut store = store_with(vec![simple("a", 10.0, 0), simple("b", 10.0, 1)], &[("b", 5.0)]);
        reconcile_player(&mut store);
        store
    }

    #[test]
    fn commands_apply_to_the_clip_at_the_playhead_or_the_selected_one() {
        let mut store = store();
        store.editor.selected_second = 7.0;
        assert_eq!(resolve_edit_target(&store).map(|(clip, second)| (clip.id().to_string(), second)), Some(("a".into(), 7.0)));
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("b".into()), segment_key: None })).unwrap();
        assert_eq!(resolve_edit_target(&store).map(|(clip, second)| (clip.id().to_string(), second)), Some(("b".into(), 2.0)));
        assert!(cut_at_playhead(&mut store));
        reconcile_player(&mut store);
        assert_eq!(store.video_import.video_edits["b"].cuts[0].frame, 60);
        assert!(!video_edit_state(&store).can_cut, "the playhead sits on the cut");
    }

    #[test]
    fn exclusion_toggles_the_selected_segment_or_the_one_at_the_playhead() {
        let mut store = store();
        store.editor.selected_second = 2.0;
        assert_eq!(video_edit_state(&store).excluded, Some(false));
        assert!(toggle_excluded_at_playhead(&mut store));
        reconcile_player(&mut store);
        assert!(store.video_import.video_edits["a"].segments["start"].excluded);
        assert_eq!(video_edit_state(&store).excluded, Some(true));
    }

    #[test]
    fn trims_at_the_playhead_then_move_their_cut() {
        let mut store = store();
        store.editor.selected_second = 2.0;
        assert!(video_edit_state(&store).can_trim_start);
        assert!(trim_at_playhead(&mut store, TrimSide::Start));
        reconcile_player(&mut store);
        assert_eq!(store.video_import.video_edits["a"].cuts[0].frame, 60);
        store.editor.selected_second = 3.0;
        assert!(trim_at_playhead(&mut store, TrimSide::Start), "a trimmed start moves its trim cut");
        assert_eq!(store.video_import.video_edits["a"].cuts.len(), 1);
        assert_eq!(store.video_import.video_edits["a"].cuts[0].frame, 90);
    }
}
