//! The tools of the overlay being edited: its widgets and its scene settings.

use crate::catalog::{Answer, NoArguments, Tool, ToolError, ToolResult};
use ovrley_app::features::canvas::interaction::{self, ResizeTarget};
use ovrley_app::features::scene;
use ovrley_app::features::widgets::drawer::drawer_view;
use ovrley_app::features::widgets::{self, rules::Input};
use ovrley_app::features::widgets::preview::present_pending_frame;
use ovrley_app::features::widgets::FontOption;
use ovrley_app::fps::update_rate_options;
use ovrley_app::store::agent::{describe_widget, describe_widgets, AbsolutePath, AgentReply};
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use ovrley_widgets::model::document::{number, Collection, WidgetSelection};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::num::NonZeroU32;

/// The widget to read or delete.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WidgetArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    widget_id: String,
}

/// Where to move a widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MoveArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    widget_id: String,
    /// The widget's x in scene pixels, as the Position section shows it.
    x: f64,
    /// The widget's y in scene pixels.
    y: f64,
}

/// How to resize a widget: `width` and `height`, or `factor`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ResizeArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    widget_id: String,
    /// The content width in scene pixels; with `height`.
    width: Option<f64>,
    /// The content height in scene pixels; with `width`.
    height: Option<f64>,
    /// A scale factor of the current size, instead of `width` and `height`.
    factor: Option<f64>,
}

/// The rotation to set.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RotateArguments {
    /// A course or image widget's id.
    widget_id: String,
    /// The rotation in degrees; it is rounded to a whole degree.
    rotation: f64,
}

/// A field patch for a widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct UpdateArguments {
    /// A widget id from `ovrley_summary`'s widgets.
    widget_id: String,
    /// The fields to change and their new values, for example `{"decimals": 1, "show_units": false}`. `widget_get` lists the
    /// widget's fields and current values. A display variant's fields go under `display_variants.<display type>`.
    fields: Map<String, Value>,
}

/// An image for a raster widget.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImageArguments {
    /// A raster (image) widget's id from `ovrley_summary`'s widgets.
    widget_id: String,
    /// The image's absolute path: PNG, JPEG, BMP or TIFF.
    path: AbsolutePath,
}

/// One global default.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GlobalDefaultArguments {
    /// A key of `ovrley_summary`'s scene.globalDefaults, such as `color_values`, `font_text`,
    /// `opacity` (0–1) or `scale` (0.5–2).
    key: String,
    /// Its new value, of the type it has now.
    value: Value,
}

/// An aspect ratio.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AspectRatioArguments {
    /// `16:9`, `9:16`, `1:1`, `4:3`, `21:9` or `custom`. A preset sets its first resolution; custom
    /// keeps the size.
    ratio: String,
}

/// The scene size.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SceneSizeArguments {
    /// Whole pixels, up to 8200 (8K).
    #[schemars(range(max = scene::MAX_SCENE_SIDE))]
    width: NonZeroU32,
    /// Whole pixels, up to 8200 (8K).
    #[schemars(range(max = scene::MAX_SCENE_SIDE))]
    height: NonZeroU32,
}

/// The render frame rate.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FpsArguments {
    /// Whole frames per second.
    fps: NonZeroU32,
}

/// The widget update rate.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateRateArguments {
    /// Widgets update every `rate` frames: one of `ovrley_summary`'s scene.updateRateOptions.
    rate: u32,
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool::read(
            "widget_get",
            "Read a widget",
            "Describes one widget: its saved data, its content rectangle on the canvas in scene \
             pixels, the fields its type takes, and the fields its editor offers (editorFields: \
             id, kind, label, range or options, disabled).",
            get_widget,
        ),
        Tool::edit(
            "widget_add",
            "Add a widget",
            "Adds a widget as the Widgets drawer does, and selects it: give its type and, where \
             the drawer offers them, a displayType, a mode or a preset. The new widget starts in \
             the chosen unit system. Unknown combinations fail and list the options. Replies with \
             the new widget. One undo step.",
            add_widget,
        ),
        Tool::edit(
            "widget_update",
            "Change widget settings",
            "Changes a widget's settings, as its editor in the Widgets tab does: give the fields \
             to change. The widget's fields and current values come from widget_get. A field the \
             editor changes with a rule (a bar fill style suggesting bars, a size clamping the \
             corner radius, a map shape, …) goes through the same rule. A patch with an unknown \
             field or a value the widget cannot show changes nothing and fails with the reason. \
             One undo step.",
            update_widget,
        ),
        Tool::edit(
            "widget_move",
            "Move a widget",
            "Moves a widget to an absolute position in scene pixels, as the Position section \
             does. One undo step.",
            move_widget,
        ),
        Tool::edit(
            "widget_resize",
            "Resize a widget",
            "Resizes a widget as dragging its bottom-right handle does, keeping its top-left \
             corner: give width and height in scene pixels, or a factor. Widgets that keep their \
             proportions use the width. One undo step.",
            resize_widget,
        ),
        Tool::edit(
            "widget_rotate",
            "Rotate a widget",
            "Rotates a course or image widget about its centre, as its rotation handle does. \
             Other widgets do not rotate and the call fails. One undo step.",
            rotate_widget,
        ),
        Tool::edit(
            "widget_delete",
            "Delete a widget",
            "Deletes a widget, as the Delete key and the sidebar's delete button do. One undo \
             step.",
            delete_widget,
        ),
        Tool::waiting(
            "widget_raster_image_set",
            "Set an image",
            "Shows an image file in a raster (image) widget, as its folder button does: the image \
             loads and the widget takes its size. A file OVRLEY cannot use fails with the reason. \
             One undo step.",
            set_image,
        ),
        Tool::read(
            "font_list",
            "List fonts",
            "Lists the fonts the font selects offer: the recommended (bundled) fonts and the \
             system fonts, each with the id a widget's font field takes and its name.",
            list_fonts,
        ),
        Tool::read(
            "map_style_list",
            "List map styles",
            "Lists the installed map styles a map widget's Map Style select offers, each with the \
             id its map_style field takes and its name. A map naming a style that is not \
             installed draws the first.",
            list_map_styles,
        ),
        Tool::waiting(
            "global_default_set",
            "Set a global default",
            "Sets one global default, as Settings > Global Settings does, and applies it to the \
             widgets that follow it. font_text waits until the font is read; scale moves every \
             widget about their bounds. One undo step.",
            set_global_default,
        ),
        Tool::waiting(
            "global_defaults_reset",
            "Reset the global defaults",
            "Resets every global default and applies them to the widgets, as the Global Settings \
             reset button does. One undo step.",
            reset_global_defaults,
        ),
        Tool::edit(
            "aspect_ratio_set",
            "Set the aspect ratio",
            "Sets the overlay's aspect ratio: a preset also sets its first resolution; custom \
             keeps the size. One undo step.",
            set_aspect_ratio,
        ),
        Tool::edit(
            "scene_size_set",
            "Set the scene size",
            "Sets the overlay's width and height in pixels, up to 8200 each (8K). One undo step.",
            set_scene_size,
        ),
        Tool::edit(
            "fps_set",
            "Set the frame rate",
            "Sets the render frame rate; the widget update rate falls back to full rate when the \
             new rate does not offer it. Fails while a video sets the rate. One undo step.",
            set_fps,
        ),
        Tool::edit(
            "widget_update_rate_set",
            "Set the widget update rate",
            "Sets how often widgets update, in frames: one of the rates the current frame rate \
             offers. One undo step.",
            set_update_rate,
        ),
    ]
}

fn get_widget(args: WidgetArguments, store: &Store, _: &Services) -> ToolResult {
    let mut described = describe_widget(store, &args.widget_id)?;
    let fields = widgets::editable_fields(store, &args.widget_id)?;
    described["editorFields"] = Value::Array(fields);
    Ok(described)
}

fn add_widget(
    selection: WidgetSelection,
    store: &mut Store,
    services: &mut Services,
) -> ToolResult {
    let kind = &selection.kind;
    // A type without a button of its own is offered by its group's.
    let offered: Vec<WidgetSelection> = drawer_view(store)
        .into_iter()
        .flat_map(|group| group.items)
        .flat_map(|item| item.options)
        .map(|option| option.selection)
        .filter(|option| option.kind == *kind)
        .collect();
    if offered.is_empty() {
        return Err(ToolError::Failed(format!(
            "The drawer offers no widget of type {kind}."
        )));
    }
    if !offered.contains(&selection) {
        let options = json!(offered);
        return Err(ToolError::Failed(format!(
            "The drawer offers {kind} as: {options}."
        )));
    }
    let id = widgets::add_widget(store, services, &selection)?;
    present_pending_frame(store, services);
    Ok(describe_widget(store, &id)?)
}

fn update_widget(args: UpdateArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    widgets::update_widget_fields(store, services, &args.widget_id, args.fields)?;
    Ok(describe_widget(store, &args.widget_id)?)
}

fn move_widget(args: MoveArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    let patch = Map::from_iter([
        ("x".to_string(), number(args.x)),
        ("y".to_string(), number(args.y)),
    ]);
    widgets::update_widget_data(store, &args.widget_id, patch)?;
    present_pending_frame(store, services);
    Ok(describe_widget(store, &args.widget_id)?)
}

fn resize_widget(args: ResizeArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    let target = match (args.width, args.height, args.factor) {
        (Some(width), Some(height), None) => ResizeTarget::Size { width, height },
        (None, None, Some(factor)) => ResizeTarget::Factor(factor),
        _ => {
            return Err(ToolError::InvalidParams(
                "Give width and height, or factor.".into(),
            ))
        }
    };
    interaction::resize_widget(store, &args.widget_id, target)?;
    present_pending_frame(store, services);
    Ok(describe_widget(store, &args.widget_id)?)
}

fn rotate_widget(args: RotateArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    interaction::rotate_widget(store, &args.widget_id, args.rotation)?;
    present_pending_frame(store, services);
    Ok(describe_widget(store, &args.widget_id)?)
}

fn delete_widget(args: WidgetArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    widgets::delete_widget(store, &args.widget_id)?;
    Ok(json!({ "widgets": describe_widgets(store) }))
}

fn set_image(
    args: ImageArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let (collection, _) = store.editor.config.find_widget(&args.widget_id)?;
    if collection != Collection::Rasters {
        return Err(ToolError::Failed(format!(
            "{} is not an image widget.",
            args.widget_id
        )));
    }
    let widget_id = args.widget_id.clone();
    let done = Box::new(move |store: &mut Store, result: Result<(), String>| {
        reply.send(result.and_then(|()| describe_widget(store, &widget_id)));
    });
    let path = args.path.into();
    widgets::raster::load_raster_image(store, services, &args.widget_id, path, Some(done));
    Ok(Answer::Later)
}

fn list_fonts(_: NoArguments, store: &Store, _: &Services) -> ToolResult {
    let list = |fonts: &[FontOption]| -> Vec<Value> {
        fonts
            .iter()
            .map(|font| json!({ "id": font.id, "name": font.name }))
            .collect()
    };
    let fonts = &store.widgets.fonts;
    Ok(json!({ "recommended": list(&fonts.recommended), "system": list(&fonts.system) }))
}

fn list_map_styles(_: NoArguments, _: &Store, services: &Services) -> ToolResult {
    let styles: Vec<Value> = services
        .maps
        .styles()
        .styles()
        .iter()
        .map(|style| json!({ "id": style.id.as_str(), "name": style.label }))
        .collect();
    Ok(json!({ "styles": styles }))
}

/// The scene settings, for `ovrley_summary` and these tools' replies.
pub(crate) fn describe_scene(store: &Store) -> Value {
    let scene = &store.editor.config.scene;
    json!({
        "aspectRatio": store.template.aspect_ratio,
        "width": scene.get("width"),
        "height": scene.get("height"),
        "fps": store.render_settings.fps,
        "lockedFps": scene::locked_fps(store),
        "widgetUpdateRate": store.render_settings.widget_update_rate,
        "updateRateOptions": update_rate_options(store.render_settings.fps),
        "globalDefaults": *store.template.global_defaults,
    })
}

/// Answers with the scene once an edit that may wait for a font has ended.
fn scene_reply(reply: AgentReply) -> scene::FontEditReply {
    Box::new(move |store, result| reply.send(result.map(|()| describe_scene(store))))
}

fn set_global_default(
    args: GlobalDefaultArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    scene::validate_global_default(&args.key, &args.value)
        .map_err(ToolError::InvalidParams)?;
    let done = Some(scene_reply(reply));
    // The result also goes to the reply, now or when the font has been read.
    let _ = scene::change_global_default(store, services, &args.key, args.value, done);
    Ok(Answer::Later)
}

fn reset_global_defaults(
    _: NoArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    scene::reset_globals(store, services, Some(scene_reply(reply)));
    Ok(Answer::Later)
}

fn set_aspect_ratio(
    args: AspectRatioArguments,
    store: &mut Store,
    services: &mut Services,
) -> ToolResult {
    let ratio = Input::Text(args.ratio);
    // The edit's only refusal is a ratio it does not know.
    scene::edit(store, services, "aspect_ratio", ratio, ovrley_app::features::widgets::Edit::Commit)
        .map_err(ToolError::InvalidParams)?;
    Ok(describe_scene(store))
}

fn set_scene_size(args: SceneSizeArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    scene::set_scene_size(store, args.width.get(), args.height.get())
        .map_err(ToolError::InvalidParams)?;
    Ok(describe_scene(store))
}

fn set_fps(args: FpsArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    if let Some(fps) = scene::locked_fps(store) {
        return Err(ToolError::Failed(format!(
            "A video sets the frame rate to {fps} fps"
        )));
    }
    scene::set_fps(store, args.fps.get())?;
    Ok(describe_scene(store))
}

fn set_update_rate(args: UpdateRateArguments, store: &mut Store, _: &mut Services) -> ToolResult {
    let fps = store.render_settings.fps;
    let options = update_rate_options(fps);
    if !options.contains(&args.rate) {
        return Err(ToolError::InvalidParams(format!(
            "At {fps} fps the update rate is one of {options:?}"
        )));
    }
    store.render_settings.set_widget_update_rate(args.rate).map_err(|error| error.to_string())?;
    Ok(describe_scene(store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{with_videos, Editor};
    use ovrley_app::features::history::HistoryMode;
    use ovrley_widgets::model::document::OverlayConfig;

    fn editor_with(collections: Value) -> Editor {
        let editor = Editor::new();
        editor.dispatcher.dispatch(move |store, _| {
            let mut document = collections;
            document["scene"] = json!({"width": 1920, "height": 1080});
            store.history.set_mode(HistoryMode::Reset);
            store
                .editor
                .set_config(OverlayConfig::from_value(&document).unwrap());
        });
        editor
    }

    #[test]
    fn widget_tools_move_rotate_and_delete_as_one_undo_step_each() {
        let editor = editor_with(json!({"labels": [{"id": "a", "text": "A", "x": 1, "y": 2}]}));
        let moved = editor.ok("widget_move", json!({"widgetId": "a", "x": 40, "y": 50}));
        assert_eq!(
            (&moved["data"]["x"], &moved["data"]["y"]),
            (&json!(40), &json!(50))
        );
        assert_eq!(editor.undo_steps(), 1);
        // Labels do not rotate.
        editor.failed("widget_rotate", json!({"widgetId": "a", "rotation": 10}));
        editor.failed("widget_move", json!({"widgetId": "nope", "x": 0, "y": 0}));
        editor.invalid("widget_resize", json!({"widgetId": "a", "width": 10}));
        assert_eq!(
            editor.ok("widget_delete", json!({"widgetId": "a"}))["widgets"],
            json!([])
        );
        assert_eq!(editor.undo_steps(), 2);
    }

    #[test]
    fn widget_update_patches_fields_as_one_undo_step_and_refuses_unknown_ones() {
        let editor = editor_with(json!({"values": [{
            "id": "s", "value": "speed", "display_type": "text", "display_unit": "kmh",
            "decimals": 0, "x": 10, "y": 10,
        }]}));
        let fields = json!({"decimals": 1, "show_units": false});
        let updated = editor.ok("widget_update", json!({"widgetId": "s", "fields": fields}));
        assert_eq!(
            (&updated["data"]["decimals"], &updated["data"]["show_units"]),
            (&json!(1), &json!(false))
        );
        assert_eq!(editor.undo_steps(), 1);
        let reason = editor.failed(
            "widget_update",
            json!({"widgetId": "s", "fields": {"radius": 3}}),
        );
        assert!(reason.contains("no field radius"), "{reason}");
        editor.invalid("widget_update", json!({"widgetId": "s"}));
        assert_eq!(editor.undo_steps(), 1, "refused patches leave no undo step");

        editor.ok("unit_system_set", json!({"unitSystem": "imperial"}));
        editor.dispatcher.with_store(|store| {
            assert_eq!(
                store.editor.config.widget("s").unwrap()["display_unit"],
                "mph"
            );
        });
    }

    #[test]
    fn widget_add_adds_a_drawer_selection_as_one_undo_step() {
        let editor = Editor::new();
        let added = editor.ok(
            "widget_add",
            json!({"type": "speed", "displayType": "linear"}),
        );
        let id = added["id"].as_str().unwrap();
        editor.dispatcher.with_store(|store| {
            assert_eq!(store.editor.selected_widget_id.as_deref(), Some(id));
            assert_eq!(
                store.editor.config.widget(id).unwrap()["display_type"],
                "linear"
            );
        });
        assert_eq!(editor.undo_steps(), 1);
        let reason = editor.failed(
            "widget_add",
            json!({"type": "speed", "displayType": "nonsense"}),
        );
        assert!(reason.contains("\"displayType\":\"text\""), "{reason}");
        editor.failed("widget_add", json!({"type": "nonsense"}));
    }

    #[test]
    fn widget_update_applies_the_editors_rules_and_widget_get_lists_its_fields() {
        let editor = Editor::new();
        let added = editor.ok(
            "widget_add",
            json!({"type": "speed", "displayType": "linear"}),
        );
        let id = &added["id"];
        // A ruled field: switching to bars suggests the bar geometry, as the inspector does.
        let bars = json!({"display_variants": {"linear": {"track_fill_style": "bars"}}});
        let updated = editor.ok("widget_update", json!({"widgetId": id, "fields": bars}));
        let linear = &updated["data"]["display_variants"]["linear"];
        assert_eq!(linear["track_fill_style"], "bars");
        assert!(
            linear["bar_count"].as_f64().unwrap() >= 3.0,
            "bars are suggested: {linear}"
        );
        // A plain field patches as it is.
        let updated = editor.ok(
            "widget_update",
            json!({"widgetId": id, "fields": {"decimals": 1}}),
        );
        assert_eq!(updated["data"]["decimals"], 1);
        assert_eq!(editor.undo_steps(), 3, "add and two updates");

        let described = editor.ok("widget_get", json!({"widgetId": id}));
        let fields = described["editorFields"].as_array().unwrap();
        let field = |id: &str| {
            fields
                .iter()
                .find(|field| field["id"] == id)
                .expect("a listed field")
        };
        let width = field("display_variants.linear.width");
        assert_eq!(
            (&width["kind"], &width["label"]),
            (&json!("number"), &json!("Width"))
        );
        assert_eq!(
            field("opacity")["max"],
            1,
            "ranges are in the field's units"
        );
        assert_eq!(field("display_type")["kind"], "choice");
    }

    #[test]
    fn map_style_list_lists_the_installed_styles_default_first() {
        let listed = Editor::new().ok("map_style_list", json!({}));
        let styles = listed["styles"].as_array().unwrap();
        let ids: Vec<&str> = styles
            .iter()
            .map(|style| style["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["liberty", "bright", "dark", "fiord", "positron"]);
        assert!(styles[0]["name"].is_string());
    }

    #[test]
    fn scene_tools_set_the_overlay_as_the_settings_tab_does() {
        let editor = Editor::new();
        let scene = editor.ok("aspect_ratio_set", json!({"ratio": "1:1"}));
        assert_eq!(
            (&scene["aspectRatio"], &scene["width"], &scene["height"]),
            (&json!("1:1"), &json!(1080), &json!(1080))
        );
        editor.invalid("aspect_ratio_set", json!({"ratio": "3:2"}));
        let scene = editor.ok("scene_size_set", json!({"width": 1280, "height": 720}));
        assert_eq!(
            (&scene["width"], &scene["height"]),
            (&json!(1280), &json!(720))
        );
        editor.invalid("scene_size_set", json!({"width": scene::MAX_SCENE_SIDE + 1, "height": 720}));
        let scene = editor.ok("fps_set", json!({"fps": 60}));
        assert_eq!(scene["updateRateOptions"], json!([1, 2, 6, 10]));
        let scene = editor.ok("widget_update_rate_set", json!({"rate": 6}));
        assert_eq!(scene["widgetUpdateRate"], json!(6));
        editor.invalid("widget_update_rate_set", json!({"rate": 4}));
        let scene = editor.ok("fps_set", json!({"fps": 24}));
        assert_eq!(scene["widgetUpdateRate"], json!(1), "24 fps offers no 1/6");
        assert_eq!(editor.undo_steps(), 5);
        let summary = editor.ok("ovrley_summary", json!({}));
        assert_eq!(summary["scene"]["fps"], json!(24.0));
        assert!(summary["scene"]["globalDefaults"]["scale"].is_number());
    }

    #[test]
    fn global_default_set_applies_as_one_undo_step_and_refuses_bad_input() {
        let editor = Editor::new();
        let scene = editor.ok(
            "global_default_set",
            json!({"key": "color_values", "value": "#123456"}),
        );
        assert_eq!(scene["globalDefaults"]["color_values"], "#123456");
        for (key, value) in [
            ("nonsense", json!(1)),
            ("scale", json!(0)),
            ("opacity", json!("half")),
            ("font_text", json!(5)),
            ("color_values", json!("not a colour")),
        ] {
            editor.invalid("global_default_set", json!({"key": key, "value": value}));
        }
        assert_eq!(editor.undo_steps(), 1);
    }

    #[test]
    fn a_video_locks_the_frame_rate() {
        let editor = Editor::with(with_videos(&[("a", 0.0, 10.0)]), json!({}));
        let reason = editor.failed("fps_set", json!({"fps": 60}));
        assert!(reason.contains("A video sets the frame rate"), "{reason}");
        let summary = editor.ok("ovrley_summary", json!({}));
        assert!(summary["scene"]["lockedFps"].is_number());
    }
}
