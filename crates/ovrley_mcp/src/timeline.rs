//! The tools of the timeline: playback, the activity trim, and the video clips with their cuts and
//! transitions. Times are seconds.

use crate::catalog::{Answer, NoArguments, Tool, ToolError, ToolResult};
use ovrley_app::features::player::playback::{self, player_timing};
use ovrley_app::features::player::{edits, reconcile_player};
use ovrley_app::features::transitions::{self, DEFAULT_TRANSITION_OPTION};
use ovrley_app::features::import::{self, ImportRequest};
use ovrley_app::i18n::DEFAULT_LOCALE;
use ovrley_app::activity::{trim_activity_edge, TrimEdge};
use ovrley_app::dialogs::filename_from_selected_path;
use ovrley_app::controls::seconds::{snap_seconds, SecondsMode};
use ovrley_app::video::collection::sort_clips_by_timeline;
use ovrley_app::video::edits::CreationTimeSource;
use ovrley_app::video::program::{JoinSide, PlaybackSource};
use ovrley_app::video::segments::{list_video_segments, TrimSide};
use ovrley_app::video::collection::format_video_warning;
use ovrley_app::video::Clip;
use ovrley_app::store::agent::{AbsolutePath, AgentReply};
use ovrley_app::store::services::Services;
use ovrley_app::store::editor::PreviewPlaybackState;
use ovrley_app::store::videos as collection;
use ovrley_app::store::Store;
use ovrley_core::media::edits::TimezoneMode;
use ovrley_widgets::model::validation::require_positive_f64;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Instant;

/// Where to put the playhead.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SeekArguments {
    /// The timeline second; it is clamped to the timeline, as a click on the axis is.
    second: f64,
}

/// Whether the preview's sound is off.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MuteArguments {
    /// `true` silences the video preview, `false` plays its sound.
    muted: bool,
}

/// New trim edges.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ActivityTrimArguments {
    /// The first activity second to keep. It stays inside the activity and at least 1 s before the end.
    start_second: Option<f64>,
    /// The last activity second to keep. It stays inside the activity and at least 1 s after the start.
    end_second: Option<f64>,
}

/// A video clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct VideoArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
}

/// Videos to import.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImportArguments {
    /// The videos' absolute paths: MP4, MOV or MKV.
    paths: Vec<AbsolutePath>,
    /// When given, exactly these of `paths` join, placed as the import dialog places them, and no
    /// dialog opens.
    include: Option<Vec<AbsolutePath>>,
}

/// The file of a missing video.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LocateArguments {
    /// A missing video's id.
    video_id: String,
    /// The file's absolute path.
    path: AbsolutePath,
}

/// Where to put a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// The clip's new start on the timeline, in seconds; it is snapped to the clip's nearest frame.
    offset_seconds: f64,
}

/// How far to shift every clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ShiftArguments {
    /// Seconds to move every clip by; negative moves them earlier.
    delta_seconds: f64,
}

/// Where recording times come from.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TimeSourceArguments {
    /// `detected`, or `filename` (a date-then-time timestamp in the file name, such as `20240601_103000` or `2024-06-01 10.30.00`, read as local time).
    source: CreationTimeSource,
    /// One video; without it, every video.
    video_id: Option<String>,
}

/// How camera clocks are read against the activity.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TimezoneArguments {
    /// `local` or `utc` (Apply Timezone on).
    mode: TimezoneMode,
    /// One video; without it, every video.
    video_id: Option<String>,
}

/// A clip-local second of a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CutArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// Seconds from the clip's first frame; it is converted to the clip's nearest frame.
    second: f64,
}

/// A cut of a clip, and where to move it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CutMoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// A cut id from `video_get`'s cuts.
    cut_id: String,
    /// The new clip-local second; converted to the nearest frame, which stays strictly between the neighbouring cuts.
    second: f64,
}

/// The export state of a merged part.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
enum MergedState {
    Include,
    Exclude,
}

/// A cut to remove.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CutRemoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// A cut id from `video_get`'s cuts.
    cut_id: String,
    /// When the parts on either side differ (one exported, one excluded): `include` or `exclude` the merged part. Without it
    /// OVRLEY asks the user and this call waits for the answer. Ignored when both parts are in the same state.
    merged: Option<MergedState>,
}

/// A trim of one end of a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrimArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// `start` excludes the part before the second, `end` the part after it.
    side: TrimSide,
    /// The clip-local second of the trim; converted to the nearest frame.
    second: f64,
}

/// A segment's export state.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExcludeArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// A segment key from `video_get`'s segments.
    segment_key: String,
    /// `true` leaves the segment out of the export; it stays on the timeline and in the preview.
    excluded: bool,
}

/// A part's activity overlay.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ShowOverlayArguments {
    video_id: String,
    /// A segment key from `video_get`.
    segment_key: String,
    show_overlay: bool,
}

/// A join's transition: an `xfade` type or `none`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TransitionChoice {
    /// `none` for a hard cut, or an `xfade` type such as `fade`, `dissolve` or `wipeleft`.
    #[serde(rename = "type")]
    name: String,
    /// The `xfade` duration in seconds, snapped to whole frames of the joined pieces. Omitted, an existing transition's
    /// duration is kept, else the default duration is used. Not allowed with `none`.
    duration_seconds: Option<f64>,
}

/// The transition into a segment from the piece before it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct JoinArguments {
    /// A video id from `ovrley_summary`'s videos.
    video_id: String,
    /// The segment key of a join from `video_get`'s joins.
    segment_key: String,
    /// The transition, or `null` to follow the default transition.
    transition: Option<TransitionChoice>,
}

/// The default transitions; omitted fields keep their value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TransitionDefaultsArguments {
    /// `none` or an `xfade` transition name.
    #[serde(rename = "type")]
    name: Option<String>,
    duration_seconds: Option<f64>,
    /// Videos closer together than this join without a transition.
    split_gap_threshold_seconds: Option<f64>,
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool::edit(
            "playback_play",
            "Play",
            "Starts playback from the playhead, as the player's Play button does: from the start \
             when the playhead is at the end. Fails when there is nothing to play (no activity).",
            play,
        ),
        Tool::edit(
            "playback_pause",
            "Pause",
            "Pauses playback at the playhead, as the player's Pause button does. Fails when there \
             is nothing to play.",
            pause,
        ),
        Tool::edit(
            "playhead_seek",
            "Move the playhead",
            "Moves the playhead to a timeline second and pauses there, as a click on the \
             timeline's axis does. The second is clamped to the timeline (ovrley_summary's \
             playback.timelineMinimum to playback.totalDurationSeconds).",
            seek,
        ),
        Tool::edit(
            "video_mute_set",
            "Mute the video preview",
            "Turns the sound of the video preview off or on, as the player's mute button does. \
             Fails without videos, where the player has no mute button. Not an undo step, and the \
             export is not affected.",
            set_muted,
        ),
        Tool::edit(
            "activity_trim_set",
            "Trim the activity",
            "Trims the activity to the part an overlay export renders when no video decides it, \
             as dragging the activity lane's edge handles does: give startSecond and/or endSecond \
             in activity seconds. Each edge stays inside the activity and the edges at least 1 s \
             apart; a trim covering the whole activity is no trim. One undo step.",
            set_activity_trim,
        ),
        Tool::edit(
            "activity_trim_reset",
            "Reset the activity trim",
            "Removes the activity trim, so the whole activity renders, as Mod+X does. One undo \
             step.",
            reset_activity_trim,
        ),
        Tool::waiting(
            "video_import",
            "Import videos",
            "Imports videos as the import dialog does. Without include, one video into a project \
             without videos joins at once; otherwise the dialog opens and the call waits for the \
             user (it fails when the user cancels). With include, exactly those paths join, \
             placed by recording time, without a dialog; a listed path that cannot join \
             (unreadable, already imported, another resolution than the overlay) fails the call \
             and nothing joins. Replies with the new videos and the files that failed. Not \
             undoable.",
            import_videos,
        ),
        Tool::read(
            "video_get",
            "Read a video clip",
            "Describes one video clip on the timeline: its file, start (offset), frame rate and \
             frame count, its cuts, its segments (frames, seconds, excluded from export), the \
             joins into its segments with their transitions, and its warnings. Segment keys and \
             cut ids are what the cut and segment tools take.",
            get_video,
        ),
        Tool::waiting(
            "video_locate",
            "Locate a missing video",
            "Gives a missing video its file, as Locate video… does: the file is read under the \
             video's id and keeps its edits (cuts past the end of a shorter file go). Not \
             undoable.",
            locate_video,
        )
        .destructive(),
        Tool::edit(
            "video_move",
            "Move a video clip",
            "Sets a clip's start on the timeline, as dragging the clip does: the offset is \
             snapped to the clip's nearest frame. One undo step.",
            move_video,
        ),
        Tool::edit(
            "videos_shift",
            "Shift every video clip",
            "Moves every clip by the same number of seconds, keeping their relative placement, as \
             dragging the activity lane or the [ and ] keys do. One undo step.",
            shift_videos,
        ),
        Tool::edit(
            "video_sync",
            "Sync a video",
            "Places one video by its recording time again, as the drawer's Sync the video does, \
             also after it was placed by hand. One undo step.",
            sync_video,
        ),
        Tool::edit(
            "videos_sync",
            "Sync all videos",
            "Places every video by its recording time again, also the ones placed by hand. One \
             undo step.",
            sync_videos,
        ),
        Tool::edit(
            "video_time_source_set",
            "Set where recording times come from",
            "Reads a video's recording time (or every video's) as detected, or from a \
             date-then-time timestamp in the file name (e.g. 20240601_103000), then places it \
             again. filename fails for a name without a timestamp. One undo step.",
            set_time_source,
        ),
        Tool::edit(
            "video_timezone_set",
            "Set Apply Timezone",
            "Reads a video's camera clock (or every video's) as local time or as UTC, then places \
             it again. Needs an activity with a timezone. One undo step.",
            set_timezone,
        ),
        Tool::edit(
            "video_remove",
            "Remove a video clip",
            "Removes a clip and its edits from the timeline, as Delete on the selected clip does. \
             Playback pauses. This is not an undo step: undo never brings the clip back.",
            remove_video,
        )
        .destructive(),
        Tool::edit(
            "videos_clear",
            "Remove all videos",
            "Removes every video from the project, as the video panel's Remove all does. Not \
             undoable.",
            clear_videos,
        )
        .destructive(),
        Tool::edit(
            "video_cut",
            "Cut a video clip",
            "Splits a clip at a clip-local second, as C at the playhead does. The second is \
             converted to the clip's nearest frame; the call fails on the clip's first or last \
             frame or on an existing cut. The new part keeps the export state of the part it \
             comes from. One undo step.",
            cut_video,
        ),
        Tool::edit(
            "video_cut_move",
            "Move a cut",
            "Moves a cut to another clip-local second (its nearest frame), strictly between its \
             neighbouring cuts or the clip's ends, as dragging the cut marker does. Both parts \
             keep their edits. One undo step.",
            move_cut,
        ),
        Tool::waiting(
            "video_cut_remove",
            "Remove a cut",
            "Removes a cut, merging its two parts, as the cut marker's Remove cut does. When one \
             part is exported and the other excluded, give merged (include or exclude); without \
             it OVRLEY asks the user and the call waits for the answer ({removed: false} when the \
             user keeps the cut). One undo step.",
            remove_cut,
        ),
        Tool::edit(
            "video_trim",
            "Trim a video clip",
            "Trims one end of a clip at a clip-local second (its nearest frame), as [ and ] at \
             the playhead do: the outer part is excluded from export. An end already trimmed \
             moves its trim instead. A trim never passes another cut. One undo step.",
            trim_video,
        ),
        Tool::edit(
            "video_segment_set_excluded",
            "Exclude or include a part",
            "Excludes a part (segment) of a clip from the export or includes it again, as X does. \
             Segment keys come from video_get. One undo step.",
            set_excluded,
        ),
        Tool::edit(
            "video_segment_set_overlay",
            "Show or hide the overlay over a part",
            "Shows or hides the activity widgets over one part of a video, in the preview and the \
             render. One undo step.",
            set_show_overlay,
        ),
        Tool::edit(
            "video_join_transition_set",
            "Set a join's transition",
            "Sets the transition at a join (where one video piece follows another), as the join \
             marker's panel does: an xfade type with its duration, none for a hard cut, or null \
             to follow the default transition. Joins and their segment keys come from video_get. \
             One undo step.",
            set_join_transition,
        ),
        Tool::edit(
            "transition_defaults_set",
            "Set the default transitions",
            "Sets the default transition, its duration and the continuity threshold, as the video \
             panel does. Not undoable.",
            set_transition_defaults,
        ),
    ]
}

/// The playback part of `ovrley_summary`.
pub(crate) fn describe_playback(store: &Store) -> Value {
    let timing = player_timing(store);
    let state = match store.editor.preview_playback_state {
        PreviewPlaybackState::Paused => "paused",
        PreviewPlaybackState::Playing => "playing",
        PreviewPlaybackState::Scrubbing => "scrubbing",
    };
    let clock = match store.editor.preview_playback_source {
        PlaybackSource::Video => "video",
        PlaybackSource::Timeline => "timeline",
    };
    json!({
        "state": state,
        "clock": clock,
        "muted": store.editor.is_video_muted,
        "playheadSecond": timing.clamped_playhead,
        "timelineMinimum": timing.timeline_minimum,
        "totalDurationSeconds": timing.total_duration,
        "canPlay": timing.has_activity,
    })
}

/// The activity trim part of `ovrley_summary`: `null` for the whole activity.
pub(crate) fn describe_activity_trim(store: &Store) -> Value {
    store.media.activity_trim.map_or(
        Value::Null,
        |trim| json!({ "startSecond": trim.start_second, "endSecond": trim.end_second }),
    )
}

/// The videos part of `ovrley_summary`, in timeline order.
pub(crate) fn describe_videos(store: &Store) -> Value {
    let timeline = store.player.timeline.timeline();
    sort_clips_by_timeline(&timeline.committed.clips)
        .iter()
        .map(|clip| {
            let path = &clip.video.path;
            json!({
                "id": clip.id(),
                "fileName": filename_from_selected_path(path).unwrap_or(path),
                "offsetSeconds": clip.offset_seconds(),
                "durationSeconds": clip.duration_seconds(),
                "missing": clip.video.missing,
                "warningCount": timeline.warnings.get(clip.id()).map_or(0, Vec::len),
            })
        })
        .collect()
}

fn find_clip<'store>(store: &'store Store, video_id: &str) -> Result<&'store Clip, ToolError> {
    store
        .player
        .timeline
        .timeline()
        .committed
        .clips
        .iter()
        .find(|clip| clip.id() == video_id)
        .ok_or_else(|| ToolError::Failed(format!("There is no video {video_id}.")))
}

/// One clip, as `video_get` describes it.
fn describe_video(store: &Store, video_id: &str) -> ToolResult {
    let clip = find_clip(store, video_id)?;
    let timeline = store.player.timeline.timeline();
    let fps = clip.fps();
    let cuts: Vec<Value> = clip
        .cuts()
        .iter()
        .map(|cut| {
            json!({ "id": cut.id, "frame": cut.frame, "second": fps.frames_to_seconds(cut.frame) })
        })
        .collect();
    let segments: Vec<Value> = list_video_segments(clip)
        .into_iter()
        .map(|segment| {
            json!({
                "key": segment.key,
                "startFrame": segment.start_frame,
                "endFrame": segment.end_frame,
                "startSecond": fps.frames_to_seconds(segment.start_frame),
                "endSecond": fps.frames_to_seconds(segment.end_frame),
                "excluded": segment.edits.excluded,
                "skipOverlay": segment.edits.skip_overlay,
                "transitionOverride": segment.edits.transition_in,
            })
        })
        .collect();
    let joins: Vec<Value> = timeline
        .committed
        .program
        .joins
        .iter()
        .filter(|join| join.video_id == video_id)
        .map(|join| {
            let skipped = join.skipped_default.map(|skipped| {
                let side = match skipped.side {
                    JoinSide::Previous => "previous",
                    JoinSide::Next => "next",
                };
                json!({
                    "side": side,
                    "durationSeconds": skipped.duration_seconds,
                    "roomSeconds": skipped.room_seconds,
                })
            });
            json!({
                "segmentKey": join.segment_key,
                "transition": join.transition,
                "isDefault": join.is_default,
                "skippedDefault": skipped,
            })
        })
        .collect();
    let warnings: Vec<String> = timeline
        .warnings
        .get(video_id)
        .into_iter()
        .flatten()
        .map(|warning| format_video_warning(warning, DEFAULT_LOCALE))
        .collect();
    // The drawer's detail rows, in English.
    let timezone = store
        .media
        .sync_activity()
        .and_then(|activity| activity.timezone);
    let rows = ovrley_app::features::toolbar::metadata_rows(
        clip,
        timezone.as_deref(),
        DEFAULT_LOCALE,
    );
    let details: Map<String, Value> = rows
        .into_iter()
        .map(|row| (row.label, Value::String(row.value)))
        .collect();
    let path = &clip.video.path;
    Ok(json!({
        "id": clip.id(),
        "path": path,
        "fileName": filename_from_selected_path(path).unwrap_or(path),
        "offsetSeconds": clip.offset_seconds(),
        "durationSeconds": clip.duration_seconds(),
        "frameRate": fps.ffmpeg_arg(),
        "frameCount": clip.frame_count(),
        "missing": clip.video.missing,
        "offsetSource": clip.edits.offset_source,
        "timezoneMode": clip.edits.timezone_mode,
        "creationTime": clip.video.creation_time,
        "timeSource": clip.video.time_source,
        "details": details,
        "cuts": cuts,
        "segments": segments,
        "joins": joins,
        "warnings": warnings,
    }))
}

/// A clip after an edit. The timeline cache is refreshed first, as after a dispatch.
fn answer_video(store: &mut Store, video_id: &str) -> ToolResult {
    reconcile_player(store);
    describe_video(store, video_id)
}

/// Every clip after an edit.
fn answer_videos(store: &mut Store) -> ToolResult {
    reconcile_player(store);
    Ok(json!({ "videos": describe_videos(store) }))
}

fn answer_playback(store: &Store) -> ToolResult {
    Ok(json!({ "playback": describe_playback(store) }))
}

fn require_something_to_play(store: &Store) -> Result<(), ToolError> {
    if player_timing(store).has_activity {
        Ok(())
    } else {
        Err(ToolError::Failed(
            "There is nothing to play: import an activity first.".into(),
        ))
    }
}

fn play(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    require_something_to_play(store)?;
    playback::play(store, Instant::now());
    answer_playback(store)
}

fn pause(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    require_something_to_play(store)?;
    playback::pause(store);
    answer_playback(store)
}

fn seek(args: SeekArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    playback::commit_scrub(store, args.second);
    answer_playback(store)
}

fn set_muted(args: MuteArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    if !store.video_import.has_videos() {
        return Err(ToolError::Failed("No videos are on the timeline.".into()));
    }
    store.editor.is_video_muted = args.muted;
    answer_playback(store)
}

fn set_activity_trim(
    args: ActivityTrimArguments,
    store: &mut Store,
    _: &mut Services,
) -> ToolResult {
    let Some(duration) = store
        .media
        .activity_summary
        .as_ref()
        .map(|summary| summary.duration_seconds)
    else {
        return Err(ToolError::Failed("No activity is loaded.".into()));
    };
    let current = store.media.activity_trim;
    let edge = |trim, edge, second| trim_activity_edge(trim, duration, edge, second);
    let trim = match (args.start_second, args.end_second) {
        // A new window is clamped against the activity only, not against the window it replaces.
        (Some(start), Some(end)) => edge(edge(None, TrimEdge::Start, start), TrimEdge::End, end),
        (Some(start), None) => edge(current, TrimEdge::Start, start),
        (None, Some(end)) => edge(current, TrimEdge::End, end),
        (None, None) => {
            return Err(ToolError::InvalidParams(
                "Give startSecond, endSecond or both.".into(),
            ))
        }
    };
    store.media.set_activity_trim(trim);
    Ok(json!({ "activityTrim": describe_activity_trim(store) }))
}

fn reset_activity_trim(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    store.media.set_activity_trim(None);
    Ok(json!({ "activityTrim": describe_activity_trim(store) }))
}

fn import_videos(
    args: ImportArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let paths: Vec<String> = args.paths.into_iter().map(String::from).collect();
    let include: Option<Vec<String>> = args
        .include
        .map(|include| include.into_iter().map(String::from).collect());
    if let Some(path) = include.iter().flatten().find(|path| !paths.contains(path)) {
        return Err(ToolError::InvalidParams(format!(
            "include lists a path that is not in paths: {path}"
        )));
    }
    let request = ImportRequest {
        reply: Some(reply),
        include,
    };
    import::start_import(store, services, paths, request)?;
    Ok(Answer::Later)
}

fn get_video(args: VideoArguments, store: &Store, _: &Services) -> ToolResult {
    describe_video(store, &args.video_id)
}

fn locate_video(
    args: LocateArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    ovrley_app::features::toolbar::locate_missing_video(
        store,
        services,
        args.video_id,
        args.path.into(),
        Some(reply),
    );
    Ok(Answer::Later)
}

fn move_video(args: MoveArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let fps = find_clip(store, &args.video_id)?.fps();
    let offset = snap_seconds(args.offset_seconds, Some(fps), SecondsMode::Time);
    collection::set_video_offset(store, &args.video_id, offset, false)?;
    answer_video(store, &args.video_id)
}

fn shift_videos(args: ShiftArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    collection::shift_video_offsets(store, args.delta_seconds, false)?;
    answer_videos(store)
}

fn sync_video(args: VideoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    collection::reset_video_offset(store, &args.video_id)?;
    answer_video(store, &args.video_id)
}

fn sync_videos(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    collection::sync_all_videos(store)?;
    answer_videos(store)
}

fn set_time_source(args: TimeSourceArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    if let Some(video_id) = &args.video_id {
        collection::set_video_creation_time_source(store, video_id, args.source)?;
        return answer_video(store, video_id);
    }
    match args.source {
        CreationTimeSource::Filename => collection::set_video_creation_times_from_filenames(store)?,
        CreationTimeSource::Detected => collection::reset_video_creation_times(store)?,
    }
    answer_videos(store)
}

fn set_timezone(args: TimezoneArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let activity = store.media.sync_activity();
    if activity.and_then(|activity| activity.timezone).is_none() {
        return Err(ToolError::Failed(
            "Apply Timezone needs an activity with a timezone.".into(),
        ));
    }
    if let Some(video_id) = &args.video_id {
        collection::set_video_timezone_mode(store, video_id, args.mode)?;
        return answer_video(store, video_id);
    }
    collection::set_video_sync_timezone_mode(store, args.mode)?;
    answer_videos(store)
}

fn remove_video(args: VideoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    // Checked first: removing pauses playback before it looks for the clip.
    find_clip(store, &args.video_id)?;
    ovrley_app::features::player::remove_video(store, &args.video_id)?;
    answer_videos(store)
}

fn clear_videos(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    import::clear_videos(store);
    Ok(json!({ "videoCount": store.video_import.imported_videos.len() }))
}

fn cut_video(args: CutArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let frame = find_clip(store, &args.video_id)?
        .fps()
        .seconds_to_frames(args.second);
    collection::cut_video(store, &args.video_id, frame)?;
    answer_video(store, &args.video_id)
}

fn move_cut(args: CutMoveArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let frame = find_clip(store, &args.video_id)?
        .fps()
        .seconds_to_frames(args.second);
    collection::move_cut(store, &args.video_id, &args.cut_id, frame)?;
    answer_video(store, &args.video_id)
}

fn remove_cut(
    args: CutRemoveArguments,
    store: &mut Store,
    _: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    if store.video_import.cut_removal_request.is_some() {
        return Err(ToolError::Failed(
            "OVRLEY is already asking the user about removing a cut.".into(),
        ));
    }
    let removal = collection::request_cut_removal(store, &args.video_id, &args.cut_id, None)?;
    if removal == collection::CutRemoval::Asked {
        let Some(merged) = args.merged else {
            store.player.cut_removal_reply = Some(reply);
            return Ok(Answer::Later);
        };
        collection::resolve_cut_removal(store, matches!(merged, MergedState::Exclude))?;
    }
    Ok(Answer::Now(answer_video(store, &args.video_id)))
}

fn trim_video(args: TrimArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let frame = find_clip(store, &args.video_id)?
        .fps()
        .seconds_to_frames(args.second);
    edits::trim_clip(store, &args.video_id, args.side, frame)?;
    answer_video(store, &args.video_id)
}

fn set_excluded(args: ExcludeArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    collection::set_video_segment_excluded(
        store,
        &args.video_id,
        &args.segment_key,
        args.excluded,
    )?;
    answer_video(store, &args.video_id)
}

fn set_show_overlay(args: ShowOverlayArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let skip = !args.show_overlay;
    collection::set_video_segment_skip_overlay(store, &args.video_id, &args.segment_key, skip)?;
    answer_video(store, &args.video_id)
}

fn set_join_transition(args: JoinArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    find_clip(store, &args.video_id)?;
    let key = transitions::join_key_of_segment(store, &args.video_id, &args.segment_key)
        .ok_or_else(|| {
            ToolError::Failed(format!(
                "Segment {} of video {} is not a join.",
                args.segment_key, args.video_id
            ))
        })?;
    let (name, duration) = match &args.transition {
        None => (DEFAULT_TRANSITION_OPTION, None),
        Some(choice) if choice.name == DEFAULT_TRANSITION_OPTION => {
            return Err(ToolError::InvalidParams(
                "Use null, not a type, to follow the default transition".into(),
            ))
        }
        Some(choice) => (choice.name.as_str(), choice.duration_seconds),
    };
    if let Some(seconds) = duration {
        // Before snapping, which would make a zero duration one frame.
        require_positive_f64(seconds, "durationSeconds")
            .map_err(|error| ToolError::InvalidParams(error.to_string()))?;
    }
    transitions::set_join(store, &key, name, duration)?;
    answer_video(store, &args.video_id)
}

fn set_transition_defaults(
    args: TransitionDefaultsArguments,
    store: &mut Store,
    _: &mut Services,
) -> ToolResult {
    ovrley_app::controls::seconds::set_panel_defaults(store, |defaults| {
        if let Some(name) = args.name {
            defaults.transition_type = name;
        }
        if let Some(seconds) = args.duration_seconds {
            defaults.duration_seconds = seconds;
        }
        if let Some(seconds) = args.split_gap_threshold_seconds {
            defaults.split_gap_threshold_seconds = seconds;
        }
    })?;
    Ok(json!(store.video_import.transition_defaults))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{video_metadata, with_activity, with_videos, Editor};

    fn editor_with(clips: &[(&str, f64, f64)]) -> Editor {
        Editor::with(with_videos(clips), json!({}))
    }

    fn cut_at_five(editor: &Editor) -> String {
        let cut = editor.ok("video_cut", json!({"videoId": "a", "second": 5}));
        cut["cuts"][0]["id"].as_str().unwrap().to_string()
    }

    fn exclude(editor: &Editor, segment_key: &str) -> Value {
        editor.ok(
            "video_segment_set_excluded",
            json!({"videoId": "a", "segmentKey": segment_key, "excluded": true}),
        )
    }

    #[test]
    fn play_pause_and_seek_need_something_to_play() {
        Editor::new().failed("playback_play", json!({}));
        let editor = Editor::with(with_activity(60.0), json!({}));
        assert_eq!(
            editor.ok("playback_play", json!({}))["playback"]["state"],
            "playing"
        );
        assert_eq!(
            editor.ok("playback_pause", json!({}))["playback"]["state"],
            "paused"
        );
        let seek = editor.ok("playhead_seek", json!({"second": 90}));
        assert_eq!(
            seek["playback"]["playheadSecond"], 60.0,
            "clamped to the timeline"
        );
        editor.invalid("playhead_seek", json!({"second": "x"}));
        let summary = editor.ok("ovrley_summary", json!({}));
        assert_eq!(summary["playback"]["totalDurationSeconds"], 60.0);
        assert_eq!(editor.undo_steps(), 0, "playback is no edit");
    }

    #[test]
    fn mute_needs_videos_and_is_no_undo_step() {
        Editor::with(with_activity(60.0), json!({}))
            .failed("video_mute_set", json!({"muted": true}));
        let editor = editor_with(&[("a", 0.0, 10.0)]);
        assert_eq!(
            editor.ok("video_mute_set", json!({"muted": true}))["playback"]["muted"],
            true
        );
        assert_eq!(
            editor.ok("ovrley_summary", json!({}))["playback"]["muted"],
            true
        );
        assert_eq!(editor.undo_steps(), 0);
    }

    #[test]
    fn the_activity_trim_is_set_and_reset_in_one_undo_step_each() {
        let editor = Editor::with(with_activity(60.0), json!({}));
        let trim = |start: f64, end: f64| json!({"startSecond": start, "endSecond": end});
        let first = editor.ok("activity_trim_set", trim(0.0, 8.0));
        assert_eq!(first["activityTrim"], trim(0.0, 8.0));
        let replaced = editor.ok("activity_trim_set", trim(10.0, 50.0));
        assert_eq!(
            replaced["activityTrim"],
            trim(10.0, 50.0),
            "a window past the one it replaces"
        );
        let clamped = editor.ok("activity_trim_set", json!({"endSecond": 5}));
        assert_eq!(
            clamped["activityTrim"]["endSecond"], 11.0,
            "edges stay 1 s apart"
        );
        assert!(editor.ok("activity_trim_reset", json!({}))["activityTrim"].is_null());
        assert_eq!(editor.undo_steps(), 4);
        assert!(editor.ok("ovrley_summary", json!({}))["activityTrim"].is_null());
        editor.invalid("activity_trim_set", json!({}));
        Editor::new().failed("activity_trim_set", json!({"startSecond": 1}));
    }

    #[test]
    fn video_tools_read_move_shift_and_remove_clips() {
        let editor = editor_with(&[("a", 0.0, 10.0), ("b", 20.0, 10.0)]);
        let summary = editor.ok("ovrley_summary", json!({}));
        assert_eq!(summary["videos"].as_array().unwrap().len(), 2);
        let video = editor.ok("video_get", json!({"videoId": "b"}));
        assert_eq!(
            (&video["frameRate"], &video["frameCount"]),
            (&json!("30/1"), &json!(300))
        );
        assert_eq!(video["segments"][0]["key"], "start");
        let moved = editor.ok(
            "video_move",
            json!({"videoId": "b", "offsetSeconds": 12.345}),
        );
        let offset = moved["offsetSeconds"].as_f64().unwrap();
        assert!(
            (offset - 370.0 / 30.0).abs() < 1e-9,
            "snapped to the clip's frames: {offset}"
        );
        let shifted = editor.ok("videos_shift", json!({"deltaSeconds": -2}));
        assert_eq!(shifted["videos"][0]["offsetSeconds"], -2.0);
        assert_eq!(editor.undo_steps(), 2);
        let removed = editor.ok("video_remove", json!({"videoId": "a"}));
        assert_eq!(removed["videos"].as_array().unwrap().len(), 1);
        assert_eq!(editor.undo_steps(), 2, "removal is no undo step");
        editor.failed("video_get", json!({"videoId": "a"}));
    }

    #[test]
    fn cuts_trims_and_exclusion_are_one_undo_step_each() {
        let editor = editor_with(&[("a", 0.0, 10.0)]);
        let cut = editor.ok("video_cut", json!({"videoId": "a", "second": 5}));
        let cut_id = cut["cuts"][0]["id"].as_str().unwrap();
        assert_eq!(cut["cuts"][0]["frame"], 150);
        editor.failed("video_cut", json!({"videoId": "a", "second": 5}));
        let moved = editor.ok(
            "video_cut_move",
            json!({"videoId": "a", "cutId": cut_id, "second": 6}),
        );
        assert_eq!(moved["cuts"][0]["frame"], 180);
        let trimmed = editor.ok(
            "video_trim",
            json!({"videoId": "a", "side": "start", "second": 1}),
        );
        assert_eq!(trimmed["segments"][0]["excluded"], true);
        assert_eq!(exclude(&editor, cut_id)["segments"][2]["excluded"], true);
        assert_eq!(editor.undo_steps(), 4);
    }

    #[test]
    fn removing_a_cut_between_alike_parts_merges_at_once_and_merged_answers_the_question() {
        let editor = editor_with(&[("a", 0.0, 10.0)]);
        let remove = |cut_id: &str| {
            let arguments = json!({"videoId": "a", "cutId": cut_id, "merged": "exclude"});
            editor.ok("video_cut_remove", arguments)
        };
        let merged = remove(&cut_at_five(&editor));
        assert_eq!(merged["cuts"], json!([]));
        assert_eq!(
            merged["segments"][0]["excluded"], false,
            "alike parts ignore merged"
        );
        let second = cut_at_five(&editor);
        exclude(&editor, &second);
        assert_eq!(remove(&second)["segments"][0]["excluded"], true);
        assert!(editor
            .dispatcher
            .with_store(|store| store.video_import.cut_removal_request.is_none()));
    }

    #[test]
    fn without_merged_the_call_waits_for_the_user() {
        use ovrley_app::features::player::{dismiss_cut_removal, resolve_cut_removal};
        use ovrley_app::store::agent::CANCELLED;

        let editor = editor_with(&[("a", 0.0, 10.0)]);
        let ask = || {
            let cut_id = cut_at_five(&editor);
            exclude(&editor, &cut_id);
            let waiting =
                editor.start("video_cut_remove", json!({"videoId": "a", "cutId": cut_id}));
            assert!(waiting.answer.is_none());
            assert!(
                editor
                    .dispatcher
                    .with_store(|store| store.video_import.cut_removal_request.is_some()),
                "the dialog asks"
            );
            waiting
        };

        let waiting = ask();
        editor
            .dispatcher
            .dispatch(|store, _| resolve_cut_removal(store, false));
        assert_eq!(
            editor.finish(waiting),
            Ok(json!({"removed": true, "excluded": false, "videoId": "a"}))
        );

        let waiting = ask();
        editor
            .dispatcher
            .dispatch(|store, _| dismiss_cut_removal(store));
        assert_eq!(editor.finish(waiting), Ok(json!({"removed": false})));
        editor.dispatcher.with_store(|store| {
            assert_eq!(
                store.video_import.video_edits["a"].cuts.len(),
                1,
                "the cut is kept"
            );
        });

        let cut_id = editor.ok("video_get", json!({"videoId": "a"}))["cuts"][0]["id"].clone();
        let waiting = editor.start("video_cut_remove", json!({"videoId": "a", "cutId": cut_id}));
        editor.ok("videos_clear", json!({}));
        assert_eq!(
            editor.finish(waiting),
            Err(ToolError::Failed(CANCELLED.into())),
            "a question that went away cancels the call"
        );
    }

    #[test]
    fn join_transitions_are_set_snapped_reset_and_checked() {
        let editor = editor_with(&[("a", 0.0, 10.0), ("b", 10.0, 10.0)]);
        let into = |transition: Value| json!({"videoId": "b", "segmentKey": "start", "transition": transition});
        let set = |transition: Value| editor.ok("video_join_transition_set", into(transition));
        let dissolve = set(json!({"type": "dissolve", "durationSeconds": 1.01}));
        assert_eq!(
            dissolve["joins"][0]["transition"],
            json!({"type": "dissolve", "durationSeconds": 1.0}),
            "snapped to the joined frames"
        );
        assert_eq!(dissolve["joins"][0]["isDefault"], false);
        let kept = set(json!({"type": "fade"}));
        assert_eq!(
            kept["joins"][0]["transition"],
            json!({"type": "fade", "durationSeconds": 1.0}),
            "the override's duration is kept"
        );
        assert_eq!(
            set(json!({"type": "none"}))["joins"][0]["transition"]["type"],
            "none"
        );
        assert_eq!(set(Value::Null)["joins"][0]["isDefault"], true);
        assert_eq!(editor.undo_steps(), 4);
        for refused in [
            json!({"type": "none", "durationSeconds": 1}),
            json!({"type": "spin"}),
        ] {
            editor.failed("video_join_transition_set", into(refused));
        }
        for malformed in [
            json!({"type": "fade", "durationSeconds": 0}),
            json!({"type": "default"}),
        ] {
            editor.invalid("video_join_transition_set", into(malformed));
        }
        editor.failed(
            "video_join_transition_set",
            json!({"videoId": "a", "segmentKey": "start", "transition": null}),
        );
        assert_eq!(editor.undo_steps(), 4, "refused calls change nothing");
    }

    fn probe(editor: &Editor, path: &str, creation_time: &str) {
        let metadata = ovrley_core::media::probe::SourceVideoMetadata {
            creation_time: Some(creation_time.into()),
            time_source: Some("gps".into()),
            ..video_metadata(path, 10.0)
        };
        editor.fakes.video_tools.add_video(path, metadata);
    }

    #[test]
    fn a_quiet_import_answers_with_the_video_and_clear_removes_it() {
        let editor = Editor::new();
        probe(&editor, "/videos/a.mp4", "2026-07-18T08:00:00Z");
        let waiting = editor.start("video_import", json!({"paths": ["/videos/a.mp4"]}));
        assert!(waiting.answer.is_none());
        let reply = editor.finish(waiting).unwrap();
        assert_eq!(reply["videos"][0]["path"], json!("/videos/a.mp4"));
        assert_eq!(editor.ok("videos_clear", json!({}))["videoCount"], json!(0));
    }

    #[test]
    fn an_import_with_a_dialog_waits_for_the_user_and_cancelling_fails_the_call() {
        let editor = Editor::new();
        probe(&editor, "/videos/a.mp4", "2026-07-18T08:00:00Z");
        probe(&editor, "/videos/b.mp4", "2026-07-18T08:01:00Z");
        let waiting = editor.start(
            "video_import",
            json!({"paths": ["/videos/a.mp4", "/videos/b.mp4"]}),
        );
        editor.until("the review", || {
            editor.dispatcher.with_store(|store| {
                let review = store.video_import_review.review.as_ref();
                review
                    .is_some_and(|review| review.phase == import::ReviewPhase::Reviewing)
                    .then_some(())
            })
        });
        assert!(
            waiting.reply.try_recv().is_err(),
            "the call waits for the dialog"
        );
        editor
            .dispatcher
            .dispatch(|store, _| import::cancel_import_review(store));
        let result = editor.finish(waiting);
        assert!(matches!(&result, Err(ToolError::Failed(reason)) if reason.contains("cancelled")));
    }

    #[test]
    fn relative_paths_and_include_outside_paths_are_invalid() {
        let editor = Editor::new();
        editor.invalid("video_import", json!({"paths": ["a.mp4"]}));
        editor.invalid(
            "video_import",
            json!({"paths": ["/a.mp4"], "include": ["/b.mp4"]}),
        );
    }

    #[test]
    fn sync_timezone_and_defaults_tools_edit_the_collection() {
        let editor = Editor::new();
        let path = "/videos/GX_20260718_090000.mp4";
        probe(&editor, path, "2026-07-18T08:00:00Z");
        let id = editor.ok("video_import", json!({"paths": [path]}))["videos"][0]["id"].clone();
        let video = editor.ok(
            "video_time_source_set",
            json!({"source": "filename", "videoId": id}),
        );
        assert_eq!(video["timeSource"], json!("filename"));
        assert_eq!(video["details"]["Time source"], json!("Filename"));
        // The activity has no timezone.
        editor.failed("video_timezone_set", json!({"mode": "utc"}));
        let video = editor.ok(
            "video_segment_set_overlay",
            json!({"videoId": id, "segmentKey": "start", "showOverlay": false}),
        );
        assert_eq!(video["segments"][0]["skipOverlay"], json!(true));
        let defaults = editor.ok(
            "transition_defaults_set",
            json!({"type": "fade", "durationSeconds": 1.5}),
        );
        assert_eq!(
            (&defaults["type"], &defaults["durationSeconds"]),
            (&json!("fade"), &json!(1.5))
        );
        editor.failed("transition_defaults_set", json!({"type": "spin"}));
        editor.ok("video_sync", json!({"videoId": id}));
        editor.ok("videos_sync", json!({}));
        let reason = editor.failed(
            "video_locate",
            json!({"videoId": id, "path": "/videos/other.mp4"}),
        );
        assert!(reason.contains("not missing"), "{reason}");
    }
}
