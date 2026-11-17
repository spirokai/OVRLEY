//! The widget tools of phase 5: adding widgets as the drawer does, raster images, and the font and
//! map style lists. Split from `catalog.rs` as
//! `player.rs` was; the catalog lists these after its own and hands them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::widget_drawer::drawer_view;
use ovrley_app::features::widget_editor::add_widget;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use ovrley_widgets::model::document::WidgetSelection;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// An image for a raster widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RasterImageSetArguments {
    /// A raster (image) widget's id from `ovrley_summary`'s widgets.
    pub widget_id: String,
    /// The image's absolute path: PNG, JPEG, BMP or TIFF.
    pub path: String,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "widget_add",
        title: "Add a widget",
        description: "Adds a widget as the Widgets drawer does, and selects it: give its type and, where the drawer offers them, a \
                      displayType, a mode or a preset. The new widget starts in the chosen unit system. Unknown combinations fail and \
                      list the options. Replies with the new widget. One undo step.",
        input_schema: input_schema::<WidgetSelection>(),
        read_only: false,
    },
    ToolDefinition {
        name: "widget_raster_image_set",
        title: "Set an image",
        description: "Shows an image file in a raster (image) widget, as its folder button does: the image loads and the widget takes \
                      its size. A file OVRLEY cannot use fails with the reason. One undo step.",
        input_schema: input_schema::<RasterImageSetArguments>(),
        read_only: false,
    },
    ToolDefinition {
        name: "font_list",
        title: "List fonts",
        description: "Lists the fonts the font selects offer: the recommended (bundled) fonts and the system fonts, each with the id a \
                      widget's font field takes and its name.",
        input_schema: input_schema::<crate::catalog::NoArguments>(),
        read_only: true,
    },
    ToolDefinition {
        name: "map_style_list",
        title: "List map styles",
        description: "Lists the installed map styles a map widget's Map Style select offers, each with the id its map_style field takes \
                      and its name. A map naming a style that is not installed draws the first.",
        input_schema: input_schema::<crate::catalog::NoArguments>(),
        read_only: true,
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

pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services, reply: Reply) -> Result<Answer, ToolOutcome> {
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
            let id = add_widget(store, services, &selection).map_err(ToolOutcome::Failed)?;
            ovrley_app::features::widget_preview::present_pending_frame(store, services);
            Ok(Answer::Now(ToolOutcome::Ok(ovrley_app::store::agent::describe_widget(store, &id).map_err(ToolOutcome::Failed)?)))
        }),
        "widget_raster_image_set" => {
            let arguments: RasterImageSetArguments = arguments(arguments_value)?;
            if !std::path::Path::new(&arguments.path).is_absolute() {
                return Err(ToolOutcome::InvalidParams("path must be absolute".into()));
            }
            let (collection, _) = store.editor.config.find_widget(&arguments.widget_id).map_err(ToolOutcome::Failed)?;
            if collection != ovrley_widgets::model::document::Collection::Rasters {
                return Err(ToolOutcome::Failed(format!("{} is not an image widget.", arguments.widget_id)));
            }
            let widget_id = arguments.widget_id.clone();
            ovrley_app::features::widget_editor::raster::load_raster_image(
                store,
                services,
                &arguments.widget_id,
                arguments.path,
                Some(Box::new(move |store: &mut Store, result: Result<(), String>| {
                    reply(match result.and_then(|_| ovrley_app::store::agent::describe_widget(store, &widget_id)) {
                        Ok(value) => ToolOutcome::Ok(value),
                        Err(error) => ToolOutcome::Failed(error),
                    })
                })),
            );
            Ok(Answer::Later)
        }
        "font_list" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            let fonts = &store.widget_editor.fonts;
            let list = |fonts: &[ovrley_app::lib::fonts::FontOption]| fonts.iter().map(|font| json!({"id": font.id, "name": font.name})).collect::<Vec<_>>();
            Ok(Answer::Now(ToolOutcome::Ok(json!({"recommended": list(&fonts.recommended), "system": list(&fonts.system)}))))
        }
        "map_style_list" => {
            arguments::<crate::catalog::NoArguments>(arguments_value)?;
            let styles = services.maps.styles().styles().iter().map(|style| json!({"id": style.id.as_str(), "name": style.label})).collect::<Vec<_>>();
            Ok(Answer::Now(ToolOutcome::Ok(json!({ "styles": styles }))))
        }
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

    #[test]
    fn widget_update_applies_the_editors_rules_and_widget_get_lists_its_fields() {
        let dispatcher = dispatcher_with(Store::default());
        let ToolOutcome::Ok(added) = run(&dispatcher, "widget_add", json!({"type": "speed", "displayType": "linear"})) else { panic!("add failed") };
        let id = added["id"].as_str().unwrap().to_string();
        let leaked: &'static str = Box::leak(id.clone().into_boxed_str());
        // A ruled field: switching to bars suggests the bar geometry, as the inspector does.
        let ToolOutcome::Ok(updated) = run(&dispatcher, "widget_update", json!({"widgetId": leaked, "fields": {"display_variants": {"linear": {"track_fill_style": "bars"}}}})) else { panic!("update failed") };
        let linear = &updated["data"]["display_variants"]["linear"];
        assert_eq!(linear["track_fill_style"], "bars");
        assert!(linear["bar_count"].as_f64().unwrap() >= 3.0, "bars are suggested: {linear}");
        // A plain field patches as it is.
        let ToolOutcome::Ok(updated) = run(&dispatcher, "widget_update", json!({"widgetId": leaked, "fields": {"decimals": 1}})) else { panic!("update failed") };
        assert_eq!(updated["data"]["decimals"], 1);
        dispatcher.with_store(|store| assert_eq!(store.history.past.len(), 3, "add and two updates"));

        let ToolOutcome::Ok(described) = run(&dispatcher, "widget_get", json!({"widgetId": leaked})) else { panic!("get failed") };
        let fields = described["editorFields"].as_array().unwrap();
        let width = fields.iter().find(|field| field["id"] == "display_variants.linear.width").expect("the width slider");
        assert_eq!((width["kind"].clone(), width["label"].clone()), (json!("number"), json!("Width")));
        let opacity = fields.iter().find(|field| field["id"] == "opacity").unwrap();
        assert_eq!(opacity["max"], 1, "ranges are in the field's units");
        assert!(fields.iter().any(|field| field["id"] == "display_type" && field["kind"] == "choice"));
    }

    #[test]
    fn map_style_list_lists_the_installed_styles_default_first() {
        let dispatcher = dispatcher_with(Store::default());
        let ToolOutcome::Ok(listed) = run(&dispatcher, "map_style_list", json!({})) else { panic!("list failed") };
        let ids: Vec<&str> = listed["styles"].as_array().unwrap().iter().map(|style| style["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["liberty", "bright", "dark", "fiord", "positron"]);
        assert!(listed["styles"][0]["name"].is_string());
    }
}
