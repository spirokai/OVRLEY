//! The widget tools of phase 5: adding widgets as the drawer does. Split from `catalog.rs` as
//! `player.rs` was; the catalog lists these after its own and hands them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::widget_drawer::drawer_view;
use ovrley_app::features::widget_editor::add_widget;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use ovrley_widgets::model::document::WidgetSelection;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub fn tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "widget_add",
        title: "Add a widget",
        description: "Adds a widget as the Widgets drawer does, and selects it: give its type and, where the drawer offers them, a \
                      displayType, a mode or a preset. The new widget starts in the chosen unit system. Unknown combinations fail and \
                      list the options. Replies with the new widget. One undo step.",
        input_schema: input_schema::<WidgetSelection>(),
        read_only: false,
    }]
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

/// A drawer selection in the arguments' shape, for the error's list of options.
fn describe_selection(selection: &WidgetSelection) -> Value {
    let mut value = json!({ "type": selection.kind });
    if let Some(display_type) = &selection.display_type {
        value["displayType"] = json!(display_type);
    }
    if let Some(mode) = &selection.mode {
        value["mode"] = json!(mode);
    }
    if !selection.preset.is_empty() {
        value["preset"] = Value::Object(selection.preset.clone());
    }
    value
}

/// The drawer's selections for `kind`; a type without its own button is offered by its group's.
fn offered(store: &Store, kind: &str) -> Vec<WidgetSelection> {
    drawer_view(store).into_iter().flat_map(|group| group.items).flat_map(|item| item.options).map(|option| option.selection).filter(|selection| selection.kind == kind).collect()
}

/// Whether `name` is one of these tools.
pub fn handles(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services, _reply: Reply) -> Result<Answer, ToolOutcome> {
    match name {
        "widget_add" => arguments::<WidgetSelection>(arguments_value).and_then(|selection| {
            let options = offered(store, &selection.kind);
            if options.is_empty() {
                return Err(ToolOutcome::Failed(format!("The drawer offers no widget of type {}.", selection.kind)));
            }
            if !options.contains(&selection) {
                let list: Vec<Value> = options.iter().map(describe_selection).collect();
                return Err(ToolOutcome::Failed(format!("The drawer offers {} as: {}.", selection.kind, Value::Array(list))));
            }
            let id = add_widget(store, &selection).map_err(ToolOutcome::Failed)?;
            ovrley_app::features::widget_preview::present_pending_frame(store, services);
            Ok(Answer::Now(ToolOutcome::Ok(ovrley_app::store::agent::describe_widget(store, &id).map_err(ToolOutcome::Failed)?)))
        }),
        other => unreachable!("{other} is not a widget tool"),
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::ToolOutcome;
    use crate::player::tests::{dispatcher_with, run};
    use ovrley_app::store::Store;
    use serde_json::json;

    #[test]
    fn widget_add_adds_a_drawer_selection_as_one_undo_step() {
        let dispatcher = dispatcher_with(Store::default());
        let ToolOutcome::Ok(added) = run(&dispatcher, "widget_add", json!({"type": "speed", "displayType": "linear"})) else { panic!("add failed") };
        let id = added["id"].as_str().unwrap().to_string();
        dispatcher.with_store(|store| {
            assert_eq!(store.editor.selected_widget_id.as_deref(), Some(id.as_str()));
            assert_eq!(store.editor.config.widget(&id).unwrap()["display_type"], "linear");
            assert_eq!(store.history.past.len(), 1);
        });
        let ToolOutcome::Failed(message) = run(&dispatcher, "widget_add", json!({"type": "speed", "displayType": "nonsense"})) else { panic!() };
        assert!(message.contains("\"displayType\":\"text\""), "{message}");
        assert!(matches!(run(&dispatcher, "widget_add", json!({"type": "nonsense"})), ToolOutcome::Failed(_)));
    }
}
