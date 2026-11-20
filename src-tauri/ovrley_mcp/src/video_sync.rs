//! Sync Doctor's manual sync tools (Phase 8 D13): the landmarks, the course location, the
//! sensitivity, the two searches and applying a candidate. A tool that changes the manual sync
//! opens Sync Doctor on its Manual Sync tab, so the edit shows in the timeline and the drawer.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::video_sync::matching::MatchScope;
use ovrley_app::features::video_sync::{self as sync, describe_result, describe_video_sync};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::services::Services;
use ovrley_app::store::slices::manual_video_sync as slice;
use ovrley_app::store::Store;
use ovrley_core::media::manual_video_sync::LandmarkKind;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// A new landmark.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LandmarkAddArguments {
    #[serde(rename = "type")]
    pub kind: LandmarkKind,
    /// The second in the target video; without it, the playhead's, as the mark buttons place it.
    pub video_second: Option<f64>,
}

/// A landmark's new second.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LandmarkMoveArguments {
    /// A landmark id from `video_sync_get`.
    pub id: String,
    /// The second in the target video, inside it.
    pub video_second: f64,
}

/// A landmark's new type.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LandmarkTypeArguments {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: LandmarkKind,
}

/// A landmark.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LandmarkIdArguments {
    pub id: String,
}

/// The course location.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LocationArguments {
    /// The activity second of the place the video's location landmark shows.
    pub activity_second: f64,
}

/// The detection sensitivity; omitted fields keep their value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SensitivityArguments {
    /// Speed below which the rider counts as stopped, in km/h (1 to 10).
    pub speed_threshold_kmh: Option<f64>,
    /// The least heading change that counts as a turn, in degrees (70 to 180).
    pub turn_threshold_degrees: Option<f64>,
}

/// A search.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CalculateArguments {
    /// `all` (Sync All Landmarks) or `location` (Match Location Only).
    pub scope: MatchScope,
}

/// A candidate to apply.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CandidateApplyArguments {
    pub scope: MatchScope,
    /// A candidate's `offsetSeconds` from that scope's fresh result.
    pub offset_seconds: f64,
}

const NOT_UNDOABLE: &str = "Not undoable: the manual sync is outside the editor history.";

pub fn tools() -> Vec<ToolDefinition> {
    let tool = |name, title, description, input_schema, read_only| ToolDefinition {
        name,
        title,
        description,
        input_schema,
        read_only,
    };
    vec![
        tool(
            "video_sync_get",
            "Read the manual sync",
            "Sync Doctor's manual sync: the target video (the first clip), the landmarks with their video and timeline seconds, the \
             course location, the sensitivity and its ranges, the detected events, and both searches with their eligibility and \
             results.",
            input_schema::<crate::catalog::NoArguments>(),
            true,
        ),
        tool(
            "video_sync_landmark_add",
            "Mark a landmark",
            "Adds a stop, leftTurn, rightTurn or location landmark in the target video, at videoSecond or at the playhead as the mark \
             buttons do. At most five, at most one location. Replies with its id. Not undoable.",
            input_schema::<LandmarkAddArguments>(),
            false,
        ),
        tool("video_sync_landmark_move", "Move a landmark", "Moves a landmark to another second of the target video. Not undoable.", input_schema::<LandmarkMoveArguments>(), false),
        tool("video_sync_landmark_set_type", "Retype a landmark", "Changes a landmark's type; only one landmark can be the location. Not undoable.", input_schema::<LandmarkTypeArguments>(), false),
        tool("video_sync_landmark_remove", "Delete a landmark", "Deletes one landmark. Not undoable.", input_schema::<LandmarkIdArguments>(), false),
        tool("video_sync_landmarks_clear", "Clear the landmarks", "Deletes every landmark. Not undoable.", input_schema::<crate::catalog::NoArguments>(), false),
        tool(
            "video_sync_location_set",
            "Set the course location",
            "Sets the place on the activity's course that the video's location landmark shows, as Set location on the map does. Not \
             undoable.",
            input_schema::<LocationArguments>(),
            false,
        ),
        tool("video_sync_location_clear", "Clear the course location", "Clears the course location. Not undoable.", input_schema::<crate::catalog::NoArguments>(), false),
        tool(
            "video_sync_sensitivity_set",
            "Set the detection sensitivity",
            "Sets the stop speed and the turn angle the activity's events are detected with; a search that ran runs again, as a slider \
             release does. Not undoable.",
            input_schema::<SensitivityArguments>(),
            false,
        ),
        tool(
            "video_sync_calculate",
            "Search for sync offsets",
            "Runs Sync All Landmarks (all) or Match Location Only (location). Fails with the button's explanation when the search \
             cannot run. The reply waits for the search and holds its candidates; it fails when the landmarks changed meanwhile. Not \
             undoable.",
            input_schema::<CalculateArguments>(),
            false,
        ),
        tool(
            "video_sync_candidate_apply",
            "Apply a sync offset",
            "Moves the target video to a candidate's offset from that scope's fresh result; every video moves by the same amount, and \
             the playhead with them. One undo step.",
            input_schema::<CandidateApplyArguments>(),
            false,
        ),
    ]
}

/// Whether `name` is one of these tools.
pub fn handles(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value)
        .map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

/// The manual sync after an edit, with the undo note.
fn answer(store: &Store) -> Result<Answer, ToolOutcome> {
    let mut state = describe_video_sync(store);
    state["undo"] = json!(NOT_UNDOABLE);
    Ok(Answer::Now(ToolOutcome::Ok(state)))
}

/// `ovrley_summary.videoSync`: the target video, the landmark count, whether a location is set,
/// both results' status.
pub fn summary(store: &Store) -> Value {
    json!({
        "targetVideoId": store.manual_video_sync.video_id,
        "landmarkCount": store.manual_video_sync.landmarks.len(),
        "locationSet": store.manual_video_sync.detected_location_second.is_some(),
        "searches": { "all": store.manual_video_sync_results.all.status.name(), "location": store.manual_video_sync_results.location.status.name() },
    })
}

pub fn call(
    name: &str,
    arguments_value: Value,
    store: &mut Store,
    services: &mut Services,
    reply: Reply,
) -> Result<Answer, ToolOutcome> {
    if name != "video_sync_get" {
        // The window shows what an agent does.
        store.layout.open_manual_video_sync();
    }
    let failed = ToolOutcome::Failed;
    match name {
        "video_sync_get" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            Ok(Answer::Now(ToolOutcome::Ok(describe_video_sync(store))))
        }
        "video_sync_landmark_add" => {
            let arguments: LandmarkAddArguments = arguments(arguments_value)?;
            let id = match arguments.video_second {
                Some(second) => slice::add_landmark(store, arguments.kind, second),
                None => {
                    let marks = sync::mark_controls(store);
                    let reason = if arguments.kind == LandmarkKind::Location {
                        marks.location_disabled_reason
                    } else {
                        marks.mark_disabled_reason
                    };
                    if let Some(reason) = reason {
                        return Err(failed(reason));
                    }
                    sync::add_landmark_at_playhead(store, arguments.kind)
                }
            }
            .map_err(failed)?;
            let mut state = describe_video_sync(store);
            state["id"] = json!(id);
            state["undo"] = json!(NOT_UNDOABLE);
            Ok(Answer::Now(ToolOutcome::Ok(state)))
        }
        "video_sync_landmark_move" => {
            let arguments: LandmarkMoveArguments = arguments(arguments_value)?;
            slice::move_landmark(store, &arguments.id, arguments.video_second).map_err(failed)?;
            answer(store)
        }
        "video_sync_landmark_set_type" => {
            let arguments: LandmarkTypeArguments = arguments(arguments_value)?;
            slice::set_landmark_kind(store, &arguments.id, arguments.kind).map_err(failed)?;
            answer(store)
        }
        "video_sync_landmark_remove" => {
            let arguments: LandmarkIdArguments = arguments(arguments_value)?;
            slice::remove_landmark(store, &arguments.id).map_err(failed)?;
            answer(store)
        }
        "video_sync_landmarks_clear" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            slice::clear_landmarks(store);
            answer(store)
        }
        "video_sync_location_set" => {
            let arguments: LocationArguments = arguments(arguments_value)?;
            if store.media.parsed_activity.is_none() {
                return Err(failed("A course location needs an activity.".into()));
            }
            slice::set_detected_location(store, arguments.activity_second).map_err(failed)?;
            answer(store)
        }
        "video_sync_location_clear" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            slice::clear_detected_location(store);
            answer(store)
        }
        "video_sync_sensitivity_set" => {
            let arguments: SensitivityArguments = arguments(arguments_value)?;
            if let Some(kmh) = arguments.speed_threshold_kmh {
                slice::set_speed_threshold(store, kmh).map_err(failed)?;
            }
            if let Some(degrees) = arguments.turn_threshold_degrees {
                slice::set_turn_threshold(store, degrees).map_err(failed)?;
            }
            answer(store)
        }
        "video_sync_calculate" => {
            let arguments: CalculateArguments = arguments(arguments_value)?;
            sync::calculate(
                store,
                services,
                arguments.scope,
                Some(AgentReply::new(move |result| reply(result.into()))),
            )
            .map_err(failed)?;
            Ok(Answer::Later)
        }
        "video_sync_candidate_apply" => {
            let arguments: CandidateApplyArguments = arguments(arguments_value)?;
            slice::apply_candidate(store, arguments.scope, arguments.offset_seconds)
                .map_err(failed)?;
            ovrley_app::features::player::reconcile_player(store);
            let mut result = describe_result(store, arguments.scope);
            result["videos"] = crate::player::describe_videos(store);
            Ok(Answer::Now(ToolOutcome::Ok(result)))
        }
        other => Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::call as run_tool;
    use ovrley_app::store::services::fakes::{fake_services, Fakes};
    use ovrley_app::store::slices::media::{ActivitySummary, ImportedActivity};
    use ovrley_app::store::Dispatcher;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;

    /// Agent edits on, a 100 s moving activity and a 30 s clip at offset 0 as the sync target.
    fn dispatcher() -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let mut store = Store::default();
        store.agent_session.allow_agent_edits = true;
        store.theme = Some(ovrley_app::lib::template::ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() });
        let activity = ovrley_core::activity::schema::ParsedActivity {
            sample_elapsed_seconds: (0..=100).map(f64::from).collect(),
            speed: vec![Some(3.0); 101],
            heading: vec![Some(0.0); 101],
            ..Default::default()
        };
        store.media.activity_summary = Some(ActivitySummary {
            duration_seconds: 100.0,
            ..ActivitySummary::of(&activity)
        });
        store.media.parsed_activity = Some(ImportedActivity {
            activity: Arc::new(activity),
            preview_bytes: Arc::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(|store, _| {
            let metadata = ovrley_core::media::SourceVideoMetadata {
                path: "/videos/clip.mp4".into(),
                duration: Some(30.0),
                fps_num: Some(30),
                fps_den: Some(1),
                resolution: Some(ovrley_core::media::Resolution { width: 1920, height: 1080 }),
                ..Default::default()
            };
            let clip = ovrley_app::lib::video_program::create_imported_video(&metadata, "clip".into(), 0, Vec::new()).unwrap();
            ovrley_app::store::slices::video_import::commit_video_collection(store, vec![clip], ovrley_app::lib::video_program::VideoEditsMap::from([("clip".to_string(), Default::default())]));
        });
        (dispatcher, fakes)
    }

    fn start(
        dispatcher: &Dispatcher,
        name: &'static str,
        arguments: Value,
    ) -> (Option<ToolOutcome>, Receiver<ToolOutcome>) {
        let (sender, receiver) = channel();
        let answer = dispatcher
            .dispatch_with_result(move |store, services| {
                run_tool(
                    name,
                    arguments,
                    store,
                    services,
                    Box::new(move |outcome| sender.send(outcome).unwrap()),
                )
            })
            .unwrap();
        (
            match answer {
                Answer::Now(outcome) => Some(outcome),
                Answer::Later => None,
            },
            receiver,
        )
    }

    fn ok(dispatcher: &Dispatcher, name: &'static str, arguments: Value) -> Value {
        match start(dispatcher, name, arguments).0 {
            Some(ToolOutcome::Ok(value)) => value,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn wait(
        dispatcher: &Dispatcher,
        fakes: &Fakes,
        receiver: &Receiver<ToolOutcome>,
    ) -> ToolOutcome {
        for _ in 0..600 {
            if let Ok(outcome) = receiver.try_recv() {
                return outcome;
            }
            fakes.ui_post.drain(dispatcher);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("no reply");
    }

    #[test]
    fn landmark_tools_edit_the_manual_sync_and_open_sync_doctor() {
        let (dispatcher, _) = dispatcher();
        let steps = dispatcher.with_store(|store| store.history.past.len());
        let added = ok(
            &dispatcher,
            "video_sync_landmark_add",
            json!({"type": "stop", "videoSecond": 4}),
        );
        let id = added["id"].as_str().unwrap().to_string();
        assert_eq!(added["landmarks"][0]["videoSecond"], json!(4.0));
        assert_eq!(added["undo"], json!(NOT_UNDOABLE));
        dispatcher.with_store(|store| {
            assert_eq!(
                store.layout.video_sync_drawer_tab,
                ovrley_app::store::slices::layout::VideoSyncDrawerTab::Manual
            );
            assert_eq!(store.history.past.len(), steps, "landmarks are not undo steps");
        });
        ok(
            &dispatcher,
            "video_sync_landmark_move",
            json!({"id": id, "videoSecond": 6}),
        );
        let retyped = ok(
            &dispatcher,
            "video_sync_landmark_set_type",
            json!({"id": id, "type": "location"}),
        );
        assert_eq!(retyped["landmarks"][0]["type"], json!("location"));
        assert!(matches!(
            start(
                &dispatcher,
                "video_sync_landmark_add",
                json!({"type": "location", "videoSecond": 1})
            )
            .0,
            Some(ToolOutcome::Failed(_))
        ));
        ok(&dispatcher, "video_sync_landmark_remove", json!({"id": id}));
        ok(
            &dispatcher,
            "video_sync_landmark_add",
            json!({"type": "leftTurn"}),
        );
        let cleared = ok(&dispatcher, "video_sync_landmarks_clear", json!({}));
        assert_eq!(cleared["landmarks"], json!([]));
        dispatcher.dispatch(|store, _| store.editor.selected_second = 50.0);
        let ToolOutcome::Failed(reason) = start(
            &dispatcher,
            "video_sync_landmark_add",
            json!({"type": "stop"}),
        )
        .0
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            reason,
            "Move the playhead inside the video to mark a landmark"
        );
    }

    #[test]
    fn location_and_sensitivity_tools_check_their_ranges() {
        let (dispatcher, _) = dispatcher();
        let state = ok(
            &dispatcher,
            "video_sync_location_set",
            json!({"activitySecond": 12.5}),
        );
        assert_eq!(state["detectedLocationSecond"], json!(12.5));
        assert_eq!(
            ok(&dispatcher, "video_sync_location_clear", json!({}))["detectedLocationSecond"],
            Value::Null
        );
        let state = ok(
            &dispatcher,
            "video_sync_sensitivity_set",
            json!({"speedThresholdKmh": 6, "turnThresholdDegrees": 170}),
        );
        assert_eq!(
            (
                state["sensitivity"]["speedThresholdKmh"].clone(),
                state["sensitivity"]["turnThresholdDegrees"].clone()
            ),
            (json!(6.0), json!(170.0))
        );
        assert!(matches!(
            start(
                &dispatcher,
                "video_sync_sensitivity_set",
                json!({"turnThresholdDegrees": 200})
            )
            .0,
            Some(ToolOutcome::Failed(_))
        ));
    }

    #[test]
    fn a_search_answers_when_its_worker_does_and_applying_is_one_undo_step() {
        let (dispatcher, fakes) = dispatcher();
        let steps = dispatcher.with_store(|store| store.history.past.len());
        ok(
            &dispatcher,
            "video_sync_landmark_add",
            json!({"type": "location", "videoSecond": 4}),
        );
        ok(
            &dispatcher,
            "video_sync_location_set",
            json!({"activitySecond": 12}),
        );
        let ToolOutcome::Failed(reason) =
            start(&dispatcher, "video_sync_calculate", json!({"scope": "all"}))
                .0
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(reason, "At least two landmarks are required");
        let (now, receiver) = start(
            &dispatcher,
            "video_sync_calculate",
            json!({"scope": "location"}),
        );
        assert!(now.is_none(), "the reply waits for the worker");
        let ToolOutcome::Ok(result) = wait(&dispatcher, &fakes, &receiver) else {
            panic!()
        };
        assert_eq!(result["status"], json!("fresh"));
        assert_eq!(result["candidates"][0]["offsetSeconds"], json!(8.0));
        let applied = ok(
            &dispatcher,
            "video_sync_candidate_apply",
            json!({"scope": "location", "offsetSeconds": 8.0}),
        );
        assert_eq!(applied["candidates"][0]["applied"], json!(true));
        dispatcher.with_store(|store| assert_eq!(store.history.past.len(), steps + 1));
        assert!(matches!(
            start(
                &dispatcher,
                "video_sync_candidate_apply",
                json!({"scope": "location", "offsetSeconds": 9.0})
            )
            .0,
            Some(ToolOutcome::Failed(_))
        ));
    }

    #[test]
    fn a_search_dropped_by_a_landmark_edit_says_so() {
        let (dispatcher, fakes) = dispatcher();
        ok(
            &dispatcher,
            "video_sync_landmark_add",
            json!({"type": "location", "videoSecond": 4}),
        );
        ok(
            &dispatcher,
            "video_sync_location_set",
            json!({"activitySecond": 12}),
        );
        let (_, receiver) = start(
            &dispatcher,
            "video_sync_calculate",
            json!({"scope": "location"}),
        );
        ok(
            &dispatcher,
            "video_sync_landmark_move",
            json!({"id": dispatcher.with_store(|store| store.manual_video_sync.landmarks[0].id().to_string()), "videoSecond": 5}),
        );
        let ToolOutcome::Failed(message) = wait(&dispatcher, &fakes, &receiver) else {
            panic!()
        };
        assert!(message.contains("dropped"));
    }

    #[test]
    fn the_summary_and_get_describe_the_manual_sync() {
        let (dispatcher, _) = dispatcher();
        let state = ok(&dispatcher, "video_sync_get", json!({}));
        assert_eq!(state["targetVideo"]["id"], json!("clip"));
        assert_eq!(state["sensitivity"]["turnRange"], json!([70.0, 180.0]));
        let summary = dispatcher.with_store(summary);
        assert_eq!(
            summary,
            json!({"targetVideoId": "clip", "landmarkCount": 0, "locationSet": false, "searches": {"all": "idle", "location": "idle"}})
        );
    }
}
