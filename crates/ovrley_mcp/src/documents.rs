//! The tools of what the user opens and saves: projects, templates and the activity file.

use crate::catalog::{Answer, NoArguments, Tool, ToolError, ToolResult};
use ovrley_app::features::shell::{self, UnsavedAnswer};
use ovrley_app::features::projects::{self, ProjectOpenRequest};
use ovrley_app::features::templates;
use ovrley_app::store::agent::{describe_activity, describe_template, AbsolutePath, AgentReply};
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use ovrley_core::template::TemplateKind;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

/// What to do with unsaved changes; without it, OVRLEY asks the user and the call waits.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum UnsavedChoice {
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
enum MissingSourcesChoice {
    /// Load the project without it (Load Anyway).
    Skip,
}

/// The folder to list.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProjectListArguments {
    /// An absolute folder; without it, the folder OVRLEY remembers, or its default project folder.
    directory: Option<AbsolutePath>,
}

/// A new project.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProjectNewArguments {
    unsaved: Option<UnsavedChoice>,
}

/// The project to open.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProjectOpenArguments {
    /// The `.oly` file's absolute path.
    path: AbsolutePath,
    unsaved: Option<UnsavedChoice>,
    missing_sources: Option<MissingSourcesChoice>,
}

/// Where to save.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProjectSaveArguments {
    /// An absolute `.oly` path (Save As); without it, the loaded project's file.
    path: Option<AbsolutePath>,
}

/// A file to open.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PathArguments {
    /// The file's absolute path.
    path: AbsolutePath,
}

/// The template to load.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TemplateArguments {
    /// A template id from `template_list`.
    template_id: String,
}

/// Whether a lap counts in lap widgets.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LapArguments {
    /// The lap's id, from `ovrley_summary`'s activity laps.
    lap_id: usize,
    /// `false` leaves the lap out of lap widgets; `true` includes it again.
    enabled: bool,
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool::waiting(
            "project_list",
            "List projects",
            "Lists the OVRLEY projects (.oly) of a folder: name and path.",
            list_projects,
        )
        .read_only(),
        Tool::waiting(
            "project_new",
            "New project",
            "Starts a new project, as New Project does: the loaded template's saved state, no \
             activity, no videos, the default settings; the undo history is cleared. With unsaved \
             changes and without unsaved, OVRLEY asks the user and the call waits: {created: \
             false} when the user cancels. unsaved: save fails for a project never saved; call \
             project_save with a path first.",
            new_project,
        )
        .destructive(),
        Tool::waiting(
            "project_open",
            "Open a project",
            "Opens an OVRLEY project (.oly), as Load Project does, replacing the session and \
             clearing the undo history. With unsaved changes and without unsaved, OVRLEY asks the \
             user first; a missing activity, or the missing video of a single-video project, asks \
             to be located unless missingSources is skip. The call waits for those answers: \
             {opened: false} when the user cancels. Missing videos of a project with several load \
             as missing; give them their files with video_locate. Replies with the project and \
             the video folder of an older batch project.",
            open_project,
        )
        .destructive(),
        Tool::waiting(
            "project_save",
            "Save the project",
            "Saves the project, as Save Project does, or to path as Save Project As does. Without \
             path it fails for a project never saved. Replies with the project and, when an \
             older-format file was replaced, the backup it kept.",
            save_project,
        )
        .destructive(),
        Tool::read(
            "template_list",
            "List templates",
            "Lists the bundled and user templates, grouped by resolution as the template select \
             shows them. Use an id with template_load.",
            list_templates,
        ),
        Tool::waiting(
            "template_load",
            "Load a template",
            "Loads a template from template_list, replacing the overlay and clearing the undo \
             history, as the template select does. OVRLEY opens it again at the next start.",
            load_template,
        )
        .destructive(),
        Tool::waiting(
            "template_import",
            "Import a template file",
            "Opens an OVRLEY template file, replacing the overlay and clearing the undo history, \
             as Import Template does.",
            import_template,
        )
        .destructive(),
        Tool::waiting(
            "template_new",
            "New template",
            "Starts a new, empty template, as New Template does. With unsaved changes OVRLEY asks \
             the user first and this call waits for the answer: {created: false} when the user \
             cancels. If the user chooses Save and the template has never been saved to a file, \
             the call fails; call template_save with a path, then template_new again.",
            new_template,
        )
        .destructive(),
        Tool::waiting(
            "template_save",
            "Save the template",
            "Saves the template to a file (.json), as Save Template does, and makes that file the \
             loaded template.",
            save_template,
        )
        .destructive(),
        Tool::waiting(
            "activity_import",
            "Import an activity",
            "Opens an activity file (GPX, FIT, TCX, SRT, IGC, CSV, VBO, UDDF, SSRF or Subsurface \
             XML) as the File menu's Import Activity does, and answers with the activity once it \
             is loaded. It replaces the current activity.",
            import_activity,
        )
        .destructive(),
        Tool::edit(
            "activity_delete",
            "Remove the activity",
            "Removes the imported activity file, as the activity drawer's delete button does. \
             Fails when no activity file is loaded.",
            delete_activity,
        )
        .destructive(),
        Tool::edit(
            "activity_set_lap_enabled",
            "Show or hide a lap",
            "Includes or excludes one detected lap from lap widgets, as the activity drawer's lap \
             switches do.",
            set_lap_enabled,
        ),
    ]
}

fn list_projects(
    args: ProjectListArguments,
    _: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let directory = args.directory.map(String::from);
    projects::list_projects(services, directory, move |_, listed| {
        reply.send(listed.map(|listed| {
            let projects: Vec<Value> = listed
                .into_iter()
                .map(|project| json!({ "name": project.name, "path": project.path }))
                .collect();
            json!({ "projects": projects })
        }));
    });
    Ok(Answer::Later)
}

fn new_project(
    args: ProjectNewArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    projects::new_project(store, services, args.unsaved.map(Into::into), Some(reply));
    Ok(Answer::Later)
}

fn open_project(
    args: ProjectOpenArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let request = ProjectOpenRequest {
        skip_missing_sources: args.missing_sources.is_some(),
        reply: Some(reply),
        from_startup: false,
    };
    let unsaved = args.unsaved.map(Into::into);
    projects::open_project(store, services, args.path.into(), unsaved, request);
    Ok(Answer::Later)
}

fn save_project(
    args: ProjectSaveArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let path = args.path.map(String::from);
    // Without either, the app would open its file dialog.
    if path.is_none() && store.projects.loaded_project_path.is_none() {
        return Err(ToolError::Failed(projects::AGENT_SAVE_NEEDS_PATH.into()));
    }
    projects::save_project(
        store,
        services,
        if path.is_some() { projects::SaveMode::SaveAs } else { projects::SaveMode::Save },
        path,
        Box::new(move |store, _, saved| reply.send(projects::save_answer(store, saved))),
    );
    Ok(Answer::Later)
}

fn list_templates(_: NoArguments, store: &Store, _: &Services) -> ToolResult {
    let templates = &store.template.templates;
    let groups: Vec<Value> = templates::template_groups(templates, "Unknown Resolution")
        .into_iter()
        .map(|group| {
            let templates: Vec<Value> = group
                .templates
                .iter()
                .map(|template| {
                    let kind = match template.kind {
                        TemplateKind::BuiltIn => "bundled",
                        TemplateKind::User => "user",
                    };
                    json!({
                        "id": template.id,
                        "name": template.name,
                        "kind": kind,
                        "width": template.width,
                        "height": template.height,
                    })
                })
                .collect();
            json!({ "resolution": group.label, "templates": templates })
        })
        .collect();
    Ok(json!({ "groups": groups }))
}

fn load_template(
    args: TemplateArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let done = templates::reply_with(reply, describe_template);
    templates::load_template(store, services, args.template_id, done);
    Ok(Answer::Later)
}

fn import_template(
    args: PathArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    templates::import_template(store, services, args.path.into(), Some(reply));
    Ok(Answer::Later)
}

fn new_template(
    _: NoArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    templates::create_new_template_request(store, services, Some(reply));
    Ok(Answer::Later)
}

fn save_template(
    args: PathArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let done = templates::reply_with(reply, describe_template);
    templates::save_template(store, services, args.path.into(), done);
    Ok(Answer::Later)
}

fn import_activity(
    args: PathArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    shell::import_activity(store, services, args.path.into(), Some(reply));
    Ok(Answer::Later)
}

fn delete_activity(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    shell::delete_activity(store)?;
    Ok(json!({ "activity": describe_activity(store) }))
}

fn set_lap_enabled(args: LapArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let summary = store.media.activity_summary.as_ref();
    if !summary.is_some_and(|summary| summary.laps.iter().any(|lap| lap.id == args.lap_id)) {
        return Err(ToolError::Failed(format!(
            "The activity has no lap {}.",
            args.lap_id
        )));
    }
    store.media.set_lap_enabled(args.lap_id, args.enabled);
    Ok(json!({ "activity": describe_activity(store) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{activity_fixture, Editor};
    use ovrley_app::features::projects::snapshot::LAST_PROJECT_DIRECTORY_KEY;
    use ovrley_widgets::model::document::OverlayConfig;

    #[test]
    fn activity_tools_import_list_laps_and_delete() {
        let editor = Editor::new();
        let path = activity_fixture();
        let activity = editor.ok("activity_import", json!({"path": path}));
        assert_eq!(activity["format"], "fit");
        let summary = editor.ok("ovrley_summary", Value::Null);
        assert_eq!(summary["activity"]["file"], json!(path));
        editor.failed(
            "activity_set_lap_enabled",
            json!({"lapId": 999, "enabled": false}),
        );
        assert_eq!(editor.undo_steps(), 0, "an import adds no undo step");
        assert!(editor.ok("activity_delete", json!({}))["activity"].is_null());
        editor.failed("activity_delete", json!({}));
        editor.invalid("activity_import", json!({"path": "relative.fit"}));
        let reason = editor.failed("activity_import", json!({"path": "/nowhere/ride.fit"}));
        assert!(!reason.is_empty());
    }

    #[test]
    fn template_tools_list_load_save_and_ask_before_new() {
        let editor = Editor::new();
        editor
            .dispatcher
            .dispatch(|store, services| templates::fetch_templates(store, services, None));
        editor.until("the template list", || {
            editor
                .dispatcher
                .with_store(|store| (!store.template.templates.is_empty()).then_some(()))
        });
        let list = editor.ok("template_list", json!({}));
        let id = &list["groups"][0]["templates"][0]["id"];
        let loaded = editor.ok("template_load", json!({"templateId": id}));
        assert_eq!(loaded["saveStatus"], "Saved");
        assert_eq!(
            editor.ok("template_new", json!({})),
            json!({"created": true}),
            "a saved template needs no confirmation"
        );

        let directory = std::env::temp_dir().join(format!("om-template-{}", std::process::id()));
        let path = directory.join("agent.json").to_string_lossy().into_owned();
        let saved = editor.ok("template_save", json!({"path": path}));
        assert_eq!(saved["source"]["path"], json!(path));
        let imported = editor.ok("template_import", json!({"path": path}));
        assert_eq!(imported["saveStatus"], "Saved");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn template_new_waits_for_the_user_and_is_cancelled_with_the_dialog() {
        let editor = Editor::new();
        editor.dispatcher.dispatch(|store, _| {
            let document = json!({"labels": [{"id": "a"}]});
            store
                .editor
                .set_config(OverlayConfig::from_value(&document).unwrap());
        });
        let waiting = editor.start("template_new", json!({}));
        assert!(waiting.answer.is_none());
        assert!(waiting.reply.try_recv().is_err(), "it waits for the user");
        assert!(editor
            .dispatcher
            .with_store(|store| store.unsaved_confirm.is_open()));
        editor.dispatcher.dispatch(|store, services| {
            templates::answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel);
        });
        assert_eq!(editor.finish(waiting), Ok(json!({"created": false})));
    }

    #[test]
    fn project_tools_save_list_open_and_start_new() {
        let directory = std::env::temp_dir().join(format!("om-projects-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let editor = Editor::with(
            Store::default(),
            json!({ LAST_PROJECT_DIRECTORY_KEY: directory.to_string_lossy() }),
        );
        let path = directory.join("race.oly").to_string_lossy().into_owned();

        editor.ok("activity_import", json!({"path": activity_fixture()}));
        assert_eq!(
            editor.failed("project_save", json!({})),
            projects::AGENT_SAVE_NEEDS_PATH
        );
        let saved = editor.ok("project_save", json!({ "path": path }));
        assert_eq!(saved["project"]["status"], "Saved");
        assert_eq!(saved["backupPath"], Value::Null);
        let listed = editor.ok("project_list", json!({}));
        assert_eq!(listed["projects"][0]["name"], "race");
        let summary = editor.ok("ovrley_summary", json!({}));
        assert_eq!(summary["project"]["name"], "race.oly");

        editor
            .dispatcher
            .dispatch(|store, _| store.editor.set_selected_second(4.0));
        // Unsaved changes: the call waits for the user, who cancels.
        let waiting = editor.start("project_new", json!({}));
        assert!(waiting.answer.is_none());
        editor.dispatcher.dispatch(|store, services| {
            projects::answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel);
        });
        assert_eq!(editor.finish(waiting), Ok(json!({"created": false})));

        let created = editor.ok("project_new", json!({"unsaved": "discard"}));
        assert_eq!(created["project"]["status"], "New");
        let opened = editor.ok("project_open", json!({ "path": path }));
        assert_eq!(opened["project"]["status"], "Saved");
        assert_eq!(opened["legacyBatchVideoFolder"], Value::Null);
        editor.invalid("project_open", json!({"path": "race.oly"}));
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
