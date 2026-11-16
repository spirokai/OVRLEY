//! Port of `features/player`: the overlay player's playback, timeline viewport, lanes, gestures and
//! edits at the playhead (plan D6 to D11, D16).

pub mod gestures;
pub mod playback;
pub mod timeline;
pub mod viewport;

use crate::features::app_shell::KeyboardWorkspace;
use crate::features::overlay_editor::keyboard::overlay_open;
use crate::lib::keyboard_shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::store::Store;
use std::time::Instant;

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
