//! Port of `features/player`: the overlay player's playback, timeline viewport, lanes, gestures and
//! edits at the playhead (plan D6 to D11, D16).

pub mod edits;
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
    let draft = store.player.cut_drag.as_ref().and_then(gestures::CutDrag::draft);
    let clips = gestures::apply_cut_draft(&displayed.clips, draft.as_ref());
    let joins = if sync { Vec::new() } else { crate::features::transitions::join_models(displayed, store.i18n.language) };
    let dragged_segment = draft.as_ref().and_then(|draft| {
        let clip = displayed.clips.iter().find(|clip| clip.id() == draft.video_id)?;
        crate::lib::video_program::edits::segment_before_cut(&clip.edits, &draft.cut_id).ok().map(|key| (draft.video_id.clone(), key))
    });
    let selected = gestures::apply_cut_draft_to_selection(store.video_import.selected_clip.as_ref(), draft.as_ref());
    let inputs = lanes::LaneInputs {
        view: store.editor.timeline_viewport,
        metrics: store.player.viewport.metrics,
        locale: store.i18n.language,
        clips: &clips,
        overlaps: &displayed.program.overlaps,
        warnings: &store.player.timeline.timeline().warnings,
        selected: selected.as_ref(),
        hovered: store.player.hovered_lane.as_ref(),
        has_activity: store.media.activity_summary.is_some() && timing.has_activity,
        activity_summary: store.media.activity_summary.as_ref(),
        activity_filename: activity_filename(store),
        activity_trim: (!sync).then(|| (resolve_activity_window(duration, trim), dragged_edge)),
        show_cut_markers: !sync,
        dragged_cut: draft.as_ref().map(|draft| (draft.video_id.as_str(), draft.cut_id.as_str())),
        filmstrips: &store.video_filmstrip,
        joins: &joins,
        dragged_segment: dragged_segment.as_ref().map(|(video, key)| (video.as_str(), key.as_str())),
    };
    (lanes::video_lane(&inputs), lanes::activity_lane(&inputs))
}

/// The toolbar's `[` and `]` without a video: the activity's part before or after the playhead.
pub fn trim_activity_at_playhead(store: &mut Store, side: crate::lib::video_program::segments::TrimSide) {
    use crate::lib::video_program::segments::TrimSide;
    let edge = if side == TrimSide::Start { TrimEdge::Start } else { TrimEdge::End };
    let second = playback::player_timing(store).clamped_playhead;
    set_activity_trim_edge(store, edge, second);
}

/// The dialog's answer: the merged part takes the chosen export state (D19). An agent waiting for
/// it gets its reply.
pub fn resolve_cut_removal(store: &mut Store, excluded: bool) {
    let request = store.video_import.cut_removal_request.clone();
    let result = crate::store::slices::video_import::resolve_cut_removal(store, excluded);
    if let Some(reply) = store.player.cut_removal_reply.take() {
        reply.send(result.map(|()| serde_json::json!({ "removed": true, "excluded": excluded, "videoId": request.map(|request| request.video_id) })));
    }
}

/// "Keep the cut", Escape or the scrim: the cut stays. A waiting agent learns it.
pub fn dismiss_cut_removal(store: &mut Store) {
    crate::store::slices::video_import::dismiss_cut_removal(store);
    if let Some(reply) = store.player.cut_removal_reply.take() {
        reply.send(Ok(serde_json::json!({ "removed": false })));
    }
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
    /// A dragged clip (`useClipDrag`, D9).
    pub clip_drag: Option<gestures::ClipDrag>,
    /// A dragged cut marker (`useCutDrag`, D9).
    pub cut_drag: Option<gestures::CutDrag>,
    /// The real cut whose "Remove cut" popover is open: `(video id, cut id)`.
    pub cut_popover: Option<(String, String)>,
    /// The open join's transition panel (D13).
    pub join_panel: Option<crate::features::transitions::JoinPanelState>,
    /// An agent waiting for the user's answer to the cut removal question (D19).
    pub cut_removal_reply: Option<crate::store::agent::AgentReply>,
    /// A keyboard nudge of a clip, waiting for its key release (D11).
    pub clip_nudge: Option<gestures::ClipNudge>,
    /// The lane whose clip the pointer is over; its tooltip shows.
    pub hovered_lane: Option<crate::store::slices::video_import::ClipLane>,
}

impl PlayerState {
    /// A pointer interaction runs on the timeline; it suspends the viewport's clamp and follow.
    pub fn is_dragging(&self) -> bool {
        self.surface_gesture.is_some() || self.clip_drag.is_some()
    }
}

/// The `player` reconcile step: the timeline derivations, then the playback effects (D6) and the
/// viewport rules (D7), in React's effect order.
pub fn reconcile_player(store: &mut Store) {
    // A nudge commits when its clip loses the keyboard (React: the clip's blur).
    if let Some(nudge) = &store.player.clip_nudge {
        if focused_clip(store).as_ref() != Some(&nudge.lane) {
            gestures::commit_nudge(store);
        }
    }
    timeline::reconcile_timeline(store);
    playback::reconcile_playback(store);
    let dragging = store.player.is_dragging();
    viewport::reconcile_viewport(store, dragging);
}

/// Removes a video clip (D17): playback pauses first, and the removal runs outside the history,
/// which is rebased. Phase 6 adds releasing the video's decoders.
pub fn remove_video(store: &mut Store, video_id: &str) -> Result<(), String> {
    playback::pause(store);
    crate::features::undo_redo::change_video_collection(store, &[], |store| crate::store::slices::video_import::remove_imported_video(store, video_id))?;
    // Removing the video ends a cut question about it.
    if store.video_import.cut_removal_request.is_none() {
        if let Some(reply) = store.player.cut_removal_reply.take() {
            reply.send(Err("The video was removed before the user answered.".into()));
        }
    }
    Ok(())
}

/// The selected clip takes the timeline-clip keys while the player has the keyboard (React: while
/// the clip has focus, which a press gives it).
fn focused_clip(store: &Store) -> Option<crate::store::slices::video_import::ClipLane> {
    if store.editor_shell.active_keyboard_workspace != KeyboardWorkspace::Player {
        return None;
    }
    store.video_import.selected_clip.as_ref().map(|selection| selection.lane.clone())
}

/// The timeline-clip scope (D11): arrows nudge the focused clip (0.1 s, Shift 1 s), Delete and
/// Backspace remove a video clip. Returns whether the key was used.
pub fn handle_timeline_clip_key(store: &mut Store, input: &KeyInput) -> bool {
    let Some(lane) = focused_clip(store) else { return false };
    if overlay_open(store) || !store.video_import.has_videos() {
        return false;
    }
    let Some(found) = match_keyboard_shortcut(input, Scope::TimelineClip) else { return false };
    match found.command.id.as_str() {
        "timelineClip.remove" => match lane.video_id() {
            Some(video_id) => remove_video(store, video_id).is_ok(),
            None => false,
        },
        "timelineClip.nudge" => {
            gestures::nudge_lane(store, lane, found.binding.seconds.unwrap_or(0.1));
            true
        }
        _ => false,
    }
}

/// A key release: the end of a nudge commits it.
pub fn handle_key_release(store: &mut Store, input: &KeyInput) -> bool {
    let is_nudge = match_keyboard_shortcut(input, Scope::TimelineClip).is_some_and(|found| found.command.id == "timelineClip.nudge");
    if !is_nudge || store.player.clip_nudge.is_none() {
        return false;
    }
    gestures::commit_nudge(store);
    true
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
        "trim.start" | "trim.end" if !sync && has_video => {
            use crate::lib::video_program::segments::TrimSide;
            let side = if found.command.id == "trim.start" { TrimSide::Start } else { TrimSide::End };
            return edits::trim_at_playhead(store, side);
        }
        "trim.start" | "trim.end" if !sync && has_activity_data => {
            let edge = if found.command.id == "trim.start" { TrimEdge::Start } else { TrimEdge::End };
            set_activity_trim_edge(store, edge, timing.clamped_playhead);
        }
        "videoEdit.cut" if !sync => return edits::cut_at_playhead(store),
        "videoEdit.toggleExcluded" if !sync => return edits::toggle_excluded_at_playhead(store),
        "activityTrim.reset" if store.media.activity_trim.is_some() && !sync => store.media.set_activity_trim(None),
        "activityTrim.goToStart" | "activityTrim.goToEnd" if has_activity_data => {
            let window = resolve_activity_window(duration, store.media.activity_trim);
            let second = if found.command.id == "activityTrim.goToStart" { window.start_second } else { window.end_second };
            playback::commit_scrub(store, second);
        }
        "sync.globalNudge" if has_video && has_activity_data => {
            let _ = crate::store::slices::video_import::shift_video_offsets(store, found.binding.seconds.unwrap_or(0.0), false);
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
    fn a_focused_clip_nudges_on_arrows_commits_on_release_and_delete_removes_it() {
        use crate::lib::video_program::fixtures::simple;
        use crate::store::slices::video_import::{select_clip, tests::store_with, ClipLane, SelectedClip};
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        store.media.activity_summary = Some(ActivitySummary { duration_seconds: 30.0, ..ActivitySummary::of(&ParsedActivity::default()) });
        reconcile_player(&mut store);
        store.editor_shell.set_active_keyboard_workspace(KeyboardWorkspace::Player);
        select_clip(&mut store, Some(SelectedClip { lane: ClipLane::Video("ride".into()), segment_key: None })).unwrap();
        assert!(handle_timeline_clip_key(&mut store, &KeyInput { shift: true, ..key("arrowright") }));
        assert_eq!(store.video_import.video_drag_preview.as_ref().map(|preview| preview.delta_seconds), Some(1.0));
        assert!(handle_key_release(&mut store, &KeyInput { shift: true, ..key("arrowright") }));
        assert_eq!(store.video_import.video_edits["ride"].offset_seconds, 1.0);
        reconcile_player(&mut store);
        assert!(handle_timeline_clip_key(&mut store, &key("arrowleft")));
        store.editor_shell.set_active_keyboard_workspace(KeyboardWorkspace::Editor);
        reconcile_player(&mut store);
        assert!((store.video_import.video_edits["ride"].offset_seconds - 0.9).abs() < 1e-9, "focus loss commits the nudge");
        store.editor_shell.set_active_keyboard_workspace(KeyboardWorkspace::Player);
        assert!(handle_timeline_clip_key(&mut store, &key("delete")));
        assert!(store.video_import.imported_videos.is_empty());
        assert_eq!(store.video_import.selected_clip, None);
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
