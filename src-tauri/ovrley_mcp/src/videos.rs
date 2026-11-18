//! The video tools of Phase 6A: the import and the collection's edits, as the import dialog, the
//! video drawer and the video panel make them. The catalog lists these after its own and hands
//! them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::video_preview::import::{self, ImportRequest};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::slices::video_import as slice;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// Videos to import.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoImportArguments {
    /// The videos' absolute paths: MP4, MOV or MKV.
    pub paths: Vec<String>,
    /// When given, exactly these of `paths` join, placed as the import dialog places them, and no
    /// dialog opens.
    pub include: Option<Vec<String>>,
}

/// A video of the timeline.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoIdArguments {
    /// A video's id from `ovrley_summary`'s videos.
    pub video_id: String,
}

/// The file of a missing video.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoLocateArguments {
    /// A missing video's id.
    pub video_id: String,
    /// The file's absolute path.
    pub path: String,
}

/// Where recording times come from.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoTimeSourceArguments {
    /// `detected`, or `filename` (a `YYYYMMDD_HHMMSS` timestamp in the file name, read as local time).
    pub source: ovrley_app::lib::video_program::edits::CreationTimeSource,
    /// One video; without it, every video.
    pub video_id: Option<String>,
}

/// How camera clocks are read against the activity.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoTimezoneArguments {
    /// `local` or `utc` (Apply Timezone on).
    pub mode: ovrley_core::media::video_edits::TimezoneMode,
    /// One video; without it, every video.
    pub video_id: Option<String>,
}

/// A part's activity overlay.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VideoSegmentOverlayArguments {
    pub video_id: String,
    /// A segment key from `video_get`.
    pub segment_key: String,
    pub show_overlay: bool,
}

/// The default transitions; omitted fields keep their value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TransitionDefaultsArguments {
    /// `none` or an `xfade` transition name.
    #[serde(rename = "type")]
    pub transition_type: Option<String>,
    pub duration_seconds: Option<f64>,
    /// Videos closer together than this join without a transition.
    pub split_gap_threshold_seconds: Option<f64>,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "video_import",
            title: "Import videos",
            description: "Imports videos as the import dialog does. Without include, one video into a project without videos joins at \
                          once; otherwise the dialog opens and the call waits for the user (it fails when the user cancels). With \
                          include, exactly those paths join, placed by recording time, without a dialog; a listed path that cannot join \
                          (unreadable, already imported, another resolution than the overlay) fails the call and nothing joins. Replies \
                          with the new videos and the files that failed. Not undoable.",
            input_schema: input_schema::<VideoImportArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_locate",
            title: "Locate a missing video",
            description: "Gives a missing video its file, as Locate video… does: the file is read under the video's id and keeps its \
                          edits (cuts past the end of a shorter file go). Not undoable.",
            input_schema: input_schema::<VideoLocateArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_sync",
            title: "Sync a video",
            description: "Places one video by its recording time again, as the drawer's Sync the video does, also after it was placed by \
                          hand. One undo step.",
            input_schema: input_schema::<VideoIdArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "videos_sync",
            title: "Sync all videos",
            description: "Places every video by its recording time again, also the ones placed by hand. One undo step.",
            input_schema: input_schema::<crate::catalog::NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_time_source_set",
            title: "Set where recording times come from",
            description: "Reads a video's recording time (or every video's) as detected, or from a YYYYMMDD_HHMMSS file name, then places \
                          it again. filename fails for a name without a timestamp. One undo step.",
            input_schema: input_schema::<VideoTimeSourceArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_timezone_set",
            title: "Set Apply Timezone",
            description: "Reads a video's camera clock (or every video's) as local time or as UTC, then places it again. Needs an activity \
                          with a timezone. One undo step.",
            input_schema: input_schema::<VideoTimezoneArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "video_segment_set_overlay",
            title: "Show or hide the overlay over a part",
            description: "Shows or hides the activity widgets over one part of a video, in the preview and the render. One undo step.",
            input_schema: input_schema::<VideoSegmentOverlayArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "transition_defaults_set",
            title: "Set the default transitions",
            description: "Sets the default transition, its duration and the continuity threshold, as the video panel does. Not undoable.",
            input_schema: input_schema::<TransitionDefaultsArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "videos_clear",
            title: "Remove all videos",
            description: "Removes every video from the project, as the video panel's Remove all does. Not undoable.",
            input_schema: input_schema::<crate::catalog::NoArguments>(),
            read_only: false,
        },
    ]
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

fn answer_video(store: &mut Store, video_id: &str) -> Result<Answer, ToolOutcome> {
    ovrley_app::features::player::reconcile_player(store);
    Ok(Answer::Now(ToolOutcome::Ok(crate::player::describe_video(store, video_id)?)))
}

fn answer_videos(store: &mut Store) -> Result<Answer, ToolOutcome> {
    ovrley_app::features::player::reconcile_player(store);
    Ok(Answer::Now(ToolOutcome::Ok(json!({ "videos": crate::player::describe_videos(store) }))))
}

fn absolute(paths: &[String]) -> Result<(), ToolOutcome> {
    match paths.iter().find(|path| !std::path::Path::new(path).is_absolute()) {
        Some(path) => Err(ToolOutcome::InvalidParams(format!("The path must be absolute: {path}"))),
        None => Ok(()),
    }
}

/// Whether `name` is one of these tools.
pub fn handles(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services, reply: Reply) -> Result<Answer, ToolOutcome> {
    match name {
        "video_import" => {
            let arguments: VideoImportArguments = arguments(arguments_value)?;
            absolute(&arguments.paths)?;
            if let Some(include) = &arguments.include {
                absolute(include)?;
                if let Some(path) = include.iter().find(|path| !arguments.paths.contains(path)) {
                    return Err(ToolOutcome::InvalidParams(format!("include lists a path that is not in paths: {path}")));
                }
            }
            let request = ImportRequest { reply: Some(AgentReply::new(move |result| reply(result.into()))), include: arguments.include };
            import::start_import(store, services, arguments.paths, request).map_err(ToolOutcome::Failed)?;
            Ok(Answer::Later)
        }
        "video_locate" => {
            let arguments: VideoLocateArguments = arguments(arguments_value)?;
            absolute(std::slice::from_ref(&arguments.path))?;
            ovrley_app::features::toolbar::video_drawer::locate_missing_video(store, services, arguments.video_id, arguments.path, Some(AgentReply::new(move |result| reply(result.into()))));
            Ok(Answer::Later)
        }
        "video_sync" => {
            let arguments: VideoIdArguments = arguments(arguments_value)?;
            slice::reset_video_offset(store, &arguments.video_id).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }
        "videos_sync" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            slice::sync_all_videos(store).map_err(ToolOutcome::Failed)?;
            answer_videos(store)
        }
        "video_time_source_set" => {
            use ovrley_app::lib::video_program::edits::CreationTimeSource;
            let arguments: VideoTimeSourceArguments = arguments(arguments_value)?;
            match (&arguments.video_id, arguments.source) {
                (Some(video_id), source) => slice::set_video_creation_time_source(store, video_id, source),
                (None, CreationTimeSource::Filename) => slice::set_video_creation_times_from_filenames(store),
                (None, CreationTimeSource::Detected) => slice::reset_video_creation_times(store),
            }
            .map_err(ToolOutcome::Failed)?;
            match arguments.video_id {
                Some(video_id) => answer_video(store, &video_id),
                None => answer_videos(store),
            }
        }
        "video_timezone_set" => {
            let arguments: VideoTimezoneArguments = arguments(arguments_value)?;
            if store.media.sync_activity().and_then(|activity| activity.timezone).is_none() {
                return Err(ToolOutcome::Failed("Apply Timezone needs an activity with a timezone.".into()));
            }
            match &arguments.video_id {
                Some(video_id) => slice::set_video_timezone_mode(store, video_id, arguments.mode),
                None => slice::set_video_sync_timezone_mode(store, arguments.mode),
            }
            .map_err(ToolOutcome::Failed)?;
            match arguments.video_id {
                Some(video_id) => answer_video(store, &video_id),
                None => answer_videos(store),
            }
        }
        "video_segment_set_overlay" => {
            let arguments: VideoSegmentOverlayArguments = arguments(arguments_value)?;
            slice::set_video_segment_skip_overlay(store, &arguments.video_id, &arguments.segment_key, !arguments.show_overlay).map_err(ToolOutcome::Failed)?;
            answer_video(store, &arguments.video_id)
        }
        "transition_defaults_set" => {
            let arguments: TransitionDefaultsArguments = arguments(arguments_value)?;
            ovrley_app::features::seconds_fields::set_panel_defaults(store, |defaults| {
                if let Some(transition_type) = arguments.transition_type {
                    defaults.transition_type = transition_type;
                }
                if let Some(seconds) = arguments.duration_seconds {
                    defaults.duration_seconds = seconds;
                }
                if let Some(seconds) = arguments.split_gap_threshold_seconds {
                    defaults.split_gap_threshold_seconds = seconds;
                }
            })
            .map_err(ToolOutcome::Failed)?;
            Ok(Answer::Now(ToolOutcome::Ok(serde_json::to_value(&store.video_import.transition_defaults).expect("defaults serialize"))))
        }
        "videos_clear" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            import::clear_videos(store);
            Ok(Answer::Now(ToolOutcome::Ok(json!({ "videoCount": store.video_import.imported_videos.len() }))))
        }
        other => Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::call as run_tool;
    use ovrley_app::store::services::fakes::{fake_services, Fakes};
    use ovrley_app::store::Dispatcher;
    use std::sync::mpsc::{channel, Receiver};

    fn allowed() -> Store {
        let mut store = Store::default();
        store.agent_session.allow_agent_edits = true;
        store
    }

    fn probe(fakes: &Fakes, path: &str, creation_time: &str) {
        fakes.video_tools.add_video(
            path,
            ovrley_core::media::SourceVideoMetadata {
                path: path.into(),
                duration: Some(10.0),
                fps_num: Some(30),
                fps_den: Some(1),
                resolution: Some(ovrley_core::media::Resolution { width: 1920, height: 1080 }),
                creation_time: Some(creation_time.into()),
                time_source: Some("gps".into()),
                ..Default::default()
            },
        );
    }

    fn start(dispatcher: &Dispatcher, name: &'static str, arguments: Value) -> (Option<ToolOutcome>, Receiver<ToolOutcome>) {
        let (sender, receiver) = channel();
        let answer = dispatcher.dispatch_with_result(move |store, services| run_tool(name, arguments, store, services, Box::new(move |outcome| sender.send(outcome).unwrap()))).unwrap();
        (match answer {
            Answer::Now(outcome) => Some(outcome),
            Answer::Later => None,
        }, receiver)
    }

    fn wait(dispatcher: &Dispatcher, fakes: &Fakes, receiver: &Receiver<ToolOutcome>) -> ToolOutcome {
        for _ in 0..600 {
            fakes.ui_post.drain(dispatcher);
            if let Ok(outcome) = receiver.try_recv() {
                return outcome;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("no reply");
    }

    #[test]
    fn a_quiet_import_answers_with_the_video_and_clear_removes_it() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(allowed(), parts);
        probe(&fakes, "/videos/a.mp4", "2026-07-18T08:00:00Z");
        let (now, receiver) = start(&dispatcher, "video_import", json!({"paths": ["/videos/a.mp4"]}));
        assert!(now.is_none());
        let ToolOutcome::Ok(reply) = wait(&dispatcher, &fakes, &receiver) else { panic!("the import failed") };
        assert_eq!(reply["videos"][0]["path"], json!("/videos/a.mp4"));
        let (Some(ToolOutcome::Ok(cleared)), _) = start(&dispatcher, "videos_clear", json!({})) else { panic!() };
        assert_eq!(cleared["videoCount"], json!(0));
    }

    #[test]
    fn an_import_with_a_dialog_waits_for_the_user_and_cancelling_fails_the_call() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(allowed(), parts);
        probe(&fakes, "/videos/a.mp4", "2026-07-18T08:00:00Z");
        probe(&fakes, "/videos/b.mp4", "2026-07-18T08:01:00Z");
        let (_, receiver) = start(&dispatcher, "video_import", json!({"paths": ["/videos/a.mp4", "/videos/b.mp4"]}));
        for _ in 0..200 {
            fakes.ui_post.drain(&dispatcher);
            if dispatcher.with_store(|store| store.video_import_review.review.as_ref().is_some_and(|review| review.phase == import::ReviewPhase::Reviewing)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(receiver.try_recv().is_err(), "the call waits for the dialog");
        dispatcher.dispatch(|store, _| import::cancel_import_review(store));
        let ToolOutcome::Failed(message) = wait(&dispatcher, &fakes, &receiver) else { panic!() };
        assert!(message.contains("cancelled"));
    }

    #[test]
    fn relative_paths_and_include_outside_paths_are_invalid() {
        let (parts, _) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(allowed(), parts);
        assert!(matches!(start(&dispatcher, "video_import", json!({"paths": ["a.mp4"]})).0, Some(ToolOutcome::InvalidParams(_))));
        assert!(matches!(start(&dispatcher, "video_import", json!({"paths": ["/a.mp4"], "include": ["/b.mp4"]})).0, Some(ToolOutcome::InvalidParams(_))));
    }

    #[test]
    fn sync_timezone_and_defaults_tools_edit_the_collection() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(allowed(), parts);
        probe(&fakes, "/videos/GX_20260718_090000.mp4", "2026-07-18T08:00:00Z");
        let (_, receiver) = start(&dispatcher, "video_import", json!({"paths": ["/videos/GX_20260718_090000.mp4"]}));
        let ToolOutcome::Ok(reply) = wait(&dispatcher, &fakes, &receiver) else { panic!() };
        let id = reply["videos"][0]["id"].as_str().unwrap().to_string();
        let (Some(ToolOutcome::Ok(video)), _) = start(&dispatcher, "video_time_source_set", json!({"source": "filename", "videoId": id})) else { panic!() };
        assert_eq!(video["timeSource"], json!("filename"));
        assert_eq!(video["details"]["Time source"], json!("Filename"));
        assert!(matches!(start(&dispatcher, "video_timezone_set", json!({"mode": "utc"})).0, Some(ToolOutcome::Failed(_))), "no activity timezone");
        let (Some(ToolOutcome::Ok(video)), _) = start(&dispatcher, "video_segment_set_overlay", json!({"videoId": id, "segmentKey": "start", "showOverlay": false})) else { panic!() };
        assert_eq!(video["segments"][0]["skipOverlay"], json!(true));
        let (Some(ToolOutcome::Ok(defaults)), _) = start(&dispatcher, "transition_defaults_set", json!({"type": "fade", "durationSeconds": 1.5})) else { panic!() };
        assert_eq!((defaults["type"].clone(), defaults["durationSeconds"].clone()), (json!("fade"), json!(1.5)));
        assert!(matches!(start(&dispatcher, "transition_defaults_set", json!({"type": "spin"})).0, Some(ToolOutcome::Failed(_))));
        assert!(matches!(start(&dispatcher, "video_sync", json!({"videoId": id})).0, Some(ToolOutcome::Ok(_))));
        assert!(matches!(start(&dispatcher, "videos_sync", json!({})).0, Some(ToolOutcome::Ok(_))));
        let (_, receiver) = start(&dispatcher, "video_locate", json!({"videoId": id, "path": "/videos/other.mp4"}));
        let ToolOutcome::Failed(message) = wait(&dispatcher, &fakes, &receiver) else { panic!() };
        assert!(message.contains("not missing"));
    }
}
