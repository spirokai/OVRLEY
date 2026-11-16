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
    ]
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
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
