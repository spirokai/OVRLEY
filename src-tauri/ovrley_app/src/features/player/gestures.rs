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
use crate::store::slices::video_import::ClipLane;
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
    clip_cancel(store);
}

const AUTO_SCROLL_EDGE_RATIO: f64 = 0.15;
const AUTO_SCROLL_MIN_ZOOM_SCALE: f64 = 0.1;
const AUTO_SCROLL_MAX_VIEW_SPANS_PER_SECOND: f64 = 0.75;
const AUTO_SCROLL_MAX_FRAME_SECONDS: f64 = 0.05;

/// `resolveLaneMovement`: which clips move for a lane and which edges they snap to. A video lane
/// moves its clip; the activity lane moves every clip the other way, as the activity stays at 0.
#[derive(Clone, Debug, PartialEq)]
pub struct LaneMovement {
    pub video_id: Option<String>,
    pub direction: f64,
    /// The moving clips' committed offsets and durations.
    pub moving: Vec<(f64, f64)>,
    pub fixed_edges: Vec<f64>,
}

impl LaneMovement {
    fn moving_edges(&self) -> Vec<f64> {
        self.moving.iter().flat_map(|(offset, duration)| [*offset, offset + duration]).collect()
    }
}

pub fn lane_movement(store: &Store, lane: &ClipLane) -> LaneMovement {
    let clips = &store.player.timeline.timeline().committed.clips;
    let activity_edges: Vec<f64> = store.media.activity_summary.as_ref().map_or_else(Vec::new, |summary| vec![0.0, summary.duration_seconds]);
    let span = |clip: &crate::lib::video_program::Clip| (clip.offset_seconds(), clip.duration_seconds());
    match lane.video_id() {
        None => LaneMovement { video_id: None, direction: -1.0, moving: clips.iter().map(span).collect(), fixed_edges: activity_edges },
        Some(video_id) => LaneMovement {
            video_id: Some(video_id.to_string()),
            direction: 1.0,
            moving: clips.iter().filter(|clip| clip.id() == video_id).map(span).collect(),
            fixed_edges: activity_edges.into_iter().chain(clips.iter().filter(|clip| clip.id() != video_id).flat_map(|clip| [clip.offset_seconds(), clip.end_second()])).collect(),
        },
    }
}

/// `useClipDrag`'s gesture.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipDrag {
    pub movement: LaneMovement,
    initial_delta: f64,
    current_delta: f64,
    desired_delta: f64,
    start_x: f64,
    pointer_x: f64,
    pub guideline_second: Option<f64>,
    /// When the auto-scroll last stepped.
    last_scroll: Option<std::time::Instant>,
}

/// `commitShift`: one clip snaps its start to its own frames, a shared shift snaps the earliest
/// clip's; an edge snapped onto another edge commits exactly, so touching clips stay touching.
fn commit_shift(store: &mut Store, movement: &LaneMovement, delta: f64, snapped: bool) {
    use crate::store::slices::video_import::{set_video_offset, shift_video_offsets};
    let clips = &store.player.timeline.timeline().committed.clips;
    let place = |offset: f64, video_id: &str| {
        let target = offset + delta;
        if snapped {
            return target;
        }
        clips.iter().find(|clip| clip.id() == video_id).map_or(target, |clip| clip.fps().frames_to_seconds(clip.fps().seconds_to_frames(target)))
    };
    match &movement.video_id {
        Some(video_id) => {
            let Some((offset, _)) = movement.moving.first() else { return };
            let next = place(*offset, video_id);
            let _ = set_video_offset(store, video_id, next, false);
        }
        None => {
            let Some(earliest) = clips.iter().min_by(|left, right| left.offset_seconds().total_cmp(&right.offset_seconds())) else { return };
            let (offset, id) = (earliest.offset_seconds(), earliest.id().to_string());
            let shift = place(offset, &id) - offset;
            let _ = shift_video_offsets(store, shift, false);
        }
    }
}

fn shifted_minimum(store: &Store, movement: &LaneMovement, delta: f64) -> f64 {
    let clips = &store.player.timeline.timeline().committed.clips;
    let starts = clips.iter().map(|clip| clip.offset_seconds() + if movement.video_id.as_deref().is_none_or(|id| id == clip.id()) { delta } else { 0.0 });
    starts.fold(0.0_f64, f64::min)
}

/// `updateDragOffset`: the drag at a pointer x, snapped, kept after the viewport start.
fn update_clip_drag(store: &mut Store) {
    use crate::store::slices::video_import::set_video_drag_preview;
    let Some(mut drag) = store.player.clip_drag.clone() else { return };
    let view = store.editor.timeline_viewport;
    let width = store.player.viewport.metrics.width_px;
    let delta = view_px_to_seconds(store, drag.pointer_x - drag.start_x);
    let snap = super::lanes::snap_clip_delta(drag.initial_delta + drag.movement.direction * delta, &drag.movement.moving_edges(), &drag.movement.fixed_edges, view, width, super::lanes::SNAP_THRESHOLD_PX);
    drag.desired_delta = snap.delta;
    let next = match (&drag.movement.video_id, drag.movement.moving.first()) {
        (Some(_), Some((offset, _))) => snap.delta.max(view.view_start - offset),
        _ => snap.delta,
    };
    let guideline = if next == snap.delta { snap.guideline_second } else { None };
    let changed = next != drag.current_delta || guideline != drag.guideline_second;
    drag.current_delta = next;
    drag.guideline_second = guideline;
    let preview = crate::lib::video_program::collection::VideoDragPreview { video_id: drag.movement.video_id.clone(), delta_seconds: next };
    store.player.clip_drag = Some(drag);
    if changed {
        let _ = set_video_drag_preview(store, Some(preview));
    }
}

/// A primary press on a clip with videos: selects it, and starts dragging it at once.
fn clip_drag_down(store: &mut Store, lane: &ClipLane, x: f64) {
    use crate::store::slices::video_import::set_video_drag_preview;
    let movement = lane_movement(store, lane);
    let preview = crate::lib::video_program::collection::VideoDragPreview { video_id: movement.video_id.clone(), delta_seconds: 0.0 };
    let _ = set_video_drag_preview(store, Some(preview));
    store.player.clip_drag = Some(ClipDrag { movement, initial_delta: 0.0, current_delta: 0.0, desired_delta: 0.0, start_x: x, pointer_x: x, guideline_second: None, last_scroll: None });
}

/// The pointer moved during a clip drag.
pub fn clip_move(store: &mut Store, x: f64) {
    if let Some(drag) = store.player.clip_drag.as_mut() {
        drag.pointer_x = x;
        update_clip_drag(store);
    } else {
        surface_move(store, x);
    }
}

/// The release commits one offset (one undo step).
pub fn clip_up(store: &mut Store, x: f64) {
    use crate::store::slices::video_import::set_video_drag_preview;
    let Some(drag) = store.player.clip_drag.take() else {
        surface_up(store, x);
        return;
    };
    let view = store.editor.timeline_viewport;
    let width = store.player.viewport.metrics.width_px;
    let snap = super::lanes::snap_clip_delta(drag.current_delta, &drag.movement.moving_edges(), &drag.movement.fixed_edges, view, width, super::lanes::SNAP_THRESHOLD_PX);
    commit_shift(store, &drag.movement, snap.delta, snap.guideline_second.is_some());
    let _ = set_video_drag_preview(store, None);
}

/// A cancelled clip drag commits nothing.
fn clip_cancel(store: &mut Store) {
    if store.player.clip_drag.take().is_some() {
        let _ = crate::store::slices::video_import::set_video_drag_preview(store, None);
    }
}

/// The pointer's auto-scroll target (`getAutoScrollPointerSecond`): in the outer 15% of the
/// timeline, at up to 0.75 view spans per second scaled by zoom and depth.
fn auto_scroll_second(store: &Store, pointer_x: f64, elapsed: f64) -> Option<f64> {
    let width = store.player.viewport.metrics.width_px;
    if width <= 0.0 {
        return None;
    }
    let edge = width * AUTO_SCROLL_EDGE_RATIO;
    if pointer_x >= edge && pointer_x <= width - edge {
        return None;
    }
    let view = store.editor.timeline_viewport;
    let timing = player_timing(store);
    let span = view.view_end - view.view_start;
    let zoom_scale = AUTO_SCROLL_MIN_ZOOM_SCALE.max((span / (timing.total_duration - timing.timeline_minimum)).min(1.0));
    let left = pointer_x < edge;
    let intensity = (if left { edge - pointer_x } else { pointer_x - (width - edge) } / edge).min(1.0);
    let direction = if left { -1.0 } else { 1.0 };
    let edge_second = if left { view.view_start + span * AUTO_SCROLL_EDGE_RATIO } else { view.view_end - span * AUTO_SCROLL_EDGE_RATIO };
    Some(edge_second + direction * span * AUTO_SCROLL_MAX_VIEW_SPANS_PER_SECOND * zoom_scale * intensity * elapsed)
}

/// One auto-scroll step while a clip drag sits in an edge zone; `ovrley_ui` calls it from a timer.
/// Returns whether to keep scrolling.
pub fn clip_drag_auto_scroll(store: &mut Store, now: std::time::Instant) -> bool {
    let Some(drag) = store.player.clip_drag.clone() else { return false };
    let elapsed = drag.last_scroll.map_or(1.0 / 60.0, |last| now.saturating_duration_since(last).as_secs_f64().min(AUTO_SCROLL_MAX_FRAME_SECONDS));
    let Some(target) = auto_scroll_second(store, drag.pointer_x, elapsed) else {
        if let Some(drag) = store.player.clip_drag.as_mut() {
            drag.last_scroll = None;
        }
        return false;
    };
    let minimum = shifted_minimum(store, &drag.movement, drag.desired_delta);
    let Some(delta_start) = super::viewport::follow_second(store, target, minimum) else { return false };
    if let Some(drag) = store.player.clip_drag.as_mut() {
        drag.last_scroll = Some(now);
        drag.initial_delta += drag.movement.direction * delta_start;
    }
    update_clip_drag(store);
    true
}

/// Whether a clip drag sits in an edge zone, so the auto-scroll timer runs.
pub fn clip_drag_in_edge_zone(store: &Store) -> bool {
    store.player.clip_drag.as_ref().is_some_and(|drag| auto_scroll_second(store, drag.pointer_x, 0.0).is_some())
}

/// `useClipDrag`'s keyboard nudge: a transient shift of one lane, committed once.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipNudge {
    pub lane: ClipLane,
    movement: LaneMovement,
    delta_seconds: f64,
}

/// `nudgeLane`.
pub fn nudge_lane(store: &mut Store, lane: ClipLane, delta_seconds: f64) {
    use crate::store::slices::video_import::set_video_drag_preview;
    if store.player.timeline.timeline().committed.clips.is_empty() {
        return;
    }
    let mut nudge = match store.player.clip_nudge.take() {
        Some(nudge) if nudge.lane == lane => nudge,
        _ => ClipNudge { movement: lane_movement(store, &lane), lane, delta_seconds: 0.0 },
    };
    nudge.delta_seconds += nudge.movement.direction * delta_seconds;
    let preview = crate::lib::video_program::collection::VideoDragPreview { video_id: nudge.movement.video_id.clone(), delta_seconds: nudge.delta_seconds };
    store.player.clip_nudge = Some(nudge);
    let _ = set_video_drag_preview(store, Some(preview));
}

/// `commitNudge`: on key release or focus loss.
pub fn commit_nudge(store: &mut Store) {
    let Some(nudge) = store.player.clip_nudge.take() else { return };
    commit_shift(store, &nudge.movement, nudge.delta_seconds, false);
    let _ = crate::store::slices::video_import::set_video_drag_preview(store, None);
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
pub fn clip_down(store: &mut Store, lane: ClipLane, x: f64) {
    use crate::store::slices::video_import::{select_clip, SelectedClip};
    if !store.video_import.has_videos() {
        surface_down(store, SurfaceGesture::Pan, x);
        return;
    }
    commit_nudge(store);
    let segment_key = lane.video_id().and_then(|video_id| segment_at(store, video_id, x));
    // The lane comes from the displayed clips, which are the store's.
    let _ = select_clip(store, Some(SelectedClip { lane: lane.clone(), segment_key }));
    clip_drag_down(store, &lane, x);
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

    use crate::lib::video_program::fixtures::*;
    use crate::store::slices::video_import::tests::store_with;

    fn clips_store(videos: Vec<crate::lib::video_program::ImportedVideo>, offsets: &[(&str, f64)]) -> Store {
        let mut store = store_with(videos, offsets);
        store.media.activity_summary = Some(ActivitySummary { duration_seconds: 100.0, ..ActivitySummary::of(&ParsedActivity::default()) });
        store.player.viewport.metrics.width_px = 250.0;
        reconcile_player(&mut store);
        store.editor.set_timeline_viewport(TimelineViewport { view_start: 0.0, view_end: 100.0 }).unwrap();
        store
    }

    fn offset(store: &Store, id: &str) -> f64 {
        store.video_import.video_edits[id].offset_seconds
    }

    #[test]
    fn a_clip_drags_live_and_commits_a_frame_snapped_offset() {
        let mut store = clips_store(vec![simple("video-1", 20.0, 0)], &[("video-1", 5.0)]);
        clip_down(&mut store, ClipLane::Video("video-1".into()), 100.0);
        assert!(store.player.is_dragging());
        assert_eq!(store.video_import.video_drag_preview.as_ref().map(|preview| preview.delta_seconds), Some(0.0));
        clip_move(&mut store, 103.0);
        assert!((store.video_import.video_drag_preview.as_ref().unwrap().delta_seconds - 1.2).abs() < 1e-9);
        clip_up(&mut store, 103.0);
        assert!((offset(&store, "video-1") - 6.2).abs() < 1e-9);
        assert_eq!(store.video_import.video_drag_preview, None);
        assert!(!store.player.is_dragging());
    }

    #[test]
    fn a_clip_snaps_to_the_activity_and_to_other_clips_exactly() {
        let mut store = clips_store(vec![simple("video-1", 20.0, 0)], &[("video-1", 5.0)]);
        clip_down(&mut store, ClipLane::Video("video-1".into()), 100.0);
        clip_move(&mut store, 88.0);
        assert_eq!(store.player.clip_drag.as_ref().unwrap().guideline_second, Some(0.0));
        clip_up(&mut store, 88.0);
        assert_eq!(offset(&store, "video-1"), 0.0);

        let mut store = clips_store(vec![simple("first", 20.0, 0), simple("second", 10.0, 1)], &[("first", 5.03), ("second", 40.0)]);
        clip_down(&mut store, ClipLane::Video("second".into()), 100.0);
        clip_move(&mut store, 63.0);
        assert!((store.player.clip_drag.as_ref().unwrap().guideline_second.unwrap() - 25.03).abs() < 1e-9);
        clip_up(&mut store, 63.0);
        assert_eq!(offset(&store, "second"), 5.03 + 20.0, "the snapped edge commits exactly");
    }

    #[test]
    fn nudges_accumulate_per_lane_and_commit_once() {
        let mut store = clips_store(vec![simple("first", 20.0, 0), simple("second", 10.0, 1)], &[("first", 5.03), ("second", 40.0)]);
        nudge_lane(&mut store, ClipLane::Video("first".into()), -0.1);
        nudge_lane(&mut store, ClipLane::Video("first".into()), -0.1);
        assert!((store.video_import.video_drag_preview.as_ref().unwrap().delta_seconds + 0.2).abs() < 1e-9);
        commit_nudge(&mut store);
        assert!((offset(&store, "first") - 145.0 / 30.0).abs() < 1e-9);
        assert_eq!(store.video_import.video_drag_preview, None);
        reconcile_player(&mut store);
        nudge_lane(&mut store, ClipLane::Activity, 0.1);
        assert_eq!(store.video_import.video_drag_preview.as_ref().map(|preview| (preview.video_id.clone(), preview.delta_seconds)), Some((None, -0.1)));
        commit_nudge(&mut store);
        assert!((offset(&store, "first") - 142.0 / 30.0).abs() < 1e-9, "the shared shift snaps the earliest clip");
    }

    #[test]
    fn the_edge_zone_scrolls_the_view_during_a_drag() {
        let mut store = clips_store(vec![simple("video-1", 20.0, 0)], &[("video-1", 50.0)]);
        store.editor.set_timeline_viewport(TimelineViewport { view_start: 40.0, view_end: 60.0 }).unwrap();
        clip_down(&mut store, ClipLane::Video("video-1".into()), 125.0);
        clip_move(&mut store, 249.0);
        assert!(clip_drag_in_edge_zone(&store));
        let before = store.editor.timeline_viewport.view_start;
        assert!(clip_drag_auto_scroll(&mut store, std::time::Instant::now()));
        assert!(store.editor.timeline_viewport.view_start > before);
    }
}
