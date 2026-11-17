//! The MCP tools (D13): their catalog, written by hand, and how each runs against the store.
//!
//! A tool runs on the window's UI thread as one dispatch, so the projections update the window. Its
//! input is always an object; each tool's argument struct derives its schema, and its doc comments
//! become the descriptions. What the agent reads (names, descriptions, errors) is English and is not
//! in the locale files.

use ovrley_app::features::widget_preview::present_pending_frame;
use ovrley_app::features::{app_shell, template_manager, widget_editor};
use ovrley_app::lib::widget::unit_system::change_unit_system_preference;
use ovrley_app::i18n::{change_language_preference, Locale};
use ovrley_app::features::overlay_editor::interaction::{resize_widget, rotate_widget, ResizeTarget};
use ovrley_app::store::agent::{describe_activity, describe_template, describe_widget, describe_widgets, AgentReply, AgentResult};
use ovrley_app::store::services::Services;
use ovrley_app::store::slices::layout::LeftDrawerTool;
use ovrley_app::store::slices::template::TemplateKind;
use ovrley_app::store::Store;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How a tool call ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The tool's result, an object.
    Ok(Value),
    /// The arguments did not match the tool's schema (a JSON-RPC invalid-params error).
    InvalidParams(String),
    /// The tool ran and failed, or could not run (an `isError` tool result).
    Failed(String),
}

/// How a call answers: now, or later through the reply it kept (a worker's result, the user's
/// answer). A call answered later has no timeout (D15).
pub enum Answer {
    Now(ToolOutcome),
    Later,
}

/// Sends a call's outcome back to the agent's connection, from the UI thread.
pub type Reply = Box<dyn FnOnce(ToolOutcome) + Send>;

impl From<AgentResult> for ToolOutcome {
    fn from(result: AgentResult) -> Self {
        match result {
            Ok(value) => Self::Ok(value),
            Err(message) => Self::Failed(message),
        }
    }
}

fn agent_reply(reply: Reply) -> AgentReply {
    AgentReply::new(move |result| reply(result.into()))
}

/// One entry of `tools/list`.
pub struct ToolDefinition {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub read_only: bool,
}

/// No arguments.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArguments {}

/// The language to show OVRLEY in.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LanguageSetArguments {
    /// The locale code, for example `en`, `de` or `pt-BR`.
    pub language: Locale,
}

/// The drawer tool to select.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawerSelectToolArguments {
    /// The drawer to show: `projects`, `activity`, `video`, `videoSync` or `widgets`.
    pub tool: LeftDrawerTool,
}

/// Whether the drawer is pinned.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawerSetPinnedArguments {
    /// `true` pins the drawer beside the workspace; `false` makes it a temporary overlay.
    pub pinned: bool,
}

/// A file to open.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArguments {
    /// The file's absolute path.
    pub path: String,
}

/// Whether a lap counts in lap widgets.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivitySetLapEnabledArguments {
    /// The lap's id, from `ovrley_summary`'s activity laps.
    pub lap_id: usize,
    /// `false` leaves the lap out of lap widgets; `true` includes it again.
    pub enabled: bool,
}

/// The template to load.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TemplateLoadArguments {
    /// A template id from `template_list`.
    pub template_id: String,
}

/// The widget to read or delete.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    pub widget_id: String,
}

/// Where to move a widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetMoveArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    pub widget_id: String,
    /// The widget's x in scene pixels, as the Position section shows it.
    pub x: f64,
    /// The widget's y in scene pixels.
    pub y: f64,
}

/// How to resize a widget: `width` and `height`, or `factor`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetResizeArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    pub widget_id: String,
    /// The content width in scene pixels; with `height`.
    #[serde(default)]
    pub width: Option<f64>,
    /// The content height in scene pixels; with `width`.
    #[serde(default)]
    pub height: Option<f64>,
    /// A scale factor of the current size, instead of `width` and `height`.
    #[serde(default)]
    pub factor: Option<f64>,
}

/// The rotation to set.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetRotateArguments {
    /// A course or image widget's id.
    pub widget_id: String,
    /// The rotation in degrees; it is rounded to a whole degree.
    pub rotation: f64,
}

/// A field patch for a widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetUpdateArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    pub widget_id: String,
    /// The fields to change and their new values, for example `{"decimals": 1, "show_units": false}`. `widget_get` lists the
    /// widget's fields and current values. A display variant's fields go under `display_variants.<display type>`.
    pub fields: serde_json::Map<String, Value>,
}

/// The unit system to switch to.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UnitSystemSetArguments {
    /// `metric` or `imperial`.
    pub unit_system: ovrley_widgets::model::document::UnitSystem,
}

/// A tool's input schema: draft 2020-12, with every reference inlined, because not every client
/// follows `$ref`.
pub fn input_schema<T: JsonSchema>() -> Value {
    let generator = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let schema: Schema = generator.into_root_schema_for::<T>();
    let mut value = schema.to_value();
    if let Value::Object(map) = &mut value {
        map.remove("$schema");
        map.remove("title");
    }
    value
}

/// The tools, in `tools/list` order.
pub fn tools() -> Vec<ToolDefinition> {
    let mut tools = vec![
        ToolDefinition {
            name: "ovrley_summary",
            title: "OVRLEY summary",
            description: "Describes the open OVRLEY window: the app version, the language, the left drawer, the activity \
                          (file, format, duration, laps and which are disabled), the template (source, name, save status), the \
                          unit system, the widgets (id, type, name, position, selected), whether undo and redo are available, and \
                          playback (state, playhead, timeline minimum and total duration), the activity trim and the video clips \
                          (id, file, start, duration, missing, warning count). The project section is empty until projects arrive.",
            input_schema: input_schema::<NoArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "language_set",
            title: "Set language",
            description: "Changes OVRLEY's language and remembers it, as the language select in Settings does.",
            input_schema: input_schema::<LanguageSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "drawer_select_tool",
            title: "Select drawer tool",
            description: "Shows a drawer, as the toolbar's buttons do. Selecting the tool that is already shown closes the drawer \
                          when it is not pinned.",
            input_schema: input_schema::<DrawerSelectToolArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "drawer_set_pinned",
            title: "Pin or unpin the drawer",
            description: "Pins the drawer beside the workspace, or unpins it so it shows as a temporary overlay. The drawer stays visible.",
            input_schema: input_schema::<DrawerSetPinnedArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "drawer_dismiss",
            title: "Dismiss the drawer",
            description: "Hides the drawer while it is a temporary overlay. A pinned drawer stays.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_get",
            title: "Read a widget",
            description: "Describes one widget: its saved data, its content rectangle on the canvas in scene pixels, the fields its \
                          type takes, and the fields its editor offers (editorFields: id, kind, label, range or options, disabled).",
            input_schema: input_schema::<WidgetArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "widget_move",
            title: "Move a widget",
            description: "Moves a widget to an absolute position in scene pixels, as the Position section does. One undo step.",
            input_schema: input_schema::<WidgetMoveArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_resize",
            title: "Resize a widget",
            description: "Resizes a widget as dragging its bottom-right handle does, keeping its top-left corner: give width and height in \
                          scene pixels, or a factor. Widgets that keep their proportions use the width. One undo step.",
            input_schema: input_schema::<WidgetResizeArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_rotate",
            title: "Rotate a widget",
            description: "Rotates a course or image widget about its centre, as its rotation handle does. Other widgets do not rotate and \
                          the call fails. One undo step.",
            input_schema: input_schema::<WidgetRotateArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_update",
            title: "Change widget settings",
            description: "Changes a widget's settings, as its editor in the Widgets tab does: give the fields to change. The widget's \
                          fields and current values come from widget_get. A field the editor changes with a rule (a bar fill style \
                          suggesting bars, a size clamping the corner radius, a map shape, …) goes through the same rule. A patch with an \
                          unknown field or a value the widget cannot \
                          show changes nothing and fails with the reason. One undo step.",
            input_schema: input_schema::<WidgetUpdateArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_delete",
            title: "Delete a widget",
            description: "Deletes a widget, as the Delete key and the sidebar's delete button do. One undo step.",
            input_schema: input_schema::<WidgetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "history_undo",
            title: "Undo",
            description: "Undoes the last edit, as Undo in the canvas toolbar does. Fails when there is nothing to undo or a confirmation \
                          dialog is open.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "history_redo",
            title: "Redo",
            description: "Redoes the last undone edit, as Redo in the canvas toolbar does. Fails when there is nothing to redo or a \
                          confirmation dialog is open.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "unit_system_set",
            title: "Set the unit system",
            description: "Switches OVRLEY between metric and imperial units and remembers it, as Settings > Units does. The open \
                          overlay's widgets convert their units as one undo step.",
            input_schema: input_schema::<UnitSystemSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "template_list",
            title: "List templates",
            description: "Lists the bundled and user templates, grouped by resolution as the template select shows them. \
                          Use an id with template_load.",
            input_schema: input_schema::<NoArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "activity_import",
            title: "Import an activity",
            description: "Opens an activity file (GPX, FIT, TCX, SRT, IGC, CSV, VBO, UDDF, SSRF or Subsurface XML) as the File menu's \
                          Import Activity does, and answers with the activity once it is loaded. It replaces the current activity.",
            input_schema: input_schema::<PathArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "activity_delete",
            title: "Remove the activity",
            description: "Removes the imported activity file, as the activity drawer's delete button does. Fails when no activity file is loaded.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "activity_set_lap_enabled",
            title: "Show or hide a lap",
            description: "Includes or excludes one detected lap from lap widgets, as the activity drawer's lap switches do.",
            input_schema: input_schema::<ActivitySetLapEnabledArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "template_load",
            title: "Load a template",
            description: "Loads a template from template_list, replacing the overlay and clearing the undo history, as the template \
                          select does. OVRLEY opens it again at the next start.",
            input_schema: input_schema::<TemplateLoadArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "template_import",
            title: "Import a template file",
            description: "Opens an OVRLEY template file, replacing the overlay and clearing the undo history, as Import Template does.",
            input_schema: input_schema::<PathArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "template_new",
            title: "New template",
            description: "Starts a new, empty template, as New Template does. With unsaved changes OVRLEY asks the user first and this \
                          call waits for the answer: {created: false} when the user cancels. If the user chooses Save and the template \
                          has never been saved to a file, the call fails; call template_save with a path, then template_new again.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "template_save",
            title: "Save the template",
            description: "Saves the template to a file (.json), as Save Template does, and makes that file the loaded template.",
            input_schema: input_schema::<PathArguments>(),
            read_only: false,
        },
    ];
    tools.extend(crate::player::tools());
    tools.extend(crate::widgets::tools());
    tools
}

pub fn is_tool(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

/// What an agent sees when edits are off.
pub const EDITS_OFF: &str = "Agent edits are turned off in OVRLEY. Turn on \"Allow agent edits\" in Settings > General, then try again.";

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    // Clients may omit `arguments` for a tool that takes none.
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

fn drawer_state(store: &Store) -> Value {
    json!({
        "visible": store.layout.left_drawer_visible,
        "pinned": store.layout.left_drawer_pinned,
        "activeTool": store.layout.active_left_drawer_tool,
    })
}

fn summary(store: &Store) -> Value {
    json!({
        "appVersion": ovrley_app::lib::utils::app_version(),
        "language": store.i18n.language,
        "unitSystem": store.editor.unit_system,
        "drawer": drawer_state(store),
        "project": null,
        "template": describe_template(store),
        "activity": describe_activity(store),
        "videos": crate::player::describe_videos(store),
        "playback": crate::player::describe_playback(store),
        "activityTrim": crate::player::describe_activity_trim(store),
        "widgets": describe_widgets(store),
        "history": { "canUndo": store.history.can_undo(), "canRedo": store.history.can_redo() },
    })
}

fn template_list(store: &Store) -> Value {
    let groups: Vec<Value> = template_manager::template_groups(&store.template.templates, "Unknown Resolution")
        .into_iter()
        .map(|group| {
            let templates: Vec<Value> = group
                .templates
                .iter()
                .map(|template| {
                    json!({
                        "id": template.id,
                        "name": template.name,
                        "kind": match template.kind { TemplateKind::BuiltIn => "bundled", TemplateKind::User => "user" },
                        "width": template.width,
                        "height": template.height,
                    })
                })
                .collect();
            json!({ "resolution": group.label, "templates": templates })
        })
        .collect();
    json!({ "groups": groups })
}

fn absolute(path: String) -> Result<String, ToolOutcome> {
    if std::path::Path::new(&path).is_absolute() {
        Ok(path)
    } else {
        Err(ToolOutcome::InvalidParams(format!("The path must be absolute: {path}")))
    }
}

/// Runs a tool against the store, as one dispatch on the UI thread. A tool whose result comes from
/// a worker or the user keeps `reply` and answers [`Answer::Later`]. Every call fails while the user
/// has not allowed agent edits.
pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services, reply: Reply) -> Answer {
    if !store.agent_session.allow_agent_edits {
        return Answer::Now(ToolOutcome::Failed(EDITS_OFF.into()));
    }
    let run = move |store: &mut Store, services: &mut Services| -> Result<Answer, ToolOutcome> {
        let now = |value: Value| Ok(Answer::Now(ToolOutcome::Ok(value)));
        match name {
            "ovrley_summary" => {
                arguments::<NoArguments>(arguments_value)?;
                now(summary(store))
            }
            "language_set" => {
                let arguments: LanguageSetArguments = arguments(arguments_value)?;
                change_language_preference(store, services, arguments.language);
                now(json!({ "language": store.i18n.language }))
            }
            "drawer_select_tool" => {
                let arguments: DrawerSelectToolArguments = arguments(arguments_value)?;
                store.layout.select_left_drawer_tool(arguments.tool);
                now(json!({ "drawer": drawer_state(store) }))
            }
            "drawer_set_pinned" => {
                let arguments: DrawerSetPinnedArguments = arguments(arguments_value)?;
                store.layout.set_left_drawer_pinned(arguments.pinned);
                now(json!({ "drawer": drawer_state(store) }))
            }
            "drawer_dismiss" => {
                arguments::<NoArguments>(arguments_value)?;
                store.layout.dismiss_left_drawer_overlay();
                now(json!({ "drawer": drawer_state(store) }))
            }
            "template_list" => {
                arguments::<NoArguments>(arguments_value)?;
                now(template_list(store))
            }
            "history_undo" | "history_redo" => {
                arguments::<NoArguments>(arguments_value)?;
                use ovrley_app::features::undo_redo::{history_enabled, redo, undo};
                if !history_enabled(store) {
                    return Err(ToolOutcome::Failed("Undo and redo are unavailable while a confirmation dialog is open in OVRLEY.".into()));
                }
                let done = if name == "history_undo" { undo(store) } else { redo(store) };
                if !done {
                    return Err(ToolOutcome::Failed(format!("There is nothing to {}.", if name == "history_undo" { "undo" } else { "redo" })));
                }
                now(json!({ "history": { "canUndo": store.history.can_undo(), "canRedo": store.history.can_redo() } }))
            }
            "widget_get" => {
                let arguments: WidgetArguments = arguments(arguments_value)?;
                let mut described = describe_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?;
                described["editorFields"] = Value::Array(widget_editor::editable_fields(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?);
                now(described)
            }
            "widget_move" => {
                let arguments: WidgetMoveArguments = arguments(arguments_value)?;
                if !arguments.x.is_finite() || !arguments.y.is_finite() {
                    return Err(ToolOutcome::InvalidParams("x and y must be finite numbers".into()));
                }
                let mut patch = serde_json::Map::new();
                patch.insert("x".into(), ovrley_widgets::model::document::number(arguments.x));
                patch.insert("y".into(), ovrley_widgets::model::document::number(arguments.y));
                widget_editor::update_widget_data(store, &arguments.widget_id, patch).map_err(ToolOutcome::Failed)?;
                present_pending_frame(store, services);
                now(describe_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?)
            }
            "widget_update" => {
                let arguments: WidgetUpdateArguments = arguments(arguments_value)?;
                widget_editor::update_widget_fields(store, services, &arguments.widget_id, arguments.fields).map_err(ToolOutcome::Failed)?;
                now(describe_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?)
            }
            "unit_system_set" => {
                let arguments: UnitSystemSetArguments = arguments(arguments_value)?;
                change_unit_system_preference(store, services, arguments.unit_system);
                now(json!({ "unitSystem": store.editor.unit_system, "widgets": describe_widgets(store) }))
            }
            "widget_resize" => {
                let arguments: WidgetResizeArguments = arguments(arguments_value)?;
                let target = match (arguments.width, arguments.height, arguments.factor) {
                    (Some(width), Some(height), None) => ResizeTarget::Size { width, height },
                    (None, None, Some(factor)) => ResizeTarget::Factor(factor),
                    _ => return Err(ToolOutcome::InvalidParams("Give width and height, or factor.".into())),
                };
                resize_widget(store, &arguments.widget_id, target).map_err(ToolOutcome::Failed)?;
                present_pending_frame(store, services);
                now(describe_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?)
            }
            "widget_rotate" => {
                let arguments: WidgetRotateArguments = arguments(arguments_value)?;
                if !arguments.rotation.is_finite() {
                    return Err(ToolOutcome::InvalidParams("rotation must be a finite number".into()));
                }
                rotate_widget(store, &arguments.widget_id, arguments.rotation).map_err(ToolOutcome::Failed)?;
                present_pending_frame(store, services);
                now(describe_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?)
            }
            "widget_delete" => {
                let arguments: WidgetArguments = arguments(arguments_value)?;
                widget_editor::delete_widget(store, &arguments.widget_id).map_err(ToolOutcome::Failed)?;
                now(json!({ "widgets": describe_widgets(store) }))
            }
            "activity_import" => {
                let path = absolute(arguments::<PathArguments>(arguments_value)?.path)?;
                app_shell::import_activity(store, services, path, Some(agent_reply(reply)));
                Ok(Answer::Later)
            }
            "activity_delete" => {
                arguments::<NoArguments>(arguments_value)?;
                app_shell::delete_activity(store).map_err(ToolOutcome::Failed)?;
                now(json!({ "activity": describe_activity(store) }))
            }
            "activity_set_lap_enabled" => {
                let arguments: ActivitySetLapEnabledArguments = arguments(arguments_value)?;
                let known = store.media.activity_summary.as_ref().is_some_and(|summary| summary.laps.iter().any(|lap| lap.id == arguments.lap_id));
                if !known {
                    return Err(ToolOutcome::Failed(format!("The activity has no lap {}.", arguments.lap_id)));
                }
                store.media.set_lap_enabled(arguments.lap_id, arguments.enabled);
                now(json!({ "activity": describe_activity(store) }))
            }
            "template_load" => {
                let arguments: TemplateLoadArguments = arguments(arguments_value)?;
                template_manager::load_template(store, services, arguments.template_id, template_manager::reply_with(agent_reply(reply), describe_template));
                Ok(Answer::Later)
            }
            "template_import" => {
                let path = absolute(arguments::<PathArguments>(arguments_value)?.path)?;
                template_manager::import_template(store, services, path, Some(agent_reply(reply)));
                Ok(Answer::Later)
            }
            "template_new" => {
                arguments::<NoArguments>(arguments_value)?;
                template_manager::create_new_template_request(store, services, Some(agent_reply(reply)));
                Ok(Answer::Later)
            }
            "template_save" => {
                let path = absolute(arguments::<PathArguments>(arguments_value)?.path)?;
                template_manager::save_template(store, services, path, template_manager::reply_with(agent_reply(reply), describe_template));
                Ok(Answer::Later)
            }
            other if crate::widgets::handles(other) => crate::widgets::call(other, arguments_value, store, services, reply),
            other => crate::player::call(other, arguments_value, store, services, reply).unwrap_or_else(|| Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}")))),
        }
    };
    match run(store, services) {
        Ok(answer) => answer,
        Err(outcome) => Answer::Now(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_app::store::services::fakes::fake_services;
    use ovrley_app::store::Dispatcher;

    fn dispatcher(allowed: bool) -> (Dispatcher, ovrley_app::store::services::fakes::Fakes) {
        let (parts, fakes) = fake_services(json!({ "allowAgentEdits": allowed }));
        let mut store = Store::hydrate(&parts.preferences).unwrap();
        store.theme = Some(ovrley_app::lib::template::ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() });
        (Dispatcher::new(store, parts), fakes)
    }

    /// A call answered in its dispatch.
    fn run(dispatcher: &Dispatcher, name: &'static str, arguments: Value) -> ToolOutcome {
        match dispatcher.dispatch_with_result(move |store, services| call(name, arguments, store, services, Box::new(|_| panic!("answered later")))).unwrap() {
            Answer::Now(outcome) => outcome,
            Answer::Later => panic!("{name} answers later"),
        }
    }

    /// A call answered later: runs posted worker results until the reply arrives.
    fn run_later(dispatcher: &Dispatcher, fakes: &ovrley_app::store::services::fakes::Fakes, name: &'static str, arguments: Value) -> ToolOutcome {
        let (sender, receiver) = std::sync::mpsc::channel();
        let answer = dispatcher
            .dispatch_with_result(move |store, services| {
                call(name, arguments, store, services, Box::new(move |outcome| sender.send(outcome).expect("the test waits")))
            })
            .unwrap();
        if let Answer::Now(outcome) = answer {
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

    fn fixture(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ovrley_core/tests/fixtures/activity").join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn activity_tools_import_list_laps_and_delete() {
        let (dispatcher, fakes) = dispatcher(true);
        let ToolOutcome::Ok(activity) = run_later(&dispatcher, &fakes, "activity_import", json!({"path": fixture("test-ride.fit")})) else { panic!("import failed") };
        assert_eq!(activity["format"], "fit");
        let ToolOutcome::Ok(summary) = run(&dispatcher, "ovrley_summary", Value::Null) else { panic!() };
        assert_eq!(summary["activity"]["file"], json!(fixture("test-ride.fit")));
        assert!(matches!(run(&dispatcher, "activity_set_lap_enabled", json!({"lapId": 999, "enabled": false})), ToolOutcome::Failed(_)));
        assert!(dispatcher.with_store(|store| !store.history.can_undo()), "an import adds no undo step");
        let ToolOutcome::Ok(deleted) = run(&dispatcher, "activity_delete", json!({})) else { panic!() };
        assert!(deleted["activity"].is_null());
        assert!(matches!(run(&dispatcher, "activity_delete", json!({})), ToolOutcome::Failed(_)));
        assert!(matches!(run(&dispatcher, "activity_import", json!({"path": "relative.fit"})), ToolOutcome::InvalidParams(_)));
        let ToolOutcome::Failed(message) = run_later(&dispatcher, &fakes, "activity_import", json!({"path": "/nowhere/ride.fit"})) else { panic!() };
        assert!(!message.is_empty());
    }

    #[test]
    fn template_tools_list_load_save_and_ask_before_new() {
        let (dispatcher, fakes) = dispatcher(true);
        dispatcher.dispatch(|store, services| template_manager::fetch_templates(store, services, None));
        for _ in 0..400 {
            fakes.ui_post.drain(&dispatcher);
            if dispatcher.with_store(|store| !store.template.templates.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let ToolOutcome::Ok(list) = run(&dispatcher, "template_list", json!({})) else { panic!() };
        let id = list["groups"][0]["templates"][0]["id"].as_str().unwrap().to_string();
        let ToolOutcome::Ok(loaded) = run_later(&dispatcher, &fakes, "template_load", json!({"templateId": id})) else { panic!("load failed") };
        assert_eq!(loaded["saveStatus"], "Saved");
        let ToolOutcome::Ok(created) = run_later(&dispatcher, &fakes, "template_new", json!({})) else { panic!() };
        assert_eq!(created, json!({"created": true}), "a saved template needs no confirmation");

        let directory = std::env::temp_dir().join(format!("ovrley-mcp-save-{}", std::process::id()));
        let path = directory.join("agent.json").to_string_lossy().into_owned();
        let ToolOutcome::Ok(saved) = run_later(&dispatcher, &fakes, "template_save", json!({"path": path})) else { panic!("save failed") };
        assert_eq!(saved["source"]["path"], json!(path));
        let ToolOutcome::Ok(imported) = run_later(&dispatcher, &fakes, "template_import", json!({"path": path})) else { panic!("import failed") };
        assert_eq!(imported["saveStatus"], "Saved");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn widget_tools_move_rotate_and_delete_as_one_undo_step_each() {
        let (dispatcher, _) = dispatcher(true);
        dispatcher.dispatch(|store, _| {
            store.history.set_mode(ovrley_app::features::undo_redo::HistoryMode::Reset);
            store.editor.set_config(ovrley_widgets::model::document::OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "a", "text": "A", "x": 1, "y": 2}]})).unwrap());
        });
        let ToolOutcome::Ok(moved) = run(&dispatcher, "widget_move", json!({"widgetId": "a", "x": 40, "y": 50})) else { panic!() };
        assert_eq!((moved["data"]["x"].clone(), moved["data"]["y"].clone()), (json!(40), json!(50)));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1);
        assert!(matches!(run(&dispatcher, "widget_rotate", json!({"widgetId": "a", "rotation": 10})), ToolOutcome::Failed(_)), "labels do not rotate (or have not rendered)");
        assert!(matches!(run(&dispatcher, "widget_move", json!({"widgetId": "nope", "x": 0, "y": 0})), ToolOutcome::Failed(_)));
        assert!(matches!(run(&dispatcher, "widget_resize", json!({"widgetId": "a", "width": 10})), ToolOutcome::InvalidParams(_)));
        let ToolOutcome::Ok(left) = run(&dispatcher, "widget_delete", json!({"widgetId": "a"})) else { panic!() };
        assert_eq!(left["widgets"], json!([]));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2);
    }

    #[test]
    fn widget_update_patches_fields_as_one_undo_step_and_refuses_unknown_ones() {
        let (dispatcher, fakes) = dispatcher(true);
        dispatcher.dispatch(|store, _| {
            store.history.set_mode(ovrley_app::features::undo_redo::HistoryMode::Reset);
            let value = json!({"id": "s", "value": "speed", "display_type": "text", "display_unit": "kmh", "decimals": 0, "x": 10, "y": 10});
            store.editor.set_config(ovrley_widgets::model::document::OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080}, "values": [value]})).unwrap());
        });
        let ToolOutcome::Ok(updated) = run(&dispatcher, "widget_update", json!({"widgetId": "s", "fields": {"decimals": 1, "show_units": false}})) else { panic!("update failed") };
        assert_eq!((updated["data"]["decimals"].clone(), updated["data"]["show_units"].clone()), (json!(1), json!(false)));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1);
        let ToolOutcome::Failed(message) = run(&dispatcher, "widget_update", json!({"widgetId": "s", "fields": {"radius": 3}})) else { panic!() };
        assert!(message.contains("no field radius"), "{message}");
        assert!(matches!(run(&dispatcher, "widget_update", json!({"widgetId": "s"})), ToolOutcome::InvalidParams(_)));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1, "refused patches leave no undo step");

        let ToolOutcome::Ok(units) = run(&dispatcher, "unit_system_set", json!({"unitSystem": "imperial"})) else { panic!() };
        assert_eq!(units["unitSystem"], "imperial");
        assert_eq!(dispatcher.with_store(|store| store.editor.config.widget("s").unwrap()["display_unit"].clone()), json!("mph"));
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()["unitSystem"], "imperial");
        assert!(matches!(run(&dispatcher, "unit_system_set", json!({"unitSystem": "nautical"})), ToolOutcome::InvalidParams(_)));
    }

    #[test]
    fn history_tools_undo_and_redo_and_fail_when_empty() {
        let (dispatcher, _) = dispatcher(true);
        dispatcher.dispatch(|store, _| {
            store.history.set_mode(ovrley_app::features::undo_redo::HistoryMode::Reset);
            store.editor.set_config(ovrley_widgets::model::document::OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "a", "text": "A", "x": 1, "y": 2}]})).unwrap());
        });
        assert!(matches!(run(&dispatcher, "history_undo", json!({})), ToolOutcome::Failed(_)), "nothing to undo after a load");
        run(&dispatcher, "widget_move", json!({"widgetId": "a", "x": 40, "y": 50}));
        assert_eq!(run(&dispatcher, "history_undo", json!({})), ToolOutcome::Ok(json!({"history": {"canUndo": false, "canRedo": true}})));
        assert_eq!(dispatcher.with_store(|store| store.editor.config.widget("a").unwrap()["x"].clone()), json!(1));
        assert_eq!(run(&dispatcher, "history_redo", json!({})), ToolOutcome::Ok(json!({"history": {"canUndo": true, "canRedo": false}})));
        assert_eq!(dispatcher.with_store(|store| store.editor.config.widget("a").unwrap()["x"].clone()), json!(40));
        assert!(matches!(run(&dispatcher, "history_redo", json!({})), ToolOutcome::Failed(_)));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1, "undo and redo are not steps themselves");
    }

    #[test]
    fn template_new_waits_for_the_user_and_is_cancelled_with_the_dialog() {
        let (dispatcher, _) = dispatcher(true);
        dispatcher.dispatch(|store, _| store.editor.set_config(ovrley_widgets::model::document::OverlayConfig::from_value(&json!({"labels": [{"id": "a"}]})).unwrap()));
        let (sender, receiver) = std::sync::mpsc::channel();
        let answer = dispatcher
            .dispatch_with_result(move |store, services| call("template_new", json!({}), store, services, Box::new(move |outcome| sender.send(outcome).unwrap())))
            .unwrap();
        assert!(matches!(answer, Answer::Later));
        assert!(receiver.try_recv().is_err(), "it waits for the user");
        assert!(dispatcher.with_store(|store| store.unsaved_confirm.is_open()));
        dispatcher.dispatch(|store, services| template_manager::answer_unsaved_confirm(store, services, app_shell::UnsavedAnswer::Cancel));
        assert_eq!(receiver.try_recv().unwrap(), ToolOutcome::Ok(json!({"created": false})));
    }

    #[test]
    fn the_summary_describes_the_window() {
        let (dispatcher, _) = dispatcher(true);
        let ToolOutcome::Ok(summary) = run(&dispatcher, "ovrley_summary", Value::Null) else { panic!("summary failed") };
        assert_eq!(summary["language"], "en");
        assert_eq!(summary["drawer"], json!({"visible": false, "pinned": false, "activeTool": "widgets"}));
        assert!(summary["widgets"].is_array());
        assert!(summary["project"].is_null());
        assert!(summary["activity"].is_null());
        assert_eq!(summary["template"]["saveStatus"], "Draft");
        assert_eq!(summary["history"], json!({"canUndo": false, "canRedo": false}));
    }

    #[test]
    fn language_set_changes_and_persists_the_language() {
        let (dispatcher, fakes) = dispatcher(true);
        assert_eq!(run(&dispatcher, "language_set", json!({"language": "de"})), ToolOutcome::Ok(json!({"language": "de"})));
        assert_eq!(dispatcher.with_store(|store| store.i18n.language), Locale::De);
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()["lng"], "de");
    }

    #[test]
    fn drawer_tools_follow_the_drawer_rules() {
        let (dispatcher, _) = dispatcher(true);
        let drawer = |visible: bool, pinned: bool, tool: &str| ToolOutcome::Ok(json!({"drawer": {"visible": visible, "pinned": pinned, "activeTool": tool}}));
        assert_eq!(run(&dispatcher, "drawer_select_tool", json!({"tool": "video"})), drawer(true, false, "video"));
        assert_eq!(run(&dispatcher, "drawer_select_tool", json!({"tool": "video"})), drawer(false, false, "video"), "the same tool closes an unpinned drawer");
        assert_eq!(run(&dispatcher, "drawer_set_pinned", json!({"pinned": true})), drawer(true, true, "video"));
        assert_eq!(run(&dispatcher, "drawer_dismiss", json!({})), drawer(true, true, "video"), "a pinned drawer stays");
        assert_eq!(run(&dispatcher, "drawer_set_pinned", json!({"pinned": false})), drawer(true, false, "video"));
        assert_eq!(run(&dispatcher, "drawer_dismiss", Value::Null), drawer(false, false, "video"));
    }

    #[test]
    fn invalid_arguments_are_invalid_params() {
        let (dispatcher, _) = dispatcher(true);
        for (name, arguments) in [
            ("language_set", json!({"language": "xx"})),
            ("language_set", json!({})),
            ("drawer_select_tool", json!({"tool": "timeline"})),
            ("drawer_set_pinned", json!({"pinned": "yes"})),
            ("drawer_dismiss", json!({"extra": 1})),
        ] {
            assert!(matches!(run(&dispatcher, name, arguments), ToolOutcome::InvalidParams(_)), "{name}");
        }
    }

    #[test]
    fn every_call_fails_while_edits_are_off() {
        let (dispatcher, _) = dispatcher(false);
        assert_eq!(run(&dispatcher, "ovrley_summary", Value::Null), ToolOutcome::Failed(EDITS_OFF.into()));
        assert_eq!(run(&dispatcher, "drawer_select_tool", json!({"tool": "video"})), ToolOutcome::Failed(EDITS_OFF.into()));
        assert!(!dispatcher.with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn schemas_are_objects_without_references() {
        for tool in tools() {
            let text = tool.input_schema.to_string();
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            assert!(!text.contains("$ref") && !text.contains("$defs"), "{}: {text}", tool.name);
        }
        let language = input_schema::<LanguageSetArguments>();
        assert!(language["properties"]["language"]["description"].as_str().unwrap().contains("locale code"));
        assert!(language["properties"]["language"].to_string().contains("pt-BR"));
    }
}
