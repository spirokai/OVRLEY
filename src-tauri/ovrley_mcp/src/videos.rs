//! The video tools of Phase 6A: the import and the collection's edits, as the import dialog, the
//! video drawer and the video panel make them. The catalog lists these after its own and hands
//! them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::video_preview::import::{self, ImportRequest};
use ovrley_app::store::agent::AgentReply;
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
}
