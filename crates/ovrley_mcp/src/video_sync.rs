//! Sync Doctor's manual sync tools: the landmarks, the course location, the sensitivity, the two
//! searches and applying a candidate. A tool that changed the manual sync opens Sync Doctor on
//! its Manual Sync tab, so the change shows.

use crate::catalog::{Answer, NoArguments, Tool, ToolError, ToolResult};
use ovrley_app::features::sync::matching::MatchScope;
use ovrley_app::features::sync::{self as sync, describe_result, describe_video_sync};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::services::Services;
use ovrley_app::features::sync::manual as manual;
use ovrley_app::store::Store;
use ovrley_core::media::sync::LandmarkKind;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

const NOT_UNDOABLE: &str = "Not undoable: the manual sync is outside the editor history.";

/// A new landmark.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AddArguments {
    #[serde(rename = "type")]
    kind: LandmarkKind,
    /// The second in the target video; without it, the playhead's, as the mark buttons place it.
    video_second: Option<f64>,
}

/// A landmark's new second.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MoveArguments {
    /// A landmark id from `video_sync_get`.
    id: String,
    /// The second in the target video, inside it.
    video_second: f64,
}

/// A landmark's new type.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RetypeArguments {
    id: String,
    #[serde(rename = "type")]
    kind: LandmarkKind,
}

/// A landmark.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LandmarkArguments {
    id: String,
}

/// The course location.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LocationArguments {
    /// The activity second of the place the video's location landmark shows.
    activity_second: f64,
}

/// The detection sensitivity; omitted fields keep their value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SensitivityArguments {
    /// Speed below which the rider counts as stopped, in km/h (1 to 10).
    speed_threshold_kmh: Option<f64>,
    /// The least heading change that counts as a turn, in degrees (70 to 180).
    turn_threshold_degrees: Option<f64>,
}

/// A search.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SearchArguments {
    /// `all` (Sync All Landmarks) or `location` (Match Location Only).
    scope: MatchScope,
}

/// A candidate to apply.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CandidateArguments {
    scope: MatchScope,
    /// A candidate's `offsetSeconds` from that scope's fresh result.
    offset_seconds: f64,
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool::read(
            "video_sync_get",
            "Read the manual sync",
            "Sync Doctor's manual sync: the target video (the first clip), the landmarks with \
             their video and timeline seconds, the course location, the sensitivity and its \
             ranges, the detected events, and both searches with their eligibility and results.",
            get,
        ),
        Tool::edit(
            "video_sync_landmark_add",
            "Mark a landmark",
            "Adds a stop, leftTurn, rightTurn or location landmark in the target video, at \
             videoSecond or at the playhead as the mark buttons do. At most five, at most one \
             location. Replies with its id. Not undoable.",
            add_landmark,
        ),
        Tool::edit(
            "video_sync_landmark_move",
            "Move a landmark",
            "Moves a landmark to another second of the target video. Not undoable.",
            move_landmark,
        ),
        Tool::edit(
            "video_sync_landmark_set_type",
            "Retype a landmark",
            "Changes a landmark's type; only one landmark can be the location. Not undoable.",
            retype_landmark,
        ),
        Tool::edit(
            "video_sync_landmark_remove",
            "Delete a landmark",
            "Deletes one landmark. Not undoable.",
            remove_landmark,
        )
        .destructive(),
        Tool::edit(
            "video_sync_landmarks_clear",
            "Clear the landmarks",
            "Deletes every landmark. Not undoable.",
            clear_landmarks,
        )
        .destructive(),
        Tool::edit(
            "video_sync_location_set",
            "Set the course location",
            "Sets the place on the activity's course that the video's location landmark shows, as \
             Set location on the map does. Not undoable.",
            set_location,
        ),
        Tool::edit(
            "video_sync_location_clear",
            "Clear the course location",
            "Clears the course location. Not undoable.",
            clear_location,
        )
        .destructive(),
        Tool::edit(
            "video_sync_sensitivity_set",
            "Set the detection sensitivity",
            "Sets the stop speed and the turn angle the activity's events are detected with; a \
             search that ran runs again, as a slider release does. Not undoable.",
            set_sensitivity,
        ),
        Tool::waiting(
            "video_sync_calculate",
            "Search for sync offsets",
            "Runs Sync All Landmarks (all) or Match Location Only (location). Fails with the \
             button's explanation when the search cannot run. The reply waits for the search and \
             holds its candidates; it fails when the landmarks changed meanwhile. Not undoable.",
            calculate,
        ),
        Tool::edit(
            "video_sync_candidate_apply",
            "Apply a sync offset",
            "Moves the target video to a candidate's offset from that scope's fresh result; every \
             video moves by the same amount, and the playhead with them. One undo step.",
            apply_candidate,
        ),
    ]
}

/// `ovrley_summary`'s `videoSync`: the target video, the landmark count, whether a location is
/// set, and both searches' status.
pub(crate) fn summary(store: &Store) -> Value {
    let manual = &store.manual_video_sync;
    let results = &store.manual_video_sync_results;
    json!({
        "targetVideoId": manual.video_id,
        "landmarkCount": manual.landmarks.len(),
        "locationSet": manual.detected_location_second.is_some(),
        "searches": {
            "all": results.all.status.name(),
            "location": results.location.status.name(),
        },
    })
}

/// The manual sync after an edit, which Sync Doctor now shows.
fn answer_edit(store: &mut Store) -> Value {
    store.layout.open_manual_video_sync();
    let mut state = describe_video_sync(store);
    state["undo"] = json!(NOT_UNDOABLE);
    state
}

fn get(_: NoArguments, store: &Store, _: &Services) -> ToolResult {
    Ok(describe_video_sync(store))
}

fn add_landmark(args: AddArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let id = match args.video_second {
        Some(second) => manual::add_landmark(store, args.kind, second)?,
        None => sync::add_landmark_at_playhead(store, args.kind)?,
    };
    let mut state = answer_edit(store);
    state["id"] = json!(id);
    Ok(state)
}

fn move_landmark(args: MoveArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::move_landmark(store, &args.id, args.video_second)?;
    Ok(answer_edit(store))
}

fn retype_landmark(args: RetypeArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::set_landmark_kind(store, &args.id, args.kind)?;
    Ok(answer_edit(store))
}

fn remove_landmark(args: LandmarkArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::remove_landmark(store, &args.id)?;
    Ok(answer_edit(store))
}

fn clear_landmarks(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::clear_landmarks(store);
    Ok(answer_edit(store))
}

fn set_location(args: LocationArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    if store.media.parsed_activity.is_none() {
        return Err(ToolError::Failed("A course location needs an activity.".into()));
    }
    manual::set_detected_location(store, args.activity_second)?;
    Ok(answer_edit(store))
}

fn clear_location(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::clear_detected_location(store);
    Ok(answer_edit(store))
}

fn set_sensitivity(args: SensitivityArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::set_thresholds(store, args.speed_threshold_kmh, args.turn_threshold_degrees)?;
    Ok(answer_edit(store))
}

fn calculate(
    args: SearchArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    sync::calculate(store, services, args.scope, Some(reply))?;
    store.layout.open_manual_video_sync();
    Ok(Answer::Later)
}

fn apply_candidate(args: CandidateArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    manual::apply_candidate(store, args.scope, args.offset_seconds)?;
    ovrley_app::features::player::reconcile_player(store);
    store.layout.open_manual_video_sync();
    let mut result = describe_result(store, args.scope);
    result["videos"] = crate::timeline::describe_videos(store);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{video_metadata, Editor};
    use ovrley_app::features::toolbar::VideoSyncDrawerTab;
    use ovrley_app::store::media::{ActivitySummary, ImportedActivity};
    use std::sync::Arc;

    /// A 100 s moving activity, and a 30 s clip at offset 0 as the sync target.
    fn editor() -> Editor {
        let activity = ovrley_core::activity::schema::ParsedActivity {
            sample_elapsed_seconds: (0..=100).map(f64::from).collect(),
            speed: vec![Some(3.0); 101],
            heading: vec![Some(0.0); 101],
            ..Default::default()
        };
        let mut store = Store::default();
        store.media.activity_summary = Some(ActivitySummary {
            duration_seconds: 100.0,
            ..ActivitySummary::of(&activity)
        });
        store.media.parsed_activity = Some(ImportedActivity {
            activity: Arc::new(activity),
            preview_bytes: Arc::new(Vec::new()),
        });
        let editor = Editor::with(store, json!({}));
        editor.dispatcher.dispatch(|store, _| {
            use ovrley_app::video::{create_imported_video, VideoEditsMap};
            use ovrley_app::store::videos::commit_video_collection;
            let metadata = video_metadata("/videos/clip.mp4", 30.0);
            let clip = create_imported_video(&metadata, "clip".into(), 0).unwrap();
            let edits = VideoEditsMap::from([(
                "clip".to_string(),
                ovrley_core::media::edits::VideoEdits::default(),
            )]);
            let collection = vec![clip];
            commit_video_collection(store, collection, edits);
        });
        editor
    }

    fn add_location(editor: &Editor, activity_second: f64) -> Value {
        let added = editor.ok(
            "video_sync_landmark_add",
            json!({"type": "location", "videoSecond": 4}),
        );
        editor.ok("video_sync_location_set", json!({"activitySecond": activity_second}));
        added["id"].clone()
    }

    #[test]
    fn landmark_tools_edit_the_manual_sync_and_open_sync_doctor() {
        let editor = editor();
        let steps = editor.undo_steps();
        editor.invalid("video_sync_landmark_move", json!({"bogus": 1}));
        editor.failed("video_sync_landmark_remove", json!({"id": "none"}));
        assert!(
            !editor.dispatcher.with_store(|store| store.layout.left_drawer_visible),
            "a refused call opens nothing"
        );
        let added = editor.ok("video_sync_landmark_add", json!({"type": "stop", "videoSecond": 4}));
        let id = &added["id"];
        assert_eq!(added["landmarks"][0]["videoSecond"], json!(4.0));
        assert_eq!(added["undo"], json!(NOT_UNDOABLE));
        editor.dispatcher.with_store(|store| {
            assert!(store.layout.left_drawer_visible);
            assert_eq!(store.layout.video_sync_drawer_tab, VideoSyncDrawerTab::Manual);
        });
        assert_eq!(editor.undo_steps(), steps, "landmarks are not undo steps");
        editor.ok("video_sync_landmark_move", json!({"id": id, "videoSecond": 6}));
        let retype = json!({"id": id, "type": "location"});
        let retyped = editor.ok("video_sync_landmark_set_type", retype);
        assert_eq!(retyped["landmarks"][0]["type"], json!("location"));
        editor.failed("video_sync_landmark_add", json!({"type": "location", "videoSecond": 1}));
        editor.ok("video_sync_landmark_remove", json!({"id": id}));
        editor.ok("video_sync_landmark_add", json!({"type": "leftTurn"}));
        assert_eq!(editor.ok("video_sync_landmarks_clear", json!({}))["landmarks"], json!([]));
        editor.dispatcher.dispatch(|store, _| store.editor.selected_second = 50.0);
        assert_eq!(
            editor.failed("video_sync_landmark_add", json!({"type": "stop"})),
            "Move the playhead inside the video to mark a landmark"
        );
    }

    #[test]
    fn location_and_sensitivity_tools_check_their_ranges() {
        let editor = editor();
        let state = editor.ok("video_sync_location_set", json!({"activitySecond": 12.5}));
        assert_eq!(state["detectedLocationSecond"], json!(12.5));
        let state = editor.ok("video_sync_location_clear", json!({}));
        assert_eq!(state["detectedLocationSecond"], Value::Null);
        let thresholds = |speed: f64, turn: f64| {
            json!({"speedThresholdKmh": speed, "turnThresholdDegrees": turn})
        };
        editor.ok("video_sync_sensitivity_set", thresholds(6.0, 170.0));
        editor.failed("video_sync_sensitivity_set", thresholds(7.0, 200.0));
        let sensitivity = &editor.ok("video_sync_get", json!({}))["sensitivity"];
        assert_eq!(
            (&sensitivity["speedThresholdKmh"], &sensitivity["turnThresholdDegrees"]),
            (&json!(6.0), &json!(170.0)),
            "a refused call changes neither"
        );
    }

    #[test]
    fn a_search_answers_when_its_worker_does_and_applying_is_one_undo_step() {
        let editor = editor();
        let steps = editor.undo_steps();
        add_location(&editor, 12.0);
        assert_eq!(
            editor.failed("video_sync_calculate", json!({"scope": "all"})),
            "At least two landmarks are required"
        );
        let waiting = editor.start("video_sync_calculate", json!({"scope": "location"}));
        assert!(waiting.answer.is_none(), "the reply waits for the worker");
        let result = editor.finish(waiting).unwrap();
        assert_eq!(result["status"], json!("fresh"));
        assert_eq!(result["candidates"][0]["offsetSeconds"], json!(8.0));
        // An offset that went through JSON can differ in its last bit.
        let offset = 8.0_f64.next_up();
        let applied = editor.ok(
            "video_sync_candidate_apply",
            json!({"scope": "location", "offsetSeconds": offset}),
        );
        assert_eq!(applied["candidates"][0]["applied"], json!(true));
        assert_eq!(editor.undo_steps(), steps + 1);
        editor.failed(
            "video_sync_candidate_apply",
            json!({"scope": "location", "offsetSeconds": 9.0}),
        );
    }

    #[test]
    fn a_search_dropped_by_a_landmark_edit_says_so() {
        let editor = editor();
        let id = add_location(&editor, 12.0);
        let waiting = editor.start("video_sync_calculate", json!({"scope": "location"}));
        editor.ok("video_sync_landmark_move", json!({"id": id, "videoSecond": 5}));
        let result = editor.finish(waiting);
        assert!(matches!(&result, Err(ToolError::Failed(reason)) if reason.contains("dropped")));
    }

    #[test]
    fn the_summary_and_get_describe_the_manual_sync() {
        let editor = editor();
        let state = editor.ok("video_sync_get", json!({}));
        assert_eq!(state["targetVideo"]["id"], json!("clip"));
        assert_eq!(state["sensitivity"]["turnRange"], json!([70.0, 180.0]));
        assert_eq!(
            editor.dispatcher.with_store(summary),
            json!({
                "targetVideoId": "clip",
                "landmarkCount": 0,
                "locationSet": false,
                "searches": {"all": "idle", "location": "idle"},
            })
        );
    }
}
