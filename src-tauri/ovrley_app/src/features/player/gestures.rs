//! Port of `useTimelineGestures` (plan D9): scrubbing on the axis, dragging the playhead, and
//! panning on the lane background. Clip, cut and trim drags join here.
//!
//! Slint reports each press, move and release with its x in timeline coordinates; the gesture state
//! lives here, as the hook's refs do in React. A pressed `TouchArea` keeps the pointer, which stands
//! in for pointer capture.

use super::playback::{commit_scrub, player_timing, scrub_to};
use super::viewport::pan_by;
use crate::lib::activity::{resolve_activity_window, trim_activity_edge, ActivityTrim, TrimEdge};
use crate::lib::utils::clamp;
use crate::store::Store;

/// `DRAG_THRESHOLD_PX`: a cut or trim drag starts after 3 px; a press that does not move is a click.
pub const DRAG_THRESHOLD_PX: f64 = 3.0;

/// `useActivityTrim`'s gesture: an edge handle of the activity lane follows the pointer, and the
/// trim is committed once, on release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrimDrag {
    pub edge: TrimEdge,
    start_x: f64,
    start_second: f64,
    /// The trim when the drag started, which each move edits.
    base: Option<ActivityTrim>,
    /// The displayed trim (the draft) once the drag moved.
    pub draft: Option<Option<ActivityTrim>>,
}

fn activity_duration(store: &Store) -> f64 {
    store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds)
}

/// The surface a timeline gesture started on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceGesture {
    /// The axis: press and drag scrub, release commits.
    Scrub,
    /// The playhead's handle: as the axis.
    Playhead,
    /// The lane background: dragging pans.
    Pan,
}

/// A running timeline gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimelineGesture {
    Scrub,
    Playhead,
    Pan { last_x: f64 },
}

/// `pointerToSecond`: a timeline x as a second on the timeline.
pub fn pointer_to_second(store: &Store, x: f64) -> f64 {
    let timing = player_timing(store);
    let view = store.editor.timeline_viewport;
    let span = view.view_end - view.view_start;
    let width = store.player.viewport.metrics.width_px;
    if span <= 0.0 || width <= 0.0 {
        return clamp(timing.timeline_minimum, timing.timeline_minimum, timing.total_duration);
    }
    clamp(view.view_start + x / width * span, timing.timeline_minimum, timing.total_duration)
}

/// `viewPxToSeconds`: a pixel delta as seconds at the current zoom.
pub fn view_px_to_seconds(store: &Store, delta_px: f64) -> f64 {
    let view = store.editor.timeline_viewport;
    let span = view.view_end - view.view_start;
    let width = store.player.viewport.metrics.width_px;
    if span <= 0.0 || width <= 0.0 {
        return 0.0;
    }
    delta_px / width * span
}

/// A primary press on a timeline surface.
pub fn surface_down(store: &mut Store, surface: SurfaceGesture, x: f64) {
    store.player.surface_gesture = Some(match surface {
        SurfaceGesture::Scrub => TimelineGesture::Scrub,
        SurfaceGesture::Playhead => TimelineGesture::Playhead,
        SurfaceGesture::Pan => TimelineGesture::Pan { last_x: x },
    });
    if surface != SurfaceGesture::Pan {
        let second = pointer_to_second(store, x);
        scrub_to(store, second);
    }
}

/// The pointer moved while a surface gesture runs.
pub fn surface_move(store: &mut Store, x: f64) {
    match store.player.surface_gesture {
        Some(TimelineGesture::Scrub | TimelineGesture::Playhead) => {
            let second = pointer_to_second(store, x);
            scrub_to(store, second);
        }
        Some(TimelineGesture::Pan { last_x }) => {
            let delta = view_px_to_seconds(store, last_x - x);
            if delta == 0.0 {
                return;
            }
            store.player.surface_gesture = Some(TimelineGesture::Pan { last_x: x });
            pan_by(store, delta);
        }
        None => {}
    }
}

/// The release: a scrub commits.
pub fn surface_up(store: &mut Store, x: f64) {
    if let Some(TimelineGesture::Scrub | TimelineGesture::Playhead) = store.player.surface_gesture.take() {
        let second = pointer_to_second(store, x);
        commit_scrub(store, second);
    }
}

/// The gesture was cancelled (the window lost focus): it ends without a commit, as React's
/// `cancelScrub` leaves the last scrubbed second.
pub fn surface_cancel(store: &mut Store) {
    store.player.surface_gesture = None;
    store.player.trim_drag = None;
}

/// The segment of a video clip under a timeline x (D8): React reads it from the pressed element.
pub fn segment_at(store: &Store, video_id: &str, x: f64) -> Option<String> {
    let second = store.editor.timeline_viewport.view_start + view_px_to_seconds(store, x);
    let displayed = &store.player.timeline.timeline().displayed;
    let clip = displayed.clips.iter().find(|clip| clip.id() == video_id)?;
    let frame = clip.fps().seconds_to_frames(second - clip.offset_seconds()).clamp(0, clip.frame_count() - 1);
    crate::lib::video_program::segments::list_video_segments(clip).into_iter().find(|segment| frame >= segment.start_frame && frame < segment.end_frame).map(|segment| segment.key)
}

/// A primary press on a clip: with videos it selects the clip (and a video clip's segment under
/// the pointer). Without videos a clip cannot move, and the press pans as the lane background's.
pub fn clip_down(store: &mut Store, lane: crate::store::slices::video_import::ClipLane, x: f64) {
    use crate::store::slices::video_import::{select_clip, SelectedClip};
    if !store.video_import.has_videos() {
        surface_down(store, SurfaceGesture::Pan, x);
        return;
    }
    let segment_key = lane.video_id().and_then(|video_id| segment_at(store, video_id, x));
    // The lane comes from the displayed clips, which are the store's.
    let _ = select_clip(store, Some(SelectedClip { lane, segment_key }));
}

/// A press on an activity trim handle.
pub fn trim_down(store: &mut Store, edge: TrimEdge, x: f64) {
    let base = store.media.activity_trim;
    let window = resolve_activity_window(activity_duration(store), base);
    let start_second = if edge == TrimEdge::Start { window.start_second } else { window.end_second };
    store.player.trim_drag = Some(TrimDrag { edge, start_x: x, start_second, base, draft: None });
}

/// The pointer moved over a pressed trim handle.
pub fn trim_move(store: &mut Store, x: f64) {
    let Some(mut drag) = store.player.trim_drag else { return };
    let delta_px = x - drag.start_x;
    if drag.draft.is_none() && delta_px.abs() < DRAG_THRESHOLD_PX {
        return;
    }
    let second = drag.start_second + view_px_to_seconds(store, delta_px);
    drag.draft = Some(trim_activity_edge(drag.base, activity_duration(store), drag.edge, second));
    store.player.trim_drag = Some(drag);
}

/// The release commits the dragged trim, one undo step.
pub fn trim_up(store: &mut Store) {
    if let Some(TrimDrag { draft: Some(trim), .. }) = store.player.trim_drag.take() {
        store.media.set_activity_trim(trim);
    }
}

/// The displayed activity trim: the draft while a handle is dragged.
pub fn displayed_activity_trim(store: &Store) -> Option<ActivityTrim> {
    match store.player.trim_drag {
        Some(TrimDrag { draft: Some(trim), .. }) => trim,
        _ => store.media.activity_trim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::player::reconcile_player;
    use crate::store::slices::editor::{PreviewPlaybackState, TimelineViewport};
    use crate::store::slices::media::ActivitySummary;
    use ovrley_core::activity::schema::ParsedActivity;

    fn store() -> Store {
        let mut store = Store::default();
        store.media.activity_summary = Some(ActivitySummary { duration_seconds: 100.0, ..ActivitySummary::of(&ParsedActivity::default()) });
        store.player.viewport.metrics.width_px = 400.0;
        reconcile_player(&mut store);
        store
    }

    #[test]
    fn the_axis_scrubs_and_commits_seconds() {
        let mut store = store();
        surface_down(&mut store, SurfaceGesture::Scrub, 100.0);
        assert_eq!((store.editor.preview_playback_state, store.editor.selected_second), (PreviewPlaybackState::Scrubbing, 25.0));
        assert!(store.player.is_dragging());
        surface_move(&mut store, 200.0);
        assert_eq!(store.editor.selected_second, 50.0);
        surface_up(&mut store, 1000.0);
        assert_eq!((store.editor.preview_playback_state, store.editor.selected_second), (PreviewPlaybackState::Paused, 100.0));
        assert!(!store.player.is_dragging());
    }

    #[test]
    fn the_lane_background_pans_by_the_pixel_delta() {
        let mut store = store();
        store.editor.set_timeline_viewport(TimelineViewport { view_start: 0.0, view_end: 40.0 }).unwrap();
        surface_down(&mut store, SurfaceGesture::Pan, 200.0);
        surface_move(&mut store, 100.0);
        assert_eq!(store.editor.timeline_viewport, TimelineViewport { view_start: 10.0, view_end: 50.0 });
        surface_up(&mut store, 100.0);
        assert_eq!(store.editor.preview_playback_state, PreviewPlaybackState::Paused);
    }

    #[test]
    fn a_trim_handle_drags_after_three_pixels_and_commits_once() {
        let mut store = store();
        trim_down(&mut store, TrimEdge::Start, 0.0);
        trim_move(&mut store, 2.0);
        assert_eq!(displayed_activity_trim(&store), None, "below the threshold it is a click");
        trim_move(&mut store, 40.0);
        assert_eq!(displayed_activity_trim(&store), Some(ActivityTrim { start_second: 10.0, end_second: 100.0 }));
        assert_eq!(store.media.activity_trim, None, "the draft is not committed while dragging");
        trim_up(&mut store);
        assert_eq!(store.media.activity_trim, Some(ActivityTrim { start_second: 10.0, end_second: 100.0 }));
        trim_down(&mut store, TrimEdge::End, 400.0);
        trim_up(&mut store);
        assert_eq!(store.media.activity_trim, Some(ActivityTrim { start_second: 10.0, end_second: 100.0 }), "a click commits nothing");
    }
}
