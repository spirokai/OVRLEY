//! The scene settings tools of phase 5 (D12): the global defaults, the aspect ratio, the scene
//! size, the frame rate and the widget update rate, each through the same actions as the Settings
//! tab, so each is one undo step. Split from `catalog.rs` as `widgets.rs` was.

use crate::catalog::{input_schema, Answer, NoArguments, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::scene_settings::{self as scene, ASPECT_RATIOS};
use ovrley_app::features::widget_editor::rules::Input;
use ovrley_app::lib::update_rate::update_rate_options;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// One global default.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlobalDefaultSetArguments {
    /// A key of `ovrley_summary`'s scene.globalDefaults, such as `color_values`, `font_text`,
    /// `opacity` (0–1) or `scale` (0.5–2).
    pub key: String,
    /// Its new value, of the type it has now.
    pub value: Value,
}

/// An aspect ratio.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AspectRatioSetArguments {
    /// `16:9`, `9:16`, `1:1`, `4:3`, `21:9` or `custom`. A preset sets its first resolution; custom
    /// keeps the size.
    pub ratio: String,
}

/// The scene size.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneSizeSetArguments {
    /// Whole pixels.
    pub width: u32,
    /// Whole pixels.
    pub height: u32,
}

/// The render frame rate.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FpsSetArguments {
    /// Whole frames per second.
    pub fps: u32,
}

/// The widget update rate.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WidgetUpdateRateSetArguments {
    /// Widgets update every `rate` frames: one of `ovrley_summary`'s scene.updateRateOptions.
    pub rate: u32,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "global_default_set",
            title: "Set a global default",
            description: "Sets one global default, as Settings > Global Settings does, and applies it to the widgets that follow it. \
                          font_text waits until the font is read; scale moves every widget about their bounds. One undo step.",
            input_schema: input_schema::<GlobalDefaultSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "global_defaults_reset",
            title: "Reset the global defaults",
            description: "Resets every global default and applies them to the widgets, as the Global Settings reset button does. One undo step.",
            input_schema: input_schema::<NoArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "aspect_ratio_set",
            title: "Set the aspect ratio",
            description: "Sets the overlay's aspect ratio: a preset also sets its first resolution; custom keeps the size. One undo step.",
            input_schema: input_schema::<AspectRatioSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "scene_size_set",
            title: "Set the scene size",
            description: "Sets the overlay's width and height in pixels. One undo step.",
            input_schema: input_schema::<SceneSizeSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "fps_set",
            title: "Set the frame rate",
            description: "Sets the render frame rate; the widget update rate falls back to full rate when the new rate does not offer it. \
                          Fails while a video sets the rate. One undo step.",
            input_schema: input_schema::<FpsSetArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "widget_update_rate_set",
            title: "Set the widget update rate",
            description: "Sets how often widgets update, in frames: one of the rates the current frame rate offers. One undo step.",
            input_schema: input_schema::<WidgetUpdateRateSetArguments>(),
            read_only: false,
        },
    ]
}

/// Whether `name` is one of these tools.
pub fn handles(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

/// The scene settings, for `ovrley_summary` and these tools' replies.
pub fn describe_scene(store: &Store) -> Value {
    let size = |key: &str| store.editor.config.scene.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "aspectRatio": store.template.aspect_ratio,
        "width": size("width"),
        "height": size("height"),
        "fps": store.render_settings.fps,
        "lockedFps": scene::locked_fps(store),
        "widgetUpdateRate": store.render_settings.widget_update_rate,
        "updateRateOptions": update_rate_options(store.render_settings.fps),
        "globalDefaults": *store.template.global_defaults,
    })
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value).map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

fn done(store: &Store, result: Result<(), String>) -> ToolOutcome {
    match result {
        Ok(()) => ToolOutcome::Ok(describe_scene(store)),
        Err(error) => ToolOutcome::Failed(error),
    }
}

fn now(store: &Store, result: Result<(), String>) -> Result<Answer, ToolOutcome> {
    Ok(Answer::Now(done(store, result)))
}

pub fn call(name: &str, arguments_value: Value, store: &mut Store, services: &mut Services, reply: Reply) -> Result<Answer, ToolOutcome> {
    match name {
        "global_default_set" => {
            let arguments: GlobalDefaultSetArguments = arguments(arguments_value)?;
            let keys = scene::global_default_keys();
            if !keys.contains(&arguments.key.as_str()) {
                return Err(ToolOutcome::InvalidParams(format!("Unknown global default {}; the keys are {keys:?}", arguments.key)));
            }
            let answer = Box::new(move |store: &mut Store, result: Result<(), String>| reply(done(store, result)));
            // The reply answers once the edit applied or failed, now or when the font is read.
            let _ = scene::change_global_default(store, services, &arguments.key, arguments.value, Some(answer));
            Ok(Answer::Later)
        }
        "global_defaults_reset" => {
            arguments::<NoArguments>(arguments_value)?;
            scene::reset_globals(store, services, Some(Box::new(move |store: &mut Store, result| reply(done(store, result)))));
            Ok(Answer::Later)
        }
        "aspect_ratio_set" => {
            let arguments: AspectRatioSetArguments = arguments(arguments_value)?;
            if !ASPECT_RATIOS.iter().any(|(ratio, _)| *ratio == arguments.ratio) {
                return Err(ToolOutcome::InvalidParams(format!("Unknown aspect ratio {}", arguments.ratio)));
            }
            let result = scene::edit(store, services, "aspect_ratio", Input::Text(arguments.ratio), false);
            now(store, result)
        }
        "scene_size_set" => {
            let arguments: SceneSizeSetArguments = arguments(arguments_value)?;
            if arguments.width == 0 || arguments.height == 0 {
                return Err(ToolOutcome::InvalidParams("The scene size must be positive".into()));
            }
            store.editor.edit_config(|config| {
                config.scene.insert("width".into(), Value::from(arguments.width));
                config.scene.insert("height".into(), Value::from(arguments.height));
            });
            now(store, Ok(()))
        }
        "fps_set" => {
            let arguments: FpsSetArguments = arguments(arguments_value)?;
            if let Some(fps) = scene::locked_fps(store) {
                return Err(ToolOutcome::Failed(format!("A video sets the frame rate to {fps} fps")));
            }
            if arguments.fps == 0 {
                return Err(ToolOutcome::InvalidParams("The frame rate must be positive".into()));
            }
            let result = scene::set_fps(store, arguments.fps);
            now(store, result)
        }
        "widget_update_rate_set" => {
            let arguments: WidgetUpdateRateSetArguments = arguments(arguments_value)?;
            let options = update_rate_options(store.render_settings.fps);
            if !options.contains(&arguments.rate) {
                return Err(ToolOutcome::InvalidParams(format!("At {} fps the update rate is one of {options:?}", store.render_settings.fps)));
            }
            let result = store.render_settings.set_widget_update_rate(arguments.rate);
            now(store, result)
        }
        other => unreachable!("{other} is not a scene settings tool"),
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::{Answer, ToolOutcome};
    use crate::player::tests::{dispatcher_with, ok, run, with_videos};
    use ovrley_app::store::Store;
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    /// Runs a tool that may answer later, and returns its outcome once it has.
    fn run_later(dispatcher: &ovrley_app::store::Dispatcher, name: &'static str, arguments: Value) -> ToolOutcome {
        let answer = Arc::new(Mutex::new(None));
        let slot = answer.clone();
        let now = dispatcher
            .dispatch_with_result(move |store, services| crate::catalog::call(name, arguments, store, services, Box::new(move |outcome| *slot.lock().unwrap() = Some(outcome))))
            .unwrap();
        if let Answer::Now(outcome) = now {
            return outcome;
        }
        let outcome = answer.lock().unwrap().take();
        outcome.expect("answered")
    }

    #[test]
    fn scene_tools_set_the_overlay_as_the_settings_tab_does() {
        let dispatcher = dispatcher_with(Store::default());
        let scene = ok(run(&dispatcher, "aspect_ratio_set", json!({"ratio": "1:1"})));
        assert_eq!((scene["aspectRatio"].clone(), scene["width"].clone(), scene["height"].clone()), (json!("1:1"), json!(1080), json!(1080)));
        assert!(matches!(run(&dispatcher, "aspect_ratio_set", json!({"ratio": "3:2"})), ToolOutcome::InvalidParams(_)));
        let scene = ok(run(&dispatcher, "scene_size_set", json!({"width": 1280, "height": 720})));
        assert_eq!((scene["width"].clone(), scene["height"].clone()), (json!(1280), json!(720)));
        let scene = ok(run(&dispatcher, "fps_set", json!({"fps": 60})));
        assert_eq!(scene["updateRateOptions"], json!([1, 2, 6, 10]));
        let scene = ok(run(&dispatcher, "widget_update_rate_set", json!({"rate": 6})));
        assert_eq!(scene["widgetUpdateRate"], json!(6));
        assert!(matches!(run(&dispatcher, "widget_update_rate_set", json!({"rate": 4})), ToolOutcome::InvalidParams(_)));
        let scene = ok(run(&dispatcher, "fps_set", json!({"fps": 24})));
        assert_eq!(scene["widgetUpdateRate"], json!(1), "24 fps offers no 1/6");
        dispatcher.with_store(|store| assert_eq!(store.history.past.len(), 5));
        let summary = ok(run(&dispatcher, "ovrley_summary", json!({})));
        assert_eq!(summary["scene"]["fps"], json!(24.0));
        assert!(summary["scene"]["globalDefaults"]["scale"].is_number());
    }

    #[test]
    fn global_default_set_applies_as_one_undo_step_and_refuses_bad_input() {
        let dispatcher = dispatcher_with(Store::default());
        let scene = match run_later(&dispatcher, "global_default_set", json!({"key": "color_values", "value": "#123456"})) {
            ToolOutcome::Ok(scene) => scene,
            other => panic!("{other:?}"),
        };
        assert!(scene["globalDefaults"]["color_values"].as_str().unwrap().eq_ignore_ascii_case("#123456"));
        assert!(matches!(run_later(&dispatcher, "global_default_set", json!({"key": "nonsense", "value": 1})), ToolOutcome::InvalidParams(_)));
        assert!(matches!(run_later(&dispatcher, "global_default_set", json!({"key": "scale", "value": 0})), ToolOutcome::Failed(_)));
        dispatcher.with_store(|store| assert_eq!(store.history.past.len(), 1));
    }

    #[test]
    fn a_video_locks_the_frame_rate() {
        let dispatcher = dispatcher_with(with_videos(&[("a", 0.0, 10.0)]));
        let ToolOutcome::Failed(message) = run(&dispatcher, "fps_set", json!({"fps": 60})) else { panic!() };
        assert!(message.contains("A video sets the frame rate"), "{message}");
        let summary = ok(run(&dispatcher, "ovrley_summary", json!({})));
        assert!(summary["scene"]["lockedFps"].is_number());
    }
}
