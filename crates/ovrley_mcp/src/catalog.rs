//! What a tool is, the list of them all, and how a call runs against the store.
//!
//! A tool is one entry: its name, its texts, what it may do, and a handler that takes typed
//! arguments. The arguments' types derive their schemas, and their doc comments become the
//! descriptions agents read. Tool names and descriptions are English and are not in the locale
//! files.
//!
//! The tools about the window as a whole are here: the summary, language, units, drawer, history.

use crate::{documents, overlay, render, timeline, video_sync};
use ovrley_app::features::undo_redo::{history_enabled, redo, undo};
use ovrley_app::i18n::{change_language_preference, Locale};
use ovrley_app::lib::widget::unit_system::change_unit_system_preference;
use ovrley_app::store::agent::{describe_activity, describe_template, describe_widgets, AgentReply};
use ovrley_app::store::services::Services;
use ovrley_app::store::slices::layout::LeftDrawerTool;
use ovrley_app::store::Store;
use ovrley_widgets::model::document::UnitSystem;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::LazyLock;

/// Why a tool call has no result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolError {
    /// The arguments are not ones the tool takes (a JSON-RPC invalid-params error).
    InvalidParams(String),
    /// The tool ran and failed, or could not run (an `isError` tool result).
    Failed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParams(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ToolError {}

/// An action of the app that refused: the tool failed.
impl From<String> for ToolError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

/// A tool's result, or why it has none.
pub type ToolResult = Result<Value, ToolError>;

/// How a call answers.
#[derive(Debug)]
pub(crate) enum Answer {
    /// In its dispatch.
    Now(ToolResult),
    /// Through the reply it kept, once a worker or the user is done. This has no timeout.
    Later,
}

/// What an agent reads when edits are off.
pub(crate) const EDITS_OFF: &str = "Agent edits are turned off in OVRLEY. Turn on \
    \"Allow agent edits\" in Settings, then try again.";

/// What a tool may do, for the hints agents decide on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Read,
    /// Changes something that can be changed back.
    Edit,
    /// Can lose what undo does not bring back.
    Destructive,
}

type Handler = Box<dyn Fn(Value, &mut Store, &mut Services, AgentReply) -> Answer + Send + Sync>;

/// One tool.
pub(crate) struct Tool {
    pub(crate) name: &'static str,
    title: &'static str,
    description: &'static str,
    effect: Effect,
    input_schema: Value,
    handler: Handler,
}

impl Tool {
    /// A tool that only reads, and answers in its dispatch.
    pub(crate) fn read<A: DeserializeOwned + JsonSchema + 'static>(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        run: fn(A, &Store, &Services) -> ToolResult,
    ) -> Self {
        let handler: Handler = Box::new(move |value, store, services, _| {
            Answer::Now(parse(value).and_then(|arguments| run(arguments, store, services)))
        });
        Self {
            name,
            title,
            description,
            effect: Effect::Read,
            input_schema: input_schema::<A>(),
            handler,
        }
    }

    /// A tool that changes something, and answers in its dispatch.
    pub(crate) fn edit<A: DeserializeOwned + JsonSchema + 'static>(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        run: fn(A, &mut Store, &mut Services) -> ToolResult,
    ) -> Self {
        let handler: Handler = Box::new(move |value, store, services, _| {
            Answer::Now(parse(value).and_then(|arguments| run(arguments, store, services)))
        });
        Self {
            name,
            title,
            description,
            effect: Effect::Edit,
            input_schema: input_schema::<A>(),
            handler,
        }
    }

    /// A tool that may wait for a worker or the user, through the reply it is given.
    pub(crate) fn waiting<A: DeserializeOwned + JsonSchema + 'static>(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        run: fn(A, &mut Store, &mut Services, AgentReply) -> Result<Answer, ToolError>,
    ) -> Self {
        let handler: Handler = Box::new(move |value, store, services, reply| {
            parse(value)
                .and_then(|arguments| run(arguments, store, services, reply))
                .unwrap_or_else(|error| Answer::Now(Err(error)))
        });
        Self {
            name,
            title,
            description,
            effect: Effect::Edit,
            input_schema: input_schema::<A>(),
            handler,
        }
    }

    pub(crate) fn read_only(self) -> Self {
        Self { effect: Effect::Read, ..self }
    }

    pub(crate) fn destructive(self) -> Self {
        Self { effect: Effect::Destructive, ..self }
    }

    /// The tool's entry in `tools/list`.
    pub(crate) fn listing(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": {
                "readOnlyHint": self.effect == Effect::Read,
                "destructiveHint": self.effect == Effect::Destructive,
                "openWorldHint": false,
            },
        })
    }
}

/// No arguments.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArguments {}

fn parse<A: DeserializeOwned>(value: Value) -> Result<A, ToolError> {
    // Clients may omit `arguments` for a tool that takes none.
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value)
        .map_err(|error| ToolError::InvalidParams(format!("Invalid arguments: {error}")))
}

/// Draft 2020-12 with every reference inlined, because not every client follows `$ref`.
fn input_schema<A: JsonSchema>() -> Value {
    let generator = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let mut schema = generator.into_root_schema_for::<A>().to_value();
    if let Value::Object(fields) = &mut schema {
        fields.remove("$schema");
        fields.remove("title");
    }
    schema
}

static TOOLS: LazyLock<Vec<Tool>> = LazyLock::new(|| {
    let mut tools = window_tools();
    tools.extend(documents::tools());
    tools.extend(overlay::tools());
    tools.extend(timeline::tools());
    tools.extend(render::tools());
    tools.extend(video_sync::tools());
    tools
});

/// The tools, in `tools/list` order.
pub(crate) fn tools() -> &'static [Tool] {
    &TOOLS
}

pub(crate) fn find(name: &str) -> Option<&'static Tool> {
    tools().iter().find(|tool| tool.name == name)
}

/// Runs a tool against the store, as one dispatch on the UI thread.
///
/// Every call fails while the user has not allowed agent edits. A tool that answers in its
/// dispatch drops `reply`, so the caller reads that only after [`Answer::Later`].
pub(crate) fn call(
    name: &str,
    arguments: Value,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Answer {
    if !store.agent_session.allow_agent_edits {
        return Answer::Now(Err(ToolError::Failed(EDITS_OFF.into())));
    }
    match find(name) {
        Some(tool) => (tool.handler)(arguments, store, services, reply),
        None => Answer::Now(Err(ToolError::InvalidParams(format!("Unknown tool: {name}")))),
    }
}

/// The language to show OVRLEY in.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LanguageArguments {
    /// The locale code, for example `en`, `de` or `pt-BR`.
    language: Locale,
}

/// The unit system to switch to.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct UnitSystemArguments {
    /// `metric` or `imperial`.
    unit_system: UnitSystem,
}

/// The drawer tool to select.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DrawerToolArguments {
    /// The drawer to show: `projects`, `activity`, `video`, `videoSync` or `widgets`.
    tool: LeftDrawerTool,
}

/// Whether the drawer is pinned.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DrawerPinArguments {
    /// `true` pins the drawer beside the workspace; `false` makes it a temporary overlay.
    pinned: bool,
}

fn window_tools() -> Vec<Tool> {
    vec![
        Tool::read(
            "ovrley_summary",
            "OVRLEY summary",
            "Describes the open OVRLEY window: the app version, the language, the left drawer, \
             the activity (file, format, duration, laps and which are disabled), the template \
             (source, name, save status), the unit system, the widgets (id, type, name, position, \
             selected), whether undo and redo are available, and playback (state, playhead, \
             timeline minimum and total duration), the activity trim and the video clips (id, \
             file, start, duration, missing, warning count), the project (name, path, status), \
             the scene (size, aspect ratio, frame rate, widget update rate, global defaults), the \
             render (busy, target, percent) and Sync Doctor (target video, landmark count, \
             location, searches).",
            summary,
        ),
        Tool::edit(
            "language_set",
            "Set language",
            "Changes OVRLEY's language and remembers it, as the language select in Settings does.",
            set_language,
        ),
        Tool::edit(
            "unit_system_set",
            "Set the unit system",
            "Switches OVRLEY between metric and imperial units and remembers it, as Settings > \
             Units does. The open overlay's widgets convert their units as one undo step.",
            set_unit_system,
        ),
        Tool::edit(
            "drawer_select_tool",
            "Select drawer tool",
            "Shows a drawer, as the toolbar's buttons do. Selecting the tool that is already \
             shown closes the drawer when it is not pinned.",
            select_drawer_tool,
        ),
        Tool::edit(
            "drawer_set_pinned",
            "Pin or unpin the drawer",
            "Pins the drawer beside the workspace, or unpins it so it shows as a temporary \
             overlay. The drawer stays visible.",
            pin_drawer,
        ),
        Tool::edit(
            "drawer_dismiss",
            "Dismiss the drawer",
            "Hides the drawer while it is a temporary overlay. A pinned drawer stays.",
            dismiss_drawer,
        ),
        Tool::edit(
            "history_undo",
            "Undo",
            "Undoes the last edit, as Undo in the canvas toolbar does. Fails when there is \
             nothing to undo or a confirmation dialog is open.",
            undo_edit,
        ),
        Tool::edit(
            "history_redo",
            "Redo",
            "Redoes the last undone edit, as Redo in the canvas toolbar does. Fails when there is \
             nothing to redo or a confirmation dialog is open.",
            redo_edit,
        ),
    ]
}

fn summary(_: NoArguments, store: &Store, _: &Services) -> ToolResult {
    Ok(json!({
        "appVersion": ovrley_app::lib::utils::app_version(),
        "language": store.i18n.language,
        "unitSystem": store.editor.unit_system,
        "drawer": describe_drawer(store),
        "project": ovrley_app::features::projects::describe_project(store),
        "template": describe_template(store),
        "activity": describe_activity(store),
        "videos": timeline::describe_videos(store),
        "playback": timeline::describe_playback(store),
        "activityTrim": timeline::describe_activity_trim(store),
        "scene": overlay::describe_scene(store),
        "widgets": describe_widgets(store),
        "history": describe_history(store),
        "render": ovrley_app::features::render_video::agent::describe_render(store),
        "videoSync": video_sync::summary(store),
    }))
}

fn describe_drawer(store: &Store) -> Value {
    json!({
        "visible": store.layout.left_drawer_visible,
        "pinned": store.layout.left_drawer_pinned,
        "activeTool": store.layout.active_left_drawer_tool,
    })
}

fn describe_history(store: &Store) -> Value {
    json!({ "canUndo": store.history.can_undo(), "canRedo": store.history.can_redo() })
}

fn set_language(args: LanguageArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    change_language_preference(store, services, args.language);
    Ok(json!({ "language": store.i18n.language }))
}

fn set_unit_system(
    args: UnitSystemArguments,
    store: &mut Store,
    services: &mut Services,
) -> ToolResult {
    change_unit_system_preference(store, services, args.unit_system);
    Ok(json!({ "unitSystem": store.editor.unit_system, "widgets": describe_widgets(store) }))
}

fn select_drawer_tool(
    args: DrawerToolArguments,
    store: &mut Store,
    _: &mut Services,
) -> ToolResult {
    store.layout.select_left_drawer_tool(args.tool);
    Ok(json!({ "drawer": describe_drawer(store) }))
}

fn pin_drawer(args: DrawerPinArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    store.layout.set_left_drawer_pinned(args.pinned);
    Ok(json!({ "drawer": describe_drawer(store) }))
}

fn dismiss_drawer(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    store.layout.dismiss_left_drawer_overlay();
    Ok(json!({ "drawer": describe_drawer(store) }))
}

fn step_history(store: &mut Store, step: fn(&mut Store) -> bool, verb: &str) -> ToolResult {
    if !history_enabled(store) {
        return Err(ToolError::Failed(
            "Undo and redo are unavailable while a confirmation dialog is open in OVRLEY.".into(),
        ));
    }
    if !step(store) {
        return Err(ToolError::Failed(format!("There is nothing to {verb}.")));
    }
    Ok(json!({ "history": describe_history(store) }))
}

fn undo_edit(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    step_history(store, undo, "undo")
}

fn redo_edit(_: NoArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    step_history(store, redo, "redo")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Editor;
    use ovrley_app::features::undo_redo::HistoryMode;
    use ovrley_widgets::model::document::OverlayConfig;

    #[test]
    fn history_tools_undo_and_redo_and_fail_when_empty() {
        let editor = Editor::new();
        editor.dispatcher.dispatch(|store, _| {
            store.history.set_mode(HistoryMode::Reset);
            let document = json!({
                "scene": {"width": 1920, "height": 1080},
                "labels": [{"id": "a", "text": "A", "x": 1, "y": 2}],
            });
            store.editor.set_config(OverlayConfig::from_value(&document).unwrap());
        });
        let x = || {
            editor
                .dispatcher
                .with_store(|store| store.editor.config.widget("a").unwrap()["x"].clone())
        };
        editor.failed("history_undo", json!({}));
        editor.ok("widget_move", json!({"widgetId": "a", "x": 40, "y": 50}));
        assert_eq!(
            editor.ok("history_undo", json!({})),
            json!({"history": {"canUndo": false, "canRedo": true}})
        );
        assert_eq!(x(), json!(1));
        assert_eq!(
            editor.ok("history_redo", json!({})),
            json!({"history": {"canUndo": true, "canRedo": false}})
        );
        assert_eq!(x(), json!(40));
        editor.failed("history_redo", json!({}));
        assert_eq!(editor.undo_steps(), 1, "undo and redo are not steps themselves");
    }

    #[test]
    fn the_summary_describes_the_window() {
        let summary = Editor::new().ok("ovrley_summary", Value::Null);
        assert_eq!(summary["language"], "en");
        assert_eq!(
            summary["drawer"],
            json!({"visible": false, "pinned": false, "activeTool": "widgets"})
        );
        assert!(summary["widgets"].is_array());
        assert_eq!(summary["project"], json!({"name": null, "path": null, "status": "Unsaved"}));
        assert!(summary["activity"].is_null());
        assert_eq!(summary["template"]["saveStatus"], "Draft");
        assert_eq!(summary["history"], json!({"canUndo": false, "canRedo": false}));
    }

    #[test]
    fn preference_tools_change_and_persist() {
        let editor = Editor::new();
        let saved = || editor.fakes.preferences.saves.borrow().last().unwrap().clone();
        assert_eq!(editor.ok("language_set", json!({"language": "de"})), json!({"language": "de"}));
        assert_eq!(editor.dispatcher.with_store(|store| store.i18n.language), Locale::De);
        assert_eq!(saved()["lng"], "de");
        let units = editor.ok("unit_system_set", json!({"unitSystem": "imperial"}));
        assert_eq!(units["unitSystem"], "imperial");
        assert_eq!(saved()["unitSystem"], "imperial");
        editor.invalid("unit_system_set", json!({"unitSystem": "nautical"}));
    }

    #[test]
    fn drawer_tools_follow_the_drawer_rules() {
        let editor = Editor::new();
        let drawer = |visible: bool, pinned: bool| {
            json!({"drawer": {"visible": visible, "pinned": pinned, "activeTool": "video"}})
        };
        for (name, arguments, expected, why) in [
            ("drawer_select_tool", json!({"tool": "video"}), drawer(true, false), "it shows"),
            ("drawer_select_tool", json!({"tool": "video"}), drawer(false, false), "again hides it"),
            ("drawer_set_pinned", json!({"pinned": true}), drawer(true, true), "pinning shows it"),
            ("drawer_dismiss", json!({}), drawer(true, true), "a pinned drawer stays"),
            ("drawer_set_pinned", json!({"pinned": false}), drawer(true, false), "still visible"),
            ("drawer_dismiss", Value::Null, drawer(false, false), "arguments may be omitted"),
        ] {
            assert_eq!(editor.ok(name, arguments), expected, "{name}: {why}");
        }
    }

    #[test]
    fn invalid_arguments_are_invalid_params() {
        let editor = Editor::new();
        for (name, arguments) in [
            ("language_set", json!({"language": "xx"})),
            ("language_set", json!({})),
            ("drawer_select_tool", json!({"tool": "timeline"})),
            ("drawer_set_pinned", json!({"pinned": "yes"})),
            ("drawer_dismiss", json!({"extra": 1})),
        ] {
            editor.invalid(name, arguments);
        }
    }

    #[test]
    fn every_call_fails_while_edits_are_off() {
        let editor = Editor::new();
        editor.dispatcher.dispatch(|store, _| store.agent_session.allow_agent_edits = false);
        assert_eq!(editor.failed("ovrley_summary", Value::Null), EDITS_OFF);
        assert_eq!(editor.failed("drawer_select_tool", json!({"tool": "video"})), EDITS_OFF);
        assert!(!editor.dispatcher.with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn names_are_unique_and_schemas_are_objects_without_references() {
        let mut names = std::collections::HashSet::new();
        for tool in tools() {
            assert!(names.insert(tool.name), "{} is listed twice", tool.name);
            let text = tool.input_schema.to_string();
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            assert!(!text.contains("$ref") && !text.contains("$defs"), "{}: {text}", tool.name);
        }
        let language = &find("language_set").unwrap().input_schema["properties"]["language"];
        assert!(language["description"].as_str().unwrap().contains("locale code"));
        assert!(language.to_string().contains("pt-BR"));
    }
}
