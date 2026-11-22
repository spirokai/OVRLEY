//! The player's MCP tools (phase 4): playback, the playhead, the activity trim and the video
//! clips. Split from `catalog.rs` as parent §2 lays out; the catalog lists these after its own and
//! hands them their calls. Times are seconds.

use crate::catalog::{input_schema, Answer, NoArguments, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::player::playback::{self, player_timing};
use ovrley_app::lib::video_program::program::PlaybackSource;
use ovrley_app::store::services::Services;
use ovrley_app::store::slices::editor::PreviewPlaybackState;
use ovrley_app::store::Store;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;

/// Where to put the playhead.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayheadSeekArguments {
    /// The timeline second; it is clamped to the timeline, as a click on the axis is.
    pub second: f64,
}

/// New trim edges.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivityTrimSetArguments {
    /// The first activity second to keep. It stays inside the activity and at least 1 s before the end.
    #[serde(default)]
    pub start_second: Option<f64>,
    /// The last activity second to keep. It stays inside the activity and at least 1 s after the start.
    #[serde(default)]
    pub end_second: Option<f64>,
}

/// Whether the preview's sound is off.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoMuteSetArguments {
    /// `true` silences the video preview, `false` plays its sound.
    pub muted: bool,
}

/// A video clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
}

/// Where to put a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoMoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// The clip's new start on the timeline, in seconds; it is snapped to the clip's nearest frame.
    pub offset_seconds: f64,
}

/// How far to shift every clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideosShiftArguments {
    /// Seconds to move every clip by; negative moves them earlier.
    pub delta_seconds: f64,
}

/// A clip-local second of a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoSecondArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// Seconds from the clip's first frame; it is converted to the clip's nearest frame.
    pub second: f64,
}

/// A cut of a clip, and where to move it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoCutMoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// A cut id from `video_get`'s cuts.
    pub cut_id: String,
    /// The new clip-local second; converted to the nearest frame, which stays strictly between the neighbouring cuts.
    pub second: f64,
}

/// The export state of a merged part.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MergedState {
    Include,
    Exclude,
}

/// A cut to remove.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoCutRemoveArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// A cut id from `video_get`'s cuts.
    pub cut_id: String,
    /// When the parts on either side differ (one exported, one excluded): `include` or `exclude` the merged part. Without it
    /// OVRLEY asks the user and this call waits for the answer. Ignored when both parts are in the same state.
    #[serde(default)]
    pub merged: Option<MergedState>,
}

/// Which end of a clip to trim.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TrimSideArgument {
    Start,
    End,
}

/// A trim of one end of a clip.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoTrimArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// `start` excludes the part before the second, `end` the part after it.
    pub side: TrimSideArgument,
    /// The clip-local second of the trim; converted to the nearest frame.
    pub second: f64,
}

/// A segment's export state.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoSegmentSetExcludedArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// A segment key from `video_get`'s segments.
    pub segment_key: String,
    /// `true` leaves the segment out of the export; it stays on the timeline and in the preview.
    pub excluded: bool,
}

/// A join's transition: an `xfade` type or `none`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TransitionArgument {
    /// `none` for a hard cut, or an `xfade` type such as `fade`, `dissolve` or `wipeleft`.
    #[serde(rename = "type")]
    pub transition_type: String,
    /// The `xfade` duration in seconds, snapped to whole frames of the joined pieces. Omitted, an existing transition's
    /// duration is kept, else the default duration is used. Not allowed with `none`.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
}

/// The transition into a segment from the piece before it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoJoinTransitionSetArguments {
    /// A video id from `ovrley_summary`'s videos.
    pub video_id: String,
    /// The segment key of a join from `video_get`'s joins.
    pub segment_key: String,
    /// The transition, or `null` to follow the default transition.
    pub transition: Option<TransitionArgument>,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "playback_play",
            title: "Play",
            description: "Starts playback from the playhead, as the player's Play button does: from the start when the playhead is at \
                          the end. Fails when there is nothing to play (no activity).",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "playback_pause",
            title: "Pause",
            description: "Pauses playback at the playhead, as the player's Pause button does. Fails when there is nothing to play.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "playhead_seek",
            title: "Move the playhead",
            description: "Moves the playhead to a timeline second and pauses there, as a click on the timeline's axis does. The second \
                          is clamped to the timeline (ovrley_summary's playback.timelineMinimum to playback.totalDurationSeconds).",
            input_schema: input_schema::<PlayheadSeekArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_mute_set",
            title: "Mute the video preview",
            description: "Turns the sound of the video preview off or on, as the player's mute button does. Fails without videos, \
                          where the player has no mute button. Not an undo step, and the export is not affected.",
            input_schema: input_schema::<VideoMuteSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "activity_trim_set",
            title: "Trim the activity",
            description: "Trims the activity to the part an overlay export renders when no video decides it, as dragging the \
                          activity lane's edge handles does: give start_second and/or end_second in activity seconds. Each edge stays \
                          inside the activity and the edges at least 1 s apart; a trim covering the whole activity is no trim. One undo \
                          step.",
            input_schema: input_schema::<ActivityTrimSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_get",
            title: "Read a video clip",
            description: "Describes one video clip on the timeline: its file, start (offset), frame rate and frame count, its cuts, its \
                          segments (frames, seconds, excluded from export), the joins into its segments with their transitions, and its \
                          warnings. Segment keys and cut ids are what the cut and segment tools take.",
            input_schema: input_schema::<VideoArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "video_move",
            title: "Move a video clip",
            description: "Sets a clip's start on the timeline, as dragging the clip does: the offset is snapped to the clip's nearest \
                          frame. One undo step.",
            input_schema: input_schema::<VideoMoveArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "videos_shift",
            title: "Shift every video clip",
            description: "Moves every clip by the same number of seconds, keeping their relative placement, as dragging the activity \
                          lane or the [ and ] keys do. One undo step.",
            input_schema: input_schema::<VideosShiftArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_remove",
            title: "Remove a video clip",
            description: "Removes a clip and its edits from the timeline, as Delete on the selected clip does. Playback pauses. This is \
                          not an undo step: undo never brings the clip back.",
            input_schema: input_schema::<VideoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_cut",
            title: "Cut a video clip",
            description: "Splits a clip at a clip-local second, as C at the playhead does. The second is converted to the clip's \
                          nearest frame; the call fails on the clip's first or last frame or on an existing cut. The new part keeps the \
                          export state of the part it comes from. One undo step.",
            input_schema: input_schema::<VideoSecondArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_cut_move",
            title: "Move a cut",
            description: "Moves a cut to another clip-local second (its nearest frame), strictly between its neighbouring cuts or the \
                          clip's ends, as dragging the cut marker does. Both parts keep their edits. One undo step.",
            input_schema: input_schema::<VideoCutMoveArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_cut_remove",
            title: "Remove a cut",
            description: "Removes a cut, merging its two parts, as the cut marker's Remove cut does. When one part is exported and the \
                          other excluded, give merged (include or exclude); without it OVRLEY asks the user and the call waits for the \
                          answer ({removed: false} when the user keeps the cut). One undo step.",
            input_schema: input_schema::<VideoCutRemoveArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_trim",
            title: "Trim a video clip",
            description: "Trims one end of a clip at a clip-local second (its nearest frame), as [ and ] at the playhead do: the outer \
                          part is excluded from export. An end already trimmed moves its trim instead. A trim never passes another \
                          cut. One undo step.",
            input_schema: input_schema::<VideoTrimArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_segment_set_excluded",
            title: "Exclude or include a part",
            description: "Excludes a part (segment) of a clip from the export or includes it again, as X does. Segment keys come from \
                          video_get. One undo step.",
            input_schema: input_schema::<VideoSegmentSetExcludedArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_join_transition_set",
            title: "Set a join's transition",
            description: "Sets the transition at a join (where one video piece follows another), as the join marker's panel does: \
                          an xfade type with its duration, none for a hard cut, or null to follow the default transition. Joins and \
                          their segment keys come from video_get. One undo step.",
            input_schema: input_schema::<VideoJoinTransitionSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "activity_trim_reset",
            title: "Reset the activity trim",
            description: "Removes the activity trim, so the whole activity renders, as Mod+X does. One undo step.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
    ]
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

fn find_clip<'a>(store: &'a Store, video_id: &str) -> Result<&'a ovrley_app::lib::video_program::Clip, ToolOutcome> {
    store
        .player
        .timeline
        .timeline()
        .committed
        .clips
        .iter()
        .find(|clip| clip.id() == video_id)
        .ok_or_else(|| ToolOutcome::Failed(format!("There is no video {video_id}.")))
}

/// A clip-local second as the clip's nearest frame.
fn finite_frame(clip: &ovrley_app::lib::video_program::Clip, second: f64) -> Result<i64, ToolOutcome> {
    if !second.is_finite() {
        return Err(ToolOutcome::InvalidParams("second must be a finite number".into()));
    }
    Ok(clip.fps().seconds_to_frames(second))
}

fn file_name(path: &str) -> &str {
    ovrley_app::lib::file_dialog::filename_from_selected_path(path).unwrap_or(path)
}

fn transition_json(transition: ovrley_core::media::video_edits::Transition) -> Value {
    serde_json::to_value(transition).expect("a transition serializes")
}

/// The videos part of `ovrley_summary`, in timeline order.
pub fn describe_videos(store: &Store) -> Value {
    let timeline = store.player.timeline.timeline();
    let clips = ovrley_app::lib::video_program::collection::sort_clips_by_timeline(&timeline.committed.clips);
    Value::Array(
        clips
            .iter()
            .map(|clip| {
                json!({
                    "id": clip.id(),
                    "fileName": file_name(&clip.video.path),
                    "offsetSeconds": clip.offset_seconds(),
                    "durationSeconds": clip.duration_seconds(),
                    "missing": clip.video.missing,
                    "warningCount": timeline.warnings.get(clip.id()).map_or(0, Vec::len),
                })
            })
            .collect(),
    )
}

/// `video_get`.
pub fn describe_video(store: &Store, video_id: &str) -> Result<Value, ToolOutcome> {
    use ovrley_app::lib::video_program::program::JoinSide;
    use ovrley_app::lib::video_program::segments::list_video_segments;
    let clip = find_clip(store, video_id)?;
    let timeline = store.player.timeline.timeline();
    let fps = clip.fps();
    let (num, den) = fps.components();
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
                "transitionOverride": segment.edits.transition_in.map(transition_json),
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
            json!({
                "segmentKey": join.segment_key,
                "transition": transition_json(join.transition),
                "isDefault": join.is_default,
                "skippedDefault": join.skipped_default.map(|skipped| json!({
                    "side": if skipped.side == JoinSide::Previous { "previous" } else { "next" },
                    "durationSeconds": skipped.duration_seconds,
                    "roomSeconds": skipped.room_seconds,
                })),
            })
        })
        .collect();
    let warnings: Vec<String> = timeline
        .warnings
        .get(video_id)
        .map(|warnings| warnings.iter().map(|warning| ovrley_app::lib::video_program::warnings::format_video_warning(warning, ovrley_app::i18n::DEFAULT_LOCALE)).collect())
        .unwrap_or_default();
    // The drawer's details, in English: duration, frame rate, resolution, created at, time source,
    // codec, bit rate, camera and telemetry.
    let timezone = store.media.sync_activity().and_then(|activity| activity.timezone);
    let details: serde_json::Map<String, Value> = ovrley_app::features::toolbar::video_drawer::metadata_rows(clip, timezone.as_deref(), ovrley_app::i18n::DEFAULT_LOCALE)
        .into_iter()
        .map(|row| (row.label, Value::String(row.value)))
        .collect();
    Ok(json!({
        "id": clip.id(),
        "path": clip.video.path,
        "fileName": file_name(&clip.video.path),
        "offsetSeconds": clip.offset_seconds(),
        "durationSeconds": clip.duration_seconds(),
        "frameRate": format!("{num}/{den}"),
        "frameCount": clip.frame_count(),
        "missing": clip.video.missing,
        "offsetSource": clip.edits.offset_source,
        "timezoneMode": clip.edits.timezone_mode,
        "creationTime": clip.video.creation_time,
        "timeSource": clip.video.time_source,
        "details": details,
        "cuts": clip.cuts().iter().map(|cut| json!({ "id": cut.id, "frame": cut.frame, "second": fps.frames_to_seconds(cut.frame) })).collect::<Vec<_>>(),
        "segments": segments,
        "joins": joins,
        "warnings": warnings,
    }))
}

/// Answers with a clip after an edit; the timeline cache is refreshed first, as after a dispatch.
fn answer_video(store: &mut Store, video_id: &str) -> Result<Answer, ToolOutcome> {
    ovrley_app::features::player::reconcile_player(store);
    Ok(Answer::Now(ToolOutcome::Ok(describe_video(store, video_id)?)))
}

/// The activity trim part of `ovrley_summary`: `null` for the whole activity.
pub fn describe_activity_trim(store: &Store) -> Value {
    match store.media.activity_trim {
        Some(trim) => json!({ "startSecond": trim.start_second, "endSecond": trim.end_second }),
        None => Value::Null,
    }
}

/// The playback part of `ovrley_summary`.
pub fn describe_playback(store: &Store) -> Value {
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

fn require_something_to_play(store: &Store) -> Result<(), ToolOutcome> {
    if player_timing(store).has_activity {
        Ok(())
    } else {
        Err(ToolOutcome::Failed("There is nothing to play: import an activity first.".into()))
    }
}

/// Runs a player tool; `None` for a name that is not one.
pub fn call(name: &str, arguments_value: Value, store: &mut Store, _services: &mut Services, reply: Reply) -> Option<Result<Answer, ToolOutcome>> {
    let now = |value: Value| Ok(Answer::Now(ToolOutcome::Ok(value)));
    Some(match name {
        "playback_play" => arguments::<NoArguments>(arguments_value).and_then(|_| {
            require_something_to_play(store)?;
            playback::play(store, Instant::now());
            now(json!({ "playback": describe_playback(store) }))
        }),
        "playback_pause" => arguments::<NoArguments>(arguments_value).and_then(|_| {
            require_something_to_play(store)?;
            playback::pause(store);
            now(json!({ "playback": describe_playback(store) }))
        }),
        "playhead_seek" => arguments::<PlayheadSeekArguments>(arguments_value).and_then(|arguments| {
            if !arguments.second.is_finite() {
                return Err(ToolOutcome::InvalidParams("second must be a finite number".into()));
            }
            playback::commit_scrub(store, arguments.second);
            now(json!({ "playback": describe_playback(store) }))
        }),
        "video_mute_set" => arguments::<VideoMuteSetArguments>(arguments_value).and_then(|arguments| {
            if !store.video_import.has_videos() {
                return Err(ToolOutcome::Failed("No videos are on the timeline.".into()));
            }
            store.editor.is_video_muted = arguments.muted;
            now(json!({ "playback": describe_playback(store) }))
        }),
        "activity_trim_set" => arguments::<ActivityTrimSetArguments>(arguments_value).and_then(|arguments| {
            use ovrley_app::lib::activity::{trim_activity_edge, TrimEdge};
            let Some(duration) = store.media.activity_summary.as_ref().map(|summary| summary.duration_seconds) else {
                return Err(ToolOutcome::Failed("No activity is loaded.".into()));
            };
            if arguments.start_second.is_none() && arguments.end_second.is_none() {
                return Err(ToolOutcome::InvalidParams("Give start_second, end_second or both.".into()));
            }
            let mut trim = store.media.activity_trim;
            for (edge, second) in [(TrimEdge::Start, arguments.start_second), (TrimEdge::End, arguments.end_second)] {
                let Some(second) = second else { continue };
                if !second.is_finite() {
                    return Err(ToolOutcome::InvalidParams("Trim edges must be finite numbers".into()));
                }
                trim = trim_activity_edge(trim, duration, edge, second);
            }
            store.media.set_activity_trim(trim);
            now(json!({ "activityTrim": describe_activity_trim(store) }))
        }),
        "video_get" => arguments::<VideoArguments>(arguments_value).and_then(|arguments| now(describe_video(store, &arguments.video_id)?)),
        "video_move" => arguments::<VideoMoveArguments>(arguments_value).and_then(|arguments| {
            let fps = find_clip(store, &arguments.video_id)?.fps();
            if !arguments.offset_seconds.is_finite() {
                return Err(ToolOutcome::InvalidParams("offsetSeconds must be a finite number".into()));
            }
            let offset = fps.frames_to_seconds(fps.seconds_to_frames(arguments.offset_seconds));
            ovrley_app::store::slices::video_import::set_video_offset(store, &arguments.video_id, offset, false).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "videos_shift" => arguments::<VideosShiftArguments>(arguments_value).and_then(|arguments| {
            if !arguments.delta_seconds.is_finite() {
                return Err(ToolOutcome::InvalidParams("deltaSeconds must be a finite number".into()));
            }
            ovrley_app::store::slices::video_import::shift_video_offsets(store, arguments.delta_seconds, false).map_err(ToolOutcome::Failed)?;
            ovrley_app::features::player::reconcile_player(store);
            now(json!({ "videos": describe_videos(store) }))
        }),
        "video_remove" => arguments::<VideoArguments>(arguments_value).and_then(|arguments| {
            find_clip(store, &arguments.video_id)?;
            ovrley_app::features::player::remove_video(store, &arguments.video_id).map_err(ToolOutcome::Failed)?;
            ovrley_app::features::player::reconcile_player(store);
            now(json!({ "videos": describe_videos(store) }))
        }),
        "video_cut" => arguments::<VideoSecondArguments>(arguments_value).and_then(|arguments| {
            let clip = find_clip(store, &arguments.video_id)?;
            let frame = finite_frame(clip, arguments.second)?;
            ovrley_app::store::slices::video_import::cut_video(store, &arguments.video_id, frame).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "video_cut_move" => arguments::<VideoCutMoveArguments>(arguments_value).and_then(|arguments| {
            let frame = finite_frame(find_clip(store, &arguments.video_id)?, arguments.second)?;
            ovrley_app::store::slices::video_import::move_cut(store, &arguments.video_id, &arguments.cut_id, frame).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "video_cut_remove" => arguments::<VideoCutRemoveArguments>(arguments_value).and_then(|arguments| {
            use ovrley_app::store::slices::video_import::{request_cut_removal, resolve_cut_removal, CutRemoval};
            find_clip(store, &arguments.video_id)?;
            if store.video_import.cut_removal_request.is_some() {
                return Err(ToolOutcome::Failed("OVRLEY is already asking the user about removing a cut.".into()));
            }
            match request_cut_removal(store, &arguments.video_id, &arguments.cut_id, None).map_err(ToolOutcome::Failed)? {
                CutRemoval::Merged => answer_video(store, &arguments.video_id),
                CutRemoval::Asked => match arguments.merged {
                    Some(merged) => {
                        resolve_cut_removal(store, matches!(merged, MergedState::Exclude)).map_err(ToolOutcome::Failed)?;
                        answer_video(store, &arguments.video_id)
                    }
                    None => {
                        store.player.cut_removal_reply = Some(ovrley_app::store::agent::AgentReply::new(move |result| reply(result.into())));
                        Ok(Answer::Later)
                    }
                },
            }
        }),
        "video_trim" => arguments::<VideoTrimArguments>(arguments_value).and_then(|arguments| {
            use ovrley_app::lib::video_program::segments::{plan_video_trim, TrimPlan, TrimSide};
            let clip = find_clip(store, &arguments.video_id)?;
            let frame = finite_frame(clip, arguments.second)?;
            let side = match arguments.side {
                TrimSideArgument::Start => TrimSide::Start,
                TrimSideArgument::End => TrimSide::End,
            };
            let plan = plan_video_trim(clip, side, frame).ok_or_else(|| ToolOutcome::Failed("The clip cannot be trimmed there: the frame is on the clip's edge or past a cut.".into()))?;
            match plan {
                TrimPlan::Trim => ovrley_app::store::slices::video_import::trim_video(store, &arguments.video_id, side, frame).map(drop),
                TrimPlan::Move { cut_id } => ovrley_app::store::slices::video_import::move_cut(store, &arguments.video_id, &cut_id, frame),
            }
            .map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "video_segment_set_excluded" => arguments::<VideoSegmentSetExcludedArguments>(arguments_value).and_then(|arguments| {
            find_clip(store, &arguments.video_id)?;
            ovrley_app::store::slices::video_import::set_video_segment_excluded(store, &arguments.video_id, &arguments.segment_key, arguments.excluded).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "video_join_transition_set" => arguments::<VideoJoinTransitionSetArguments>(arguments_value).and_then(|arguments| {
            use ovrley_app::features::transitions::{join_key_of_segment, set_join, DEFAULT_TRANSITION_OPTION};
            find_clip(store, &arguments.video_id)?;
            let key = join_key_of_segment(store, &arguments.video_id, &arguments.segment_key)
                .ok_or_else(|| ToolOutcome::Failed(format!("Segment {} of video {} is not a join.", arguments.segment_key, arguments.video_id)))?;
            let (value, duration) = match &arguments.transition {
                None => (DEFAULT_TRANSITION_OPTION, None),
                Some(transition) => (transition.transition_type.as_str(), transition.duration_seconds),
            };
            if duration.is_some_and(|seconds| !(seconds.is_finite() && seconds > 0.0)) {
                return Err(ToolOutcome::InvalidParams("durationSeconds must be a positive number".into()));
            }
            if value == DEFAULT_TRANSITION_OPTION && arguments.transition.is_some() {
                return Err(ToolOutcome::InvalidParams("Use null, not a type, to follow the default transition".into()));
            }
            set_join(store, &key, value, duration).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }),
        "activity_trim_reset" => arguments::<NoArguments>(arguments_value).and_then(|_| {
            store.media.set_activity_trim(None);
            now(json!({ "activityTrim": describe_activity_trim(store) }))
        }),
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::call as catalog_call;
    use ovrley_app::store::services::fakes::fake_services;
    use ovrley_app::store::slices::media::ActivitySummary;
    use ovrley_app::store::Dispatcher;

    /// A dispatcher with agent edits on, over `store`.
    pub(crate) fn dispatcher_with(mut store: Store) -> Dispatcher {
        let (parts, _) = fake_services(json!({ "allowAgentEdits": true }));
        store.agent_session.allow_agent_edits = true;
        store.theme = Some(ovrley_app::lib::template::ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() });
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(|_, _| {});
        dispatcher
    }

    pub(crate) fn with_activity(duration_seconds: f64) -> Store {
        let mut store = Store::default();
        store.media.activity_summary = Some(ActivitySummary { duration_seconds, ..ActivitySummary::of(&Default::default()) });
        store
    }

    pub(crate) fn run(dispatcher: &Dispatcher, name: &'static str, arguments: Value) -> ToolOutcome {
        match dispatcher.dispatch_with_result(move |store, services| catalog_call(name, arguments, store, services, Box::new(|_| panic!("answered later")))).unwrap() {
            Answer::Now(outcome) => outcome,
            Answer::Later => panic!("{name} answers later"),
        }
    }

    pub(crate) fn ok(outcome: ToolOutcome) -> Value {
        match outcome {
            ToolOutcome::Ok(value) => value,
            other => panic!("expected a result, got {other:?}"),
        }
    }

    pub(crate) fn with_videos(offsets: &[(&'static str, f64, f64)]) -> Store {
        use ovrley_app::lib::video_program::create_imported_video;
        use ovrley_core::media::video_edits::VideoEdits;
        use std::rc::Rc;
        let mut store = with_activity(600.0);
        let videos: Vec<_> = offsets
            .iter()
            .enumerate()
            .map(|(order, (id, _, duration))| {
                let metadata = ovrley_core::media::SourceVideoMetadata {
                    path: format!("/videos/{id}.mp4"),
                    duration: Some(*duration),
                    fps_num: Some(30),
                    fps_den: Some(1),
                    resolution: Some(ovrley_core::media::Resolution { width: 1920, height: 1080 }),
                    has_audio: true,
                    ..Default::default()
                };
                create_imported_video(&metadata, id.to_string(), order as u32).unwrap()
            })
            .collect();
        store.video_import.video_edits = Rc::new(offsets.iter().map(|(id, offset, _)| (id.to_string(), VideoEdits { offset_seconds: *offset, ..Default::default() })).collect());
        store.video_import.imported_videos = Rc::new(videos);
        store
    }

    #[test]
    fn video_tools_read_move_shift_and_remove_clips() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0), ("b", 20.0, 10.0)]));
        let summary = ok(run(&dispatcher, "ovrley_summary", json!({})));
        assert_eq!(summary["videos"].as_array().unwrap().len(), 2);
        let video = ok(run(&dispatcher, "video_get", json!({ "videoId": "b" })));
        assert_eq!((video["frameRate"].clone(), video["frameCount"].clone()), (json!("30/1"), json!(300)));
        assert_eq!(video["segments"][0]["key"], "start");
        let moved = ok(run(&dispatcher, "video_move", json!({ "videoId": "b", "offsetSeconds": 12.345 })));
        assert!((moved["offsetSeconds"].as_f64().unwrap() - 370.0 / 30.0).abs() < 1e-9, "snapped to the clip's frames");
        let shifted = ok(run(&dispatcher, "videos_shift", json!({ "deltaSeconds": -2 })));
        assert_eq!(shifted["videos"][0]["offsetSeconds"], -2.0);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2);
        let removed = ok(run(&dispatcher, "video_remove", json!({ "videoId": "a" })));
        assert_eq!(removed["videos"].as_array().unwrap().len(), 1);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2, "removal is no undo step");
        assert!(matches!(run(&dispatcher, "video_get", json!({ "videoId": "a" })), ToolOutcome::Failed(_)));
    }

    #[test]
    fn mute_needs_videos_and_is_no_undo_step() {
        let empty = dispatcher_with(with_activity(60.0));
        assert!(matches!(run(&empty, "video_mute_set", json!({ "muted": true })), ToolOutcome::Failed(_)));
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0)]));
        assert_eq!(ok(run(&dispatcher, "video_mute_set", json!({ "muted": true })))["playback"]["muted"], true);
        assert_eq!(ok(run(&dispatcher, "ovrley_summary", json!({})))["playback"]["muted"], true);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 0);
    }

    #[test]
    fn cuts_trims_and_exclusion_are_one_undo_step_each() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0)]));
        let cut = ok(run(&dispatcher, "video_cut", json!({ "videoId": "a", "second": 5 })));
        let cut_id = cut["cuts"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(cut["cuts"][0]["frame"], 150);
        assert!(matches!(run(&dispatcher, "video_cut", json!({ "videoId": "a", "second": 5 })), ToolOutcome::Failed(_)));
        let moved = ok(run(&dispatcher, "video_cut_move", json!({ "videoId": "a", "cutId": cut_id, "second": 6 })));
        assert_eq!(moved["cuts"][0]["frame"], 180);
        let trimmed = ok(run(&dispatcher, "video_trim", json!({ "videoId": "a", "side": "start", "second": 1 })));
        assert_eq!(trimmed["segments"][0]["excluded"], true);
        let excluded = ok(run(&dispatcher, "video_segment_set_excluded", json!({ "videoId": "a", "segmentKey": cut_id, "excluded": true })));
        assert_eq!(excluded["segments"][2]["excluded"], true);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 4);
    }

    #[test]
    fn removing_a_cut_between_alike_parts_merges_at_once_and_merged_answers_the_question() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0)]));
        let cut = |dispatcher: &Dispatcher| ok(run(dispatcher, "video_cut", json!({ "videoId": "a", "second": 5 })))["cuts"][0]["id"].as_str().unwrap().to_string();
        let first = cut(&dispatcher);
        let merged = ok(run(&dispatcher, "video_cut_remove", json!({ "videoId": "a", "cutId": first, "merged": "exclude" })));
        assert_eq!((merged["cuts"].as_array().unwrap().len(), merged["segments"][0]["excluded"].clone()), (0, json!(false)), "alike parts ignore merged");
        let second = cut(&dispatcher);
        ok(run(&dispatcher, "video_segment_set_excluded", json!({ "videoId": "a", "segmentKey": second, "excluded": true })));
        let answered = ok(run(&dispatcher, "video_cut_remove", json!({ "videoId": "a", "cutId": second, "merged": "exclude" })));
        assert_eq!(answered["segments"][0]["excluded"], true);
        assert!(dispatcher.with_store(|store| store.video_import.cut_removal_request.is_none()));
    }

    #[test]
    fn without_merged_the_call_waits_for_the_user() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0)]));
        let cut_id = ok(run(&dispatcher, "video_cut", json!({ "videoId": "a", "second": 5 })))["cuts"][0]["id"].as_str().unwrap().to_string();
        ok(run(&dispatcher, "video_segment_set_excluded", json!({ "videoId": "a", "segmentKey": cut_id, "excluded": true })));
        for (answer, expected) in [(Some(false), json!({ "removed": true, "excluded": false, "videoId": "a" })), (None, json!({ "removed": false }))] {
            let (sender, receiver) = std::sync::mpsc::channel();
            let arguments = json!({ "videoId": "a", "cutId": cut_id });
            let later = dispatcher
                .dispatch_with_result(move |store, services| catalog_call("video_cut_remove", arguments, store, services, Box::new(move |outcome| sender.send(outcome).unwrap())))
                .unwrap();
            assert!(matches!(later, Answer::Later));
            assert!(dispatcher.with_store(|store| store.video_import.cut_removal_request.is_some()), "the dialog asks");
            match answer {
                Some(excluded) => dispatcher.dispatch(move |store, _| ovrley_app::features::player::resolve_cut_removal(store, excluded)),
                None => dispatcher.dispatch(|store, _| ovrley_app::features::player::dismiss_cut_removal(store)),
            }
            assert_eq!(receiver.try_recv().unwrap(), ToolOutcome::Ok(expected));
            if answer.is_some() {
                let again = ok(run(&dispatcher, "video_cut", json!({ "videoId": "a", "second": 5 })));
                let id = again["cuts"][0]["id"].as_str().unwrap().to_string();
                ok(run(&dispatcher, "video_segment_set_excluded", json!({ "videoId": "a", "segmentKey": id.clone(), "excluded": true })));
                return assert_cancelled(&dispatcher, &id);
            }
        }
    }

    fn assert_cancelled(dispatcher: &Dispatcher, cut_id: &str) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let arguments = json!({ "videoId": "a", "cutId": cut_id });
        dispatcher.dispatch_with_result(move |store, services| catalog_call("video_cut_remove", arguments, store, services, Box::new(move |outcome| sender.send(outcome).unwrap()))).unwrap();
        dispatcher.dispatch(|store, _| ovrley_app::features::player::dismiss_cut_removal(store));
        assert_eq!(receiver.try_recv().unwrap(), ToolOutcome::Ok(json!({ "removed": false })), "keeping the cut answers removed: false");
        assert_eq!(dispatcher.with_store(|store| store.video_import.video_edits["a"].cuts.len()), 1);
    }

    #[test]
    fn join_transitions_are_set_snapped_reset_and_checked() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0), ("b", 10.0, 10.0)]));
        let set = |transition: Value| run(&dispatcher, "video_join_transition_set", json!({ "videoId": "b", "segmentKey": "start", "transition": transition }));
        let dissolve = ok(set(json!({ "type": "dissolve", "durationSeconds": 1.01 })));
        assert_eq!(dissolve["joins"][0]["transition"], json!({ "type": "dissolve", "durationSeconds": 1.0 }), "snapped to the joined frames");
        assert_eq!(dissolve["joins"][0]["isDefault"], false);
        let kept = ok(set(json!({ "type": "fade" })));
        assert_eq!(kept["joins"][0]["transition"], json!({ "type": "fade", "durationSeconds": 1.0 }), "the override's duration is kept");
        let cut = ok(set(json!({ "type": "none" })));
        assert_eq!(cut["joins"][0]["transition"]["type"], "none");
        let default = ok(set(Value::Null));
        assert_eq!(default["joins"][0]["isDefault"], true);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 4);
        assert!(matches!(set(json!({ "type": "none", "durationSeconds": 1 })), ToolOutcome::Failed(_)));
        assert!(matches!(set(json!({ "type": "spin" })), ToolOutcome::Failed(_)));
        assert!(matches!(set(json!({ "type": "fade", "durationSeconds": 0 })), ToolOutcome::InvalidParams(_)));
        assert!(matches!(set(json!({ "type": "default" })), ToolOutcome::InvalidParams(_)));
        assert!(matches!(run(&dispatcher, "video_join_transition_set", json!({ "videoId": "a", "segmentKey": "start", "transition": null })), ToolOutcome::Failed(_)));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 4, "refused calls change nothing");
    }

    #[test]
    fn the_activity_trim_is_set_and_reset_in_one_undo_step_each() {
        let dispatcher = dispatcher_with(with_activity(60.0));
        let trimmed = ok(run(&dispatcher, "activity_trim_set", json!({ "startSecond": 10, "endSecond": 50 })));
        assert_eq!(trimmed["activityTrim"], json!({ "startSecond": 10.0, "endSecond": 50.0 }));
        let clamped = ok(run(&dispatcher, "activity_trim_set", json!({ "endSecond": 5 })));
        assert_eq!(clamped["activityTrim"]["endSecond"], 11.0, "edges stay 1 s apart");
        assert_eq!(ok(run(&dispatcher, "activity_trim_reset", json!({})))["activityTrim"], Value::Null);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 3);
        assert_eq!(ok(run(&dispatcher, "ovrley_summary", json!({})))["activityTrim"], Value::Null);
        assert!(matches!(run(&dispatcher, "activity_trim_set", json!({})), ToolOutcome::InvalidParams(_)));
        assert!(matches!(run(&dispatcher_with(Store::default()), "activity_trim_set", json!({ "startSecond": 1 })), ToolOutcome::Failed(_)));
    }

    #[test]
    fn play_pause_and_seek_need_something_to_play() {
        let empty = dispatcher_with(Store::default());
        assert!(matches!(run(&empty, "playback_play", json!({})), ToolOutcome::Failed(_)));
        let dispatcher = dispatcher_with(with_activity(60.0));
        assert_eq!(ok(run(&dispatcher, "playback_play", json!({})))["playback"]["state"], "playing");
        assert_eq!(ok(run(&dispatcher, "playback_pause", json!({})))["playback"]["state"], "paused");
        let seek = ok(run(&dispatcher, "playhead_seek", json!({ "second": 90 })));
        assert_eq!(seek["playback"]["playheadSecond"], 60.0, "clamped to the timeline");
        assert!(matches!(run(&dispatcher, "playhead_seek", json!({ "second": "x" })), ToolOutcome::InvalidParams(_)));
        let summary = ok(run(&dispatcher, "ovrley_summary", json!({})));
        assert_eq!(summary["playback"]["totalDurationSeconds"], 60.0);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 0, "playback is no edit");
    }
}
