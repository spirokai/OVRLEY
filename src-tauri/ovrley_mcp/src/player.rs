//! The player's MCP tools (phase 4): playback, the playhead, the activity trim and the video
//! clips. Split from `catalog.rs` as parent §2 lays out; the catalog lists these after its own and
//! hands them their calls. Times are seconds.

use crate::catalog::{input_schema, Answer, NoArguments, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::player::playback::{self, player_timing};
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

fn file_name(path: &str) -> &str {
    ovrley_app::lib::file_dialog::filename_from_selected_path(path).unwrap_or(path)
}

fn transition_json(transition: ovrley_app::lib::video_program::Transition) -> Value {
    json!({ "type": transition.kind().name(), "durationSeconds": transition.duration_seconds() })
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
    Ok(json!({
        "id": clip.id(),
        "path": clip.video.path,
        "fileName": file_name(&clip.video.path),
        "offsetSeconds": clip.offset_seconds(),
        "durationSeconds": clip.duration_seconds(),
        "frameRate": format!("{num}/{den}"),
        "frameCount": clip.frame_count(),
        "missing": clip.video.missing,
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
    json!({
        "state": state,
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
pub fn call(name: &str, arguments_value: Value, store: &mut Store, _services: &mut Services, _reply: Reply) -> Option<Result<Answer, ToolOutcome>> {
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
        store.theme = Some(ovrley_app::lib::template::ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into() });
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
        use ovrley_app::lib::video_program::{create_imported_video, VideoEdits};
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
                create_imported_video(&metadata, id.to_string(), order as u32, Vec::new()).unwrap()
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
