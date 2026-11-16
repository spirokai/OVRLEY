//! Port of `useTimelineGestures` (plan D9): scrubbing on the axis, dragging the playhead, and
//! panning on the lane background. Clip, cut and trim drags join here.
//!
//! Slint reports each press, move and release with its x in timeline coordinates; the gesture state
//! lives here, as the hook's refs do in React. A pressed `TouchArea` keeps the pointer, which stands
//! in for pointer capture.

use super::playback::{commit_scrub, player_timing, scrub_to};
use super::viewport::pan_by;
use crate::lib::utils::clamp;
use crate::store::Store;

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
}
