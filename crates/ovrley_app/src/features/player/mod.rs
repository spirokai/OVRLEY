//! The overlay player: playback, the timeline viewport, lanes, gestures, edits at the playhead,
//! filmstrips and the video preview, and the cached timeline they all read.

use std::rc::Rc;
use std::time::Instant;
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use crate::activity::{trim_activity_edge, ActivityWindow, TrimEdge};
use crate::dialogs::filename_from_selected_path;
use crate::features::canvas::keyboard::overlay_open;
use crate::features::shell::KeyboardWorkspace;
use crate::features::toolbar::LeftDrawerTool;
use crate::shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::store::Store;
use crate::store::media::ActivitySource;
use crate::video::{ImportedVideo, VideoEditsMap};
use crate::video::collection::{build_timeline_program, derive_video_warnings, TimelineProgram, VideoDragPreview, VideoWarnings};

pub mod edits;
pub mod filmstrips;
pub mod gestures;
pub mod lanes;
pub mod playback;
pub mod preview;
pub mod viewport;

/// The video sync drawer is the active tool.
pub fn video_sync_mode(store: &Store) -> bool {
    store.layout.active_left_drawer_tool == LeftDrawerTool::VideoSync
}

/// The activity's file name: its source's, else the parsed file's.
fn activity_filename(store: &Store) -> Option<String> {
    let from_source = store.media.activity_source.as_ref().and_then(|ActivitySource::File { path }| filename_from_selected_path(path).map(str::to_string));
    from_source.or_else(|| store.media.activity_summary.as_ref().and_then(|summary| summary.file_name.clone()))
}

/// The lanes' view model.
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
        crate::video::edits::segment_before_cut(&clip.edits, &draft.cut_id).ok().map(|key| (draft.video_id.clone(), key))
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
        activity_trim: (!sync).then(|| (ActivityWindow::new(duration, trim), dragged_edge)),
        show_cut_markers: !sync,
        dragged_cut: draft.as_ref().map(|draft| (draft.video_id.as_str(), draft.cut_id.as_str())),
        filmstrips: &store.filmstrips,
        joins: &joins,
        dragged_segment: dragged_segment.as_ref().map(|(video, key)| (video.as_str(), key.as_str())),
    };
    (lanes::video_lane(&inputs), lanes::activity_lane(&inputs))
}

/// The toolbar's `[` and `]` without a video: the activity's part before or after the playhead.
pub fn trim_activity_at_playhead(store: &mut Store, side: crate::video::segments::TrimSide) {
    use crate::video::segments::TrimSide;
    let edge = if side == TrimSide::Start { TrimEdge::Start } else { TrimEdge::End };
    let second = playback::player_timing(store).clamped_playhead;
    set_activity_trim_edge(store, edge, second);
}

/// The dialog's answer: the merged part takes the chosen export state. An agent waiting for
/// it gets its reply.
pub fn resolve_cut_removal(store: &mut Store, excluded: bool) {
    let request = store.video_import.cut_removal_request.clone();
    let result = crate::store::videos::resolve_cut_removal(store, excluded);
    if let Some(reply) = store.player.cut_removal_reply.take() {
        reply.send(result.map(|()| serde_json::json!({ "removed": true, "excluded": excluded, "videoId": request.map(|request| request.video_id) })));
    }
}

/// "Keep the cut", Escape or the scrim: the cut stays. A waiting agent learns it.
pub fn dismiss_cut_removal(store: &mut Store) {
    crate::store::videos::dismiss_cut_removal(store);
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

/// The player's state outside the document.
#[derive(Debug, Default)]
pub struct PlayerState {
    /// The cached timeline.
    pub timeline: TimelineCache,
    /// The timeline clock.
    pub playback: playback::PlaybackEngine,
    /// The video players exist, so MDK can be the video clock.
    pub video_clock_available: bool,
    /// Files MDK could not play this session. Their pieces play on the timeline clock.
    pub unplayable_paths: std::collections::BTreeSet<String>,
    /// The viewport's measurements and rule inputs.
    pub viewport: viewport::ViewportState,
    /// A scrub, playhead drag or pan.
    pub surface_gesture: Option<gestures::TimelineGesture>,
    /// A dragged activity trim handle.
    pub trim_drag: Option<gestures::TrimDrag>,
    /// A dragged clip.
    pub clip_drag: Option<gestures::ClipDrag>,
    /// A dragged cut marker.
    pub cut_drag: Option<gestures::CutDrag>,
    /// The real cut whose "Remove cut" popover is open: `(video id, cut id)`.
    pub cut_popover: Option<(String, String)>,
    /// The open join's transition panel.
    pub join_panel: Option<crate::features::transitions::JoinPanelState>,
    /// An agent waiting for the user's answer to the cut removal question.
    pub cut_removal_reply: Option<crate::store::agent::AgentReply>,
    /// A keyboard nudge of a clip, waiting for its key release.
    pub clip_nudge: Option<gestures::ClipNudge>,
    /// The lane whose clip the pointer is over; its tooltip shows.
    pub hovered_lane: Option<crate::store::videos::ClipLane>,
}

impl PlayerState {
    /// A pointer interaction runs on the timeline; it suspends the viewport's clamp and follow.
    pub fn is_dragging(&self) -> bool {
        self.surface_gesture.is_some() || self.clip_drag.is_some()
    }
}

/// The `player` reconcile step: the timeline derivations, then the playback rules and the
/// viewport rules.
pub fn reconcile_player(store: &mut Store) {
    // A cut question that went away unanswered (the videos were cleared or replaced) cancels the
    // call that waits on it, which dropping the reply does.
    if store.video_import.cut_removal_request.is_none() {
        store.player.cut_removal_reply = None;
    }
    // A nudge commits when its clip loses the keyboard.
    if let Some(nudge) = &store.player.clip_nudge {
        if focused_clip(store).as_ref() != Some(&nudge.lane) {
            gestures::commit_nudge(store);
        }
    }
    reconcile_timeline(store);
    // A join panel closes with its join.
    if let Some(panel) = &store.player.join_panel {
        if crate::features::transitions::join_model(store, &panel.key).is_none() {
            store.player.join_panel = None;
        }
    }
    playback::reconcile_playback(store);
    let dragging = store.player.is_dragging();
    viewport::reconcile_viewport(store, dragging);
}

/// Removes a video clip: playback pauses first, and the removal runs outside the history,
/// which is rebased. Without videos the background returns to checker. The preview's players let go of
/// the file at the next projection.
pub fn remove_video(store: &mut Store, video_id: &str) -> Result<(), String> {
    playback::pause(store);
    crate::features::history::change_video_collection(store, &[], |store| crate::store::videos::remove_imported_video(store, video_id))?;
    if !store.video_import.has_videos() {
        store.editor_shell.set_editor_background_mode(crate::features::shell::EditorBackgroundMode::Checker);
    }
    // Removing the video ends a cut question about it.
    if store.video_import.cut_removal_request.is_none() {
        if let Some(reply) = store.player.cut_removal_reply.take() {
            reply.send(Err("The video was removed before the user answered.".into()));
        }
    }
    Ok(())
}

/// The selected clip takes the timeline-clip keys while the player has the keyboard.
fn focused_clip(store: &Store) -> Option<crate::store::videos::ClipLane> {
    if store.editor_shell.active_keyboard_workspace != KeyboardWorkspace::Player {
        return None;
    }
    store.video_import.selected_clip.as_ref().map(|selection| selection.lane.clone())
}

/// The timeline-clip scope: arrows nudge the focused clip (0.1 s, Shift 1 s), Delete and
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

/// The player scope's window keys. Repeats are ignored, open overlays block every command, and
/// text fields keep their keys. Returns whether the key was used.
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
            use crate::video::segments::TrimSide;
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
            let window = ActivityWindow::new(duration, store.media.activity_trim);
            let second = if found.command.id == "activityTrim.goToStart" { window.start_second } else { window.end_second };
            playback::commit_scrub(store, second);
        }
        "sync.globalNudge" if has_video && has_activity_data => {
            let _ = crate::store::videos::shift_video_offsets(store, found.binding.seconds.unwrap_or(0.0), false);
        }
        "timeline.fitAll" if timing.has_activity || has_video => viewport::reset_view(store),
        "timeline.fitMedia" => return viewport::fit_target(store, viewport::FitTargetId::All),
        "timeline.fitActivity" => return viewport::fit_target(store, viewport::FitTargetId::Activity),
        "timeline.fitVideo" => return viewport::fit_target(store, viewport::FitTargetId::Video),
        _ => return false,
    }
    true
}

/// The derived timeline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoTimeline {
    pub committed: Rc<TimelineProgram>,
    /// The committed timeline itself without a drag preview.
    pub displayed: Rc<TimelineProgram>,
    /// Of the committed clips.
    pub warnings: Rc<VideoWarnings>,
}

#[derive(Clone, Debug)]
struct Inputs {
    videos: Rc<Vec<ImportedVideo>>,
    edits: Rc<VideoEditsMap>,
    defaults: TransitionDefaults,
    preview: Option<VideoDragPreview>,
    /// The external activity's duration, which the warnings read.
    activity_duration_seconds: Option<f64>,
}

/// The timeline builds, cached on the pointers of their inputs.
#[derive(Clone, Debug, Default)]
pub struct TimelineCache {
    inputs: Option<Inputs>,
    timeline: VideoTimeline,
}

impl TimelineCache {
    pub fn timeline(&self) -> &VideoTimeline {
        &self.timeline
    }
}

fn inputs_of(store: &Store) -> Inputs {
    Inputs {
        videos: Rc::clone(&store.video_import.imported_videos),
        edits: Rc::clone(&store.video_import.video_edits),
        defaults: store.video_import.transition_defaults.clone(),
        preview: store.video_import.video_drag_preview.clone(),
        activity_duration_seconds: store.media.parsed_activity.as_ref().and(store.media.activity_summary.as_ref()).map(|summary| summary.duration_seconds),
    }
}

/// Rebuilds what changed: the committed timeline on the collection or the defaults, the displayed
/// one also on the drag preview, the warnings also on the activity's duration.
pub fn reconcile_timeline(store: &mut Store) {
    let next = inputs_of(store);
    let cache = &mut store.player.timeline;
    let previous = cache.inputs.as_ref();
    let committed_changed =
        previous.is_none_or(|previous| !Rc::ptr_eq(&previous.videos, &next.videos) || !Rc::ptr_eq(&previous.edits, &next.edits) || previous.defaults != next.defaults);
    let displayed_changed = committed_changed || previous.is_some_and(|previous| previous.preview != next.preview);
    let warnings_changed = committed_changed || previous.is_some_and(|previous| previous.activity_duration_seconds != next.activity_duration_seconds);
    if committed_changed {
        cache.timeline.committed = Rc::new(build_timeline_program(&next.videos, &next.edits, None, &next.defaults));
    }
    if displayed_changed {
        cache.timeline.displayed = match &next.preview {
            None => Rc::clone(&cache.timeline.committed),
            Some(preview) => Rc::new(build_timeline_program(&next.videos, &next.edits, Some(preview), &next.defaults)),
        };
    }
    if warnings_changed {
        let committed = &cache.timeline.committed;
        cache.timeline.warnings = Rc::new(derive_video_warnings(&committed.clips, &committed.program, next.activity_duration_seconds));
    }
    cache.inputs = Some(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::editor::PreviewPlaybackState;
    use crate::store::media::ActivitySummary;
    use ovrley_core::activity::schema::ParsedActivity;
    use crate::video::fixtures::simple;
    use crate::store::videos::tests::store_with;
    use crate::store::videos::{set_video_drag_preview, set_video_offset};

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
        use ovrley_core::activity::trim::ActivityTrim;
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
        use crate::video::fixtures::simple;
        use crate::store::videos::{select_clip, tests::store_with, ClipLane, SelectedClip};
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

    #[test]
    fn one_build_per_change_and_the_displayed_timeline_follows_the_drag() {
        let mut store = store_with(vec![simple("a", 10.0, 0), simple("b", 10.0, 1)], &[("b", 30.0)]);
        reconcile_timeline(&mut store);
        let first = Rc::clone(&store.player.timeline.timeline().committed);
        assert!(Rc::ptr_eq(&first, &store.player.timeline.timeline().displayed), "without a drag both are one");
        store.editor.selected_second = 4.0;
        reconcile_timeline(&mut store);
        assert!(Rc::ptr_eq(&first, &store.player.timeline.timeline().committed), "an unrelated change rebuilds nothing");

        set_video_drag_preview(&mut store, Some(VideoDragPreview { video_id: Some("b".into()), delta_seconds: -5.0 })).unwrap();
        reconcile_timeline(&mut store);
        let timeline = store.player.timeline.timeline();
        assert!(Rc::ptr_eq(&first, &timeline.committed));
        assert_eq!(timeline.displayed.clips[1].offset_seconds(), 25.0);

        set_video_offset(&mut store, "b", 25.0, false).unwrap();
        reconcile_timeline(&mut store);
        let timeline = store.player.timeline.timeline();
        assert!(!Rc::ptr_eq(&first, &timeline.committed));
        assert!(Rc::ptr_eq(&timeline.committed, &timeline.displayed));
        assert_eq!(timeline.committed.bounds.unwrap().end_second, 35.0);
        assert!(timeline.warnings["a"].is_empty());
    }
}
