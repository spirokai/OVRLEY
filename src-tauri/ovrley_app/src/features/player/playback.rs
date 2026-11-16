//! Port of `features/player/utils/playerTiming.js` and `hooks/usePlaybackEngine.js` (plan D6): the
//! durations of the timeline, where playback starts, and the timeline clock.
//!
//! The timeline clock is anchored to an [`Instant`] and a timeline second. While it plays,
//! `ovrley_ui` calls [`playback_tick`] from a repeating timer; a tick quantizes the elapsed time to
//! the preview's frame rate (the render fps over the widget update rate) and moves the playhead only
//! when the frame changes. At the end of the timeline it pauses. Scrubs are dispatched as they come;
//! the preview frame coalesces them (Phase 3 D9).
//!
//! The video clock arrives in Phase 6. Until then video playback is never possible (the video
//! background mode does not exist yet), so the timeline clock always plays; the video guard and
//! the handoff are ported and tested with `should_use_video_playback` forced on.

use crate::lib::update_rate::container_fps;
use crate::lib::utils::clamp;
use crate::lib::video_program::program::{resolve_playback_start, PlaybackSource, PlaybackStart, Program};
use crate::lib::video_program::SecondsRange;
use crate::lib::video_program::TelemetryStatus;
use crate::store::slices::editor::PreviewPlaybackState;
use crate::store::Store;
use std::time::{Duration, Instant};

/// `snapTimelineSecondToFrame`: a timeline second on the nearest frame of a video whose frame
/// zero sits at `origin_second`.
pub fn snap_timeline_second_to_frame(second: f64, fps: f64, origin_second: f64) -> f64 {
    origin_second + crate::js::round((second - origin_second) * fps) / fps
}

/// `resolvePlaybackSource`: without video playback the timeline clock plays from the playhead.
pub fn resolve_playback_source(should_use_video_playback: bool, playhead_second: f64, program: &Program) -> PlaybackStart {
    if !should_use_video_playback {
        return PlaybackStart { source: PlaybackSource::Timeline, second: playhead_second };
    }
    resolve_playback_start(program, playhead_second)
}

/// `getTimelineMinimum`: the first second media occupies, never after 0.
pub fn timeline_minimum(video_bounds: Option<SecondsRange>) -> f64 {
    video_bounds.map_or(0.0, |bounds| bounds.start_second.min(0.0))
}

/// `getTotalPlaybackDuration`: the largest of the activity, the fallback duration (only without
/// activity and without videos) and the videos' end.
pub fn total_playback_duration(activity_duration_seconds: Option<f64>, fallback_duration_seconds: f64, video_bounds: Option<SecondsRange>) -> f64 {
    let activity = activity_duration_seconds.unwrap_or(0.0);
    let video_end = video_bounds.map_or(0.0, |bounds| bounds.end_second);
    let content = if activity > 0.0 {
        activity
    } else if video_bounds.is_some() {
        0.0
    } else {
        fallback_duration_seconds
    };
    content.max(video_end).max(0.0)
}

/// `createPlaybackAnchor`: where the timeline clock started. A video anchor (`started_at: None`)
/// does not advance; pauses and scrubs use it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaybackAnchor {
    pub started_at: Option<Instant>,
    pub started_second: f64,
}

impl PlaybackAnchor {
    fn new(source: PlaybackSource, second: f64, now: Instant) -> Self {
        Self { started_at: (source == PlaybackSource::Timeline).then_some(now), started_second: second }
    }

    fn paused(second: f64) -> Self {
        Self { started_at: None, started_second: second }
    }

    /// `getTimelinePlaybackSecond`.
    fn second_at(&self, now: Instant) -> f64 {
        self.started_second + self.started_at.map_or(0.0, |started| now.saturating_duration_since(started).as_secs_f64())
    }
}

/// The engine's refs.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackEngine {
    pub anchor: PlaybackAnchor,
    /// The last preview frame index the clock published (`previewFrameRef`, -1 as `None`).
    frame_index: Option<i64>,
    /// `previousVideoIdentityRef`.
    video_identity: String,
}

impl Default for PlaybackEngine {
    fn default() -> Self {
        Self { anchor: PlaybackAnchor::paused(0.0), frame_index: None, video_identity: String::new() }
    }
}

/// The durations and flags the transport reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerTiming {
    pub total_duration: f64,
    pub timeline_minimum: f64,
    /// There is something to play: activity data (or a clip's telemetry) and a positive duration.
    pub has_activity: bool,
    pub clamped_playhead: f64,
    pub should_use_video_playback: bool,
    /// `getContainerFps(render fps, widget update rate)`.
    pub effective_fps: f64,
}

/// The player's timing from the store.
pub fn player_timing(store: &Store) -> PlayerTiming {
    let committed = &store.player.timeline.timeline().committed;
    let bounds = committed.bounds;
    let activity_duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
    let total_duration = total_playback_duration(Some(activity_duration), store.editor.fallback_duration_seconds, bounds);
    let timeline_minimum = timeline_minimum(bounds);
    let has_video_telemetry = store.video_import.imported_videos.iter().any(|video| video.telemetry.status() == TelemetryStatus::Ready);
    let has_activity_data = store.media.activity_summary.is_some() || has_video_telemetry;
    PlayerTiming {
        total_duration,
        timeline_minimum,
        has_activity: has_activity_data && total_duration > 0.0,
        clamped_playhead: clamp(store.editor.selected_second, timeline_minimum, total_duration),
        // The video background mode arrives with Phase 6.
        should_use_video_playback: false,
        effective_fps: container_fps(store.render_settings.fps, f64::from(store.render_settings.widget_update_rate)),
    }
}

fn pause_at_second(store: &mut Store, second: f64) {
    let engine = &mut store.player.playback;
    engine.anchor = PlaybackAnchor::paused(second);
    engine.frame_index = None;
    store.editor.pause_preview_playback(second);
}

fn start_playback(store: &mut Store, timing: PlayerTiming, playhead: f64, now: Instant) {
    let start = resolve_playback_source(timing.should_use_video_playback, playhead, &store.player.timeline.timeline().committed.program);
    let engine = &mut store.player.playback;
    engine.anchor = PlaybackAnchor::new(start.source, start.second, now);
    engine.frame_index = None;
    store.editor.start_preview_playback(start.source, start.second);
}

/// `play`: from the start when at the end; from the next piece in a gap or excluded part.
pub fn play(store: &mut Store, now: Instant) {
    play_with(store, player_timing(store), now);
}

fn play_with(store: &mut Store, timing: PlayerTiming, now: Instant) {
    if !timing.has_activity {
        return;
    }
    let playhead = if timing.clamped_playhead >= timing.total_duration { timing.timeline_minimum } else { timing.clamped_playhead };
    start_playback(store, timing, playhead, now);
}

/// `pause`.
pub fn pause(store: &mut Store) {
    let playhead = player_timing(store).clamped_playhead;
    pause_at_second(store, playhead);
}

/// Play or pause (Space, the transport's middle button).
pub fn toggle_playback(store: &mut Store, now: Instant) {
    if store.editor.preview_playback_state == PreviewPlaybackState::Playing {
        pause(store);
    } else {
        play(store, now);
    }
}

/// `resetToStart`.
pub fn reset_to_start(store: &mut Store) {
    let minimum = player_timing(store).timeline_minimum;
    pause_at_second(store, minimum);
}

/// `jumpToEnd`.
pub fn jump_to_end(store: &mut Store) {
    let total = player_timing(store).total_duration;
    pause_at_second(store, total);
}

/// `stepBySeconds`: paused at the playhead moved by `delta_seconds`, on the timeline.
pub fn step_by_seconds(store: &mut Store, delta_seconds: f64) {
    let timing = player_timing(store);
    pause_at_second(store, clamp(timing.clamped_playhead + delta_seconds, timing.timeline_minimum, timing.total_duration));
}

/// `scrubTo`: a scrub sample, clamped to the timeline.
pub fn scrub_to(store: &mut Store, second: f64) {
    let timing = player_timing(store);
    let second = clamp(second, timing.timeline_minimum, timing.total_duration);
    let engine = &mut store.player.playback;
    engine.anchor = PlaybackAnchor::paused(second);
    engine.frame_index = None;
    store.editor.update_preview_scrub(second);
}

/// `commitScrub`: the final, paused playhead.
pub fn commit_scrub(store: &mut Store, second: f64) {
    let timing = player_timing(store);
    let second = clamp(second, timing.timeline_minimum, timing.total_duration);
    let engine = &mut store.player.playback;
    engine.anchor = PlaybackAnchor::paused(second);
    engine.frame_index = None;
    store.editor.commit_preview_scrub(second);
}

fn timeline_clock_active(store: &Store, timing: PlayerTiming) -> bool {
    store.editor.preview_playback_state == PreviewPlaybackState::Playing && store.editor.preview_playback_source == PlaybackSource::Timeline && timing.has_activity
}

/// One frame of the timeline clock. Returns whether the clock still plays, so `ovrley_ui` keeps
/// its timer running.
pub fn playback_tick(store: &mut Store, now: Instant) -> bool {
    let timing = player_timing(store);
    if !timeline_clock_active(store, timing) {
        return false;
    }
    let anchor = store.player.playback.anchor;
    let second = anchor.second_at(now);
    if second >= timing.total_duration {
        store.editor.pause_preview_playback(timing.total_duration);
        let engine = &mut store.player.playback;
        engine.anchor = PlaybackAnchor::paused(timing.total_duration);
        engine.frame_index = None;
        return false;
    }
    let frame = ((second - timing.timeline_minimum) * timing.effective_fps).floor() as i64;
    if store.player.playback.frame_index != Some(frame) {
        store.player.playback.frame_index = Some(frame);
        let frame_second = anchor.started_second.max(timing.timeline_minimum + frame as f64 / timing.effective_fps);
        store.editor.set_selected_second(clamp(frame_second, timing.timeline_minimum, timing.total_duration));
    }
    true
}

/// When the timeline clock reaches its next preview frame, while it plays; the timer skips the
/// ticks before it.
pub fn next_frame_at(store: &Store) -> Option<Instant> {
    let timing = player_timing(store);
    if !timeline_clock_active(store, timing) {
        return None;
    }
    let engine = &store.player.playback;
    let started_at = engine.anchor.started_at?;
    let Some(frame) = engine.frame_index else { return Some(started_at) };
    let next_second = timing.timeline_minimum + (frame + 1) as f64 / timing.effective_fps;
    let offset = (next_second - engine.anchor.started_second).max(0.0);
    Some(started_at + Duration::from_secs_f64(offset.min(timing.total_duration - engine.anchor.started_second).max(0.0)))
}

/// The playback effects of the `player` reconcile step, in React's order (D6).
pub fn reconcile_playback(store: &mut Store) {
    reconcile_playback_with(store, player_timing(store), Instant::now());
}

fn reconcile_playback_with(store: &mut Store, timing: PlayerTiming, now: Instant) {
    // 1. A new video identity moves a playhead at 0 to a negative timeline minimum.
    let identity = store.player.timeline.timeline().committed.clips.iter().map(|clip| format!("{}:{}", clip.id(), clip.duration_seconds())).collect::<Vec<_>>().join("|");
    let previous = std::mem::replace(&mut store.player.playback.video_identity, identity.clone());
    if !identity.is_empty() && identity != previous && store.editor.selected_second == 0.0 && timing.timeline_minimum < 0.0 {
        store.editor.set_selected_second(timing.timeline_minimum);
    }
    let timing = PlayerTiming { clamped_playhead: clamp(store.editor.selected_second, timing.timeline_minimum, timing.total_duration), ..timing };
    // 2. The playhead stays on the timeline while there is something to play.
    if !timing.has_activity {
        store.player.playback.anchor = PlaybackAnchor::paused(timing.timeline_minimum);
    } else if timing.clamped_playhead != store.editor.selected_second {
        store.editor.set_selected_second(timing.clamped_playhead);
    }
    // 3. Video playback that is no longer possible pauses.
    if store.editor.preview_playback_source == PlaybackSource::Video && !timing.should_use_video_playback {
        if store.editor.preview_playback_state != PreviewPlaybackState::Paused || store.editor.selected_second != timing.clamped_playhead {
            pause_at_second(store, timing.clamped_playhead);
        }
        store.player.playback.anchor = PlaybackAnchor::paused(timing.clamped_playhead);
    }
    // 4. Timeline playback entering the program hands over to the video clock.
    if store.editor.preview_playback_state == PreviewPlaybackState::Playing && timing.should_use_video_playback && store.editor.preview_playback_source == PlaybackSource::Timeline {
        let start = resolve_playback_source(true, timing.clamped_playhead, &store.player.timeline.timeline().committed.program);
        if start.source == PlaybackSource::Video {
            let engine = &mut store.player.playback;
            engine.anchor = PlaybackAnchor::new(start.source, start.second, now);
            engine.frame_index = None;
            store.editor.start_preview_playback(start.source, start.second);
        }
    }
    // The timeline clock re-anchors when it starts playing from another owner.
    if timeline_clock_active(store, timing) && store.player.playback.anchor.started_at.is_none() {
        store.player.playback.anchor = PlaybackAnchor::new(PlaybackSource::Timeline, timing.clamped_playhead, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::video_program::collection::build_timeline_program;
    use crate::lib::video_program::fixtures::*;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;

    fn bounds(start_second: f64, end_second: f64) -> Option<SecondsRange> {
        Some(SecondsRange { start_second, end_second })
    }

    #[test]
    fn frames_snap_from_their_origin() {
        assert!((snap_timeline_second_to_frame(12.38, 30.0, 5.25) - 12.383_333_333_3).abs() < 1e-9);
    }

    #[test]
    fn the_total_duration_reaches_the_videos_end_and_ignores_the_fallback_with_videos() {
        assert_eq!(total_playback_duration(Some(12.0), 9.0, bounds(10.0, 16.0)), 16.0);
        assert_eq!(total_playback_duration(Some(0.0), 73.0, bounds(0.0, 12.509)), 12.509);
        assert_eq!(total_playback_duration(None, 73.0, None), 73.0);
        assert_eq!(total_playback_duration(Some(25.0), 0.0, bounds(-5.0, 25.0)), 25.0);
    }

    #[test]
    fn the_minimum_is_a_negative_video_start() {
        assert_eq!(timeline_minimum(bounds(-5.0, 25.0)), -5.0);
        assert_eq!(timeline_minimum(bounds(3.0, 25.0)), 0.0);
        assert_eq!(timeline_minimum(None), 0.0);
    }

    use crate::features::player::timeline::reconcile_timeline;
    use crate::lib::video_program::ImportedVideo;
    use crate::store::slices::media::ActivitySummary;
    use crate::store::slices::video_import::tests::store_with;
    use ovrley_core::activity::schema::ParsedActivity;

    /// A store with an activity of `duration` seconds and videos at offsets, reconciled.
    fn player(activity_duration: f64, videos: Vec<ImportedVideo>, offsets: &[(&str, f64)], selected: f64) -> Store {
        let mut store = store_with(videos, offsets);
        store.media.activity_summary = Some(ActivitySummary { duration_seconds: activity_duration, ..ActivitySummary::of(&ParsedActivity::default()) });
        store.editor.fallback_duration_seconds = 0.0;
        store.editor.selected_second = selected;
        reconcile_timeline(&mut store);
        store
    }

    fn with_video_playback(store: &Store) -> PlayerTiming {
        PlayerTiming { should_use_video_playback: true, ..player_timing(store) }
    }

    fn state(store: &Store) -> (PreviewPlaybackState, PlaybackSource, f64) {
        (store.editor.preview_playback_state, store.editor.preview_playback_source, store.editor.selected_second)
    }

    #[test]
    fn video_backed_playback_starts_inside_the_video_window() {
        let mut store = player(8.0, vec![simple("video-1", 4.0, 0)], &[("video-1", 2.0)], 3.0);
        let timing = with_video_playback(&store);
        play_with(&mut store, timing, Instant::now());
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Video, 3.0));
    }

    #[test]
    fn a_new_negative_video_moves_a_playhead_at_zero_once() {
        let mut store = player(8.0, vec![simple("video-1", 10.0, 0)], &[("video-1", -5.0)], 0.0);
        reconcile_playback(&mut store);
        assert_eq!(store.editor.selected_second, -5.0);
        store.editor.selected_second = 0.0;
        reconcile_playback(&mut store);
        assert_eq!(store.editor.selected_second, 0.0, "the same videos keep the playhead");
        let timing = with_video_playback(&store);
        play_with(&mut store, timing, Instant::now());
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Video, 0.0));
    }

    #[test]
    fn the_timeline_clock_advances_on_ticks_and_pauses_at_the_exact_end() {
        let mut store = player(3.0, Vec::new(), &[], 1.0);
        let start = Instant::now();
        play(&mut store, start);
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Timeline, 1.0));
        assert!(playback_tick(&mut store, start + Duration::from_millis(500)));
        assert_eq!(store.editor.selected_second, 1.5);
        assert!(!playback_tick(&mut store, start + Duration::from_millis(2500)));
        assert_eq!(state(&store), (PreviewPlaybackState::Paused, PlaybackSource::Timeline, 3.0));
    }

    #[test]
    fn ticks_step_at_the_render_fps_over_the_update_rate() {
        let mut store = player(10.0, Vec::new(), &[], 0.0);
        store.render_settings.widget_update_rate = 3;
        let start = Instant::now();
        play(&mut store, start);
        playback_tick(&mut store, start + Duration::from_millis(150));
        assert_eq!(store.editor.selected_second, 0.1, "10 fps: the frame at 0.1 s");
        assert_eq!(next_frame_at(&store), Some(start + Duration::from_millis(200)));
        playback_tick(&mut store, start + Duration::from_millis(190));
        assert_eq!(store.editor.selected_second, 0.1, "the same frame sets nothing");
    }

    #[test]
    fn play_at_the_end_restarts_and_commands_pause() {
        let mut store = player(8.0, Vec::new(), &[], 8.0);
        play(&mut store, Instant::now());
        assert_eq!(store.editor.selected_second, 0.0);
        jump_to_end(&mut store);
        assert_eq!(state(&store), (PreviewPlaybackState::Paused, PlaybackSource::Timeline, 8.0));
        step_by_seconds(&mut store, -1.0);
        assert_eq!(store.editor.selected_second, 7.0);
        reset_to_start(&mut store);
        assert_eq!(store.editor.selected_second, 0.0);
    }

    #[test]
    fn scrubs_update_and_commit_clamped_to_the_timeline() {
        let mut store = player(8.0, Vec::new(), &[], 1.0);
        scrub_to(&mut store, 4.75);
        assert_eq!(state(&store), (PreviewPlaybackState::Scrubbing, PlaybackSource::Timeline, 4.75));
        scrub_to(&mut store, 20.0);
        assert_eq!(store.editor.selected_second, 8.0);
        commit_scrub(&mut store, 3.5);
        assert_eq!(state(&store), (PreviewPlaybackState::Paused, PlaybackSource::Timeline, 3.5));
    }

    #[test]
    fn playback_in_a_gap_starts_at_the_next_clip() {
        let mut store = player(20.0, vec![simple("first", 4.0, 0), simple("second", 4.0, 1)], &[("second", 10.0)], 6.0);
        let timing = with_video_playback(&store);
        play_with(&mut store, timing, Instant::now());
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Video, 10.0));
        store.editor.selected_second = 11.0;
        let timing = with_video_playback(&store);
        play_with(&mut store, timing, Instant::now());
        assert_eq!(store.editor.selected_second, 11.0);
    }

    #[test]
    fn the_guard_pauses_video_playback_that_is_no_longer_possible_and_the_handoff_starts_it() {
        let mut store = player(8.0, vec![simple("video-1", 4.0, 0)], &[("video-1", 2.0)], 6.0);
        store.editor.start_preview_playback(PlaybackSource::Video, 6.0);
        reconcile_playback(&mut store);
        assert_eq!(state(&store), (PreviewPlaybackState::Paused, PlaybackSource::Video, 6.0));

        let mut store = player(8.0, vec![simple("video-1", 4.0, 0)], &[("video-1", 2.0)], 3.0);
        store.editor.start_preview_playback(PlaybackSource::Timeline, 3.0);
        let timing = with_video_playback(&store);
        reconcile_playback_with(&mut store, timing, Instant::now());
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Video, 3.0));

        let mut store = player(8.0, vec![simple("video-1", 4.0, 0)], &[("video-1", 2.0)], 6.0);
        store.editor.start_preview_playback(PlaybackSource::Video, 6.0);
        let timing = with_video_playback(&store);
        reconcile_playback_with(&mut store, timing, Instant::now());
        assert_eq!(state(&store), (PreviewPlaybackState::Playing, PlaybackSource::Video, 6.0), "the video clock owns its piece-end handoffs");
    }

    #[test]
    fn a_stale_playhead_is_clamped_while_there_is_something_to_play() {
        let mut store = player(8.0, Vec::new(), &[], 12.0);
        reconcile_playback(&mut store);
        assert_eq!(store.editor.selected_second, 8.0);
    }

    #[test]
    fn video_playback_is_scoped_to_the_video_window() {
        let program = |offset| {
            let videos = [simple("video-1", 4.0, 0)];
            build_timeline_program(&videos, &edits_at(&videos, &[("video-1", offset)]), None, transition_defaults()).program
        };
        let (shifted, negative) = (program(5.0), program(-5.0));
        let source = |program: &Program, at, video| resolve_playback_source(video, at, program);
        let start = |source, second| PlaybackStart { source, second };
        assert_eq!(source(&shifted, 4.99, true), start(PlaybackSource::Timeline, 4.99));
        assert_eq!(source(&shifted, 5.0, true), start(PlaybackSource::Video, 5.0));
        assert_eq!(source(&shifted, 8.99, true), start(PlaybackSource::Video, 8.99));
        assert_eq!(source(&shifted, 9.0, true), start(PlaybackSource::Timeline, 9.0));
        assert_eq!(source(&shifted, 6.0, false), start(PlaybackSource::Timeline, 6.0));
        assert_eq!(source(&negative, -4.99, true), start(PlaybackSource::Video, -4.99));
        assert_eq!(source(&negative, -1.0, true), start(PlaybackSource::Timeline, -1.0));
    }
}
