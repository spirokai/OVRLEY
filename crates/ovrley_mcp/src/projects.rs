//! The project tools of Phase 6A (D13): list, new, open and save, as the projects drawer and the
//! File menu make them. The catalog lists these after its own and hands them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::app_shell::UnsavedAnswer;
use ovrley_app::features::projects::{self, ProjectOpenRequest};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// What to do with unsaved changes; without it, OVRLEY asks the user and the call waits.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum UnsavedChoice {
    /// Save the project first (it must have been saved to a file before).
    Save,
    /// Discard the changes.
    Discard,
}

impl From<UnsavedChoice> for UnsavedAnswer {
    fn from(choice: UnsavedChoice) -> Self {
        match choice {
            UnsavedChoice::Save => Self::Save,
            UnsavedChoice::Discard => Self::Discard,
        }
    }
}

/// What to do with a missing activity, or the missing video of a single-video project.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MissingSourcesChoice {
    /// Load the project without it (Load Anyway).
    Skip,
}

/// The folder to list.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectListArguments {
    /// An absolute folder; without it, the folder OVRLEY remembers, or its default project folder.
    pub directory: Option<String>,
}

/// A new project.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectNewArguments {
    pub unsaved: Option<UnsavedChoice>,
}

/// The project to open.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectOpenArguments {
    /// The `.oly` file's absolute path.
    pub path: String,
    pub unsaved: Option<UnsavedChoice>,
    pub missing_sources: Option<MissingSourcesChoice>,
}

/// Where to save.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSaveArguments {
    /// An absolute `.oly` path (Save As); without it, the loaded project's file.
    pub path: Option<String>,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "project_list",
            title: "List projects",
            description: "Lists the OVRLEY projects (.oly) of a folder: name and path.",
            input_schema: input_schema::<ProjectListArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "project_new",
            title: "New project",
            description: "Starts a new project, as New Project does: the loaded template's saved state, no activity, no videos, the default \
                          settings; the undo history is cleared. With unsaved changes and without unsaved, OVRLEY asks the user and the \
                          call waits: {created: false} when the user cancels. unsaved: save fails for a project never saved; call \
                          project_save with a path first.",
            input_schema: input_schema::<ProjectNewArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "project_open",
            title: "Open a project",
            description: "Opens an OVRLEY project (.oly), as Load Project does, replacing the session and clearing the undo history. With \
                          unsaved changes and without unsaved, OVRLEY asks the user first; a missing activity, or the missing video of a \
                          single-video project, asks to be located unless missingSources is skip. The call waits for those answers: \
                          {opened: false} when the user cancels. Missing videos of a project with several load as missing; give them \
                          their files with video_locate. Replies with the project and the video folder of an older batch project.",
            input_schema: input_schema::<ProjectOpenArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "project_save",
            title: "Save the project",
            description: "Saves the project, as Save Project does, or to path as Save Project As does. Without path it fails for a project \
                          never saved. Replies with the project and, when an older-format file was replaced, the backup it kept.",
            input_schema: input_schema::<ProjectSaveArguments>(),
            read_only: false,
        },
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

fn absolute(path: &str) -> Result<(), ToolOutcome> {
    if std::path::Path::new(path).is_absolute() {
        Ok(())
    } else {
        Err(ToolOutcome::InvalidParams(format!(
            "The path must be absolute: {path}"
        )))
    }
}

fn agent_reply(reply: Reply) -> AgentReply {
    AgentReply::new(move |result| reply(result.into()))
}

pub fn call(
    name: &str,
    arguments_value: Value,
    store: &mut Store,
    services: &mut Services,
    reply: Reply,
) -> Result<Answer, ToolOutcome> {
    match name {
        "project_list" => {
            let arguments: ProjectListArguments = arguments(arguments_value)?;
            if let Some(directory) = &arguments.directory {
                absolute(directory)?;
            }
            projects::list_projects(services, arguments.directory, move |_, listed| {
                let listed = listed.map(|projects| {
                    let projects: Vec<Value> = projects.into_iter().map(|project| json!({ "name": project.name, "path": project.path })).collect();
                    json!({ "projects": projects })
                });
                reply(listed.into());
            });
            Ok(Answer::Later)
        }
        "project_new" => {
            let arguments: ProjectNewArguments = arguments(arguments_value)?;
            projects::new_project(
                store,
                services,
                arguments.unsaved.map(Into::into),
                Some(agent_reply(reply)),
            );
            Ok(Answer::Later)
        }
        "project_open" => {
            let arguments: ProjectOpenArguments = arguments(arguments_value)?;
            absolute(&arguments.path)?;
            let request = ProjectOpenRequest {
                skip_missing_sources: arguments.missing_sources.is_some(),
                reply: Some(agent_reply(reply)),
                from_startup: false,
            };
            projects::open_project(
                store,
                services,
                arguments.path,
                arguments.unsaved.map(Into::into),
                request,
            );
            Ok(Answer::Later)
        }
        "project_save" => {
            let arguments: ProjectSaveArguments = arguments(arguments_value)?;
            if let Some(path) = &arguments.path {
                absolute(path)?;
            } else if store.projects.loaded_project_path.is_none() {
                return Err(ToolOutcome::Failed(projects::AGENT_SAVE_NEEDS_PATH.into()));
            }
            let reply = agent_reply(reply);
            let save_as = arguments.path.is_some();
            projects::save_project(
                store,
                services,
                save_as,
                arguments.path,
                Box::new(move |store, _, saved| reply.send(projects::save_answer(store, saved))),
            );
            Ok(Answer::Later)
        }
        other => Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_app::features::projects::snapshot::LAST_PROJECT_DIRECTORY_KEY;
    use crate::catalog::call as run_tool;
    use ovrley_app::store::services::fakes::{fake_services, Fakes};
    use ovrley_app::store::Dispatcher;
    use std::sync::mpsc::{channel, Receiver};

    fn dispatcher(directory: &std::path::Path) -> (Dispatcher, Fakes) {
        let (parts, fakes) =
            fake_services(json!({ LAST_PROJECT_DIRECTORY_KEY: directory.to_string_lossy() }));
        let mut store = Store::default();
        store.agent_session.allow_agent_edits = true;
        (Dispatcher::new(store, parts), fakes)
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

    fn run(
        dispatcher: &Dispatcher,
        fakes: &Fakes,
        name: &'static str,
        arguments: Value,
    ) -> ToolOutcome {
        let (now, receiver) = start(dispatcher, name, arguments);
        if let Some(outcome) = now {
            return outcome;
        }
        for _ in 0..600 {
            fakes.ui_post.drain(dispatcher);
            if let Ok(outcome) = receiver.try_recv() {
                return outcome;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("{name} did not answer");
    }

    #[test]
    fn project_tools_save_list_open_and_start_new() {
        let directory = std::env::temp_dir().join(format!(
            "ovrley-mcp-projects-{}-{}",
            std::process::id(),
            uuid_like()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let (dispatcher, fakes) = dispatcher(&directory);
        let path = directory.join("race.oly").to_string_lossy().into_owned();

        let activity = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ovrley_core/tests/fixtures/activity/test-ride.fit");
        assert!(matches!(run(&dispatcher, &fakes, "activity_import", json!({ "path": activity })), ToolOutcome::Ok(_)));
        assert_eq!(
            run(&dispatcher, &fakes, "project_save", json!({})),
            ToolOutcome::Failed(projects::AGENT_SAVE_NEEDS_PATH.into())
        );
        let ToolOutcome::Ok(saved) =
            run(&dispatcher, &fakes, "project_save", json!({ "path": path }))
        else {
            panic!("the save failed")
        };
        assert_eq!(saved["project"]["status"], "Saved");
        assert_eq!(saved["backupPath"], Value::Null);

        let ToolOutcome::Ok(listed) = run(&dispatcher, &fakes, "project_list", json!({})) else {
            panic!("the list failed")
        };
        assert_eq!(listed["projects"][0]["name"], "race");

        let ToolOutcome::Ok(summary) = run(&dispatcher, &fakes, "ovrley_summary", json!({})) else {
            panic!()
        };
        assert_eq!(summary["project"]["name"], "race.oly");

        dispatcher.dispatch(|store, _| store.editor.set_selected_second(4.0));
        // Unsaved changes: the call waits for the user, who cancels.
        let (now, receiver) = start(&dispatcher, "project_new", json!({}));
        assert!(now.is_none());
        dispatcher.dispatch(|store, services| {
            projects::answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel)
        });
        assert_eq!(
            receiver.recv().unwrap(),
            ToolOutcome::Ok(json!({ "created": false }))
        );

        let ToolOutcome::Ok(created) = run(
            &dispatcher,
            &fakes,
            "project_new",
            json!({ "unsaved": "discard" }),
        ) else {
            panic!()
        };
        assert_eq!(created["project"]["status"], "New");

        let ToolOutcome::Ok(opened) =
            run(&dispatcher, &fakes, "project_open", json!({ "path": path }))
        else {
            panic!("the open failed")
        };
        assert_eq!(opened["project"]["status"], "Saved");
        assert_eq!(opened["legacyBatchVideoFolder"], Value::Null);
        assert!(matches!(
            run(
                &dispatcher,
                &fakes,
                "project_open",
                json!({ "path": "race.oly" })
            ),
            ToolOutcome::InvalidParams(_)
        ));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    fn uuid_like() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
