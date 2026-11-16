//! Port of `features/player`: the overlay player's playback, timeline viewport, lanes, gestures and
//! edits at the playhead (plan D6 to D11, D16).

pub mod gestures;
pub mod lanes;
pub mod playback;
pub mod timeline;
pub mod viewport;

use crate::features::app_shell::KeyboardWorkspace;
use crate::features::overlay_editor::keyboard::overlay_open;
use crate::lib::keyboard_shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::lib::activity::{resolve_activity_window, trim_activity_edge, TrimEdge};
use crate::lib::file_dialog::filename_from_selected_path;
use crate::store::slices::layout::LeftDrawerTool;
use crate::store::slices::media::ActivitySource;
use crate::store::Store;
use std::time::Instant;

/// `videoSyncMode`: the video sync drawer is the active tool.
pub fn video_sync_mode(store: &Store) -> bool {
    store.layout.active_left_drawer_tool == LeftDrawerTool::VideoSync
}

/// The activity's file name: its source's, else the parsed file's.
fn activity_filename(store: &Store) -> Option<String> {
    let from_source = store.media.activity_source.as_ref().and_then(|ActivitySource::File { path }| filename_from_selected_path(path).map(str::to_string));
    from_source.or_else(|| store.media.activity_summary.as_ref().and_then(|summary| summary.file_name.clone()))
}

/// The lanes' view model (`useTimelineClips` as `useOverlayPlayer` composes it).
pub fn lanes_view(store: &Store) -> (Option<lanes::VideoLaneView>, Option<lanes::ActivityLaneView>) {
    let timing = playback::player_timing(store);
    let displayed = &store.player.timeline.timeline().displayed;
    let sync = video_sync_mode(store);
    let duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
    let trim = gestures::displayed_activity_trim(store);
    let dragged_edge = store.player.trim_drag.and_then(|drag| drag.draft.map(|_| drag.edge));
    let inputs = lanes::LaneInputs {
        view: store.editor.timeline_viewport,
        metrics: store.player.viewport.metrics,
        locale: store.i18n.language,
        clips: &displayed.clips,
        overlaps: &displayed.program.overlaps,
        warnings: &store.player.timeline.timeline().warnings,
        selected: store.video_import.selected_clip.as_ref(),
        hovered: store.player.hovered_lane.as_ref(),
        has_activity: store.media.activity_summary.is_some() && timing.has_activity,
        activity_summary: store.media.activity_summary.as_ref(),
        activity_filename: activity_filename(store),
        activity_trim: (!sync).then(|| (resolve_activity_window(duration, trim), dragged_edge)),
        show_cut_markers: !sync,
        dragged_cut: None,
    };
    (lanes::video_lane(&inputs), lanes::activity_lane(&inputs))
}

/// The activity trim's edge at the playhead (I and O without a video).
fn set_activity_trim_edge(store: &mut Store, edge: TrimEdge, second: f64) {
    let duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
    let trim = trim_activity_edge(store.media.activity_trim, duration, edge, second);
    store.media.set_activity_trim(trim);
}

/// The player's hook state (D3, D9): what React keeps in the player hooks' refs and state.
#[derive(Debug, Default)]
pub struct PlayerState {
    /// The cached timeline (D5).
    pub timeline: timeline::TimelineCache,
    /// `usePlaybackEngine`'s refs (D6).
    pub playback: playback::PlaybackEngine,
    /// `useTimelineViewport`'s measurements and rule inputs (D7).
    pub viewport: viewport::ViewportState,
    /// A scrub, playhead drag or pan (`useTimelineGestures`, D9).
    pub surface_gesture: Option<gestures::TimelineGesture>,
    /// A dragged activity trim handle (`useActivityTrim`, D14).
    pub trim_drag: Option<gestures::TrimDrag>,
    /// The lane whose clip the pointer is over; its tooltip shows.
    pub hovered_lane: Option<crate::store::slices::video_import::ClipLane>,
}

impl PlayerState {
    /// A pointer interaction runs on the timeline; it suspends the viewport's clamp and follow.
    pub fn is_dragging(&self) -> bool {
        self.surface_gesture.is_some()
    }
}

/// The `player` reconcile step: the timeline derivations, then the playback effects (D6) and the
/// viewport rules (D7), in React's effect order.
pub fn reconcile_player(store: &mut Store) {
    timeline::reconcile_timeline(store);
    playback::reconcile_playback(store);
    let dragging = store.player.is_dragging();
    viewport::reconcile_viewport(store, dragging);
}

/// `useOverlayPlayer`'s window keys (the player scope, D11). Repeats are ignored, open overlays
/// block every command, and text fields keep their keys (Phase 2 D7). Returns whether the key was
/// used.
pub fn handle_player_key(store: &mut Store, input: &KeyInput, now: Instant) -> bool {
    if input.repeat || overlay_open(store) {
        return false;
    }
    let Some(found) = match_keyboard_shortcut(input, Scope::Player) else { return false };
    let timing = playback::player_timing(store);
    let has_video = store.video_import.has_videos();
    let has_activity_data = store.media.activity_summary.is_some();
    let sync = video_sync_mode(store);
    let duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
    match found.command.id.as_str() {
        "player.stepBackward" | "player.stepForward" => {
            if store.editor_shell.active_keyboard_workspace != KeyboardWorkspace::Player || !timing.has_activity {
                return false;
            }
            playback::step_by_seconds(store, if found.command.id == "player.stepForward" { 1.0 } else { -1.0 });
        }
        "player.togglePlayback" if timing.has_activity => playback::toggle_playback(store, now),
        "player.resetToStart" if timing.has_activity => playback::reset_to_start(store),
        "player.jumpToEnd" if timing.has_activity => playback::jump_to_end(store),
        "player.toggleMute" if has_video => store.editor.toggle_video_mute(),
        "trim.start" | "trim.end" if !sync && !has_video && has_activity_data => {
            let edge = if found.command.id == "trim.start" { TrimEdge::Start } else { TrimEdge::End };
            set_activity_trim_edge(store, edge, timing.clamped_playhead);
        }
        "activityTrim.reset" if store.media.activity_trim.is_some() && !sync => store.media.set_activity_trim(None),
        "activityTrim.goToStart" | "activityTrim.goToEnd" if has_activity_data => {
            let window = resolve_activity_window(duration, store.media.activity_trim);
            let second = if found.command.id == "activityTrim.goToStart" { window.start_second } else { window.end_second };
            playback::commit_scrub(store, second);
        }
        "timeline.fitAll" if timing.has_activity || has_video => viewport::reset_view(store),
        "timeline.fitMedia" => return viewport::fit_target(store, viewport::FitTargetId::All),
        "timeline.fitActivity" => return viewport::fit_target(store, viewport::FitTargetId::Activity),
        "timeline.fitVideo" => return viewport::fit_target(store, viewport::FitTargetId::Video),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::slices::editor::PreviewPlaybackState;
    use crate::store::slices::media::ActivitySummary;
    use ovrley_core::activity::schema::ParsedActivity;

    fn with_activity(duration_seconds: f64) -> Store {
        let mut store = Store::default();
        store.media.activity_summary = Some(ActivitySummary { duration_seconds, ..ActivitySummary::of(&ParsedActivity::default()) });
        reconcile_player(&mut store);
        store
    }

    fn key(name: &str) -> KeyInput {
        KeyInput::new(name)
    }

    #[test]
    fn transport_keys_need_something_to_play() {
        let mut store = Store::default();
        reconcile_player(&mut store);
        assert!(!handle_player_key(&mut store, &key("space"), Instant::now()), "the fallback duration alone plays nothing");
        let mut store = with_activity(30.0);
        assert!(handle_player_key(&mut store, &key("space"), Instant::now()));
        assert_eq!(store.editor.preview_playback_state, PreviewPlaybackState::Playing);
        assert!(handle_player_key(&mut store, &key("space"), Instant::now()));
        assert_eq!(store.editor.preview_playback_state, PreviewPlaybackState::Paused);
        assert!(handle_player_key(&mut store, &key("end"), Instant::now()));
        assert_eq!(store.editor.selected_second, 30.0);
        assert!(handle_player_key(&mut store, &key("home"), Instant::now()));
        assert_eq!(store.editor.selected_second, 0.0);
        assert!(!handle_player_key(&mut store, &key("m"), Instant::now()), "mute needs a video");
    }

    #[test]
    fn i_and_o_trim_the_activity_at_the_playhead_and_mod_x_resets() {
        use crate::lib::activity::ActivityTrim;
        let mut store = with_activity(60.0);
        store.editor.set_selected_second(10.0);
        assert!(handle_player_key(&mut store, &key("i"), Instant::now()));
        store.editor.set_selected_second(40.0);
        assert!(handle_player_key(&mut store, &key("o"), Instant::now()));
        assert_eq!(store.media.activity_trim, Some(ActivityTrim { start_second: 10.0, end_second: 40.0 }));
        assert!(handle_player_key(&mut store, &KeyInput { alt: true, ..key("i") }, Instant::now()));
        assert_eq!(store.editor.selected_second, 10.0);
        let reset = if cfg!(target_os = "macos") { KeyInput { meta: true, ..key("x") } } else { KeyInput { ctrl: true, ..key("x") } };
        assert!(handle_player_key(&mut store, &reset, Instant::now()));
        assert_eq!(store.media.activity_trim, None);
        assert!(!handle_player_key(&mut store, &reset, Instant::now()), "nothing to reset");
    }

    #[test]
    fn arrows_step_only_in_the_player_workspace_and_repeats_and_overlays_block() {
        let mut store = with_activity(30.0);
        assert!(!handle_player_key(&mut store, &key("arrowright"), Instant::now()));
        store.editor_shell.set_active_keyboard_workspace(KeyboardWorkspace::Player);
        assert!(handle_player_key(&mut store, &key("arrowright"), Instant::now()));
        assert_eq!(store.editor.selected_second, 1.0);
        let repeat = KeyInput { repeat: true, ..key("arrowright") };
        assert!(!handle_player_key(&mut store, &repeat, Instant::now()));
        store.editor_shell.settings_open = true;
        assert!(!handle_player_key(&mut store, &key("space"), Instant::now()));
    }
}
