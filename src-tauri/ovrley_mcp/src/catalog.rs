//! The MCP tools (D13): their catalog, written by hand, and how each runs against the store.
//!
//! A tool runs on the window's UI thread as one dispatch, so the projections update the window. Its
//! input is always an object; each tool's argument struct derives its schema, and its doc comments
//! become the descriptions. What the agent reads (names, descriptions, errors) is English and is not
//! in the locale files.

use ovrley_app::i18n::{change_language_preference, Locale};
use ovrley_app::store::services::Services;
use ovrley_app::store::slices::layout::LeftDrawerTool;
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
    vec![
        ToolDefinition {
            name: "ovrley_summary",
            title: "OVRLEY summary",
            description: "Describes the open OVRLEY window: the app version, the language and the left drawer. \
                          The project, template, activity, video and widgets sections are empty until those features arrive.",
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
    ]
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
        "drawer": drawer_state(store),
        "project": null,
        "template": null,
        "activity": null,
        "video": null,
        "widgets": [],
    })
}

/// Runs a tool against the store, as one dispatch on the UI thread. Every call fails while the user
/// has not allowed agent edits.
pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services) -> ToolOutcome {
    if !store.agent_session.allow_agent_edits {
        return ToolOutcome::Failed(EDITS_OFF.into());
    }
    let run = || -> Result<Value, ToolOutcome> {
        match name {
            "ovrley_summary" => {
                arguments::<NoArguments>(arguments_value)?;
                Ok(summary(store))
            }
            "language_set" => {
                let arguments: LanguageSetArguments = arguments(arguments_value)?;
                change_language_preference(store, services, arguments.language);
                Ok(json!({ "language": store.i18n.language }))
            }
            "drawer_select_tool" => {
                let arguments: DrawerSelectToolArguments = arguments(arguments_value)?;
                store.layout.select_left_drawer_tool(arguments.tool);
                Ok(json!({ "drawer": drawer_state(store) }))
            }
            "drawer_set_pinned" => {
                let arguments: DrawerSetPinnedArguments = arguments(arguments_value)?;
                store.layout.set_left_drawer_pinned(arguments.pinned);
                Ok(json!({ "drawer": drawer_state(store) }))
            }
            "drawer_dismiss" => {
                arguments::<NoArguments>(arguments_value)?;
                store.layout.dismiss_left_drawer_overlay();
                Ok(json!({ "drawer": drawer_state(store) }))
            }
            other => Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}"))),
        }
    };
    match run() {
        Ok(value) => ToolOutcome::Ok(value),
        Err(outcome) => outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_app::store::services::fakes::fake_services;
    use ovrley_app::store::Dispatcher;

    fn dispatcher(allowed: bool) -> (Dispatcher, ovrley_app::store::services::fakes::Fakes) {
        let (parts, fakes) = fake_services(json!({ "allowAgentEdits": allowed }));
        let store = Store::hydrate(&parts.preferences).unwrap();
        (Dispatcher::new(store, parts), fakes)
    }

    fn run(dispatcher: &Dispatcher, name: &'static str, arguments: Value) -> ToolOutcome {
        dispatcher.dispatch_with_result(move |store, services| call(name, arguments, store, services)).unwrap()
    }

    #[test]
    fn the_summary_describes_the_window() {
        let (dispatcher, _) = dispatcher(true);
        let ToolOutcome::Ok(summary) = run(&dispatcher, "ovrley_summary", Value::Null) else { panic!("summary failed") };
        assert_eq!(summary["language"], "en");
        assert_eq!(summary["drawer"], json!({"visible": false, "pinned": false, "activeTool": "widgets"}));
        assert_eq!(summary["widgets"], json!([]));
        assert!(summary["project"].is_null());
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
