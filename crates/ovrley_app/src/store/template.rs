//! The template state (the loaded template, the global defaults and the aspect ratio), and the
//! editor-effective document: global defaults resolved into widget data for rendering and editing.

use std::rc::Rc;
use ovrley_core::commands::TemplateDescriptor;
use ovrley_core::normalize::template::{normalize_global_defaults, template_manifest, DurableTemplateState};
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::{font_family_name, font_selection, is_color_field_key, normalize_color_fields, pick_defined, resolve_active_widget_data, Collection, LabelStyle, OverlayConfig, WidgetData};
use serde_json::{Map, Value};
use crate::i18n::{catalog as translations, Locale};
use crate::store::editor::{default_config, EditorState};

/// Where the loaded template came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateSource {
    /// A bundled template, by its list id.
    Bundled { template_id: String },
    /// A template file: a user template, an import or a save target.
    File { path: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateState {
    pub loaded_template_source: Option<TemplateSource>,
    pub templates: Vec<TemplateDescriptor>,
    pub global_defaults: Rc<Map<String, Value>>,
    pub aspect_ratio: String,
    pub last_saved_template_state: Option<DurableTemplateState>,
}

impl Default for TemplateState {
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

impl TemplateState {
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

/// The default document and global defaults, no source, nothing saved.
pub fn create_new_template(editor: &mut EditorState, template: &mut TemplateState) {
    editor.config = Rc::new(default_config());
    template.global_defaults = Rc::new(template_manifest().globals.clone());
    template.aspect_ratio = "16:9".into();
    template.loaded_template_source = None;
    template.last_saved_template_state = None;
}

/// A loaded document, its global defaults and its source; the aspect
/// ratio stays.
pub fn hydrate_template_state(editor: &mut EditorState, template: &mut TemplateState, config: OverlayConfig, global_defaults: Map<String, Value>, source: Option<TemplateSource>) {
    editor.config = Rc::new(config);
    template.global_defaults = Rc::new(global_defaults);
    template.loaded_template_source = source;
}

/// The scale a global-default map holds (`globalDefaults.scale`, validated at ingress).
fn scale(defaults: &Map<String, Value>) -> f64 {
    defaults.get("scale").and_then(Value::as_f64).expect("global defaults carry a scale")
}

/// One global default, applied to the existing widgets. A scale change first
/// moves every widget about `scale_origin` (the widgets' bounds). Applying `font_text` resolves the
/// labels' styles in the new family through `label_style`.
pub fn set_global_default(
    editor: &mut EditorState,
    template: &mut TemplateState,
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

/// The manifest's defaults, applied to every widget.
pub fn reset_global_defaults(editor: &mut EditorState, template: &mut TemplateState, label_style: LabelStyle, scale_origin: Option<(f64, f64)>) -> Result<(), String> {
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

pub fn set_custom_aspect_ratio(template: &mut TemplateState) {
    template.aspect_ratio = "custom".into();
}

/// The ratio and its resolution.
pub fn set_aspect_ratio_preset(editor: &mut EditorState, template: &mut TemplateState, ratio: &str, width: u32, height: u32) {
    template.aspect_ratio = ratio.into();
    editor.edit_config(|config| {
        config.scene.insert("width".into(), width.into());
        config.scene.insert("height".into(), height.into());
    });
}

/// The theme colours widget fallbacks use. `ovrley_ui` reads them from the Slint `Theme` global,
/// their one source, when it composes the window (`Store::theme`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeColors {

    pub ice: String,

    pub aqua: String,

    pub teal: String,

    pub accent: String,
}

/// A value that is set: not null, false, zero or an empty string. Widget fields in that state
/// take the global default.
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    })
}

fn or_color(value: Option<&Value>, fallback: &str) -> Value {
    present(value).cloned().unwrap_or_else(|| Value::String(fallback.into()))
}

/// The effective scene: app defaults, the scene, then the global defaults.
pub fn effective_scene(scene: &Map<String, Value>, globals: &Map<String, Value>) -> Map<String, Value> {
    let defaults = &template_manifest().globals;
    let mut effective = defaults.clone();
    for (key, value) in scene {
        effective.insert(key.clone(), value.clone());
    }
    for (key, value) in pick_defined(Some(globals), &defaults.keys().cloned().collect::<Vec<_>>()) {
        effective.insert(key, value);
    }
    effective
}

fn set_font(data: &mut Map<String, Value>, resolved: &Map<String, Value>, global: Option<&Value>) {
    let font = present(resolved.get("font")).or(global).cloned();
    if present(data.get("font")).is_none() {
        if let Some(font) = present(font.as_ref()) {
            data.insert("font".into(), font.clone());
        }
    }
    if present(data.get("font_family")).is_none() {
        let source = present(font.as_ref()).or_else(|| data.get("font_family")).and_then(Value::as_str).unwrap_or("");
        data.insert("font_family".into(), Value::String(font_family_name(source)));
    }
}

fn effective_label(data: &WidgetData, globals: &Map<String, Value>, theme: &ThemeColors) -> WidgetData {
    let mut next = data.clone();
    set_font(&mut next, data, globals.get("font_text"));
    if present(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_text"), &theme.ice));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    next
}

fn effective_value(data: &WidgetData, globals: &Map<String, Value>, theme: &ThemeColors) -> WidgetData {
    let resolved = resolve_active_widget_data(data);
    let mut next = resolved.clone();
    set_font(&mut next, &resolved, globals.get("font_values"));
    if present(next.get("min_max_label_font")).is_none() {
        if let Some(font) = present(resolved.get("min_max_label_font")).or_else(|| present(globals.get("font_values"))) {
            next.insert("min_max_label_font".into(), font.clone());
        }
    }
    if present(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_values"), &theme.ice));
    }
    let kind = resolved.get("value").and_then(Value::as_str).unwrap_or_default();
    if kind == "lap_timer" || kind == "splits_run" {
        if present(next.get("label_font")).is_none() {
            next.insert("label_font".into(), globals.get("font_text").cloned().unwrap_or(Value::Null));
        }
        if present(next.get("label_color")).is_none() {
            next.insert("label_color".into(), or_color(globals.get("color_text"), &theme.ice));
        }
    }
    if resolved.get("display_type").and_then(Value::as_str) == Some("heading_tape") && next.get("label_font").is_none_or(Value::is_null) {
        next.insert("label_font".into(), globals.get("font_values").cloned().unwrap_or(Value::Null));
    }
    if !next.contains_key("icon_color") {
        next.insert("icon_color".into(), or_color(globals.get("color_icons"), &theme.aqua));
    }
    if !next.contains_key("unit_color") && kind != "time" {
        next.insert("unit_color".into(), or_color(globals.get("color_units"), "#ffffff"));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    next
}

/// Chart legends name each metric in the app language at render time, upper-cased.
fn chart_legend_series(series: &Value, language: Locale) -> Value {
    let Some(rows) = series.as_array() else { return series.clone() };
    Value::Array(
        rows.iter()
            .map(|row| {
                let mut row = row.as_object().cloned().unwrap_or_default();
                let label = row
                    .get("metric")
                    .and_then(Value::as_str)
                    .and_then(|metric| catalog().widget(metric))
                    .map(|definition| translations().t(language, &definition.short_name_key).to_uppercase())
                    .unwrap_or_default();
                row.insert("label".into(), Value::String(label));
                Value::Object(row)
            })
            .collect(),
    )
}

fn effective_plot(data: &WidgetData, globals: &Map<String, Value>, language: Locale, theme: &ThemeColors) -> WidgetData {
    let mut next = data.clone();
    let kind = data.get("value").and_then(Value::as_str).unwrap_or_default();
    if kind == "rolling_chart" {
        let series = chart_legend_series(next.get("series").unwrap_or(&Value::Null), language);
        next.insert("series".into(), series);
    }
    if present(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_values"), &theme.ice));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    if (kind == "elevation" || kind == "diving_profile") && present(globals.get("font_values")).is_some() {
        let mut label = next.get("point_label").and_then(Value::as_object).cloned().unwrap_or_default();
        if present(label.get("font")).is_none() {
            let font = globals["font_values"].as_str().unwrap_or_default();
            for (key, value) in font_selection(font) {
                label.insert(key, value);
            }
        }
        next.insert("point_label".into(), Value::Object(label));
    }
    next
}

/// One widget's render-effective data. `globals` are normalized.
pub fn effective_widget_data(collection: Collection, data: &WidgetData, globals: &Map<String, Value>, language: Locale, theme: &ThemeColors) -> WidgetData {
    match collection {
        Collection::Backdrops => resolve_active_widget_data(data),
        Collection::Labels => effective_label(data, globals, theme),
        Collection::Values => effective_value(data, globals, theme),
        Collection::Plots => effective_plot(data, globals, language, theme),
        Collection::Rasters => data.clone(),
    }
}

pub fn editor_effective_config(config: &OverlayConfig, global_defaults: &Map<String, Value>, language: Locale, theme: &ThemeColors) -> OverlayConfig {
    let globals = normalize_global_defaults(Some(global_defaults));
    let mut effective = OverlayConfig {
        scene: effective_scene(&config.scene, &globals),
        ..OverlayConfig::default()
    };
    for collection in Collection::ALL {
        *effective.collection_mut(collection) = config.collection(collection).iter().map(|data| effective_widget_data(collection, data, &globals, language, theme)).collect();
    }
    effective
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_new_template_resets_document_defaults_source_and_saved_state() {
        let mut editor = EditorState::default();
        let mut template = TemplateState::default();
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
        let mut editor = EditorState::default();
        let mut template = TemplateState::default();
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
        let mut editor = EditorState::default();
        let mut template = TemplateState::default();
        set_aspect_ratio_preset(&mut editor, &mut template, "9:16", 1080, 1920);
        assert_eq!((template.aspect_ratio.as_str(), editor.config.scene["width"].clone(), editor.config.scene["height"].clone()), ("9:16", json!(1080), json!(1920)));
        set_custom_aspect_ratio(&mut template);
        assert_eq!(template.aspect_ratio, "custom");
        assert_eq!(editor.config.scene["width"], json!(1080), "custom keeps the size");
    }

    #[test]
    fn hydration_keeps_the_aspect_ratio_and_the_playhead() {
        let mut editor = EditorState::default();
        let mut template = TemplateState { aspect_ratio: "custom".into(), ..TemplateState::default() };
        let config = OverlayConfig::from_value(&json!({"scene": {"start": 4, "end": 20}})).unwrap();
        hydrate_template_state(&mut editor, &mut template, config, Map::new(), Some(TemplateSource::Bundled { template_id: "a.json".into() }));
        assert_eq!(template.aspect_ratio, "custom");
        assert_eq!(editor.selected_second, 0.0, "hydration no longer moves the playhead");
    }

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn theme() -> ThemeColors {
        ThemeColors {
            ice: "#010101".into(),
            aqua: "#020202".into(),
            teal: "#030303".into(),
            accent: "#040404".into(),
        }
    }

    fn globals() -> Map<String, Value> {
        normalize_global_defaults(Some(&map(json!({"font_text": "Text.ttf", "font_values": "Value.ttf", "color_text": "#111111",
            "color_values": "#222222", "color_icons": "#333333", "color_units": "#444444", "opacity": 0.5}))))
    }

    #[test]
    fn global_defaults_resolve_into_labels_values_and_plots() {
        let config = OverlayConfig::from_value(&json!({
            "scene": {"width": 1920},
            "labels": [{"id": "l", "text": "Ride"}],
            "values": [{"id": "v", "value": "speed"}, {"id": "t", "value": "time"}],
            "plots": [{"id": "p", "value": "elevation", "point_label": {}}]
        }))
        .unwrap();
        let effective = editor_effective_config(&config, &globals(), Locale::En, &theme());
        assert_eq!(effective.scene["width"], 1920);
        assert_eq!(effective.scene["opacity"], 0.5);
        let label = &effective.labels[0];
        assert_eq!((label["font"].clone(), label["font_family"].clone(), label["color"].clone()), (json!("Text.ttf"), json!("Text"), json!("#111111")));
        let value = &effective.values[0];
        assert_eq!(value["font"], "Value.ttf");
        assert_eq!(value["icon_color"], "#333333");
        assert_eq!(value["unit_color"], "#444444");
        assert!(!effective.values[1].contains_key("unit_color"), "time widgets have no unit colour");
        assert_eq!(effective.plots[0]["point_label"]["font"], "Value.ttf");
        assert_eq!(effective.plots[0]["color"], "#222222");
    }

    #[test]
    fn values_resolve_their_active_display() {
        let data = map(json!({"id": "h", "value": "heading", "display_type": "heading_tape", "display_variants": {"heading_tape": {"width": 500}}}));
        let effective = effective_widget_data(Collection::Values, &data, &globals(), Locale::En, &theme());
        assert_eq!(effective["width"], 500);
        assert_eq!(effective["label_font"], "Value.ttf", "an absent heading label font takes the value font");
    }

    #[test]
    fn chart_legends_are_named_in_the_app_language() {
        let data = map(json!({"id": "c", "value": "rolling_chart", "series": [{"metric": "speed"}]}));
        let effective = effective_widget_data(Collection::Plots, &data, &globals(), Locale::En, &theme());
        let expected = translations().t(Locale::En, &catalog().require_widget("speed").unwrap().short_name_key).to_uppercase();
        assert_eq!(effective["series"][0]["label"], Value::String(expected));
    }

    #[test]
    fn missing_colours_fall_back_to_the_theme() {
        let data = map(json!({"id": "v", "value": "speed"}));
        let effective = effective_widget_data(Collection::Values, &data, &Map::new(), Locale::En, &theme());
        assert_eq!((effective["color"].clone(), effective["icon_color"].clone()), (json!("#010101"), json!("#020202")));
    }

    #[test]
    fn explicit_widget_values_win() {
        let data = map(json!({"id": "l", "text": "x", "font": "Own.ttf", "color": "#abcdef", "opacity": 1}));
        let effective = effective_widget_data(Collection::Labels, &data, &globals(), Locale::En, &theme());
        assert_eq!((effective["font"].clone(), effective["color"].clone(), effective["opacity"].clone()), (json!("Own.ttf"), json!("#abcdef"), json!(1)));
    }
}
