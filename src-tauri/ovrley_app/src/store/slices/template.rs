//! Port of `store/slices/createTemplateSlice.js`: the loaded template, the global defaults and the
//! aspect ratio.

use super::editor::{default_config, EditorSlice};
use ovrley_core::normalize::template::{normalize_global_defaults, template_manifest, DurableTemplateState};
use ovrley_widgets::model::document::{is_color_field_key, normalize_color_fields, LabelStyle, OverlayConfig};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::rc::Rc;

/// Where the loaded template came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateSource {
    /// A bundled template, by its list id.
    Bundled { template_id: String },
    /// A template file: a user template, an import or a save target.
    File { path: String },
}

/// Whether a listed template is bundled or the user's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum TemplateKind {
    #[serde(rename = "built-in")]
    BuiltIn,
    #[serde(rename = "user")]
    User,
}

/// One entry of `backend_list_templates`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDescriptor {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: TemplateKind,
    pub width: u32,
    pub height: u32,
    /// User templates only.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateSlice {
    pub loaded_template_source: Option<TemplateSource>,
    pub templates: Vec<TemplateDescriptor>,
    pub global_defaults: Rc<Map<String, Value>>,
    pub aspect_ratio: String,
    pub last_saved_template_state: Option<DurableTemplateState>,
}

impl Default for TemplateSlice {
    fn default() -> Self {
        Self {
            loaded_template_source: None,
            templates: Vec::new(),
            global_defaults: Rc::new(template_manifest().globals.clone()),
            aspect_ratio: "16:9".into(),
            last_saved_template_state: None,
        }
    }
}

impl TemplateSlice {
    pub fn set_templates(&mut self, templates: Vec<TemplateDescriptor>) {
        self.templates = templates;
    }

    pub fn set_last_saved_template_state(&mut self, state: Option<DurableTemplateState>) {
        self.last_saved_template_state = state;
    }

    pub fn set_loaded_template_source(&mut self, source: Option<TemplateSource>) {
        self.loaded_template_source = source;
    }

    pub fn find_template(&self, id: &str) -> Option<&TemplateDescriptor> {
        self.templates.iter().find(|template| template.id == id)
    }
}

/// `createNewTemplate`: the default document and global defaults, no source, nothing saved.
pub fn create_new_template(editor: &mut EditorSlice, template: &mut TemplateSlice) {
    editor.config = Rc::new(default_config());
    template.global_defaults = Rc::new(template_manifest().globals.clone());
    template.aspect_ratio = "16:9".into();
    template.loaded_template_source = None;
    template.last_saved_template_state = None;
}

/// `hydrateTemplateState`: a loaded document, its global defaults and its source; the aspect
/// ratio stays.
pub fn hydrate_template_state(editor: &mut EditorSlice, template: &mut TemplateSlice, config: OverlayConfig, global_defaults: Map<String, Value>, source: Option<TemplateSource>) {
    editor.config = Rc::new(config);
    template.global_defaults = Rc::new(global_defaults);
    template.loaded_template_source = source;
}

/// The scale a global-default map holds (`globalDefaults.scale`, validated at ingress).
fn scale(defaults: &Map<String, Value>) -> f64 {
    defaults.get("scale").and_then(Value::as_f64).expect("global defaults carry a scale")
}

/// `setGlobalDefault`: one global default, applied to the existing widgets. A scale change first
/// moves every widget about `scale_origin` (the widgets' bounds). Applying `font_text` resolves the
/// labels' styles in the new family through `label_style`.
pub fn set_global_default(
    editor: &mut EditorSlice,
    template: &mut TemplateSlice,
    key: &str,
    value: Value,
    label_style: LabelStyle,
    scale_origin: Option<(f64, f64)>,
) -> Result<(), String> {
    if key == "scale" && !value.as_f64().is_some_and(|scale| scale.is_finite() && scale > 0.0) {
        return Err("globalDefaults.scale must be a finite positive number".into());
    }
    let mut merged = (*template.global_defaults).clone();
    let value = if is_color_field_key(key) {
        let mut field = Map::new();
        field.insert(key.into(), value);
        normalize_color_fields(&field).remove(key).expect("normalized colours keep their key")
    } else {
        value
    };
    merged.insert(key.into(), value);
    let next = normalize_global_defaults(Some(&merged));
    let mut config = (*editor.config).clone();
    if key == "scale" {
        let factor = scale(&next) / scale(&template.global_defaults);
        if factor != 1.0 {
            config.scale_widget_positions(factor, scale_origin.ok_or("a scale change needs its origin")?);
        }
    }
    let config = config.sync_global_defaults(&next, Some(&[key]), label_style)?;
    template.global_defaults = Rc::new(next);
    editor.set_config(config);
    Ok(())
}

/// `resetGlobalDefaults`: the manifest's defaults, applied to every widget.
pub fn reset_global_defaults(editor: &mut EditorSlice, template: &mut TemplateSlice, label_style: LabelStyle, scale_origin: Option<(f64, f64)>) -> Result<(), String> {
    let next = template_manifest().globals.clone();
    let mut config = (*editor.config).clone();
    let factor = scale(&next) / scale(&template.global_defaults);
    if factor != 1.0 {
        config.scale_widget_positions(factor, scale_origin.ok_or("a scale change needs its origin")?);
    }
    let config = config.sync_global_defaults(&next, None, label_style)?;
    template.global_defaults = Rc::new(next);
    editor.set_config(config);
    Ok(())
}

/// `setCustomAspectRatio`.
pub fn set_custom_aspect_ratio(template: &mut TemplateSlice) {
    template.aspect_ratio = "custom".into();
}

/// `setAspectRatioPreset`: the ratio and its resolution.
pub fn set_aspect_ratio_preset(editor: &mut EditorSlice, template: &mut TemplateSlice, ratio: &str, width: u32, height: u32) {
    template.aspect_ratio = ratio.into();
    editor.edit_config(|config| {
        config.scene.insert("width".into(), width.into());
        config.scene.insert("height".into(), height.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn descriptors_parse_the_template_list() {
        let list: Vec<TemplateDescriptor> = serde_json::from_value(json!([
            {"id": "a.json", "name": "A", "type": "built-in", "width": 1920, "height": 1080},
            {"id": "user:b.json", "name": "B", "type": "user", "width": 0, "height": 0, "path": "/t/b.json"}
        ]))
        .unwrap();
        assert_eq!(list[0].kind, TemplateKind::BuiltIn);
        assert_eq!(list[1].path.as_deref(), Some("/t/b.json"));
    }

    #[test]
    fn a_new_template_resets_document_defaults_source_and_saved_state() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice::default();
        editor.set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "a"}], "scene": {"start": 9}})).unwrap());
        editor.selected_second = 30.0;
        template.loaded_template_source = Some(TemplateSource::File { path: "/a.json".into() });
        template.aspect_ratio = "custom".into();
        create_new_template(&mut editor, &mut template);
        assert!(editor.config.labels.is_empty());
        assert_eq!(template.loaded_template_source, None);
        assert_eq!(template.aspect_ratio, "16:9");
        assert_eq!(editor.selected_second, 30.0, "a new template keeps the playhead");
    }

    fn no_labels(_: bool, _: f64) -> Result<(bool, f64), String> {
        panic!("no label is resolved")
    }

    #[test]
    fn a_global_default_normalizes_and_applies_to_widgets() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice::default();
        editor.set_config(OverlayConfig::from_value(&json!({"values": [{"id": "v", "value": "speed", "x": 100, "y": 50, "color": "#ffffff"}]})).unwrap());
        set_global_default(&mut editor, &mut template, "color_values", json!("#FFAA00"), &no_labels, None).unwrap();
        assert_eq!(template.global_defaults["color_values"], "#ffaa00");
        assert_eq!(editor.config.values[0]["color"], "#ffaa00");
        assert!(set_global_default(&mut editor, &mut template, "scale", json!(0), &no_labels, None).is_err());
        // Scale moves widgets about the origin.
        set_global_default(&mut editor, &mut template, "scale", json!(2), &no_labels, Some((0.0, 0.0))).unwrap();
        assert_eq!((editor.config.values[0]["x"].clone(), editor.config.values[0]["y"].clone()), (json!(200), json!(100)));
        reset_global_defaults(&mut editor, &mut template, &no_labels, Some((0.0, 0.0))).unwrap();
        assert_eq!(editor.config.values[0]["x"], json!(100), "reset scales back");
        assert_eq!(*template.global_defaults, template_manifest().globals);
    }

    #[test]
    fn aspect_ratio_presets_set_the_scene_size() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice::default();
        set_aspect_ratio_preset(&mut editor, &mut template, "9:16", 1080, 1920);
        assert_eq!((template.aspect_ratio.as_str(), editor.config.scene["width"].clone(), editor.config.scene["height"].clone()), ("9:16", json!(1080), json!(1920)));
        set_custom_aspect_ratio(&mut template);
        assert_eq!(template.aspect_ratio, "custom");
        assert_eq!(editor.config.scene["width"], json!(1080), "custom keeps the size");
    }

    #[test]
    fn hydration_keeps_the_aspect_ratio_and_the_playhead() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice { aspect_ratio: "custom".into(), ..TemplateSlice::default() };
        let config = OverlayConfig::from_value(&json!({"scene": {"start": 4, "end": 20}})).unwrap();
        hydrate_template_state(&mut editor, &mut template, config, Map::new(), Some(TemplateSource::Bundled { template_id: "a.json".into() }));
        assert_eq!(template.aspect_ratio, "custom");
        assert_eq!(editor.selected_second, 0.0, "hydration no longer moves the playhead");
    }
}
